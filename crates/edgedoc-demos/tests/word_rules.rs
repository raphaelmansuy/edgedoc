//! The rules a Word reader enforces, which a plain ZIP check does not catch.
//!
//! A package can hold valid XML and still be rejected or misrendered by Word. These
//! cases lock the rules that the WordCraft reader exposed: fixed child order in the
//! property containers, a numbering part when a paragraph refers to a list, a style
//! definition for every style a paragraph uses, one part per media payload, and the
//! namespace declarations that a part with a drawing needs.

use std::collections::BTreeSet;

use edgedoc::{CsvOptions, Document, Mm, Paragraph, PartKind, Pt, Records, StoryRef, Style, Table};

type Test = Result<(), String>;

fn err(err: impl std::fmt::Display) -> String {
    err.to_string()
}

fn fail(message: impl Into<String>) -> Test {
    Err(message.into())
}

/// The child order that ECMA-376 requires, for each container this library writes.
/// A reader may accept another order, but Word does not.
fn schema_order(tag: &str) -> Option<&'static [&'static str]> {
    match tag {
        "pPr" => Some(&[
            "pStyle",
            "keepNext",
            "keepLines",
            "pageBreakBefore",
            "framePr",
            "widowControl",
            "numPr",
            "suppressLineNumbers",
            "pBdr",
            "shd",
            "tabs",
            "suppressAutoHyphens",
            "kinsoku",
            "wordWrap",
            "overflowPunct",
            "topLinePunct",
            "autoSpaceDE",
            "autoSpaceDN",
            "bidi",
            "adjustRightInd",
            "snapToGrid",
            "spacing",
            "ind",
            "contextualSpacing",
            "mirrorIndents",
            "suppressOverlap",
            "jc",
            "textDirection",
            "textAlignment",
            "textboxTightWrap",
            "outlineLvl",
            "divId",
            "cnfStyle",
            "rPr",
            "sectPr",
            "pPrChange",
        ]),
        "sectPr" => Some(&[
            "headerReference",
            "footerReference",
            "footnotePr",
            "endnotePr",
            "type",
            "pgSz",
            "pgMar",
            "pgBorders",
            "lnNumType",
            "pgNumType",
            "cols",
            "formProt",
            "vAlign",
            "noEndnote",
            "titlePg",
            "textDirection",
            "bidi",
            "rtlGutter",
            "docGrid",
            "printerSettings",
        ]),
        "tblPr" => Some(&[
            "tblStyle",
            "tblpPr",
            "tblOverlap",
            "bidiVisual",
            "tblStyleRowBandSize",
            "tblStyleColBandSize",
            "tblW",
            "jc",
            "tblCellSpacing",
            "tblInd",
            "tblBorders",
            "shd",
            "tblLayout",
            "tblCellMar",
            "tblLook",
        ]),
        "trPr" => Some(&[
            "cnfStyle",
            "divId",
            "gridBefore",
            "gridAfter",
            "wBefore",
            "wAfter",
            "cantSplit",
            "trHeight",
            "tblHeader",
            "tblCellSpacing",
            "jc",
            "hidden",
        ]),
        "tcPr" => Some(&[
            "cnfStyle",
            "tcW",
            "gridSpan",
            "hMerge",
            "vMerge",
            "tcBorders",
            "shd",
            "noWrap",
            "tcMar",
            "textDirection",
            "tcFitText",
            "vAlign",
            "hideMark",
        ]),
        "rPr" => Some(&[
            "rStyle",
            "rFonts",
            "b",
            "bCs",
            "i",
            "iCs",
            "caps",
            "smallCaps",
            "strike",
            "dstrike",
            "outline",
            "shadow",
            "emboss",
            "imprint",
            "noProof",
            "snapToGrid",
            "vanish",
            "webHidden",
            "color",
            "spacing",
            "w",
            "kern",
            "position",
            "sz",
            "szCs",
            "highlight",
            "u",
            "effect",
            "bdr",
            "shd",
            "fitText",
            "vertAlign",
            "rtl",
            "cs",
            "em",
            "lang",
            "eastAsianLayout",
            "specVanish",
            "oMath",
        ]),
        "style" => Some(&[
            "name",
            "aliases",
            "basedOn",
            "next",
            "link",
            "autoRedefine",
            "hidden",
            "uiPriority",
            "semiHidden",
            "unhideWhenUsed",
            "qFormat",
            "locked",
            "personal",
            "personalCompose",
            "personalReply",
            "rsid",
            "pPr",
            "rPr",
            "tblPr",
            "trPr",
            "tcPr",
            "tblStylePr",
        ]),
        "styles" => Some(&["docDefaults", "latentStyles", "style"]),
        "tbl" => Some(&["tblPr", "tblGrid", "tr"]),
        "tr" => Some(&["trPr", "tc"]),
        _ => None,
    }
}

