//! End-to-end checks named in `docs/08-edge-cases.md`.

use std::path::Path;

use edgedoc::{
    Budget, CsvOptions, Document, EditOptions, Flavor, Join, MergeKind, MissingField, Mm, Page,
    Paragraph, Pt, ReadOptions, RecordRange, Records, RunBuilder, StoryRef, Style, Table,
};
use edgedoc_opc::write_zip;

type Test = Result<(), String>;

fn err(err: impl std::fmt::Display) -> String {
    err.to_string()
}

fn docx(document: &str, extra: &[(&str, &[u8])]) -> TestResult {
    let mut parts = vec![
        ("[Content_Types].xml".into(), types_xml(extra)),
        (
            "_rels/.rels".into(),
            br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#.to_vec(),
        ),
        ("word/document.xml".into(), document.as_bytes().to_vec()),
    ];
    for (name, bytes) in extra {
        parts.push(((*name).into(), bytes.to_vec()));
    }
    write_zip(&parts).map_err(err)
}

type TestResult = Result<Vec<u8>, String>;

fn types_xml(extra: &[(&str, &[u8])]) -> Vec<u8> {
    let mut xml = String::from(
        r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Default Extension="bin" ContentType="application/vnd.ms-office.vbaProject"/><Default Extension="emf" ContentType="image/x-emf"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>"#,
    );
    for (name, _) in extra {
        if name.ends_with(".xml") && *name != "word/document.xml" {
            xml.push_str(&format!(
                r#"<Override PartName="/{name}" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml"/>"#
            ));
        }
    }
    xml.push_str("</Types>");
    xml.into_bytes()
}

fn body(inner: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><w:body>{inner}</w:body></w:document>"#
    )
}

fn part_text(bytes: &[u8], name: &str) -> Result<String, String> {
    let package = edgedoc_opc::read_zip(bytes, &Budget::default()).map_err(err)?;
    let part = package
        .part(name)
        .ok_or_else(|| format!("missing {name}"))?;
    Ok(String::from_utf8_lossy(&part.bytes).into_owned())
}

fn code(result: Result<Document, edgedoc::Error>) -> Result<&'static str, String> {
    match result {
        Ok(_) => Err("the call returned a document".into()),
        Err(err) => Ok(err.code),
    }
}

#[test]
fn empty_document_has_one_paragraph() -> Test {
    let doc = Document::builder().build().map_err(err)?;
    if doc.body_len() != 1 || !doc.plain_text(StoryRef::Body).is_empty() {
        return Err(format!(
            "body {} text {:?}",
            doc.body_len(),
            doc.plain_text(StoryRef::Body)
        ));
    }
    let bytes = doc.write_bytes().map_err(err)?;
    let again = Document::read_bytes(&bytes).map_err(err)?;
    if again.body_len() != 1 {
        return Err("the saved empty document lost its paragraph".into());
    }
    Ok(())
}

#[test]
fn missing_main_part() -> Test {
    let bytes = write_zip(&[
        ("[Content_Types].xml".into(), b"<Types></Types>".to_vec()),
        (
            "_rels/.rels".into(),
            b"<Relationships></Relationships>".to_vec(),
        ),
    ])
    .map_err(err)?;
    let got = code(Document::read_bytes(&bytes))?;
    if got != "package.not_word" {
        return Err(got.into());
    }
    Ok(())
}

#[test]
fn zip_budget() -> Test {
    let bytes = docx(&body("<w:p><w:r><w:t>Hi</w:t></w:r></w:p>"), &[])?;
    let budget = Budget {
        max_part: 8,
        ..Budget::default()
    };
    let options = ReadOptions {
        password: None,
        budget,
        unknown: edgedoc::UnknownPart::Retain,
    };
    let got = code(Document::read_bytes_with(&bytes, &options))?;
    if got != "package.limit" {
        return Err(got.into());
    }
    Ok(())
}

#[test]
fn xml_depth() -> Test {
    let bytes = docx(&body("<w:p><w:r><w:t>Hi</w:t></w:r></w:p>"), &[])?;
    let budget = Budget {
        max_depth: 3,
        ..Budget::default()
    };
    let options = ReadOptions {
        password: None,
        budget,
        unknown: edgedoc::UnknownPart::Retain,
    };
    let got = code(Document::read_bytes_with(&bytes, &options))?;
    if got != "xml.limit" {
        return Err(got.into());
    }
    Ok(())
}

#[test]
fn no_dtd() -> Test {
    let xml = r#"<?xml version="1.0"?><!DOCTYPE document [<!ENTITY x "hi">]><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>&x;</w:t></w:r></w:p></w:body></w:document>"#;
    let bytes = docx(xml, &[])?;
    let got = code(Document::read_bytes(&bytes))?;
    if got != "xml.malformed" {
        return Err(got.into());
    }
    Ok(())
}

#[test]
fn strict_read() -> Test {
    let xml = r#"<?xml version="1.0"?><w:document xmlns:w="http://purl.oclc.org/ooxml/wordprocessingml/main"><w:body><w:p><w:r><w:t>Strict</w:t></w:r></w:p></w:body></w:document>"#;
    let bytes = docx(xml, &[])?;
    let mut doc = Document::read_bytes(&bytes).map_err(err)?;
    if doc.plain_text(StoryRef::Body) != "Strict" {
        return Err(doc.plain_text(StoryRef::Body));
    }
    doc.edit(EditOptions::default(), |edit| {
        edit.body().paragraph_at(0, |para| para.text("!"))?;
        Ok(())
    })
    .map_err(err)?;
    let out = doc.write_bytes().map_err(err)?;
    let text = part_text(&out, "word/document.xml")?;
    if !text.contains("http://schemas.openxmlformats.org/wordprocessingml/2006/main") {
        return Err("the writer did not use the Transitional namespace".into());
    }
    Ok(())
}

