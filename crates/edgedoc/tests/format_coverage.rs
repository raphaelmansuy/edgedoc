//! Executable rows of `docs/11-format-coverage.md`.
//! Each case id in that table must have a case here, and each case id must appear in the table.

use edgedoc::{
    Budget, CsvOptions, Document, EditOptions, Flavor, Join, MergeKind, MissingField, Paragraph,
    ReadOptions, Records, StoryRef,
};
use edgedoc_opc::write_zip;

type Test = Result<(), String>;

fn err(err: impl std::fmt::Display) -> String {
    err.to_string()
}

fn fail(message: impl Into<String>) -> Test {
    Err(message.into())
}

fn office(
    main_type: &str,
    document: &str,
    rels: &str,
    extra: &[(&str, &[u8])],
) -> Result<Vec<u8>, String> {
    let mut types = format!(
        r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Default Extension="png" ContentType="image/png"/><Default Extension="jpeg" ContentType="image/jpeg"/><Default Extension="emf" ContentType="image/x-emf"/><Default Extension="bin" ContentType="application/octet-stream"/><Override PartName="/word/document.xml" ContentType="{main_type}"/>"#
    );
    for (name, _) in extra {
        if name.ends_with(".xml") {
            types.push_str(&format!(
                r#"<Override PartName="/{name}" ContentType="application/xml"/>"#
            ));
        }
    }
    types.push_str("</Types>");
    let mut parts = vec![
        ("[Content_Types].xml".into(), types.into_bytes()),
        (
            "_rels/.rels".into(),
            br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#.to_vec(),
        ),
        ("word/document.xml".into(), document.as_bytes().to_vec()),
        ("word/_rels/document.xml.rels".into(), rels.as_bytes().to_vec()),
    ];
    for (name, bytes) in extra {
        parts.push(((*name).into(), bytes.to_vec()));
    }
    write_zip(&parts).map_err(err)
}

fn body(inner: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><w:body>{inner}</w:body></w:document>"#
    )
}

fn rels(inner: &str) -> String {
    format!(
        r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">{inner}</Relationships>"#
    )
}

fn touch(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut doc = Document::read_bytes(bytes).map_err(err)?;
    doc.edit(EditOptions::default(), |edit| {
        edit.body().paragraph_after_last(|para| para.text("Tail"))?;
        Ok(())
    })
    .map_err(err)?;
    doc.write_bytes().map_err(err)
}

fn part(bytes: &[u8], name: &str) -> Result<Vec<u8>, String> {
    let package = edgedoc_opc::read_zip(bytes, &Budget::default()).map_err(err)?;
    package
        .part(name)
        .map(|part| part.bytes.clone())
        .ok_or_else(|| format!("missing {name}"))
}

fn part_string(bytes: &[u8], name: &str) -> Result<String, String> {
    Ok(String::from_utf8_lossy(&part(bytes, name)?).into_owned())
}

fn all_text(bytes: &[u8]) -> Result<String, String> {
    let package = edgedoc_opc::read_zip(bytes, &Budget::default()).map_err(err)?;
    Ok(package
        .parts
        .values()
        .map(|part| String::from_utf8_lossy(&part.bytes).into_owned())
        .collect::<Vec<_>>()
        .join("\n"))
}

fn contains(bytes: &[u8], needle: &str) -> Result<(), String> {
    let text = all_text(bytes)?;
    if text.contains(needle) {
        Ok(())
    } else {
        Err(format!("missing {needle}"))
    }
}

fn rel_target_name(rels: &str, id: &str) -> Result<String, String> {
    let marker = format!("Id=\"{id}\"");
    let Some(pos) = rels.find(&marker) else {
        return Err(format!("missing relationship {id}"));
    };
    let slice = &rels[pos..];
    let Some(target_at) = slice.find("Target=\"") else {
        return Err(format!("missing target for {id}"));
    };
    let rest = &slice[target_at + 8..];
    let Some(end) = rest.find('"') else {
        return Err(format!("bad target for {id}"));
    };
    let target = &rest[..end];
    if target.starts_with('/') {
        Ok(target.trim_start_matches('/').to_string())
    } else if target.starts_with("word/") {
        Ok(target.to_string())
    } else {
        Ok(format!("word/{target}"))
    }
}

