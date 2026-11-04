//! The rules Word enforces that a well-formed-XML check does not catch.
//!
//! Word rejects a package that is valid XML and passes every ZIP check. The usual
//! cause is a missing mandatory child element, which the ECMA-376 schema marks with
//! `minOccurs` of one. This file encodes the rules that a reader such as WordCraft
//! tolerates but Word refuses, so the build fails before a user sees the error.
//!
//! Each case names the schema type it enforces. The check reads the bytes that a
//! document writes and walks the elements in schema order.

use edgedoc::{Budget, Document, Emu, Mm, Paragraph, Picture, StoryRef};
use edgedoc_opc::read_zip;

type Test = Result<(), String>;

fn err(err: impl std::fmt::Display) -> String {
    err.to_string()
}

fn fail(message: impl Into<String>) -> Test {
    Err(message.into())
}

/// Every occurrence of an element, as the text from its start tag to its end tag.
fn elements<'a>(xml: &'a str, name: &str) -> Vec<&'a str> {
    let open = format!("<{name}");
    let close = format!("</{name}>");
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(start) = rest.find(&open) {
        let after = &rest[start..];
        // A self-closing element has no children, so skip it.
        let Some(tag_end) = after.find('>') else {
            break;
        };
        if after[..tag_end].ends_with('/') {
            out.push(&after[..tag_end + 1]);
            rest = &after[tag_end + 1..];
            continue;
        }
        match after.find(&close) {
            Some(end) => {
                out.push(&after[..end + close.len()]);
                rest = &after[end + close.len()..];
            }
            None => break,
        }
    }
    out
}

/// The direct-child element names of a container, in order.
fn children(xml: &str) -> Vec<String> {
    // Start after the container's own start tag.
    let Some(open) = xml.find('>') else {
        return Vec::new();
    };
    let body = &xml[open + 1..];
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut index = 0usize;
    while let Some(start) = body[index..].find('<') {
        let at = index + start;
        let Some(close) = body[at..].find('>') else {
            break;
        };
        let end = at + close;
        let tag = &body[at + 1..end];
        index = end + 1;
        if tag.starts_with('/') {
            depth = depth.saturating_sub(1);
            continue;
        }
        if tag.starts_with('?') || tag.starts_with('!') {
            continue;
        }
        let self_closing = tag.ends_with('/');
        if depth == 0 {
            let raw = tag
                .trim_end_matches('/')
                .split_whitespace()
                .next()
                .unwrap_or("");
            out.push(raw.to_string());
        }
        if !self_closing {
            depth += 1;
        }
    }
    out
}

fn local(name: &str) -> &str {
    name.rsplit(':').next().unwrap_or(name)
}

/// A document with one inline picture and one floating picture.
fn with_pictures() -> Result<Vec<u8>, String> {
    Document::builder()
        .paragraph(
            Paragraph::new().picture(
                Picture::png(b"INLINE-PNG".to_vec())
                    .size(Emu(914_400), Emu(457_200))
                    .alt("Inline logo"),
            ),
        )
        .paragraph(
            Paragraph::new().picture(
                Picture::jpeg(b"FLOAT-JPEG".to_vec())
                    .floating(edgedoc::Anchor::inline_text_distance().behind_text())
                    .alt("Floating mark"),
            ),
        )
        .build()
        .map_err(err)?
        .write_bytes()
        .map_err(err)
}

fn document_part(bytes: &[u8]) -> Result<String, String> {
    let package = read_zip(bytes, &Budget::default()).map_err(err)?;
    package
        .part("word/document.xml")
        .map(|part| String::from_utf8_lossy(&part.bytes).into_owned())
        .ok_or_else(|| "no document part".to_string())
}

