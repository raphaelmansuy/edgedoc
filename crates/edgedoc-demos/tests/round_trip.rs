//! A document that a user did not write must survive an edit unchanged.
//!
//! Each case builds a package that holds one Word feature, edits an unrelated
//! paragraph, saves, and checks that the feature is still in the saved part. A
//! feature that the model does not understand must be kept as markup, not dropped.
//!
//! The list of cases is the table in `docs/14-round-trip.md`. A case without a row
//! fails, and a row without a case fails.

use edgedoc::{Budget, Document, EditOptions};
use edgedoc_opc::write_zip;

type Test = Result<(), String>;

fn err(err: impl std::fmt::Display) -> String {
    err.to_string()
}

fn fail(message: impl Into<String>) -> Test {
    Err(message.into())
}

fn office(document: &str, rels_xml: &str, extra: &[(&str, &[u8])]) -> Result<Vec<u8>, String> {
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
            format!(
                r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">{rels_xml}</Relationships>"#
            )
            .into_bytes(),
        ),
    ];
    for (name, bytes) in extra {
        parts.push(((*name).into(), bytes.to_vec()));
    }
    write_zip(&parts).map_err(err)
}

fn body(inner: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:w14="http://schemas.microsoft.com/office/word/2010/wordml" xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math" xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006" xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape" xmlns:v="urn:schemas-microsoft-com:vml"><w:body>{inner}</w:body></w:document>"#
    )
}

const STYLE_RELS: &str = r#"<Relationship Id="rIdS" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>"#;

/// Read the document, add a paragraph at the end, save, and return the stated part.
fn after_edit(bytes: &[u8], part: &str) -> Result<String, String> {
    let mut doc = Document::read_bytes(bytes).map_err(err)?;
    doc.edit(
        EditOptions::default().allow(edgedoc::DropKind::Objects),
        |edit| {
            edit.body().paragraph_after_last(|para| para.text("Tail"))?;
            Ok(())
        },
    )
    .map_err(err)?;
    let out = doc.write_bytes().map_err(err)?;
    let package = edgedoc_opc::read_zip(&out, &Budget::default()).map_err(err)?;
    package
        .part(part)
        .map(|part| String::from_utf8_lossy(&part.bytes).into_owned())
        .ok_or_else(|| format!("missing part {part}"))
}