fn first_embed(document: &str) -> Result<&str, String> {
    let marker = "r:embed=\"";
    let Some(pos) = document.find(marker) else {
        return Err("the drawing has no embed".into());
    };
    let rest = &document[pos + marker.len()..];
    let Some(end) = rest.find('"') else {
        return Err("bad embed".into());
    };
    Ok(&rest[..end])
}

fn code_of(result: Result<Document, edgedoc::Error>) -> Result<&'static str, String> {
    match result {
        Ok(_) => Err("the call returned a document".into()),
        Err(error) => Ok(error.code),
    }
}

fn pkg_docx() -> Test {
    let bytes = office(
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml",
        &body("<w:p><w:r><w:t>Docx</w:t></w:r></w:p>"),
        &rels(""),
        &[],
    )?;
    let out = touch(&bytes)?;
    let types = part_string(&out, "[Content_Types].xml")?;
    if !types.contains("wordprocessingml.document.main+xml") {
        return fail(types);
    }
    contains(&out, "Docx")
}

fn pkg_docm() -> Test {
    let bytes = office(
        "application/vnd.ms-word.document.macroEnabled.main+xml",
        &body("<w:p><w:r><w:t>Macro</w:t></w:r></w:p>"),
        &rels(""),
        &[("word/vbaProject.bin", b"vba-bytes")],
    )?;
    let doc = Document::read_bytes(&bytes).map_err(err)?;
    if doc.flavor() != Flavor::Docm {
        return fail("the flavor is not docm");
    }
    let out = touch(&bytes)?;
    if Document::read_bytes(&out).map_err(err)?.flavor() != Flavor::Docm {
        return fail("the saved flavor is not docm");
    }
    if part(&out, "word/vbaProject.bin")? != b"vba-bytes" {
        return fail("the macro bytes changed");
    }
    Ok(())
}

fn pkg_dotx() -> Test {
    let bytes = office(
        "application/vnd.openxmlformats-officedocument.wordprocessingml.template.main+xml",
        &body("<w:p><w:r><w:t>Template</w:t></w:r></w:p>"),
        &rels(""),
        &[],
    )?;
    if Document::read_bytes(&bytes).map_err(err)?.flavor() != Flavor::Dotx {
        return fail("the flavor is not dotx");
    }
    let out = touch(&bytes)?;
    let types = part_string(&out, "[Content_Types].xml")?;
    if !types.contains("template.main+xml") {
        return fail(types);
    }
    Ok(())
}

fn pkg_dotm() -> Test {
    let bytes = office(
        "application/vnd.ms-word.template.macroEnabledTemplate.main+xml",
        &body("<w:p><w:r><w:t>Template</w:t></w:r></w:p>"),
        &rels(""),
        &[("word/vbaProject.bin", b"dotm-vba")],
    )?;
    if Document::read_bytes(&bytes).map_err(err)?.flavor() != Flavor::Dotm {
        return fail("the flavor is not dotm");
    }
    let out = touch(&bytes)?;
    if part(&out, "word/vbaProject.bin")? != b"dotm-vba" {
        return fail("the template macro bytes changed");
    }
    Ok(())
}

fn pkg_strict() -> Test {
    let xml = r#"<?xml version="1.0"?><w:document xmlns:w="http://purl.oclc.org/ooxml/wordprocessingml/main"><w:body><w:p><w:r><w:t>Strict</w:t></w:r></w:p></w:body></w:document>"#;
    let bytes = office(
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml",
        xml,
        &rels(""),
        &[],
    )?;
    let out = touch(&bytes)?;
    let text = part_string(&out, "word/document.xml")?;
    if !text.contains("http://schemas.openxmlformats.org/wordprocessingml/2006/main")
        || !text.contains("Strict")
    {
        return fail(text);
    }
    Ok(())
}

fn pkg_flat() -> Test {
    let xml = r#"<?xml version="1.0"?><pkg:package xmlns:pkg="http://schemas.microsoft.com/office/2006/xmlPackage"><pkg:part pkg:name="/[Content_Types].xml" pkg:contentType="application/xml"><pkg:xmlData><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"></Types></pkg:xmlData></pkg:part><pkg:part pkg:name="/_rels/.rels" pkg:contentType="application/vnd.openxmlformats-package.relationships+xml"><pkg:xmlData><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships></pkg:xmlData></pkg:part><pkg:part pkg:name="/word/document.xml" pkg:contentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"><pkg:xmlData><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>Flat</w:t></w:r></w:p></w:body></w:document></pkg:xmlData></pkg:part></pkg:package>"#;
    let doc = Document::read_bytes(xml.as_bytes()).map_err(err)?;
    if doc.plain_text(StoryRef::Body) != "Flat" {
        return fail(doc.plain_text(StoryRef::Body));
    }
    Ok(())
}

