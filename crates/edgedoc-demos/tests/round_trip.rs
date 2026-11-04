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