/// Direct child names of a container, ignoring anything nested deeper.
fn children(xml: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0usize;
    let bytes = xml.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() {
        let Some(open) = xml[index..].find('<') else {
            break;
        };
        let start = index + open;
        let Some(close) = xml[start..].find('>') else {
            break;
        };
        let end = start + close;
        let tag = &xml[start + 1..end];
        index = end + 1;
        if tag.starts_with('?') || tag.starts_with('!') {
            continue;
        }
        if tag.starts_with('/') {
            depth = depth.saturating_sub(1);
            continue;
        }
        let self_closing = tag.ends_with('/');
        if depth == 0 {
            let name = tag
                .trim_end_matches('/')
                .split_whitespace()
                .next()
                .unwrap_or("");
            out.push(name.rsplit(':').next().unwrap_or(name).to_string());
        }
        if !self_closing {
            depth += 1;
        }
    }
    out
}

fn containers(xml: &str, tag: &str) -> Vec<String> {
    let open = format!("<w:{tag}");
    let close = format!("</w:{tag}>");
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(start) = rest.find(&open) {
        let after = &rest[start..];
        let Some(body_start) = after.find('>') else {
            break;
        };
        if after[..body_start].ends_with('/') {
            rest = &after[body_start..];
            continue;
        }
        let Some(body_end) = after.find(&close) else {
            break;
        };
        out.push(after[body_start + 1..body_end].to_string());
        rest = &after[body_end + close.len()..];
    }
    out
}

fn part_of(bytes: &[u8], name: &str) -> Result<String, String> {
    let package = edgedoc_opc::read_zip(bytes, &edgedoc::Budget::default()).map_err(err)?;
    package
        .parts
        .values()
        .find(|part| part.name.eq_ignore_ascii_case(name))
        .map(|part| String::from_utf8_lossy(&part.bytes).into_owned())
        .ok_or_else(|| format!("no part {name}"))
}

/// A document that exercises every container: a list, a table, a section, and a style.
fn rich_document() -> Result<Vec<u8>, String> {
    let mut table = Table::new()
        .style("Grid")
        .widths([Mm(50.0), Mm(50.0)])
        .header(["Left", "Right"])
        .row(["a", "b"]);
    table.rows[0].header = true;
    table.align = Some("center".into());
    table.rows[0].height = Some(edgedoc::Twips(300));
    table.rows[0].height_rule = Some("exact".into());
    table.rows[0].cant_split = true;

    Document::builder()
        .style(
            Style::paragraph("Heading1")
                .named("heading 1")
                .size(Pt(16.0))
                .bold(),
        )
        .style(Style::table("Grid"))
        .page(edgedoc::Page::a4().landscape().columns(2).title_page())
        .paragraph(Paragraph::new().style("Heading1").text("Title"))
        .paragraph(
            Paragraph::new()
                .text("Bullet")
                .list(edgedoc::ListKind::Bullet, 0),
        )
        .paragraph(
            Paragraph::new()
                .text("Number")
                .list(edgedoc::ListKind::Decimal, 1),
        )
        .paragraph(
            Paragraph::new()
                .text("Section end")
                .section_break(edgedoc::SectionProps {
                    start: edgedoc::SectionStart::Continuous,
                    ..edgedoc::SectionProps::default()
                }),
        )
        .table(table)
        .build()
        .map_err(err)?
        .write_bytes()
        .map_err(err)
}

fn child_order_follows_the_schema() -> Test {
    let bytes = rich_document()?;
    let package = edgedoc_opc::read_zip(&bytes, &edgedoc::Budget::default()).map_err(err)?;
    let mut checked = 0usize;
    for part in package.parts.values() {
        if !part.name.ends_with(".xml") || part.name.ends_with(".rels") {
            continue;
        }
        let text = String::from_utf8_lossy(&part.bytes).into_owned();
        for tag in [
            "pPr", "sectPr", "tblPr", "trPr", "tcPr", "rPr", "style", "styles", "tbl", "tr",
        ] {
            let Some(order) = schema_order(tag) else {
                continue;
            };
            for body in containers(&text, tag) {
                let names: Vec<String> = children(&body)
                    .into_iter()
                    .filter(|name| order.contains(&name.as_str()))
                    .collect();
                let positions: Vec<usize> = names
                    .iter()
                    .filter_map(|name| order.iter().position(|item| item == name))
                    .collect();
                let mut sorted = positions.clone();
                sorted.sort_unstable();
                if positions != sorted {
                    return fail(format!("{tag} out of order in {}: {:?}", part.name, names));
                }
                checked += 1;
            }
        }
    }
    if checked == 0 {
        return fail("the check found no container");
    }
    Ok(())
}

