//! A file that EdgeDoc writes must read the same through another Word reader.
//!
//! The cases here build a document with the features that this session added, write
//! it, and check the bytes that EdgeDoc reads back. The WordCraft check runs with
//! `make verify`; these cases lock the parts that a second reader must see.

use edgedoc::{Budget, Document, Field, FormField, FormKind, Paragraph, StoryRef};
use edgedoc_opc::write_zip;

type Test = Result<(), String>;

fn err(err: impl std::fmt::Display) -> String {
    err.to_string()
}

fn fail(message: impl Into<String>) -> Test {
    Err(message.into())
}

fn office(document: &str, extra: &[(&str, &[u8])]) -> Result<Vec<u8>, String> {
    let mut types = String::from(
        r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>"#,
    );
    for (name, _) in extra {
        if name.ends_with(".xml") && *name != "word/document.xml" {
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
        (
            "word/_rels/document.xml.rels".into(),
            br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"></Relationships>"#.to_vec(),
        ),
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

fn part_of(bytes: &[u8], name: &str) -> Result<String, String> {
    let package = edgedoc_opc::read_zip(bytes, &Budget::default()).map_err(err)?;
    package
        .part(name)
        .map(|part| String::from_utf8_lossy(&part.bytes).into_owned())
        .ok_or_else(|| format!("no part {name}"))
}

/// A field written by the builder keeps its flags and its form settings.
fn field_flags_are_written() -> Test {
    let mut para = Paragraph::new().text("Amount: ");
    para.push_object(
        edgedoc::InlineObject::Field(Field::new(" FORMTEXT ", "0").dirty().locked()),
        &edgedoc::CharProps::default(),
    );
    let bytes = Document::builder()
        .paragraph(para)
        .build()
        .map_err(err)?
        .write_bytes()
        .map_err(err)?;
    let document = part_of(&bytes, "word/document.xml")?;
    if !document.contains(r#"w:dirty="true""#) || !document.contains(r#"w:fldLock="true""#) {
        return fail(document);
    }
    // The flags survive a read.
    let reread = Document::read_bytes(&bytes).map_err(err)?;
    let text = reread.plain_text(StoryRef::Body);
    if !text.contains("Amount: 0") {
        return fail(text);
    }
    Ok(())
}

/// A legacy form field written by the builder reads back as a form field.
fn form_field_round_trip() -> Test {
    let form = FormField {
        name: "Pick".into(),
        enabled: true,
        calc_on_exit: false,
        help: Some("Choose one".into()),
        status: None,
        kind: FormKind::DropDown {
            result: Some(1),
            entries: vec!["One".into(), "Two".into()],
        },
    };
    let mut para = Paragraph::new().text("Value: ");
    para.push_object(
        edgedoc::InlineObject::Field(Field::new(" FORMDROPDOWN ", "Two").form(form)),
        &edgedoc::CharProps::default(),
    );
    let bytes = Document::builder()
        .paragraph(para)
        .build()
        .map_err(err)?
        .write_bytes()
        .map_err(err)?;
    let document = part_of(&bytes, "word/document.xml")?;
    for needle in [
        "<w:ffData>",
        r#"<w:name w:val="Pick"/>"#,
        "<w:enabled/>",
        "<w:ddList>",
        r#"w:val="One""#,
        r#"<w:result w:val="1"/>"#,
        r#"w:val="Choose one""#,
    ] {
        if !document.contains(needle) {
            return fail(format!("missing {needle}"));
        }
    }
    Ok(())
}

/// Section vertical alignment and a fitted run reach the file in schema order.
fn section_and_run_extras() -> Test {
    let section = edgedoc::SectionProps {
        v_align: Some("center".into()),
        ..edgedoc::SectionProps::default()
    };
    let bytes = Document::builder()
        .page(edgedoc::Page::a4())
        .paragraph(
            Paragraph::new().text("Fitted").run(
                edgedoc::RunBuilder::new()
                    .text("x")
                    .fit_text(edgedoc::Twips(1200)),
            ),
        )
        .build()
        .map_err(err)?
        .write_bytes()
        .map_err(err)?;
    let document = part_of(&bytes, "word/document.xml")?;
    if !document.contains(r#"<w:fitText w:val="1200"/>"#) {
        return fail("the fitText is missing");
    }
    // A section with vAlign needs the writer to place it before docGrid.
    let mut doc = edgedoc::Document::builder()
        .paragraph(Paragraph::new().text("A"))
        .build()
        .map_err(err)?;
    doc.edit(edgedoc::EditOptions::default(), |edit| {
        edit.body().paragraph_at(0, |para| {
            para.paragraph_mut().section = Some(edgedoc::SectionBreak {
                props: section.clone(),
            });
            Ok(())
        })
    })
    .map_err(err)?;
    let out = doc.write_bytes().map_err(err)?;
    if !part_of(&out, "word/document.xml")?.contains(r#"<w:vAlign w:val="center"/>"#) {
        return fail("the vAlign is missing");
    }
    Ok(())
}

/// A shape that a user wrote must stay whole, and its text must reach a reader.
fn shape_survives_a_round_trip() -> Test {
    let inner = r#"<w:p><w:r><mc:AlternateContent xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006"><mc:Choice Requires="wps"><w:drawing><wps:wsp xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><wps:txbx><w:txbxContent><w:p><w:r><w:t>Label</w:t></w:r></w:p></w:txbxContent></wps:txbx></wps:wsp></w:drawing></mc:Choice></mc:AlternateContent></w:r></w:p>"#;
    let bytes = office(&body(inner), &[])?;
    let mut doc = Document::read_bytes(&bytes).map_err(err)?;
    doc.edit(
        edgedoc::EditOptions::default().allow(edgedoc::DropKind::Objects),
        |edit| {
            edit.body().paragraph_after_last(|para| para.text("Tail"))?;
            Ok(())
        },
    )
    .map_err(err)?;
    let out = doc.write_bytes().map_err(err)?;
    let document = part_of(&out, "word/document.xml")?;
    if !document.contains("Label") || !document.contains("wps:wsp") {
        return fail("the shape was lost");
    }
    Ok(())
}

type Case = (&'static str, fn() -> Test);

fn cases() -> Vec<Case> {
    vec![
        ("field-flags", field_flags_are_written),
        ("form-field", form_field_round_trip),
        ("section-and-run-extras", section_and_run_extras),
        ("shape", shape_survives_a_round_trip),
    ]
}

#[test]
fn written_features() -> Test {
    for (name, case) in cases() {
        case().map_err(|message| format!("{name}: {message}"))?;
    }
    Ok(())
}