/// CT_Inline: `extent`, `docPr`, and `graphic` are required, in that order.
///
/// A file without `wp:docPr` opens in a lenient reader and fails in Word with
/// "Word experienced an error trying to open the file".
fn inline_has_the_required_children() -> Test {
    let text = document_part(&with_pictures()?)?;
    let found = elements(&text, "wp:inline");
    if found.is_empty() {
        return fail("the document has no inline drawing");
    }
    for body in found {
        let names: Vec<String> = children(body)
            .iter()
            .map(|name| local(name).to_string())
            .collect();
        for required in ["extent", "docPr", "graphic"] {
            if !names.iter().any(|have| have == required) {
                return fail(format!("wp:inline lacks {required}: got {names:?}"));
            }
        }
        // The schema sequence puts extent before docPr, and docPr before graphic.
        let position = |name: &str| {
            names
                .iter()
                .position(|have| have == name)
                .unwrap_or(usize::MAX)
        };
        if !(position("extent") < position("docPr") && position("docPr") < position("graphic")) {
            return fail(format!("wp:inline child order is wrong: {names:?}"));
        }
    }
    Ok(())
}

/// CT_Anchor: the same required children as CT_Inline, plus `simplePos` and `extent`.
fn anchor_has_the_required_children() -> Test {
    let text = document_part(&with_pictures()?)?;
    let found = elements(&text, "wp:anchor");
    if found.is_empty() {
        return fail("the document has no floating drawing");
    }
    for body in found {
        let names: Vec<String> = children(body)
            .iter()
            .map(|name| local(name).to_string())
            .collect();
        for required in ["simplePos", "extent", "docPr", "graphic"] {
            if !names.iter().any(|have| have == required) {
                return fail(format!("wp:anchor lacks {required}: got {names:?}"));
            }
        }
    }
    Ok(())
}

/// `wp:docPr/@id` must be unique across the package. Word repairs a duplicate.
fn drawing_ids_are_unique() -> Test {
    let bytes = with_pictures()?;
    let package = read_zip(&bytes, &Budget::default()).map_err(err)?;
    let mut ids: Vec<String> = Vec::new();
    for part in package.parts.values() {
        if !part.name.ends_with(".xml") {
            continue;
        }
        let text = String::from_utf8_lossy(&part.bytes).into_owned();
        for body in elements(&text, "wp:docPr") {
            if let Some(start) = body.find("id=\"") {
                let after = &body[start + 4..];
                if let Some(end) = after.find('"') {
                    ids.push(after[..end].to_string());
                }
            }
        }
    }
    if ids.is_empty() {
        return fail("the package records no drawing id");
    }
    let mut sorted = ids.clone();
    sorted.sort();
    sorted.dedup();
    if sorted.len() != ids.len() {
        return fail(format!("a drawing id repeats: {ids:?}"));
    }
    Ok(())
}

/// A picture needs `a:blip`, `pic:spPr`, and `a:prstGeom` for Word to draw it.
fn picture_graphic_has_the_required_children() -> Test {
    let text = document_part(&with_pictures()?)?;
    for body in elements(&text, "pic:pic") {
        for required in ["pic:nvPicPr", "pic:blipFill", "pic:spPr"] {
            if !body.contains(&format!("<{required}")) {
                return fail(format!("pic:pic lacks {required}"));
            }
        }
        if !body.contains("<a:blip ") {
            return fail("pic:blipFill lacks a:blip");
        }
        if !body.contains("<a:prstGeom ") {
            return fail("pic:spPr lacks a:prstGeom");
        }
    }
    Ok(())
}

/// Every part name appears once, and every relationship target exists.
fn package_is_consistent() -> Test {
    let bytes = with_pictures()?;
    let package = read_zip(&bytes, &Budget::default()).map_err(err)?;
    let names: Vec<String> = package.parts.keys().cloned().collect();
    let mut sorted = names.clone();
    sorted.sort();
    sorted.dedup();
    if sorted.len() != names.len() {
        return fail("a part name repeats");
    }
    for (name, part) in &package.parts {
        if !name.ends_with(".rels") {
            continue;
        }
        let rels = edgedoc_opc::read_rels(&part.bytes, &Budget::default()).map_err(err)?;
        for rel in rels {
            if rel.external {
                continue;
            }
            let target = if rel.target.starts_with('/') {
                rel.target.trim_start_matches('/').to_string()
            } else {
                let dir = name.rsplit_once('/').map(|(dir, _)| dir).unwrap_or("");
                let dir = dir.trim_end_matches("_rels").trim_end_matches('/');
                if dir.is_empty() {
                    rel.target.clone()
                } else {
                    format!("{dir}/{}", rel.target)
                }
            };
            if package.part(&target).is_none() {
                return fail(format!("{name} points at missing {target}"));
            }
        }
    }
    Ok(())
}