/// A paragraph that refers to a list needs the numbering part, and the part needs a relationship.
fn numbering_exists_when_a_list_is_used() -> Test {
    let bytes = rich_document()?;
    let document = part_of(&bytes, "word/document.xml")?;
    if !document.contains("<w:numPr>") {
        return fail("the document has no list");
    }
    let numbering = part_of(&bytes, "word/numbering.xml")?;
    if !numbering.contains("<w:numFmt") {
        return fail("the numbering part holds no format");
    }
    let rels = part_of(&bytes, "word/_rels/document.xml.rels")?;
    if !rels.contains("relationships/numbering") {
        return fail("document.xml.rels has no numbering relationship");
    }
    Ok(())
}

/// Every style a paragraph names must have a definition, or Word shows plain text.
fn every_used_style_is_defined() -> Test {
    let bytes = rich_document()?;
    let document = part_of(&bytes, "word/document.xml")?;
    let styles = part_of(&bytes, "word/styles.xml")?;
    let mut used = BTreeSet::new();
    for body in containers(&document, "pPr") {
        for child in children(&body) {
            let _ = child;
        }
        if let Some(start) = body.find("<w:pStyle") {
            let rest = &body[start..];
            if let Some(value) = rest.find("w:val=\"") {
                let after = &rest[value + 7..];
                if let Some(end) = after.find('"') {
                    used.insert(after[..end].to_string());
                }
            }
        }
    }
    for body in containers(&document, "rPr") {
        if let Some(start) = body.find("<w:rStyle") {
            let rest = &body[start..];
            if let Some(value) = rest.find("w:val=\"") {
                let after = &rest[value + 7..];
                if let Some(end) = after.find('"') {
                    used.insert(after[..end].to_string());
                }
            }
        }
    }
    for body in containers(&document, "tblPr") {
        if let Some(start) = body.find("<w:tblStyle") {
            let rest = &body[start..];
            if let Some(value) = rest.find("w:val=\"") {
                let after = &rest[value + 7..];
                if let Some(end) = after.find('"') {
                    used.insert(after[..end].to_string());
                }
            }
        }
    }
    for name in used {
        let needle = format!("w:styleId=\"{name}\"");
        if !styles.contains(&needle) {
            return fail(format!("the style {name} is used but not defined"));
        }
    }
    Ok(())
}

/// The demos name styles, so their style sheet must define them.
fn demo_styles_are_defined() -> Test {
    for files in [
        edgedoc_demos::invoice::render(),
        edgedoc_demos::report::render(),
        edgedoc_demos::letter::render(),
    ] {
        for file in files.map_err(err)? {
            let package =
                edgedoc_opc::read_zip(&file.bytes, &edgedoc::Budget::default()).map_err(err)?;
            let styles = package
                .part("word/styles.xml")
                .map(|part| String::from_utf8_lossy(&part.bytes).into_owned())
                .unwrap_or_default();
            let document = package
                .part("word/document.xml")
                .map(|part| String::from_utf8_lossy(&part.bytes).into_owned())
                .unwrap_or_default();
            let mut used = BTreeSet::new();
            for tag in ["pStyle", "rStyle", "tblStyle"] {
                let marker = format!("<w:{tag} w:val=\"");
                let mut rest = document.as_str();
                while let Some(start) = rest.find(&marker) {
                    let after = &rest[start + marker.len()..];
                    let Some(end) = after.find('"') else { break };
                    used.insert(after[..end].to_string());
                    rest = &after[end..];
                }
            }
            for name in used {
                if !styles.contains(&format!("w:styleId=\"{name}\"")) {
                    return fail(format!("{} uses {name} with no definition", file.name));
                }
            }
        }
    }
    Ok(())
}

/// One part name, one payload. A merge copies the same logo into several stories.
fn media_parts_are_unique() -> Test {
    for files in [
        edgedoc_demos::invoice::render(),
        edgedoc_demos::report::render(),
    ] {
        for file in files.map_err(err)? {
            let package =
                edgedoc_opc::read_zip(&file.bytes, &edgedoc::Budget::default()).map_err(err)?;
            let media: Vec<&Vec<u8>> = package
                .parts
                .values()
                .filter(|part| part.name.starts_with("word/media/"))
                .map(|part| &part.bytes)
                .collect();
            for (index, first) in media.iter().enumerate() {
                for second in media.iter().skip(index + 1) {
                    if first == second {
                        return fail(format!("{} writes the same picture twice", file.name));
                    }
                }
            }
        }
    }
    Ok(())
}