fn pkg_wordml2003() -> Test {
    let xml = r#"<?xml version="1.0"?><w:wordDocument xmlns:w="http://schemas.microsoft.com/office/word/2003/wordml"><w:body><w:p><w:r><w:t>Old</w:t></w:r></w:p></w:body></w:wordDocument>"#;
    if code_of(Document::read_bytes(xml.as_bytes()))? != "unsupported" {
        return fail("Word 2003 XML was accepted");
    }
    Ok(())
}

fn pkg_doc_binary() -> Test {
    let mut bytes = vec![0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
    bytes.extend(b"WordDocument");
    match Document::read_bytes(&bytes) {
        Ok(_) => fail("a binary .doc was accepted"),
        Err(error) if error.code == "unsupported" && error.message.contains("Binary") => Ok(()),
        Err(error) => fail(format!("{} {}", error.code, error.message)),
    }
}

fn encrypted_bytes() -> Vec<u8> {
    let mut bytes = vec![0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
    for unit in "EncryptionInfo".encode_utf16() {
        bytes.extend(unit.to_le_bytes());
    }
    bytes
}

fn pkg_encrypted() -> Test {
    if code_of(Document::read_bytes(&encrypted_bytes()))? != "package.password_required" {
        return fail("an encrypted package opened");
    }
    Ok(())
}

fn pkg_encrypted_password() -> Test {
    let options = ReadOptions {
        password: Some("secret"),
        ..ReadOptions::new()
    };
    let got = code_of(Document::read_bytes_with(&encrypted_bytes(), &options))?;
    if got != "package.encrypted_unsupported" && got != "package.wrong_password" {
        return fail(got);
    }
    Ok(())
}

fn pkg_clean() -> Test {
    let bytes = office(
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml",
        &body("<w:p><w:r><w:t>Same</w:t></w:r></w:p>"),
        &rels(""),
        &[],
    )?;
    let doc = Document::read_bytes(&bytes).map_err(err)?;
    if doc.write_bytes().map_err(err)? != bytes {
        return fail("an unedited save changed the package");
    }
    Ok(())
}

fn pkg_unknown() -> Test {
    let bytes = office(
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml",
        &body("<w:p><w:r><w:t>Body</w:t></w:r></w:p>"),
        &rels(""),
        &[("customXml/item1.xml", b"<kept/>")],
    )?;
    let out = touch(&bytes)?;
    if part(&out, "customXml/item1.xml")? != b"<kept/>" {
        return fail("the custom XML part changed");
    }
    Ok(())
}

fn text_tab() -> Test {
    let bytes = office(
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml",
        &body("<w:p><w:r><w:t>A</w:t><w:tab/><w:t>B</w:t></w:r></w:p>"),
        &rels(""),
        &[],
    )?;
    let out = touch(&bytes)?;
    let doc = Document::read_bytes(&out).map_err(err)?;
    if doc.plain_text(StoryRef::Body) != "A\tB\nTail"
        && doc.plain_text(StoryRef::Body) != "A\tBTail"
    {
        let text = doc.plain_text(StoryRef::Body);
        if !text.contains('A') || !text.contains('\t') || !text.contains('B') {
            return fail(text);
        }
    }
    Ok(())
}

fn text_break() -> Test {
    let bytes = office(
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml",
        &body(r#"<w:p><w:r><w:t>A</w:t><w:br/><w:t>B</w:t></w:r></w:p>"#),
        &rels(""),
        &[],
    )?;
    let doc = Document::read_bytes(&touch(&bytes)?).map_err(err)?;
    let text = doc.plain_text(StoryRef::Body);
    if !text.contains('A') || !text.contains('\n') || !text.contains('B') {
        return fail(text);
    }
    Ok(())
}

fn text_footnote_ref() -> Test {
    let bytes = office(
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml",
        &body(r#"<w:p><w:r><w:t>Note</w:t><w:footnoteReference w:id="1"/></w:r></w:p>"#),
        &rels(r#"<Relationship Id="rIdFn" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/footnotes" Target="footnotes.xml"/>"#),
        &[("word/footnotes.xml", br#"<w:footnotes xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">Foot note</w:footnotes>"#)],
    )?;
    let out = touch(&bytes)?;
    contains(&out, "footnoteReference")?;
    contains(&out, "Foot note")
}

fn text_endnote_ref() -> Test {
    let bytes = office(
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml",
        &body(r#"<w:p><w:r><w:t>Note</w:t><w:endnoteReference w:id="1"/></w:r></w:p>"#),
        &rels(r#"<Relationship Id="rIdEn" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/endnotes" Target="endnotes.xml"/>"#),
        &[("word/endnotes.xml", br#"<w:endnotes xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">End note</w:endnotes>"#)],
    )?;
    let out = touch(&bytes)?;
    contains(&out, "endnoteReference")?;
    contains(&out, "End note")
}

fn text_comment() -> Test {
    let bytes = office(
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml",
        &body(r#"<w:p><w:r><w:t>Said</w:t><w:commentReference w:id="1"/></w:r></w:p><w:p><w:r><w:t>Next</w:t></w:r></w:p>"#),
        &rels(r#"<Relationship Id="rIdC" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments" Target="comments.xml"/>"#),
        &[("word/comments.xml", br#"<w:comments xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">A comment</w:comments>"#)],
    )?;
    let out = touch(&bytes)?;
    contains(&out, "commentReference")?;
    contains(&out, "A comment")
}

fn text_revision() -> Test {
    let bytes = office(
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml",
        &body(r#"<w:p><w:ins w:id="1"><w:r><w:t>Old</w:t></w:r></w:ins></w:p>"#),
        &rels(""),
        &[],
    )?;
    contains(&touch(&bytes)?, "w:ins")?;
    let mut doc = Document::read_bytes(&bytes).map_err(err)?;
    let denied = doc.edit(EditOptions::default(), |edit| {
        edit.body().paragraph_at(0, |para| para.text("x"))?;
        Ok(())
    });
    match denied {
        Err(error) if error.code == "model.needs_drop_consent" => Ok(()),
        Err(error) => fail(error.code),
        Ok(_) => fail("a revision edit did not ask for consent"),
    }
}

fn text_equation() -> Test {
    let bytes = office(
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml",
        &body(
            r#"<w:p><m:oMath xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math"><m:r><m:t>x</m:t></m:r></m:oMath></w:p>"#,
        ),
        &rels(""),
        &[],
    )?;
    contains(&touch(&bytes)?, "oMath")
}

fn text_bookmark() -> Test {
    let bytes = office(
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml",
        &body(
            r#"<w:p><w:bookmarkStart w:id="1" w:name="Here"/><w:r><w:t>Mark</w:t></w:r><w:bookmarkEnd w:id="1"/></w:p>"#,
        ),
        &rels(""),
        &[],
    )?;
    contains(&touch(&bytes)?, "Here")
}

fn text_hyperlink() -> Test {
    let bytes = office(
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml",
        &body(r#"<w:p><w:hyperlink r:id="rIdL"><w:r><w:t>Site</w:t></w:r></w:hyperlink></w:p>"#),
        &rels(
            r#"<Relationship Id="rIdL" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink" Target="https://example.test/format" TargetMode="External"/>"#,
        ),
        &[],
    )?;
    contains(&touch(&bytes)?, "https://example.test/format")
}

fn text_page_field() -> Test {
    let bytes = office(
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml",
        &body(
            r#"<w:p><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText> PAGE </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>4</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r></w:p>"#,
        ),
        &rels(""),
        &[],
    )?;
    let doc = Document::read_bytes(&touch(&bytes)?).map_err(err)?;
    if !doc.plain_text(StoryRef::Body).contains('4') {
        return fail(doc.plain_text(StoryRef::Body));
    }
    Ok(())
}

fn text_simple_field() -> Test {
    let bytes = office(
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml",
        &body(
            r#"<w:p><w:fldSimple w:instr=" MERGEFIELD Name "><w:r><w:t>Ada</w:t></w:r></w:fldSimple></w:p>"#,
        ),
        &rels(""),
        &[],
    )?;
    let out = touch(&bytes)?;
    contains(&out, "MERGEFIELD Name")?;
    let doc = Document::read_bytes(&out).map_err(err)?;
    if !doc.plain_text(StoryRef::Body).contains("Ada") {
        return fail(doc.plain_text(StoryRef::Body));
    }
    Ok(())
}

fn text_rtl() -> Test {
    let bytes = office(
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml",
        &body(
            r#"<w:p><w:r><w:rPr><w:rtl/><w:lang w:val="en-US" w:bidi="he-IL"/></w:rPr><w:t>שלום</w:t></w:r></w:p>"#,
        ),
        &rels(""),
        &[],
    )?;
    let out = touch(&bytes)?;
    let doc = Document::read_bytes(&out).map_err(err)?;
    if !doc.run_is_rtl(0) || doc.run_lang(0).as_deref() != Some("en-US") {
        return fail(format!(
            "rtl {} lang {:?}",
            doc.run_is_rtl(0),
            doc.run_lang(0)
        ));
    }
    contains(&out, "he-IL")
}

fn sect_headers() -> Test {
    let bytes = office(
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml",
        &body(r#"<w:p><w:r><w:t>Body</w:t></w:r></w:p><w:sectPr><w:headerReference w:type="default" r:id="rIdD"/><w:headerReference w:type="first" r:id="rIdF"/><w:headerReference w:type="even" r:id="rIdE"/><w:footerReference w:type="first" r:id="rIdZ"/></w:sectPr>"#),
        &rels(
            r#"<Relationship Id="rIdD" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/header" Target="headerD.xml"/><Relationship Id="rIdF" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/header" Target="headerF.xml"/><Relationship Id="rIdE" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/header" Target="headerE.xml"/><Relationship Id="rIdZ" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/footer" Target="footerZ.xml"/>"#,
        ),
        &[
            ("word/headerD.xml", br#"<w:hdr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:p><w:r><w:t>DefaultHead</w:t></w:r></w:p></w:hdr>"#),
            ("word/headerF.xml", br#"<w:hdr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:p><w:r><w:t>FirstHead</w:t></w:r></w:p></w:hdr>"#),
            ("word/headerE.xml", br#"<w:hdr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:p><w:r><w:t>EvenHead</w:t></w:r></w:p></w:hdr>"#),
            ("word/footerZ.xml", br#"<w:ftr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:p><w:r><w:t>FirstFoot</w:t></w:r></w:p></w:ftr>"#),
        ],
    )?;
    let out = touch(&bytes)?;
    let document = String::from_utf8_lossy(&part(&out, "word/document.xml")?).into_owned();
    if !document.contains("w:type=\"first\"") || !document.contains("w:type=\"even\"") {
        return fail(document);
    }
    let rels_xml =
        String::from_utf8_lossy(&part(&out, "word/_rels/document.xml.rels")?).into_owned();
    let mut stories = String::new();
    let mut rest = document.as_str();
    let marker = "r:id=\"";
    while let Some(pos) = rest.find(marker) {
        let after = &rest[pos + marker.len()..];
        let Some(end) = after.find('"') else { break };
        let name = rel_target_name(&rels_xml, &after[..end])?;
        stories.push_str(&String::from_utf8_lossy(&part(&out, &name)?));
        rest = &after[end..];
    }
    for needle in ["DefaultHead", "FirstHead", "EvenHead", "FirstFoot"] {
        if !stories.contains(needle) {
            return fail(format!("missing {needle}"));
        }
    }
    Ok(())
}

fn tbl_clamp() -> Test {
    let bytes = office(
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml",
        &body(
            r#"<w:tbl><w:tr><w:tc><w:tcPr><w:gridSpan w:val="64"/></w:tcPr><w:p><w:r><w:t>A</w:t></w:r></w:p></w:tc></w:tr></w:tbl>"#,
        ),
        &rels(""),
        &[],
    )?;
    let doc = Document::read_bytes(&bytes).map_err(err)?;
    if doc.cell_span(0, 0, 0) != Some(63) {
        return fail(format!("{:?}", doc.cell_span(0, 0, 0)));
    }
    contains(&touch(&bytes)?, r#"w:gridSpan w:val="63""#)
}

fn picture(zip_name: &str, rel_target: &str, payload: &[u8]) -> Test {
    let bytes = office(
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml",
        &body(
            r#"<w:p><w:r><w:drawing><a:blip xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" r:embed="rIdP"/></w:drawing></w:r></w:p>"#,
        ),
        &rels(&format!(
            r#"<Relationship Id="rIdP" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="{rel_target}"/>"#
        )),
        &[(zip_name, payload)],
    )?;
    let out = touch(&bytes)?;
    let document = String::from_utf8_lossy(&part(&out, "word/document.xml")?).into_owned();
    let id = first_embed(&document)?;
    let rels_xml =
        String::from_utf8_lossy(&part(&out, "word/_rels/document.xml.rels")?).into_owned();
    let name = rel_target_name(&rels_xml, id)?;
    if part(&out, &name)? != payload {
        return fail("the picture bytes were dropped");
    }
    Ok(())
}

fn pic_png() -> Test {
    picture("word/media/pic.png", "media/pic.png", b"PNG-FORMAT")
}

fn pic_jpeg() -> Test {
    picture("word/media/pic.jpeg", "media/pic.jpeg", b"JPEG-FORMAT")
}

fn pic_emf() -> Test {
    let bytes = office(
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml",
        &body(
            r#"<w:p><w:r><w:drawing><a:blip xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" r:embed="rIdE"/></w:drawing></w:r></w:p>"#,
        ),
        &rels(
            r#"<Relationship Id="rIdE" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/pic.emf"/>"#,
        ),
        &[("word/media/pic.emf", b"EMF-FORMAT")],
    )?;
    let doc = Document::read_bytes(&bytes).map_err(err)?;
    if !doc.has_opaque_drawing() {
        return fail("the metafile was treated as an owned picture");
    }
    let out = touch(&bytes)?;
    let document = String::from_utf8_lossy(&part(&out, "word/document.xml")?).into_owned();
    if !document.contains("r:embed=\"rIdE\"") {
        return fail(document);
    }
    let rels_xml =
        String::from_utf8_lossy(&part(&out, "word/_rels/document.xml.rels")?).into_owned();
    let name = rel_target_name(&rels_xml, "rIdE")?;
    if part(&out, &name)? != b"EMF-FORMAT" {
        return fail("the metafile bytes were dropped");
    }
    Ok(())
}

fn sdt_kinds() -> Test {
    let bytes = office(
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml",
        &body(
            r#"<w:sdt><w:sdtPr><w:alias w:val="When"/><w:tag w:val="WhenTag"/><w:date/></w:sdtPr><w:sdtContent><w:p><w:r><w:t>Day</w:t></w:r></w:p></w:sdtContent></w:sdt><w:sdt><w:sdtPr><w:dropDownList/></w:sdtPr><w:sdtContent><w:p><w:r><w:t>Choice</w:t></w:r></w:p></w:sdtContent></w:sdt><w:sdt><w:sdtPr><w:checkbox/></w:sdtPr><w:sdtContent><w:p><w:r><w:t>Box</w:t></w:r></w:p></w:sdtContent></w:sdt>"#,
        ),
        &rels(""),
        &[],
    )?;
    let out = touch(&bytes)?;
    for needle in ["w:date", "w:dropDownList", "w:checkbox", "WhenTag"] {
        contains(&out, needle)?;
    }
    Ok(())
}

fn part_kept(rel_type: &str, zip_name: &str, rel_target: &str, payload: &[u8]) -> Test {
    let bytes = office(
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml",
        &body("<w:p><w:r><w:t>Body</w:t></w:r></w:p>"),
        &rels(&format!(
            r#"<Relationship Id="rIdK" Type="{rel_type}" Target="{rel_target}"/>"#
        )),
        &[(zip_name, payload)],
    )?;
    let out = touch(&bytes)?;
    let rels_xml =
        String::from_utf8_lossy(&part(&out, "word/_rels/document.xml.rels")?).into_owned();
    // The writer may reuse the id or issue a new one, so find the target by rel type.
    if !rels_xml.contains(rel_target) {
        return fail(format!("{zip_name} has no relationship: {rels_xml}"));
    }
    if part(&out, zip_name)? != payload {
        return fail(format!("dropped {zip_name}"));
    }
    Ok(())
}

fn part_numbering() -> Test {
    part_kept(
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering",
        "word/numbering.xml",
        "numbering.xml",
        br#"<w:numbering xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:num w:numId="7"/></w:numbering>"#,
    )
}

fn part_theme() -> Test {
    part_kept(
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme",
        "word/theme/theme1.xml",
        "theme/theme1.xml",
        b"<a:theme>theme-bytes</a:theme>",
    )
}

fn part_chart() -> Test {
    part_kept(
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/chart",
        "word/charts/chart1.xml",
        "charts/chart1.xml",
        b"<c:chart>chart-bytes</c:chart>",
    )
}

fn part_fonts() -> Test {
    part_kept(
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/fontTable",
        "word/fontTable.xml",
        "fontTable.xml",
        b"<w:fonts>font-bytes</w:fonts>",
    )
}

fn part_settings() -> Test {
    let bytes = office(
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml",
        &body("<w:p><w:r><w:t>Body</w:t></w:r></w:p>"),
        &rels(
            r#"<Relationship Id="rIdS" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings" Target="settings.xml"/>"#,
        ),
        &[(
            "word/settings.xml",
            b"<w:settings>settings-marker</w:settings>",
        )],
    )?;
    contains(&touch(&bytes)?, "settings-marker")
}

fn merge_letter() -> Test {
    let doc = Document::builder()
        .paragraph(Paragraph::new().text("Hello ").merge_field("Name"))
        .build()
        .map_err(err)?;
    let records = Records::from_csv(b"Name\nAda\n", &CsvOptions::default()).map_err(err)?;
    let letter = doc
        .merge()
        .records(records)
        .kind(MergeKind::Letters)
        .missing(MissingField::Error)
        .finish()
        .map_err(err)?
        .join(Join::Auto)
        .map_err(err)?;
    if letter.plain_text(StoryRef::Body) != "Hello Ada" {
        return fail(letter.plain_text(StoryRef::Body));
    }
    Ok(())
}

type Case = (&'static str, fn() -> Test);

fn coverage_cases() -> Vec<Case> {
    vec![
        ("pkg-docx", pkg_docx),
        ("pkg-docm", pkg_docm),
        ("pkg-dotx", pkg_dotx),
        ("pkg-dotm", pkg_dotm),
        ("pkg-strict", pkg_strict),
        ("pkg-flat", pkg_flat),
        ("pkg-wordml2003", pkg_wordml2003),
        ("pkg-doc-binary", pkg_doc_binary),
        ("pkg-encrypted", pkg_encrypted),
        ("pkg-encrypted-password", pkg_encrypted_password),
        ("pkg-clean", pkg_clean),
        ("pkg-unknown", pkg_unknown),
        ("text-tab", text_tab),
        ("text-break", text_break),
        ("text-footnote-ref", text_footnote_ref),
        ("text-endnote-ref", text_endnote_ref),
        ("text-comment", text_comment),
        ("text-revision", text_revision),
        ("text-equation", text_equation),
        ("text-bookmark", text_bookmark),
        ("text-hyperlink", text_hyperlink),
        ("text-page-field", text_page_field),
        ("text-simple-field", text_simple_field),
        ("text-rtl", text_rtl),
        ("sect-headers", sect_headers),
        ("tbl-clamp", tbl_clamp),
        ("pic-png", pic_png),
        ("pic-jpeg", pic_jpeg),
        ("pic-emf", pic_emf),
        ("sdt-kinds", sdt_kinds),
        ("part-numbering", part_numbering),
        ("part-theme", part_theme),
        ("part-chart", part_chart),
        ("part-fonts", part_fonts),
        ("part-settings", part_settings),
        ("merge-letter", merge_letter),
    ]
}

#[test]
fn format_matrix() -> Test {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/11-format-coverage.md");
    let doc = std::fs::read_to_string(path).map_err(err)?;
    let cases = coverage_cases();
    for (id, _) in &cases {
        if !doc.contains(id) {
            return fail(format!("docs/11-format-coverage.md has no row for {id}"));
        }
    }
    for line in doc.lines() {
        let mut columns = line.split('|');
        let Some(test_cell) = columns.nth(5) else {
            continue;
        };
        let id = test_cell.trim().trim_matches('`');
        if id.is_empty()
            || id == "Test"
            || id == "none"
            || id.contains(' ')
            || id.chars().all(|ch| ch == '-')
        {
            continue;
        }
        if !cases.iter().any(|(name, _)| *name == id) {
            return fail(format!("the table lists {id}, and the matrix has no case"));
        }
    }
    for (id, check) in cases {
        check().map_err(|message| format!("{id}: {message}"))?;
    }
    Ok(())
}