/// The core properties part holds the elements that its content type declares.
fn core_properties_are_well_formed() -> Test {
    let doc = Document::builder()
        .core(edgedoc::CoreProps::new().title("Title").creator("Author"))
        .paragraph(Paragraph::new().text("Body"))
        .build()
        .map_err(err)?;
    let bytes = doc.write_bytes().map_err(err)?;
    let package = read_zip(&bytes, &Budget::default()).map_err(err)?;
    let core = package
        .part("docProps/core.xml")
        .map(|part| String::from_utf8_lossy(&part.bytes).into_owned())
        .ok_or("no core properties part")?;
    for required in ["cp:coreProperties", "dc:title"] {
        if !core.contains(required) {
            return fail(format!("core properties lack {required}"));
        }
    }
    Ok(())
}

/// A header with a picture carries its own relationship part and its own namespaces.
fn header_picture_is_complete() -> Test {
    let doc = Document::builder()
        .header(vec![edgedoc::Block::Paragraph(
            Paragraph::new().picture(Picture::png(b"HEADER-PNG".to_vec())),
        )])
        .paragraph(Paragraph::new().text("Body"))
        .build()
        .map_err(err)?;
    let bytes = doc.write_bytes().map_err(err)?;
    let package = read_zip(&bytes, &Budget::default()).map_err(err)?;
    let header = package
        .parts
        .values()
        .find(|part| part.name.starts_with("word/header"))
        .map(|part| String::from_utf8_lossy(&part.bytes).into_owned())
        .ok_or("no header part")?;
    for prefix in [
        "xmlns:w=",
        "xmlns:r=",
        "xmlns:wp=",
        "xmlns:a=",
        "xmlns:pic=",
    ] {
        if !header.contains(prefix) {
            return fail(format!("the header root lacks {prefix}"));
        }
    }
    if !header.contains("<wp:docPr ") {
        return fail("the header drawing lacks wp:docPr");
    }
    Ok(())
}

/// A table cell ends with a paragraph, which the schema requires.
fn table_cell_ends_with_a_paragraph() -> Test {
    let mut table = edgedoc::Table::new().widths([Mm(60.0), Mm(60.0)]);
    table.rows.push(edgedoc::Row::cells([
        edgedoc::Cell::paragraph("A"),
        edgedoc::Cell::paragraph("B"),
    ]));
    let doc = Document::builder().table(table).build().map_err(err)?;
    let text = document_part(&doc.write_bytes().map_err(err)?)?;
    for cell in elements(&text, "w:tc") {
        let names: Vec<String> = children(cell)
            .iter()
            .map(|name| local(name).to_string())
            .collect();
        if names.last().map(String::as_str) != Some("p") {
            return fail(format!("a table cell ends with {names:?}"));
        }
    }
    Ok(())
}