/// A part that holds a drawing needs the drawing namespaces on its root.
fn story_parts_declare_drawing_namespaces() -> Test {
    let files = edgedoc_demos::report::render().map_err(err)?;
    let file = files
        .iter()
        .find(|file| file.name == "report.docx")
        .ok_or("no report")?;
    let package = edgedoc_opc::read_zip(&file.bytes, &edgedoc::Budget::default()).map_err(err)?;
    let mut seen = 0usize;
    for part in package.parts.values() {
        if !(part.name.starts_with("word/header")
            || part.name.starts_with("word/footer")
            || part.name.ends_with("notes.xml"))
        {
            continue;
        }
        seen += 1;
        let text = String::from_utf8_lossy(&part.bytes).into_owned();
        for prefix in [
            "xmlns:w=",
            "xmlns:r=",
            "xmlns:wp=",
            "xmlns:a=",
            "xmlns:pic=",
        ] {
            if !text.contains(prefix) {
                return fail(format!("{} lacks {prefix}", part.name));
            }
        }
    }
    if seen == 0 {
        return fail("the document has no story part");
    }
    Ok(())
}

/// The header and footer parts appear once each, however many sections point at them.
fn one_part_per_story() -> Test {
    let files = edgedoc_demos::report::render().map_err(err)?;
    let file = files
        .iter()
        .find(|file| file.name == "report.docx")
        .ok_or("no report")?;
    let package = edgedoc_opc::read_zip(&file.bytes, &edgedoc::Budget::default()).map_err(err)?;
    let mut names: Vec<String> = package
        .parts
        .keys()
        .filter(|name| name.starts_with("word/header") || name.starts_with("word/footer"))
        .cloned()
        .collect();
    let before = names.len();
    names.sort();
    names.dedup();
    if names.len() != before {
        return fail("a story part is written twice");
    }
    Ok(())
}

/// The merged report gives every region its own footer, so the field is not shared.
fn each_section_has_its_own_footer() -> Test {
    let files = edgedoc_demos::report::render().map_err(err)?;
    let file = files
        .iter()
        .find(|file| file.name == "report.docx")
        .ok_or("no report")?;
    let doc = Document::read_bytes(&file.bytes).map_err(err)?;
    let mut named = 0usize;
    for id in doc.parts(PartKind::Footer) {
        let text = doc.plain_text(StoryRef::Part(id));
        if text.contains("Region: ") && !text.contains('«') {
            named += 1;
        }
    }
    if named < 5 {
        return fail(format!("{named} footers name a region"));
    }
    Ok(())
}

/// A merge must not leave a field result in the saved file.
fn no_field_survives_a_merge() -> Test {
    let template = Document::builder()
        .paragraph(Paragraph::new().text("Invoice ").merge_field("Number"))
        .paragraph(Paragraph::new().merge_field("Name"))
        .build()
        .map_err(err)?
        .write_bytes()
        .map_err(err)?;
    let records =
        Records::from_csv(b"Number,Name\nINV-1,Ada\n", &CsvOptions::default()).map_err(err)?;
    let merged = Document::read_bytes(&template)
        .map_err(err)?
        .merge()
        .records(records)
        .missing(edgedoc::MissingField::Error)
        .finish()
        .map_err(err)?
        .document(0)
        .ok_or("the merge produced no document")?
        .write_bytes()
        .map_err(err)?;
    let text = part_of(&merged, "word/document.xml")?;
    if text.contains("MERGEFIELD") || text.contains('«') {
        return fail("a merge field is still in the output");
    }
    Ok(())
}

type Case = (&'static str, fn() -> Test);

fn cases() -> Vec<Case> {
    vec![
        (
            "child-order-follows-the-schema",
            child_order_follows_the_schema,
        ),
        (
            "numbering-exists-when-a-list-is-used",
            numbering_exists_when_a_list_is_used,
        ),
        ("every-used-style-is-defined", every_used_style_is_defined),
        ("demo-styles-are-defined", demo_styles_are_defined),
        ("media-parts-are-unique", media_parts_are_unique),
        (
            "story-parts-declare-drawing-namespaces",
            story_parts_declare_drawing_namespaces,
        ),
        ("one-part-per-story", one_part_per_story),
        (
            "each-section-has-its-own-footer",
            each_section_has_its_own_footer,
        ),
        ("no-field-survives-a-merge", no_field_survives_a_merge),
    ]
}

#[test]
fn word_reader_rules() -> Test {
    for (name, case) in cases() {
        case().map_err(|message| format!("{name}: {message}"))?;
    }
    Ok(())
}