/// Check that every needle is in the stated part after an edit.
fn keeps(
    label: &str,
    part: &str,
    inner: &str,
    rels_xml: &str,
    extra: &[(&str, &[u8])],
    needles: &[&str],
) -> Test {
    let bytes =
        office(&body(inner), rels_xml, extra).map_err(|message| format!("{label}: {message}"))?;
    let text = after_edit(&bytes, part).map_err(|message| format!("{label}: {message}"))?;
    for needle in needles {
        if !text.contains(needle) {
            return fail(format!("{label}: {part} lost {needle}"));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------- settings and styles

fn document_protection() -> Test {
    keeps(
        "documentProtection",
        "word/settings.xml",
        r#"<w:p><w:r><w:t>Body</w:t></w:r></w:p>"#,
        "",
        &[(
            "word/settings.xml",
            br#"<w:settings><w:documentProtection w:edit="readOnly" w:enforcement="1"/></w:settings>"#,
        )],
        &["documentProtection"],
    )
}

fn hyphenation() -> Test {
    keeps(
        "hyphenation",
        "word/settings.xml",
        r#"<w:p><w:r><w:t>Body</w:t></w:r></w:p>"#,
        "",
        &[(
            "word/settings.xml",
            br#"<w:settings><w:autoHyphenation w:val="1"/><w:consecutiveHyphenLimit w:val="2"/></w:settings>"#,
        )],
        &["autoHyphenation", "consecutiveHyphenLimit"],
    )
}

fn style_properties() -> Test {
    keeps(
        "style properties",
        "word/styles.xml",
        r#"<w:p><w:pPr><w:pStyle w:val="S1"/></w:pPr><w:r><w:t>B</w:t></w:r></w:p>"#,
        STYLE_RELS,
        &[(
            "word/styles.xml",
            br#"<w:styles><w:style w:type="paragraph" w:styleId="S1"><w:name w:val="S1"/><w:pPr><w:keepNext/><w:spacing w:before="240"/></w:pPr><w:rPr><w:b/><w:sz w:val="28"/></w:rPr></w:style></w:styles>"#,
        )],
        &[r#"w:before="240""#, r#"w:val="28""#],
    )
}

fn document_defaults() -> Test {
    keeps(
        "document defaults",
        "word/styles.xml",
        r#"<w:p><w:r><w:t>B</w:t></w:r></w:p>"#,
        STYLE_RELS,
        &[(
            "word/styles.xml",
            br#"<w:styles><w:docDefaults><w:rPrDefault><w:rPr><w:sz w:val="22"/></w:rPr></w:rPrDefault><w:pPrDefault><w:pPr><w:spacing w:after="160"/></w:pPr></w:pPrDefault></w:docDefaults></w:styles>"#,
        )],
        &["docDefaults", r#"w:val="22""#],
    )
}

fn theme_part() -> Test {
    keeps(
        "theme",
        "word/theme/theme1.xml",
        r#"<w:p><w:r><w:t>B</w:t></w:r></w:p>"#,
        "",
        &[(
            "word/theme/theme1.xml",
            br#"<a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><a:themeElements><a:clrScheme name="Office"/></a:themeElements></a:theme>"#,
        )],
        &["themeElements"],
    )
}

// ---------------------------------------------------------------- runs and paragraphs

fn positional_tab() -> Test {
    keeps(
        "ptab",
        "word/document.xml",
        r#"<w:p><w:r><w:ptab w:alignment="center" w:relativeTo="margin"/><w:t>X</w:t></w:r></w:p>"#,
        "",
        &[],
        &["ptab"],
    )
}

fn ruby_text() -> Test {
    keeps(
        "ruby",
        "word/document.xml",
        r#"<w:p><w:ruby><w:rubyPr><w:rubyAlign w:val="center"/></w:rubyPr><w:rt><w:r><w:t>kanji</w:t></w:r></w:rt><w:rubyBase><w:r><w:t>base</w:t></w:r></w:rubyBase></w:ruby></w:p>"#,
        "",
        &[],
        &["w:ruby"],
    )
}

fn east_asian_layout() -> Test {
    keeps(
        "eastAsianLayout",
        "word/document.xml",
        r#"<w:p><w:r><w:rPr><w:eastAsianLayout w:id="1" w:combine="1"/></w:rPr><w:t>A</w:t></w:r></w:p>"#,
        "",
        &[],
        &["eastAsianLayout"],
    )
}

fn fit_text() -> Test {
    keeps(
        "fitText",
        "word/document.xml",
        r#"<w:p><w:r><w:rPr><w:fitText w:val="1200"/></w:rPr><w:t>A</w:t></w:r></w:p>"#,
        "",
        &[],
        &[r#"<w:fitText w:val="1200"/>"#],
    )
}

fn br_page_break() -> Test {
    keeps(
        "page break",
        "word/document.xml",
        r#"<w:p><w:r><w:t>A</w:t><w:br w:type="page"/><w:t>B</w:t></w:r></w:p>"#,
        "",
        &[],
        &[r#"w:type="page""#],
    )
}

// ---------------------------------------------------------------- fields and forms

fn field_dirty_and_locked() -> Test {
    keeps(
        "field flags",
        "word/document.xml",
        r#"<w:p><w:r><w:fldChar w:fldCharType="begin" w:dirty="true" w:fldLock="true"/></w:r><w:r><w:instrText> TOC </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>Contents</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r></w:p>"#,
        "",
        &[],
        &[r#"w:dirty="true""#, r#"w:fldLock="true""#],
    )
}

fn legacy_form_field() -> Test {
    let fields = keeps(
        "legacy form field",
        "word/document.xml",
        r#"<w:p><w:r><w:fldChar w:fldCharType="begin"><w:ffData><w:name w:val="Field1"/><w:enabled/><w:checkBox><w:sizeAuto/><w:checked w:val="1"/></w:checkBox></w:ffData></w:fldChar></w:r><w:r><w:instrText> FORMCHECKBOX </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>X</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r></w:p>"#,
        "",
        &[],
        &[
            "ffData",
            r#"w:val="Field1""#,
            "<w:enabled/>",
            "<w:sizeAuto/>",
        ],
    );
    fields?;
    let text_field = keeps(
        "text form field",
        "word/document.xml",
        r#"<w:p><w:r><w:fldChar w:fldCharType="begin"><w:ffData><w:name w:val="Amount"/><w:textInput w:type="number"><w:maxLength w:val="12"/></w:textInput></w:ffData></w:fldChar></w:r><w:r><w:instrText> FORMTEXT </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>0</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r></w:p>"#,
        "",
        &[],
        &[
            r#"<w:textInput w:type="number">"#,
            r#"<w:maxLength w:val="12"/>"#,
        ],
    );
    text_field?;
    keeps(
        "drop-down form field",
        "word/document.xml",
        r#"<w:p><w:r><w:fldChar w:fldCharType="begin"><w:ffData><w:name w:val="Pick"/><w:ddList><w:result w:val="1"/><w:listEntry w:val="One"/><w:listEntry w:val="Two"/></w:ddList></w:ffData></w:fldChar></w:r><w:r><w:instrText> FORMDROPDOWN </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>Two</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r></w:p>"#,
        "",
        &[],
        &["<w:ddList>", r#"w:val="One""#, r#"<w:result w:val="1"/>"#],
    )
}

fn simple_field() -> Test {
    keeps(
        "fldSimple",
        "word/document.xml",
        r#"<w:p><w:fldSimple w:instr=" MERGEFIELD X "><w:r><w:t>v</w:t></w:r></w:fldSimple></w:p>"#,
        "",
        &[],
        &["MERGEFIELD X"],
    )
}

// ---------------------------------------------------------------- sections

fn section_vertical_align() -> Test {
    keeps(
        "section vAlign",
        "word/document.xml",
        r#"<w:p><w:r><w:t>B</w:t></w:r></w:p><w:sectPr><w:vAlign w:val="center"/></w:sectPr>"#,
        "",
        &[],
        &[r#"<w:vAlign w:val="center"/>"#],
    )
}

fn section_text_direction() -> Test {
    keeps(
        "section text direction",
        "word/document.xml",
        r#"<w:p><w:r><w:t>B</w:t></w:r></w:p><w:sectPr><w:textDirection w:val="tbRl"/></w:sectPr>"#,
        "",
        &[],
        &["tbRl"],
    )
}

fn section_type() -> Test {
    keeps(
        "section type",
        "word/document.xml",
        r#"<w:p><w:r><w:t>B</w:t></w:r></w:p><w:sectPr><w:type w:val="continuous"/></w:sectPr>"#,
        "",
        &[],
        &["continuous"],
    )
}

fn section_permissions() -> Test {
    keeps(
        "permStart",
        "word/document.xml",
        r#"<w:p><w:permStart w:id="1" w:edGrp="everyone"/><w:r><w:t>Body</w:t></w:r><w:permEnd w:id="1"/></w:p>"#,
        "",
        &[],
        &["permStart", "permEnd"],
    )
}

// ---------------------------------------------------------------- tables

fn table_alt_text() -> Test {
    keeps(
        "table alt text",
        "word/document.xml",
        r#"<w:tbl><w:tblPr><w:tblCaption w:val="Cap"/><w:tblDescription w:val="Desc"/></w:tblPr><w:tblGrid><w:gridCol w:w="1000"/></w:tblGrid><w:tr><w:tc><w:p><w:r><w:t>C</w:t></w:r></w:p></w:tc></w:tr></w:tbl>"#,
        "",
        &[],
        &["tblCaption", "tblDescription"],
    )
}

fn table_style() -> Test {
    keeps(
        "table style",
        "word/document.xml",
        r#"<w:tbl><w:tblPr><w:tblStyle w:val="TblGrid"/></w:tblPr><w:tblGrid><w:gridCol w:w="900"/></w:tblGrid><w:tr><w:tc><w:p><w:r><w:t>C</w:t></w:r></w:p></w:tc></w:tr></w:tbl>"#,
        "",
        &[],
        &["TblGrid"],
    )
}

fn table_row_properties() -> Test {
    keeps(
        "row properties",
        "word/document.xml",
        r#"<w:tbl><w:tblGrid><w:gridCol w:w="900"/></w:tblGrid><w:tr><w:trPr><w:cantSplit/><w:trHeight w:val="400" w:hRule="exact"/><w:tblHeader/></w:trPr><w:tc><w:p><w:r><w:t>C</w:t></w:r></w:p></w:tc></w:tr></w:tbl>"#,
        "",
        &[],
        &[r#"w:hRule="exact""#, "<w:tblHeader/>", "<w:cantSplit/>"],
    )
}

fn table_cell_properties() -> Test {
    keeps(
        "cell properties",
        "word/document.xml",
        r#"<w:tbl><w:tblGrid><w:gridCol w:w="900"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcMar><w:top w:w="120" w:type="dxa"/></w:tcMar><w:textDirection w:val="btLr"/></w:tcPr><w:p><w:r><w:t>C</w:t></w:r></w:p></w:tc></w:tr></w:tbl>"#,
        "",
        &[],
        &[r#"w:w="120""#, "btLr"],
    )
}

fn nested_table() -> Test {
    keeps(
        "nested table",
        "word/document.xml",
        r#"<w:tbl><w:tblGrid><w:gridCol w:w="2000"/></w:tblGrid><w:tr><w:tc><w:tbl><w:tblGrid><w:gridCol w:w="1000"/></w:tblGrid><w:tr><w:tc><w:p><w:r><w:t>Inner</w:t></w:r></w:p></w:tc></w:tr></w:tbl></w:tc></w:tr></w:tbl>"#,
        "",
        &[],
        &["Inner"],
    )
}

#[test]
fn row_table_property_exception_survives_an_edit() -> Test {
    keeps(
        "row table property exception",
        "word/document.xml",
        r#"<w:tbl><w:tblGrid><w:gridCol w:w="900"/><w:gridCol w:w="900"/></w:tblGrid><w:tr><w:tblPrEx><w:tblBorders><w:insideV w:val="single" w:sz="8"/></w:tblBorders></w:tblPrEx><w:tc><w:p><w:r><w:t>A</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>B</w:t></w:r></w:p></w:tc></w:tr></w:tbl>"#,
        "",
        &[],
        &["<w:tblPrEx>", "<w:insideV", r#"w:sz="8""#],
    )
}

#[test]
fn omitted_table_grid_columns_survive_an_edit() -> Test {
    keeps(
        "row grid offsets",
        "word/document.xml",
        r#"<w:tbl><w:tblGrid><w:gridCol w:w="900"/><w:gridCol w:w="900"/><w:gridCol w:w="900"/></w:tblGrid><w:tr><w:trPr><w:gridBefore w:val="1"/><w:gridAfter w:val="1"/></w:trPr><w:tc><w:p><w:r><w:t>A</w:t></w:r></w:p></w:tc></w:tr></w:tbl>"#,
        "",
        &[],
        &[
            r#"<w:gridBefore w:val="1"/>"#,
            r#"<w:gridAfter w:val="1"/>"#,
        ],
    )
}

fn control_in_table() -> Test {
    keeps(
        "control in cell",
        "word/document.xml",
        r#"<w:tbl><w:tblGrid><w:gridCol w:w="1000"/></w:tblGrid><w:tr><w:tc><w:sdt><w:sdtPr><w:alias w:val="CellCtl"/></w:sdtPr><w:sdtContent><w:p><w:r><w:t>C</w:t></w:r></w:p></w:sdtContent></w:sdt></w:tc></w:tr></w:tbl>"#,
        "",
        &[],
        &["CellCtl"],
    )
}

fn list_in_table() -> Test {
    keeps(
        "list in cell",
        "word/document.xml",
        r#"<w:tbl><w:tblGrid><w:gridCol w:w="900"/></w:tblGrid><w:tr><w:tc><w:p><w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr></w:pPr><w:r><w:t>Item</w:t></w:r></w:p></w:tc></w:tr></w:tbl>"#,
        "",
        &[(
            "word/numbering.xml",
            br#"<w:numbering><w:abstractNum w:abstractNumId="0"><w:lvl w:ilvl="0"><w:numFmt w:val="decimal"/><w:lvlText w:val="%1."/></w:lvl></w:abstractNum><w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num></w:numbering>"#,
        )],
        &[r#"<w:numId w:val="1"/>"#],
    )
}

fn numbering_override() -> Test {
    keeps(
        "lvlOverride",
        "word/numbering.xml",
        r#"<w:p><w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="5"/></w:numPr></w:pPr><w:r><w:t>Item</w:t></w:r></w:p>"#,
        "",
        &[(
            "word/numbering.xml",
            br#"<w:numbering><w:num w:numId="5"><w:abstractNumId w:val="1"/><w:lvlOverride w:ilvl="0"><w:startOverride w:val="3"/></w:lvlOverride></w:num></w:numbering>"#,
        )],
        &["lvlOverride", "startOverride"],
    )
}

// ---------------------------------------------------------------- stories

fn two_footnotes() -> Test {
    keeps(
        "two footnotes",
        "word/footnotes.xml",
        r#"<w:p><w:r><w:t>B</w:t><w:footnoteReference w:id="1"/><w:footnoteReference w:id="2"/></w:r></w:p>"#,
        "",
        &[(
            "word/footnotes.xml",
            br#"<w:footnotes xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:footnote w:id="1"><w:p><w:r><w:t>First</w:t></w:r></w:p></w:footnote><w:footnote w:id="2"><w:p><w:r><w:t>Second</w:t></w:r></w:p></w:footnote></w:footnotes>"#,
        )],
        &["First", "Second"],
    )
}

fn note_separator() -> Test {
    keeps(
        "separator",
        "word/footnotes.xml",
        r#"<w:p><w:r><w:t>B</w:t></w:r></w:p>"#,
        "",
        &[(
            "word/footnotes.xml",
            br#"<w:footnotes xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:footnote w:type="separator" w:id="-1"><w:p><w:r><w:separator/></w:r></w:p></w:footnote></w:footnotes>"#,
        )],
        &[r#"w:type="separator""#],
    )
}

#[test]
fn note_story_number_marks_survive_an_edited_save() -> Test {
    for (kind, element) in [("footnote", "footnoteRef"), ("endnote", "endnoteRef")] {
        let rels = format!(
            r#"<Relationship Id="note" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/{kind}s" Target="{kind}s.xml"/>"#
        );
        let name = format!("word/{kind}s.xml");
        let story = format!(
            r#"<w:{kind}s xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:{kind} w:type="separator" w:id="-1"><w:p><w:r><w:separator/></w:r></w:p></w:{kind}><w:{kind} w:type="continuationSeparator" w:id="0"><w:p><w:r><w:continuationSeparator/></w:r></w:p></w:{kind}><w:{kind} w:id="1"><w:p><w:r><w:{element}/><w:t> note</w:t></w:r></w:p></w:{kind}></w:{kind}s>"#
        );
        let bytes = office(
            &body("<w:p><w:r><w:t>Body</w:t></w:r></w:p>"),
            &rels,
            &[(&name, story.as_bytes())],
        )?;
        let model = Document::read_bytes(&bytes).map_err(err)?;
        let part_kind = if kind == "footnote" {
            edgedoc::PartKind::Footnote
        } else {
            edgedoc::PartKind::Endnote
        };
        let parts = model.parts(part_kind);
        let part = *parts
            .iter()
            .find(|id| model.part_meta(**id).is_some_and(|meta| meta.id == Some(1)))
            .ok_or("note part missing")?;
        let blocks = model.part_blocks(part);
        let found = blocks.iter().any(|block| match block {
            edgedoc::Block::Paragraph(paragraph) => paragraph.objects.iter().any(|object| {
                matches!(object, edgedoc::InlineObject::Note { kind: value, .. } if value == &format!("{kind}Mark"))
            }),
            _ => false,
        });
        if !found {
            return fail(format!("{kind} story number mark was not read"));
        }
        for (id, continuation) in [(-1, false), (0, true)] {
            let part = *parts
                .iter()
                .find(|part| {
                    model
                        .part_meta(**part)
                        .is_some_and(|meta| meta.id == Some(id))
                })
                .ok_or("separator story missing")?;
            let found = model.part_blocks(part).iter().any(|block| match block {
                edgedoc::Block::Paragraph(paragraph) => paragraph.objects.iter().any(|object| matches!(object, edgedoc::InlineObject::NoteSeparator { continuation: value } if *value == continuation)),
                _ => false,
            });
            if !found {
                return fail(format!("{kind} separator {id} was not read"));
            }
        }
        let saved = after_edit(&bytes, &name)?;
        if !saved.contains(&format!("<w:{element}/>")) {
            return fail(format!("{kind} story lost its number mark: {saved}"));
        }
        if !saved.contains("<w:separator/>") || !saved.contains("<w:continuationSeparator/>") {
            return fail(format!("{kind} story lost a separator: {saved}"));
        }
    }
    Ok(())
}

fn endnote_and_comment() -> Test {
    keeps(
        "endnote and comment",
        "word/endnotes.xml",
        r#"<w:p><w:r><w:t>B</w:t><w:endnoteReference w:id="1"/><w:commentReference w:id="1"/></w:r></w:p>"#,
        "",
        &[
            (
                "word/endnotes.xml",
                br#"<w:endnotes xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:endnote w:id="1"><w:p><w:r><w:t>EndText</w:t></w:r></w:p></w:endnote></w:endnotes>"#,
            ),
            (
                "word/comments.xml",
                br#"<w:comments xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:comment w:id="1"><w:p><w:r><w:t>CommentText</w:t></w:r></w:p></w:comment></w:comments>"#,
            ),
        ],
        &["EndText"],
    )
}

fn header_bookmark_and_link() -> Test {
    keeps(
        "header content",
        "word/header1.xml",
        r#"<w:p><w:r><w:t>Body</w:t></w:r></w:p><w:sectPr><w:headerReference w:type="default" r:id="rIdH"/></w:sectPr>"#,
        "",
        &[(
            "word/header1.xml",
            br#"<w:hdr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><w:p><w:bookmarkStart w:id="1" w:name="HeadMark"/><w:hyperlink r:id="rIdX"><w:r><w:t>Link</w:t></w:r></w:hyperlink></w:p></w:hdr>"#,
        )],
        &["HeadMark", "Link"],
    )
}

// ---------------------------------------------------------------- unknown markup

fn tracked_changes() -> Test {
    keeps(
        "move tracking",
        "word/document.xml",
        r#"<w:p><w:moveFrom w:id="1"><w:r><w:t>Old</w:t></w:r></w:moveFrom><w:moveTo w:id="2"><w:r><w:t>New</w:t></w:r></w:moveTo></w:p>"#,
        "",
        &[],
        &["moveFrom", "moveTo"],
    )
}

fn formatting_revision() -> Test {
    keeps(
        "formatting revision",
        "word/document.xml",
        r#"<w:p><w:r><w:rPr><w:b/><w:rPrChange w:id="1" w:author="A" w:date="2026-01-01T00:00:00Z"><w:rPr/></w:rPrChange></w:rPr><w:t>Bold</w:t></w:r></w:p>"#,
        "",
        &[],
        &["rPrChange"],
    )
}

fn equation() -> Test {
    keeps(
        "equation",
        "word/document.xml",
        r#"<w:p><m:oMath><m:r><m:t>x+y</m:t></m:r></m:oMath></w:p>"#,
        "",
        &[],
        &["oMath"],
    )
}

fn shape_text_box() -> Test {
    keeps(
        "shape text box",
        "word/document.xml",
        r#"<w:p><w:r><mc:AlternateContent><mc:Choice Requires="wps"><w:drawing><wps:wsp><wps:txbx><w:txbxContent><w:p><w:r><w:t>ShapeText</w:t></w:r></w:p></w:txbxContent></wps:txbx></wps:wsp></w:drawing></mc:Choice><mc:Fallback><w:pict><v:shape/></w:pict></mc:Fallback></mc:AlternateContent></w:r></w:p>"#,
        "",
        &[],
        &["ShapeText"],
    )
}

fn alt_chunk() -> Test {
    keeps(
        "altChunk",
        "word/document.xml",
        r#"<w:altChunk r:id="rId9"/><w:p><w:r><w:t>After</w:t></w:r></w:p>"#,
        "",
        &[],
        &["altChunk"],
    )
}

/// A table row marked `repeats` takes one row per record, and the other rows stay put.
fn merge_repeats_a_table_row() -> Test {
    use edgedoc::{
        Block, Cell, CsvOptions, MergeKind, MissingField, Paragraph, Records, Row, StoryRef, Table,
    };
    use std::sync::Arc;

    let mut table = Table::new()
        .widths([edgedoc::Mm(40.0), edgedoc::Mm(30.0)])
        .header(["Name", "City"]);
    table.rows.push(Row::cells([
        Cell::paragraph("").blocks(vec![Arc::new(Block::Paragraph(
            Paragraph::new().merge_field("Name"),
        ))]),
        Cell::paragraph("").blocks(vec![Arc::new(Block::Paragraph(
            Paragraph::new().merge_field("City"),
        ))]),
    ]));
    let doc = Document::builder()
        .paragraph(Paragraph::new().text("List").merge_field("Name"))
        .table(table)
        .build()
        .map_err(err)?;
    let records = Records::from_csv(
        b"Name,City\nAda,Leeds\nGrace,Oslo\nAlan,Paris\n",
        &CsvOptions::default(),
    )
    .map_err(err)?;
    let merged = doc
        .merge()
        .records(records)
        .kind(MergeKind::Directory)
        .missing(MissingField::Error)
        .finish()
        .map_err(err)?
        .join(edgedoc::Join::Auto)
        .map_err(err)?;
    let text = merged.plain_text(StoryRef::Body);
    for name in ["Ada", "Grace", "Alan"] {
        if !text.contains(name) {
            return fail(format!("{name} is missing from {text}"));
        }
    }
    // Three records produce one document, not three.
    let bytes = merged.write_bytes().map_err(err)?;
    let reread = Document::read_bytes(&bytes).map_err(err)?;
    if reread.plain_text(StoryRef::Body) != text {
        return fail("the saved copy differs");
    }
    Ok(())
}

type Case = (&'static str, fn() -> Test);

fn cases() -> Vec<Case> {
    vec![
        ("document-protection", document_protection),
        ("hyphenation", hyphenation),
        ("style-properties", style_properties),
        ("document-defaults", document_defaults),
        ("theme-part", theme_part),
        ("positional-tab", positional_tab),
        ("ruby-text", ruby_text),
        ("east-asian-layout", east_asian_layout),
        ("fit-text", fit_text),
        ("br-page-break", br_page_break),
        ("field-dirty-and-locked", field_dirty_and_locked),
        ("legacy-form-field", legacy_form_field),
        ("simple-field", simple_field),
        ("section-vertical-align", section_vertical_align),
        ("section-text-direction", section_text_direction),
        ("section-type", section_type),
        ("section-permissions", section_permissions),
        ("table-alt-text", table_alt_text),
        ("table-style", table_style),
        ("table-row-properties", table_row_properties),
        ("table-cell-properties", table_cell_properties),
        ("nested-table", nested_table),
        ("control-in-table", control_in_table),
        ("list-in-table", list_in_table),
        ("numbering-override", numbering_override),
        ("two-footnotes", two_footnotes),
        ("note-separator", note_separator),
        ("endnote-and-comment", endnote_and_comment),
        ("header-bookmark-and-link", header_bookmark_and_link),
        ("tracked-changes", tracked_changes),
        ("formatting-revision", formatting_revision),
        ("equation", equation),
        ("shape-text-box", shape_text_box),
        ("alt-chunk", alt_chunk),
        ("merge-repeats-a-table-row", merge_repeats_a_table_row),
    ]
}

#[test]
fn round_trip() -> Test {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/14-round-trip.md");
    let doc = std::fs::read_to_string(path).map_err(err)?;
    let cases = cases();

    for (id, _) in &cases {
        if !doc.contains(id) {
            return fail(format!("docs/14-round-trip.md has no row for {id}"));
        }
    }
    let mut seen = 0usize;
    for line in doc.lines() {
        let cells: Vec<&str> = line.split('|').map(str::trim).collect();
        if cells.len() < 6 {
            continue;
        }
        let id = cells[4].trim_matches('`');
        if id.is_empty() || id == "Test" || id.chars().all(|ch| ch == '-') || id == "none" {
            continue;
        }
        seen += 1;
        if !cases.iter().any(|(name, _)| *name == id) {
            return fail(format!("the table lists {id} with no case"));
        }
    }
    if seen < cases.len() {
        return fail(format!(
            "the table parsed {seen} rows, and the matrix has {} cases",
            cases.len()
        ));
    }

    for (id, case) in cases {
        case().map_err(|message| format!("{id}: {message}"))?;
    }
    Ok(())
}

#[test]
fn nested_toc_field_preserves_results_and_boundaries_after_edit() -> Test {
    let inner = r##"<w:p><w:r><w:fldChar w:fldCharType="begin" w:dirty="true"/></w:r><w:r><w:instrText> TOC \o "1-3" </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:hyperlink w:anchor="heading"><w:r><w:rPr><w:b/></w:rPr><w:t>First heading</w:t></w:r><w:r><w:tab/></w:r><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText> PAGEREF heading </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>3</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r></w:hyperlink></w:p><w:p><w:r><w:t>Second heading</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r><w:r><w:t> after</w:t></w:r></w:p>"##;
    let bytes = office(&body(inner), "", &[])?;
    let doc = Document::read_bytes(&bytes).map_err(err)?;
    let text = doc.plain_text(edgedoc::StoryRef::Body);
    if !text.contains("First heading\t3") || !text.contains("Second heading after") {
        return fail(format!("nested TOC lost visible text: {text:?}"));
    }
    let saved = after_edit(&bytes, "word/document.xml")?;
    for needle in [
        "TOC",
        "PAGEREF",
        "First heading",
        "Second heading",
        "w:dirty=\"true\"",
    ] {
        if !saved.contains(needle) {
            return fail(format!("saved TOC lost {needle}"));
        }
    }
    let saved_bytes = office(&saved, "", &[])?;
    let reread = Document::read_bytes(&saved_bytes).map_err(err)?;
    let text = reread.plain_text(edgedoc::StoryRef::Body);
    if !text.contains("First heading\t3") || !text.contains("Second heading after") {
        return fail(format!("reread TOC lost visible text: {text:?}"));
    }
    Ok(())
}

#[test]
fn nested_field_result_stays_in_its_parent() -> Test {
    let inner = r#"<w:p><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText> IF 1 = 1 </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>before </w:t></w:r><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText> REF x </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>middle</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r><w:r><w:t> after</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r></w:p>"#;
    let bytes = office(&body(inner), "", &[])?;
    let doc = Document::read_bytes(&bytes).map_err(err)?;
    let text = doc.plain_text(edgedoc::StoryRef::Body);
    if text != "before middle after" {
        return fail(format!("nested field result: {text:?}"));
    }
    Ok(())
}

#[test]
fn cached_field_tabs_and_nested_simple_fields_round_trip_in_order() -> Test {
    let inner = r#"<w:p><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText> IF 1 = 1 </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>before</w:t><w:tab/></w:r><w:fldSimple w:instr=" REF target "><w:r><w:t>middle</w:t></w:r></w:fldSimple><w:r><w:br/><w:t>after</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r></w:p>"#;
    let bytes = office(&body(inner), "", &[])?;
    let before = Document::read_bytes(&bytes)
        .map_err(err)?
        .plain_text(edgedoc::StoryRef::Body);
    if before != "before\tmiddle\nafter" {
        return fail(format!("cached field content: {before:?}"));
    }
    let saved = after_edit(&bytes, "word/document.xml")?;
    if !saved.contains("<w:tab/>") || !saved.contains("<w:br/>") {
        return fail("field result did not retain tab and line-break elements");
    }
    let bytes = office(&saved, "", &[])?;
    let after = Document::read_bytes(&bytes)
        .map_err(err)?
        .plain_text(edgedoc::StoryRef::Body);
    if !after.starts_with(&before) {
        return fail(format!("reread cached field: {after:?}"));
    }
    Ok(())
}

#[test]
fn empty_paragraph_border_set_survives_an_edited_save() -> Test {
    let bytes = office(
        &body(r#"<w:p><w:pPr><w:pBdr/></w:pPr><w:r><w:t>Text</w:t></w:r></w:p>"#),
        "",
        &[],
    )?;
    let saved = after_edit(&bytes, "word/document.xml")?;
    if !saved.contains("<w:pBdr/>") {
        return fail("the empty paragraph border override was lost");
    }
    Ok(())
}

#[test]
fn paragraph_frame_alignment_attributes_survive_an_edited_save() -> Test {
    let bytes = office(
        &body(
            r#"<w:p><w:pPr><w:framePr w:xAlign="right" w:yAlign="bottom" w:hAnchor="margin"/></w:pPr><w:r><w:t>Frame</w:t></w:r></w:p>"#,
        ),
        "",
        &[],
    )?;
    let saved = after_edit(&bytes, "word/document.xml")?;
    if !saved.contains("w:xAlign=\"right\"") || !saved.contains("w:yAlign=\"bottom\"") {
        return fail("paragraph frame alignment attributes were lost");
    }
    Ok(())
}

#[test]
fn inline_control_survives_edited_save() -> Test {
    let xml = r#"<w:p><w:r><w:t>Before </w:t></w:r><w:sdt><w:sdtPr><w:alias w:val="Label"/></w:sdtPr><w:sdtContent><w:proofErr w:type="spellStart"/><w:r><w:t>Visible</w:t></w:r><w:proofErr w:type="spellEnd"/></w:sdtContent></w:sdt><w:r><w:t> After</w:t></w:r></w:p>"#;
    let bytes = office(&body(xml), "", &[])?;
    let mut model = Document::read_bytes(&bytes).map_err(err)?;
    model
        .edit(EditOptions::default(), |edit| {
            edit.body()
                .paragraph_at(0, |paragraph| paragraph.insert_text(16, "X"))?;
            Ok(())
        })
        .map_err(err)?;
    let output = model.write_bytes().map_err(err)?;
    let package = edgedoc_opc::read_zip(&output, &Budget::default()).map_err(err)?;
    let saved = String::from_utf8_lossy(
        &package
            .part("word/document.xml")
            .ok_or("missing document")?
            .bytes,
    )
    .into_owned();
    for text in [
        "<w:sdt>",
        "<w:sdtPr>",
        "Label",
        "<w:sdtContent>",
        "X",
        "</w:sdtContent>",
        "</w:sdt>",
        "proofErr",
    ] {
        if !saved.contains(text) {
            return fail(format!("inline control lost {text}"));
        }
    }
    edgedoc_opc::xml::parse(saved.as_bytes(), &Budget::default()).map_err(err)?;
    let start = saved
        .find("<w:sdtContent>")
        .ok_or("missing control start")?;
    let end = saved.find("</w:sdtContent>").ok_or("missing control end")?;
    if !saved[start..end].contains("X") {
        return fail("edit moved outside the inline control");
    }
    Ok(())
}

#[test]
fn cell_control_survives_edited_save() -> Test {
    let xml = r#"<w:tbl><w:tr><w:tc><w:p><w:r><w:t>Left</w:t></w:r></w:p></w:tc><w:sdt><w:sdtPr><w:alias w:val="Cell"/></w:sdtPr><w:sdtContent><w:tc><w:p><w:r><w:t>Inside</w:t></w:r></w:p></w:tc></w:sdtContent></w:sdt></w:tr></w:tbl>"#;
    let bytes = office(&body(xml), "", &[])?;
    let saved = after_edit(&bytes, "word/document.xml")?;
    for text in [
        "<w:sdt>",
        "Cell",
        "<w:sdtContent>",
        "Inside",
        "</w:sdtContent>",
        "</w:sdt>",
    ] {
        if !saved.contains(text) {
            return fail(format!("cell control lost {text}"));
        }
    }
    edgedoc_opc::xml::parse(saved.as_bytes(), &Budget::default()).map_err(err)?;
    Ok(())
}