/// A merge that fills a table produces the same structural rules as a new document.
fn a_merged_document_is_valid() -> Test {
    use edgedoc::{Cell, CsvOptions, MergeKind, MissingField, Records, Row, Table};
    use std::sync::Arc;

    let mut table = Table::new()
        .widths([Mm(50.0), Mm(50.0)])
        .header(["Name", "City"]);
    table.rows.push(Row::cells([
        Cell::paragraph("").blocks(vec![Arc::new(edgedoc::Block::Paragraph(
            Paragraph::new().merge_field("Name"),
        ))]),
        Cell::paragraph("").blocks(vec![Arc::new(edgedoc::Block::Paragraph(
            Paragraph::new().merge_field("City"),
        ))]),
    ]));
    let doc = Document::builder()
        .paragraph(Paragraph::new().picture(Picture::png(b"MERGED-PNG".to_vec())))
        .table(table)
        .build()
        .map_err(err)?;
    let records = Records::from_csv(
        b"Name,City\nAda,Leeds\nGrace,Oslo\n",
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
    let text = document_part(&merged.write_bytes().map_err(err)?)?;
    for body in elements(&text, "wp:inline") {
        if !body.contains("<wp:docPr ") {
            return fail("the merged drawing lacks wp:docPr");
        }
    }
    if !merged.plain_text(StoryRef::Body).contains("Ada") {
        return fail("the merge lost a record");
    }
    Ok(())
}

/// The check must fail on a file that lacks `wp:docPr`, which is the defect that made
/// Word refuse a document while a lenient reader accepted it.
fn the_check_catches_a_missing_doc_pr() -> Test {
    let bytes = with_pictures()?;
    let package = read_zip(&bytes, &Budget::default()).map_err(err)?;
    let document = package
        .part("word/document.xml")
        .map(|part| part.bytes.clone())
        .ok_or("no document part")?;
    let text = String::from_utf8_lossy(&document).into_owned();
    if !text.contains("<wp:docPr ") {
        return fail("the fixture has no docPr to remove");
    }
    // Remove the element, as the old writer did.
    let mut stripped = String::new();
    let mut rest = text.as_str();
    while let Some(start) = rest.find("<wp:docPr ") {
        stripped.push_str(&rest[..start]);
        let after = &rest[start..];
        match after.find("/>") {
            Some(end) => rest = &after[end + 2..],
            None => return fail("a docPr is not self-closing"),
        }
    }
    stripped.push_str(rest);
    if stripped.contains("<wp:docPr ") {
        return fail("the strip did not remove docPr");
    }
    // The rule reads a whole package, so rebuild it with the stripped part.
    let mut parts: Vec<(String, Vec<u8>)> = Vec::new();
    for part in package.parts.values() {
        let bytes = if part.name.eq_ignore_ascii_case("word/document.xml") {
            stripped.clone().into_bytes()
        } else {
            part.bytes.clone()
        };
        parts.push((part.name.clone(), bytes));
    }
    let broken = edgedoc_opc::write_zip(&parts).map_err(err)?;
    let broken_text = document_part(&broken)?;
    let found = elements(&broken_text, "wp:inline");
    if found.is_empty() {
        return fail("the broken package has no inline drawing");
    }
    let has_doc_pr = found.iter().any(|body| body.contains("<wp:docPr "));
    if has_doc_pr {
        return fail("the check did not notice the missing docPr");
    }
    Ok(())
}

type Case = (&'static str, fn() -> Test);

fn cases() -> Vec<Case> {
    vec![
        ("inline-required-children", inline_has_the_required_children),
        ("anchor-required-children", anchor_has_the_required_children),
        ("drawing-ids-unique", drawing_ids_are_unique),
        (
            "picture-graphic-children",
            picture_graphic_has_the_required_children,
        ),
        ("package-consistent", package_is_consistent),
        ("core-properties", core_properties_are_well_formed),
        ("header-picture", header_picture_is_complete),
        ("table-cell-ends-with-p", table_cell_ends_with_a_paragraph),
        ("merged-document-valid", a_merged_document_is_valid),
        ("catches-missing-doc-pr", the_check_catches_a_missing_doc_pr),
    ]
}

#[test]
fn word_schema_rules() -> Test {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/13-word-validation.md");
    let doc = std::fs::read_to_string(path).map_err(err)?;
    let cases = cases();

    for (id, _) in &cases {
        if !doc.contains(id) {
            return fail(format!("docs/13-word-validation.md has no row for {id}"));
        }
    }
    // Only the section that holds the case table. A later table has its own columns.
    let section = doc
        .split("## Structural rules under test")
        .nth(1)
        .and_then(|rest| rest.split("## ").next())
        .ok_or("docs/13-word-validation.md has no case table")?;
    let mut seen = 0usize;
    for line in section.lines() {
        let cells: Vec<&str> = line.split('|').map(str::trim).collect();
        if cells.len() < 4 {
            continue;
        }
        let id = cells[3].trim_matches('`');
        if id.is_empty() || id == "Test" || id.chars().all(|ch| ch == '-') {
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

    for (name, case) in cases {
        case().map_err(|message| format!("{name}: {message}"))?;
    }
    Ok(())
}