#[test]
fn alternate_content() -> Test {
    let xml = body(
        r#"<mc:AlternateContent xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006"><mc:Choice Requires="w14"><w:p><w:r><w:t>Choice</w:t></w:r></w:p></mc:Choice><mc:Fallback><w:p><w:r><w:t>Fallback</w:t></w:r></w:p></mc:Fallback></mc:AlternateContent>"#,
    );
    let bytes = docx(&xml, &[])?;
    let doc = Document::read_bytes(&bytes).map_err(err)?;
    if doc.plain_text(StoryRef::Body) != "Choice" {
        return Err(doc.plain_text(StoryRef::Body));
    }
    if doc.write_bytes().map_err(err)? != bytes {
        return Err("an unedited document was rewritten".into());
    }
    if !part_text(&bytes, "word/document.xml")?.contains("Fallback") {
        return Err("the fallback is missing from the package".into());
    }
    Ok(())
}

fn encrypted_fixture() -> Vec<u8> {
    let mut bytes = vec![0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
    for unit in "EncryptionInfo".encode_utf16() {
        bytes.extend(unit.to_le_bytes());
    }
    bytes
}

#[test]
fn password_required() -> Test {
    let got = code(Document::read_bytes(&encrypted_fixture()))?;
    if got != "package.password_required" {
        return Err(got.into());
    }
    Ok(())
}

#[test]
fn wrong_password() -> Test {
    let options = ReadOptions {
        password: Some("not-the-password"),
        ..ReadOptions::new()
    };
    let got = code(Document::read_bytes_with(&encrypted_fixture(), &options))?;
    if got != "package.encrypted_unsupported" && got != "package.wrong_password" {
        return Err(got.into());
    }
    if got == "package.wrong_password" {
        return Ok(());
    }
    Ok(())
}

#[test]
fn vba_passthrough() -> Test {
    let vba = b"macro-bytes-not-executed";
    let bytes = docx(
        &body("<w:p><w:r><w:t>Body</w:t></w:r></w:p>"),
        &[("word/vbaProject.bin", vba)],
    )?;
    let mut doc = Document::read_bytes(&bytes).map_err(err)?;
    doc.edit(EditOptions::default(), |edit| {
        edit.body().paragraph_at(0, |para| para.text("!"))?;
        Ok(())
    })
    .map_err(err)?;
    let out = doc.write_bytes().map_err(err)?;
    let package = edgedoc_opc::read_zip(&out, &Budget::default()).map_err(err)?;
    let kept = package
        .part("word/vbaProject.bin")
        .ok_or("vba part missing")?;
    if kept.bytes != vba {
        return Err("vba bytes changed".into());
    }
    Ok(())
}

#[test]
fn clean_save_is_identical() -> Test {
    let bytes = docx(&body("<w:p><w:r><w:t>Same</w:t></w:r></w:p>"), &[])?;
    let doc = Document::read_bytes(&bytes).map_err(err)?;
    if doc.is_dirty() {
        return Err("a fresh read is dirty".into());
    }
    if doc.write_bytes().map_err(err)? != bytes {
        return Err("clean save changed the bytes".into());
    }
    Ok(())
}

#[test]
fn unknown_part_kept() -> Test {
    let custom = b"<custom>kept</custom>";
    let bytes = docx(
        &body("<w:p><w:r><w:t>Body</w:t></w:r></w:p>"),
        &[("customXml/item1.xml", custom)],
    )?;
    let mut doc = Document::read_bytes(&bytes).map_err(err)?;
    doc.edit(EditOptions::default(), |edit| {
        edit.body().paragraph_after_last(|para| para.text("More"))?;
        Ok(())
    })
    .map_err(err)?;
    let out = doc.write_bytes().map_err(err)?;
    let package = edgedoc_opc::read_zip(&out, &Budget::default()).map_err(err)?;
    let kept = package
        .part("customXml/item1.xml")
        .ok_or("custom part missing")?;
    if kept.bytes != custom {
        return Err("unknown part bytes changed".into());
    }
    Ok(())
}

#[test]
fn revision_consent() -> Test {
    let xml = body(r#"<w:p><w:ins w:id="1"><w:r><w:t>Old</w:t></w:r></w:ins></w:p>"#);
    let bytes = docx(&xml, &[])?;
    let mut doc = Document::read_bytes(&bytes).map_err(err)?;
    let denied = doc.edit(EditOptions::default(), |edit| {
        edit.body().paragraph_at(0, |para| para.text("x"))?;
        Ok(())
    });
    if code(denied.map(|_| doc.clone()))? != "model.needs_drop_consent" {
        return Err("consent was not required".into());
    }
    doc.edit(
        EditOptions::default().allow(edgedoc::DropKind::Revisions),
        |edit| {
            edit.body().paragraph_at(0, |para| para.text("x"))?;
            Ok(())
        },
    )
    .map_err(err)?;
    Ok(())
}

#[test]
fn undefined_style() -> Test {
    let xml = body(
        r#"<w:p><w:pPr><w:pStyle w:val="Missing"/></w:pPr><w:r><w:t>Styled</w:t></w:r></w:p>"#,
    );
    let bytes = docx(&xml, &[])?;
    let mut doc = Document::read_bytes(&bytes).map_err(err)?;
    if doc.plain_text(StoryRef::Body) != "Styled" || doc.style_outline("Missing").is_some() {
        return Err("undefined style did not behave as Normal".into());
    }
    doc.edit(EditOptions::default(), |edit| {
        edit.body().paragraph_at(0, |para| para.text("!"))?;
        Ok(())
    })
    .map_err(err)?;
    let out = part_text(&doc.write_bytes().map_err(err)?, "word/document.xml")?;
    if !out.contains("Missing") {
        return Err("the undefined style name was dropped".into());
    }
    Ok(())
}

#[test]
fn builtin_heading_level() -> Test {
    let styles = br#"<?xml version="1.0"?><w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/></w:style></w:styles>"#;
    let rels = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>"#;
    let bytes = docx(
        &body(
            r#"<w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>Title</w:t></w:r></w:p>"#,
        ),
        &[
            ("word/styles.xml", styles.as_slice()),
            ("word/_rels/document.xml.rels", rels.as_slice()),
        ],
    )?;
    let doc = Document::read_bytes(&bytes).map_err(err)?;
    if doc.style_outline("Heading1") != Some(0)
        && doc.style_outline("heading 1") != Some(0)
        && doc.style_outline("Heading1") != Some(1)
    {
        return Err(format!("outline {:?}", doc.style_outline("Heading1")));
    }
    let level = doc
        .style_outline("Heading1")
        .or_else(|| doc.style_outline("heading 1"));
    if level != Some(1) && level != Some(0) {
        return Err(format!("level {level:?}"));
    }
    if level != Some(1) {
        return Err("heading 1 outline level is not 1".into());
    }
    Ok(())
}

#[test]
fn table_63_columns() -> Test {
    let xml = body(
        r#"<w:tbl><w:tr><w:tc><w:tcPr><w:gridSpan w:val="64"/></w:tcPr><w:p><w:r><w:t>A</w:t></w:r></w:p></w:tc></w:tr></w:tbl>"#,
    );
    let doc = Document::read_bytes(&docx(&xml, &[])?).map_err(err)?;
    if doc.cell_span(0, 0, 0) != Some(63) {
        return Err(format!("span {:?}", doc.cell_span(0, 0, 0)));
    }
    Ok(())
}

#[test]
fn table_depth() -> Test {
    let mut xml = String::from("<w:p><w:r><w:t>x</w:t></w:r></w:p>");
    for _ in 0..30 {
        xml = format!("<w:tbl><w:tr><w:tc>{xml}</w:tc></w:tr></w:tbl>");
    }
    let got = code(Document::read_bytes(&docx(&body(&xml), &[])?))?;
    if got != "model.limit" {
        return Err(got.into());
    }
    Ok(())
}

#[test]
fn char_boundary() -> Test {
    let mut doc = Document::builder()
        .paragraph(Paragraph::new().text("é"))
        .build()
        .map_err(err)?;
    let denied = doc.edit(EditOptions::default(), |edit| {
        edit.body()
            .paragraph_at(0, |para| para.insert_text(1, "x"))?;
        Ok(())
    });
    if code(denied.map(|_| doc.clone()))? != "model.bad_offset" {
        return Err("a byte inside a character was accepted".into());
    }
    Ok(())
}

#[test]
fn balance_fields() -> Test {
    let xml =
        body(r#"<w:p><w:r><w:fldChar w:fldCharType="end"/></w:r><w:r><w:t>Keep</w:t></w:r></w:p>"#);
    let mut doc = Document::read_bytes(&docx(&xml, &[])?).map_err(err)?;
    let removed = doc.balance_fields();
    if removed == 0 {
        return Err("the orphan field was kept".into());
    }
    if !doc.plain_text(StoryRef::Body).contains("Keep") {
        return Err(doc.plain_text(StoryRef::Body));
    }
    Ok(())
}

#[test]
fn mergefield_boundary() -> Test {
    let doc = Document::builder()
        .paragraph(
            Paragraph::new()
                .field_instr("MERGEFIELDX Name")
                .merge_field("Name"),
        )
        .build()
        .map_err(err)?;
    let names = doc.merge_field_names();
    if names != vec!["Name".to_string()] {
        return Err(format!("{names:?}"));
    }
    Ok(())
}

#[test]
fn quoted_field_name() -> Test {
    let doc = Document::builder()
        .paragraph(Paragraph::new().merge_field("Invoice Number"))
        .build()
        .map_err(err)?;
    let records =
        Records::from_rows(vec!["Invoice Number".into()], vec![vec!["A-1".into()]]).map_err(err)?;
    let batch = doc
        .merge()
        .records(records)
        .missing(MissingField::Error)
        .finish()
        .map_err(err)?;
    let letter = batch.document(0).ok_or("no letter")?;
    if letter.plain_text(StoryRef::Body) != "«Invoice Number»".replace("«Invoice Number»", "A-1")
        && letter.plain_text(StoryRef::Body) != "A-1"
    {
        return Err(letter.plain_text(StoryRef::Body));
    }
    if letter.plain_text(StoryRef::Body) != "A-1" {
        return Err(letter.plain_text(StoryRef::Body));
    }
    Ok(())
}

#[test]
fn missing_field_errors() -> Test {
    let doc = Document::builder()
        .paragraph(Paragraph::new().merge_field("Amount"))
        .build()
        .map_err(err)?;
    let records = Records::from_rows(vec!["Name".into()], vec![vec!["Ada".into()]]).map_err(err)?;
    match doc
        .merge()
        .records(records)
        .missing(MissingField::Error)
        .finish()
    {
        Err(err) if err.code == "merge.unknown_field" => Ok(()),
        Ok(_) => Err("missing field did not fail the batch".into()),
        Err(err) => Err(err.message),
    }
}

#[test]
fn missing_field_empty() -> Test {
    let doc = Document::builder()
        .paragraph(Paragraph::new().text("Due ").merge_field("Amount"))
        .build()
        .map_err(err)?;
    let records = Records::from_rows(vec!["Name".into()], vec![vec!["Ada".into()]]).map_err(err)?;
    let batch = doc
        .merge()
        .records(records)
        .missing(MissingField::Empty)
        .finish()
        .map_err(err)?;
    let letter = batch.document(0).ok_or("no letter")?;
    if letter.plain_text(StoryRef::Body) != "Due " {
        return Err(letter.plain_text(StoryRef::Body));
    }
    Ok(())
}

#[test]
fn duplicate_header() -> Test {
    let result = Records::from_rows(
        vec!["Name".into(), "name".into()],
        vec![vec!["A".into(), "B".into()]],
    );
    match result {
        Err(err) if err.code == "merge.duplicate_header" => Ok(()),
        Ok(_) => Err("duplicate headers were accepted".into()),
        Err(err) => Err(err.message),
    }
}

#[test]
fn numeric_compare() -> Test {
    let doc = Document::builder()
        .paragraph(Paragraph::new().field_instr(r#"IF { MERGEFIELD Amount } = "7.0" "yes" "no""#))
        .build()
        .map_err(err)?;
    let records = Records::from_rows(vec!["Amount".into()], vec![vec!["7".into()]]).map_err(err)?;
    let letter = doc
        .merge()
        .records(records)
        .finish()
        .map_err(err)?
        .document(0)
        .ok_or("no letter")?;
    if letter.plain_text(StoryRef::Body) != "yes" {
        return Err(letter.plain_text(StoryRef::Body));
    }
    Ok(())
}

#[test]
fn next_record() -> Test {
    let doc = Document::builder()
        .paragraph(Paragraph::new().field_instr("NEXT").merge_field("Name"))
        .build()
        .map_err(err)?;
    let records = Records::from_rows(
        vec!["Name".into()],
        vec![vec!["Ada".into()], vec!["Bea".into()]],
    )
    .map_err(err)?;
    let letter = doc
        .merge()
        .records(records)
        .finish()
        .map_err(err)?
        .document(0)
        .ok_or("no letter")?;
    if letter.plain_text(StoryRef::Body) != "Bea" {
        return Err(letter.plain_text(StoryRef::Body));
    }
    Ok(())
}

#[test]
fn skipif() -> Test {
    let doc = Document::builder()
        .paragraph(
            Paragraph::new()
                .field_instr(r#"SKIPIF { MERGEFIELD Flag } = "Y""#)
                .merge_field("Name"),
        )
        .build()
        .map_err(err)?;
    let records = Records::from_rows(
        vec!["Flag".into(), "Name".into()],
        vec![
            vec!["Y".into(), "Skip".into()],
            vec!["N".into(), "Keep".into()],
        ],
    )
    .map_err(err)?;
    let batch = doc.merge().records(records).finish().map_err(err)?;
    if batch.len() != 1 {
        return Err(format!("copies {}", batch.len()));
    }
    let letter = batch.document(0).ok_or("no letter")?;
    if letter.plain_text(StoryRef::Body) != "Keep" {
        return Err(letter.plain_text(StoryRef::Body));
    }
    Ok(())
}

#[test]
fn header_field_sections() -> Test {
    let header = br#"<?xml version="1.0"?><w:hdr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:p><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText> MERGEFIELD City </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>City</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r></w:p></w:hdr>"#;
    let rels = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdH" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/header" Target="header1.xml"/></Relationships>"#;
    let document = body(
        r#"<w:p><w:r><w:t>Letter</w:t></w:r></w:p><w:sectPr><w:headerReference w:type="default" r:id="rIdH"/></w:sectPr>"#,
    );
    let bytes = docx(
        &document,
        &[
            ("word/header1.xml", header.as_slice()),
            ("word/_rels/document.xml.rels", rels.as_slice()),
        ],
    )?;
    let doc = Document::read_bytes(&bytes).map_err(err)?;
    let records = Records::from_rows(
        vec!["City".into()],
        vec![vec!["London".into()], vec!["Paris".into()]],
    )
    .map_err(err)?;
    let joined = doc
        .merge()
        .records(records)
        .kind(MergeKind::Letters)
        .finish()
        .map_err(err)?
        .join(Join::Auto)
        .map_err(err)?;
    if joined.section_break_count() == 0 {
        return Err("header fields did not start a section per record".into());
    }
    Ok(())
}

#[test]
fn directory_join() -> Test {
    let doc = Document::builder()
        .paragraph(Paragraph::new().merge_field("Name"))
        .build()
        .map_err(err)?;
    let records = Records::from_rows(
        vec!["Name".into()],
        vec![vec!["Ada".into()], vec!["Bea".into()]],
    )
    .map_err(err)?;
    let joined = doc
        .merge()
        .records(records)
        .kind(MergeKind::Directory)
        .finish()
        .map_err(err)?
        .join(Join::Auto)
        .map_err(err)?;
    if joined.plain_text(StoryRef::Body).contains('\u{000C}') {
        return Err("a directory insert has a page break".into());
    }
    if !joined.plain_text(StoryRef::Body).contains("Ada")
        || !joined.plain_text(StoryRef::Body).contains("Bea")
    {
        return Err(joined.plain_text(StoryRef::Body));
    }
    Ok(())
}

#[test]
fn empty_range() -> Test {
    let doc = Document::builder()
        .paragraph(Paragraph::new().text("X"))
        .build()
        .map_err(err)?;
    let records = Records::from_rows(vec!["Name".into()], vec![vec!["Ada".into()]]).map_err(err)?;
    match doc
        .merge()
        .records(records)
        .range(RecordRange::from_to(0, 0))
        .finish()
    {
        Err(err) if err.code == "merge.no_records" => Ok(()),
        Ok(_) => Err("an empty range returned a document".into()),
        Err(err) => Err(err.message),
    }
}

#[test]
fn preview_does_not_mutate() -> Test {
    let doc = Document::builder()
        .paragraph(Paragraph::new().text("Hello ").merge_field("Name"))
        .build()
        .map_err(err)?;
    let before = doc.plain_text(StoryRef::Body);
    let records = Records::from_rows(vec!["Name".into()], vec![vec!["Ada".into()]]).map_err(err)?;
    let preview = doc.preview(&records, 0).map_err(err)?;
    if doc.plain_text(StoryRef::Body) != before {
        return Err("preview changed the template".into());
    }
    if preview.plain_text(StoryRef::Body) != "Hello Ada" {
        return Err(preview.plain_text(StoryRef::Body));
    }
    Ok(())
}

#[test]
fn csv_rfc4180() -> Test {
    let csv = "Name,Note\r\n\"Ada\",\"one, two\r\nthree\"\r\n";
    let records = Records::from_csv(csv.as_bytes(), &CsvOptions::default()).map_err(err)?;
    if records.len() != 1 {
        return Err(format!("rows {}", records.len()));
    }
    let note = records.cell(0, "Note").unwrap_or_default();
    if note != "one, two\r\nthree" && note != "one, two\nthree" {
        return Err(note);
    }
    if !note.contains("one, two") || !note.contains("three") {
        return Err(note);
    }
    Ok(())
}

#[test]
fn csv_is_text() -> Test {
    let records =
        Records::from_csv(b"Name,Amount\nAda,=1+1\n", &CsvOptions::default()).map_err(err)?;
    if records.cell(0, "Amount").as_deref() != Some("=1+1") {
        return Err(records.cell(0, "Amount").unwrap_or_default());
    }
    Ok(())
}

#[test]
fn sheet_cached_value() -> Test {
    let file = edgedoc::sheet_fixture().map_err(err)?;
    let records = edgedoc::records_from_xlsx(&file, &Budget::default()).map_err(err)?;
    if records.cell(0, "Amount").as_deref() != Some("2") {
        return Err(format!("{:?}", records.cell(0, "Amount")));
    }
    if records.cell(0, "Amount").as_deref() == Some("1+1") {
        return Err("the formula was evaluated or stored as text".into());
    }
    Ok(())
}

#[test]
fn save_refuses_symlink() -> Test {
    let dir = std::env::temp_dir().join(format!("edgedoc-link-{}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(err)?;
    let target = dir.join("target.docx");
    let link = dir.join("link.docx");
    std::fs::write(&target, b"original").map_err(err)?;
    let _ = std::fs::remove_file(&link);
    std::os::unix::fs::symlink(&target, &link).map_err(err)?;
    let doc = Document::builder()
        .paragraph(Paragraph::new().text("New"))
        .build()
        .map_err(err)?;
    let saved = doc.save(&link);
    let got = match saved {
        Ok(()) => "ok",
        Err(err) => err.code,
    };
    let kept = std::fs::read(&target).map_err(err)?;
    let _ = std::fs::remove_dir_all(&dir);
    if got != "io.destination" {
        return Err(got.into());
    }
    if kept != b"original" {
        return Err("the link target changed".into());
    }
    Ok(())
}

#[test]
fn save_keeps_destination() -> Test {
    let dir = std::env::temp_dir().join(format!("edgedoc-ro-{}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(err)?;
    let dest = dir.join("out.docx");
    std::fs::write(&dest, b"keep-me").map_err(err)?;
    let restore = Reset(&dir);
    let mut perms = std::fs::metadata(&dir).map_err(err)?.permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut perms, 0o555);
    std::fs::set_permissions(&dir, perms).map_err(err)?;
    let doc = Document::builder()
        .paragraph(Paragraph::new().text("New"))
        .build()
        .map_err(err)?;
    let saved = doc.save(&dest);
    let kept = std::fs::read(&dest).map_err(err)?;
    drop(restore);
    match saved {
        Err(err) if err.code == "io.destination" => {}
        Ok(()) => return Err("a read-only directory accepted the save".into()),
        Err(err) => return Err(err.message),
    }
    if kept != b"keep-me" {
        return Err("the destination changed".into());
    }
    Ok(())
}

struct Reset<'a>(&'a Path);

impl Drop for Reset<'_> {
    fn drop(&mut self) {
        if let Ok(meta) = std::fs::metadata(self.0) {
            let mut perms = meta.permissions();
            std::os::unix::fs::PermissionsExt::set_mode(&mut perms, 0o755);
            let _ = std::fs::set_permissions(self.0, perms);
        }
    }
}

#[test]
fn rtl_run() -> Test {
    let doc = Document::builder()
        .paragraph(
            Paragraph::new()
                .run_builder(RunBuilder::new().text("שלום").rtl().lang("he-IL"))
                .map_err(err)?,
        )
        .build()
        .map_err(err)?;
    let bytes = doc.write_bytes().map_err(err)?;
    let again = Document::read_bytes(&bytes).map_err(err)?;
    if !again.run_is_rtl(0) {
        return Err("rtl was dropped".into());
    }
    if again.run_lang(0).as_deref() != Some("he-IL") {
        return Err(format!("{:?}", again.run_lang(0)));
    }
    let xml = part_text(&bytes, "word/document.xml")?;
    if !xml.contains("<w:rtl") || !xml.contains("he-IL") {
        return Err("the package has no rtl markup".into());
    }
    Ok(())
}

#[test]
fn flat_opc() -> Test {
    let xml = r#"<?xml version="1.0"?><pkg:package xmlns:pkg="http://schemas.microsoft.com/office/2006/xmlPackage"><pkg:part pkg:name="/[Content_Types].xml" pkg:contentType="application/xml"><pkg:xmlData><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"></Types></pkg:xmlData></pkg:part><pkg:part pkg:name="/_rels/.rels" pkg:contentType="application/vnd.openxmlformats-package.relationships+xml"><pkg:xmlData><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships></pkg:xmlData></pkg:part><pkg:part pkg:name="/word/document.xml" pkg:contentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"><pkg:xmlData><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>Flat</w:t></w:r></w:p></w:body></w:document></pkg:xmlData></pkg:part></pkg:package>"#;
    let doc = Document::read_bytes(xml.as_bytes()).map_err(err)?;
    if doc.plain_text(StoryRef::Body) != "Flat" {
        return Err(doc.plain_text(StoryRef::Body));
    }
    Ok(())
}

#[test]
fn wordml2003_rejected() -> Test {
    let xml = r#"<?xml version="1.0"?><w:wordDocument xmlns:w="http://schemas.microsoft.com/office/word/2003/wordml"><w:body><w:p><w:r><w:t>Old</w:t></w:r></w:p></w:body></w:wordDocument>"#;
    let got = code(Document::read_bytes(xml.as_bytes()))?;
    if got != "unsupported" {
        return Err(got.into());
    }
    Ok(())
}

#[test]
fn metafile_opaque() -> Test {
    let drawing = r#"<w:p><w:r><w:drawing><a:blip r:embed="rIdE"/></w:drawing></w:r></w:p>"#;
    let rels = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdE" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/pic.emf"/></Relationships>"#;
    let bytes = docx(
        &body(drawing),
        &[
            ("word/media/pic.emf", b"emf"),
            ("word/_rels/document.xml.rels", rels.as_slice()),
        ],
    )?;
    let doc = Document::read_bytes(&bytes).map_err(err)?;
    if !doc.has_opaque_drawing() {
        return Err("the metafile was treated as an owned picture".into());
    }
    Ok(())
}

#[test]
fn locked_control() -> Test {
    let xml = body(
        r#"<w:sdt><w:sdtPr><w:lock w:val="sdtContentLocked"/></w:sdtPr><w:sdtContent><w:p><w:r><w:t>Locked</w:t></w:r></w:p></w:sdtContent></w:sdt>"#,
    );
    let mut doc = Document::read_bytes(&docx(&xml, &[])?).map_err(err)?;
    let denied = doc.edit(EditOptions::default(), |edit| {
        edit.body().control_at(0, |control| {
            control.tag = "x".into();
            Ok(())
        })?;
        Ok(())
    });
    if code(denied.map(|_| doc.clone()))? != "model.locked" {
        return Err("a locked control was edited".into());
    }
    Ok(())
}

#[test]
fn no_secret_in_error() -> Test {
    let doc = Document::builder()
        .paragraph(Paragraph::new().merge_field("Amount"))
        .build()
        .map_err(err)?;
    let records = Records::from_rows(vec!["Secret".into()], vec![vec!["s3cret-value".into()]])
        .map_err(err)?;
    let message = match doc
        .merge()
        .records(records)
        .missing(MissingField::Error)
        .finish()
    {
        Ok(_) => return Err("the missing field succeeded".into()),
        Err(err) => err.message,
    };
    if message.contains("s3cret-value") {
        return Err("the error contains a record value".into());
    }
    Ok(())
}

#[test]
fn merge_block_budget() -> Test {
    if Budget::default().max_merged_blocks != 200_000 {
        return Err("the default block cap is not 200000".into());
    }
    let doc = Document::builder()
        .paragraph(Paragraph::new().merge_field("Name"))
        .build()
        .map_err(err)?;
    let mut doc = doc;
    let budget = Budget {
        max_merged_blocks: 1,
        ..Budget::default()
    };
    doc.set_budget(budget);
    let records = Records::from_rows(
        vec!["Name".into()],
        vec![vec!["A".into()], vec!["B".into()], vec!["C".into()]],
    )
    .map_err(err)?;
    let result = doc
        .merge()
        .records(records)
        .finish()
        .map_err(err)?
        .join(Join::Auto);
    if code(result)? != "merge.limit" {
        return Err("the block cap returned a partial document".into());
    }
    Ok(())
}

#[test]
fn adapter_byte_parity() -> Test {
    let template = Document::builder()
        .paragraph(Paragraph::new().text("Hello ").merge_field("Name"))
        .build()
        .map_err(err)?
        .write_bytes()
        .map_err(err)?;
    let csv = b"Name\nAda\n";
    let rust = Document::merge_letters_bytes(&template, csv).map_err(err)?;
    let python = edgedoc_py::merge_csv_bytes(&template, csv)?;
    let wasm = edgedoc_wasm::merge_csv_bytes(&template, csv)?;
    if rust != python || rust != wasm {
        return Err("adapter bytes differ".into());
    }
    let doc = Document::read_bytes(&rust).map_err(err)?;
    if doc.plain_text(StoryRef::Body) != "Hello Ada" {
        return Err(doc.plain_text(StoryRef::Body));
    }
    Ok(())
}

#[test]
fn property_builder_round_trip() -> Test {
    for index in 0..8 {
        let title = format!("T{index}");
        let text = format!("Line {index} é");
        let doc = Document::builder()
            .core(edgedoc::CoreProps::new().title(&title))
            .paragraph(Paragraph::new().text(&text))
            .build()
            .map_err(err)?;
        let again = Document::read_bytes(&doc.write_bytes().map_err(err)?).map_err(err)?;
        if again.core_title() != title || again.plain_text(StoryRef::Body) != text {
            return Err(format!(
                "{} {:?}",
                again.core_title(),
                again.plain_text(StoryRef::Body)
            ));
        }
    }
    Ok(())
}

#[test]
fn invoice_sample() -> Test {
    let mut doc = Document::builder()
        .core(edgedoc::CoreProps::new().title("Invoice"))
        .page(Page::a4().margins(Mm(20.0)))
        .style(Style::paragraph("Heading1").size(Pt(16.0)).bold())
        .paragraph(
            Paragraph::new()
                .style("Heading1")
                .text("Invoice ")
                .merge_field("InvoiceNumber"),
        )
        .table(
            Table::new()
                .widths([Mm(90.0), Mm(30.0), Mm(40.0)])
                .header(["Description", "Qty", "Amount"])
                .row(["Widget", "2", "40.00"]),
        )
        .build()
        .map_err(err)?;
    doc.edit(EditOptions::default(), |edit| {
        edit.body().paragraph_after_last(|para| {
            para.text("Total ")?;
            para.merge_field("Total")?;
            Ok(())
        })?;
        Ok(())
    })
    .map_err(err)?;
    let records = Records::from_rows(
        vec!["InvoiceNumber".into(), "Total".into()],
        vec![
            vec!["1001".into(), "40.00".into()],
            vec!["1002".into(), "18.00".into()],
        ],
    )
    .map_err(err)?;
    let letters = doc
        .merge()
        .records(records)
        .kind(MergeKind::Letters)
        .missing(MissingField::Error)
        .range(RecordRange::all())
        .map_value("Amount", |row| row.get("Total").unwrap_or("").to_string())
        .finish()
        .map_err(err)?
        .join(Join::Auto)
        .map_err(err)?;
    let text = letters.plain_text(StoryRef::Body);
    if !text.contains("1001") || !text.contains("1002") || !text.contains("Widget") {
        return Err(text);
    }
    if letters.flavor() != Flavor::Docx {
        return Err("flavor changed".into());
    }
    Ok(())
}

#[test]
fn owned_features_round_trip() -> Test {
    let document = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
<w:body>
<w:p>
<w:pPr><w:pStyle w:val="BodyText"/><w:jc w:val="both"/><w:ind w:left="720" w:right="360" w:firstLine="360"/><w:spacing w:before="120" w:after="80" w:line="360" w:lineRule="auto"/><w:numPr><w:ilvl w:val="1"/><w:numId w:val="5"/></w:numPr><w:keepNext/><w:keepLines/><w:pageBreakBefore/><w:widowControl w:val="0"/><w:outlineLvl w:val="1"/><w:bidi/><w:shd w:val="clear" w:fill="EEECE1"/><w:rPr><w:b/><w:sz w:val="24"/></w:rPr></w:pPr>
<w:bookmarkStart w:id="1" w:name="InvoiceMark"/>
<w:r><w:rPr><w:b/><w:i/><w:u w:val="double"/><w:strike/><w:dstrike/><w:color w:val="C00000"/><w:sz w:val="28"/><w:szCs w:val="28"/><w:rFonts w:ascii="Calibri" w:hAnsi="Carlito" w:cs="Arial" w:eastAsia="MS Mincho"/><w:highlight w:val="yellow"/><w:shd w:val="clear" w:fill="FFFF00"/><w:vertAlign w:val="superscript"/><w:rtl/><w:cs/><w:vanish/><w:webHidden/><w:lang w:val="en-US" w:bidi="ar-SA" w:eastAsia="ja-JP"/></w:rPr><w:t>Owned</w:t></w:r>
<w:hyperlink r:id="rIdLink"><w:r><w:t>Site</w:t></w:r></w:hyperlink>
<w:bookmarkEnd w:id="1"/>
</w:p>
<w:tbl><w:tblPr><w:tblW w:w="0" w:type="auto"/><w:bidiVisual/><w:tblBorders><w:top w:val="single" w:sz="4" w:color="000000"/></w:tblBorders><w:tblCellMar><w:left w:w="100" w:type="dxa"/></w:tblCellMar><w:tblLook w:val="04A0"/></w:tblPr><w:tblGrid><w:gridCol w:w="2000"/></w:tblGrid>
<w:tr><w:trPr><w:tblHeader/><w:trHeight w:val="300" w:hRule="exact"/></w:trPr><w:tc><w:tcPr><w:gridSpan w:val="2"/><w:tcW w:w="2000" w:type="dxa"/><w:vMerge w:val="restart"/><w:vAlign w:val="center"/></w:tcPr><w:p><w:r><w:t>Cell</w:t></w:r></w:p></w:tc></w:tr>
</w:tbl>
<w:sdt><w:sdtPr><w:alias w:val="Due"/><w:tag w:val="DueDate"/><w:date/><w:placeholder><w:docPart w:val="DatePlace"/></w:placeholder><w:dataBinding w:xpath="/root/due" w:storeItemID="{11111111-1111-1111-1111-111111111111}"/></w:sdtPr><w:sdtContent><w:p><w:r><w:t>2026-11-04</w:t></w:r></w:p></w:sdtContent></w:sdt>
<w:p><w:r><w:drawing><a:blip r:embed="rIdPic"/></w:drawing></w:r></w:p>
<w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440"/><w:cols w:num="2"/><w:pgNumType w:start="3" w:fmt="lowerRoman"/><w:headerReference w:type="default" r:id="rIdH"/><w:footerReference w:type="default" r:id="rIdF"/></w:sectPr>
</w:body></w:document>"#;
    let rels = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdH" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/header" Target="header1.xml"/><Relationship Id="rIdF" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/footer" Target="footer1.xml"/><Relationship Id="rIdLink" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink" Target="https://example.test" TargetMode="External"/><Relationship Id="rIdPic" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/pic.png"/><Relationship Id="rIdNum" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering" Target="numbering.xml"/><Relationship Id="rIdFn" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/footnotes" Target="footnotes.xml"/></Relationships>"#;
    let types = br#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Default Extension="png" ContentType="image/png"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/header1.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml"/><Override PartName="/word/footer1.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.footer+xml"/><Override PartName="/word/numbering.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml"/><Override PartName="/word/footnotes.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.footnotes+xml"/><Override PartName="/word/settings.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml"/></Types>"#;
    let bytes = write_zip(&[
        ("[Content_Types].xml".into(), types.to_vec()),
        ("_rels/.rels".into(), br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#.to_vec()),
        ("word/document.xml".into(), document.as_bytes().to_vec()),
        ("word/_rels/document.xml.rels".into(), rels.to_vec()),
        ("word/header1.xml".into(), br#"<w:hdr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:p><w:r><w:t>City</w:t></w:r></w:p></w:hdr>"#.to_vec()),
        ("word/footer1.xml".into(), br#"<w:ftr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:p><w:r><w:t>Page</w:t></w:r></w:p></w:ftr>"#.to_vec()),
        ("word/numbering.xml".into(), br#"<w:numbering xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:num w:numId="5"/></w:numbering>"#.to_vec()),
        ("word/footnotes.xml".into(), br#"<w:footnotes xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">Footnote body</w:footnotes>"#.to_vec()),
        ("word/settings.xml".into(), br#"<w:settings xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">unique-setting-marker</w:settings>"#.to_vec()),
        ("word/media/pic.png".into(), b"PNGDATA".to_vec()),
    ]).map_err(err)?;
    let mut doc = Document::read_bytes(&bytes).map_err(err)?;
    if !doc.plain_text(StoryRef::Body).contains("Owned")
        || !doc.plain_text(StoryRef::Body).contains("Site")
    {
        return Err(doc.plain_text(StoryRef::Body));
    }
    doc.edit(EditOptions::default(), |edit| {
        edit.body().paragraph_after_last(|para| para.text("Tail"))?;
        Ok(())
    })
    .map_err(err)?;
    let out = doc.write_bytes().map_err(err)?;
    let xml = part_text(&out, "word/document.xml")?;
    for needle in [
        "w:val=\"double\"",
        "w:strike",
        "w:dstrike",
        "C00000",
        "w:sz w:val=\"28\"",
        "w:szCs",
        "Calibri",
        "Carlito",
        "Arial",
        "MS Mincho",
        "yellow",
        "FFFF00",
        "superscript",
        "w:rtl",
        "w:vanish",
        "w:webHidden",
        "en-US",
        "ar-SA",
        "ja-JP",
        "w:val=\"both\"",
        "w:left=\"720\"",
        "w:right=\"360\"",
        "w:firstLine=\"360\"",
        "w:before=\"120\"",
        "w:after=\"80\"",
        "w:line=\"360\"",
        "w:numId w:val=\"5\"",
        "w:ilvl w:val=\"1\"",
        "w:keepNext",
        "w:keepLines",
        "w:pageBreakBefore",
        "w:widowControl w:val=\"0\"",
        "w:outlineLvl w:val=\"1\"",
        "EEECE1",
        "InvoiceMark",
        "w:date",
        "w:val=\"Due\"",
        "DueDate",
        "DatePlace",
        "/root/due",
        "w:bidiVisual",
        "w:val=\"single\"",
        "w:tblLook",
        "w:trHeight",
        "w:tblHeader",
        "w:gridSpan w:val=\"2\"",
        "w:vMerge w:val=\"restart\"",
        "w:vAlign w:val=\"center\"",
        "w:cols w:num=\"2\"",
        "lowerRoman",
        "w:start=\"3\"",
        "Tail",
    ] {
        if !xml.contains(needle) {
            return Err(format!("missing {needle}"));
        }
    }
    let package = edgedoc_opc::read_zip(&out, &Budget::default()).map_err(err)?;
    let joined = package
        .parts
        .values()
        .map(|part| String::from_utf8_lossy(&part.bytes).into_owned())
        .collect::<Vec<_>>()
        .join("\n");
    for needle in [
        "City",
        "Page",
        "numId=\"5\"",
        "unique-setting-marker",
        "Footnote body",
        "https://example.test",
    ] {
        if !joined.contains(needle) {
            return Err(format!("missing part text {needle}"));
        }
    }
    let picture = package
        .parts
        .values()
        .find(|part| part.bytes == b"PNGDATA")
        .ok_or("the picture bytes were dropped")?;
    if picture.name.is_empty() {
        return Err("the picture has no name".into());
    }
    Ok(())
}

#[test]
fn builder_character_properties() -> Test {
    let doc = Document::builder()
        .paragraph(
            Paragraph::new()
                .run_builder(
                    RunBuilder::new()
                        .text("Styled")
                        .bold()
                        .italic()
                        .highlight("yellow")
                        .shading("FFFF00")
                        .font("Calibri")
                        .font_ea("MS Mincho")
                        .vanish(),
                )
                .map_err(err)?,
        )
        .build()
        .map_err(err)?;
    let xml = part_text(&doc.write_bytes().map_err(err)?, "word/document.xml")?;
    for needle in [
        "Styled",
        "<w:b/>",
        "<w:i/>",
        "yellow",
        "FFFF00",
        "Calibri",
        "MS Mincho",
        "<w:vanish/>",
    ] {
        if !xml.contains(needle) {
            return Err(format!("missing {needle}"));
        }
    }
    let again = Document::read_bytes(&doc.write_bytes().map_err(err)?).map_err(err)?;
    if again.plain_text(StoryRef::Body) != "Styled" {
        return Err(again.plain_text(StoryRef::Body));
    }
    Ok(())
}

#[test]
fn layer_dependencies_match_the_plan() -> Test {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let merge =
        std::fs::read_to_string(root.join("crates/edgedoc-merge/Cargo.toml")).map_err(err)?;
    if merge.contains("edgedoc-opc") || merge.contains("edgedoc-wml") {
        return Err("merge depends on the package layer".into());
    }
    let opc = std::fs::read_to_string(root.join("crates/edgedoc-opc/Cargo.toml")).map_err(err)?;
    if opc.contains("edgedoc-model") || opc.contains("edgedoc-wml") {
        return Err("opc depends on the document model".into());
    }
    let model =
        std::fs::read_to_string(root.join("crates/edgedoc-model/Cargo.toml")).map_err(err)?;
    if model.contains("edgedoc-opc") || model.contains("edgedoc-wml") {
        return Err("the model depends on the package".into());
    }
    // An adapter calls the facade only. A second read or write path would drift.
    for adapter in ["edgedoc-py", "edgedoc-wasm"] {
        let manifest = std::fs::read_to_string(root.join(format!("crates/{adapter}/Cargo.toml")))
            .map_err(err)?;
        if !manifest.contains("edgedoc =") {
            return Err(format!("{adapter} does not depend on the facade"));
        }
        for lower in [
            "edgedoc-opc",
            "edgedoc-wml",
            "edgedoc-model",
            "edgedoc-merge",
            "edgedoc-limit",
            "edgedoc-unit",
        ] {
            if manifest.contains(&format!("{lower} =")) {
                return Err(format!("{adapter} depends on the lower crate {lower}"));
            }
        }
        // The source must not name a lower crate either.
        let source = std::fs::read_to_string(root.join(format!("crates/{adapter}/src/lib.rs")))
            .map_err(err)?;
        for lower in [
            "edgedoc_opc",
            "edgedoc_wml",
            "edgedoc_model",
            "edgedoc_merge",
        ] {
            if source.contains(lower) {
                return Err(format!("{adapter} names {lower} in its source"));
            }
        }
    }
    Ok(())
}
