//! Format round-trip checks. Every `ED-W-*` id in `docs/12-word-features.md` has a case here.

use edgedoc::{
    Anchor, Block, Borders, Cell, CharProps, ContentControl, CoreProps, Document, EditOptions, Emu,
    LineBorder, ListKind, Mm, ParaProps, Paragraph, PartKind, Picture, Pt, RunBuilder, Style,
    StyleKind, TabStop, Table, TablePosition, Twips,
};
use edgedoc_opc::write_zip;
use edgedoc_unit::IntoTwips;

type Test = Result<(), String>;

fn err(err: impl std::fmt::Display) -> String {
    err.to_string()
}

fn fail(message: impl Into<String>) -> Test {
    Err(message.into())
}

fn body(inner: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture"><w:body>{inner}</w:body></w:document>"#
    )
}

fn rels(inner: &str) -> String {
    format!(
        r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">{inner}</Relationships>"#
    )
}

fn office(document: &str, rels_xml: &str, extra: &[(&str, &[u8])]) -> Result<Vec<u8>, String> {
    let mut types = String::from(
        r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Default Extension="png" ContentType="image/png"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>"#,
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
        ("word/_rels/document.xml.rels".into(), rels_xml.as_bytes().to_vec()),
    ];
    for (name, bytes) in extra {
        parts.push(((*name).into(), bytes.to_vec()));
    }
    write_zip(&parts).map_err(err)
}

fn part(bytes: &[u8], name: &str) -> Result<String, String> {
    let package = edgedoc_opc::read_zip(bytes, &edgedoc::Budget::default()).map_err(err)?;
    package
        .part(name)
        .map(|part| String::from_utf8_lossy(&part.bytes).into_owned())
        .ok_or_else(|| format!("missing part {name}"))
}

fn parts_text(bytes: &[u8]) -> Result<String, String> {
    let package = edgedoc_opc::read_zip(bytes, &edgedoc::Budget::default()).map_err(err)?;
    Ok(package
        .parts
        .values()
        .map(|part| String::from_utf8_lossy(&part.bytes).into_owned())
        .collect::<Vec<_>>()
        .join("\n"))
}

fn read(bytes: &[u8]) -> Result<Document, String> {
    Document::read_bytes(bytes).map_err(err)
}

/// Save a document that was read, after one edit, so that the writer runs.
fn save(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut doc = read(bytes)?;
    doc.edit(
        EditOptions::default().allow(edgedoc::DropKind::Objects),
        |edit| {
            edit.body().paragraph_after_last(|para| para.text("Tail"))?;
            Ok(())
        },
    )
    .map_err(err)?;
    doc.write_bytes().map_err(err)
}

fn has(text: &str, needle: &str) -> Test {
    if text.contains(needle) {
        Ok(())
    } else {
        Err(format!("missing {needle}"))
    }
}

/// The paragraph properties must survive the save, not only appear as text.
fn same_paragraph(before: &[u8], after: &[u8], index: usize) -> Test {
    let want = read(before)?
        .paragraph_props(index)
        .cloned()
        .ok_or("the input has no paragraph properties")?;
    let got = read(after)?
        .paragraph_props(index)
        .cloned()
        .ok_or("the output has no paragraph properties")?;
    if want != got {
        return Err(format!("before {want:?}\nafter {got:?}"));
    }
    Ok(())
}

fn same_run(before: &[u8], after: &[u8], block: usize, run: usize) -> Test {
    let want = read(before)?
        .run_props(block, run)
        .cloned()
        .ok_or("the input has no run properties")?;
    let got = read(after)?
        .run_props(block, run)
        .cloned()
        .ok_or("the output has no run properties")?;
    if want != got {
        return Err(format!("before {want:?}\nafter {got:?}"));
    }
    Ok(())
}

fn same_section(before: &[u8], after: &[u8]) -> Test {
    let want = read(before)?.section().clone();
    let got = read(after)?.section().clone();
    if want != got {
        return Err(format!("before {want:?}\nafter {got:?}"));
    }
    Ok(())
}

fn same_table(before: &[u8], after: &[u8], index: usize) -> Test {
    let want = read(before)?
        .table(index)
        .cloned()
        .ok_or("the input has no table")?;
    let got = read(after)?
        .table(index)
        .cloned()
        .ok_or("the output has no table")?;
    if want != got {
        return Err(format!("before {want:?}\nafter {got:?}"));
    }
    Ok(())
}

fn sample(inner: &str) -> Result<Vec<u8>, String> {
    office(&body(inner), &rels(""), &[])
}

// ---------------------------------------------------------------- character formatting

fn char_extended() -> Test {
    let inner = r#"<w:p><w:r><w:rPr><w:caps/><w:smallCaps/><w:emboss/><w:imprint/><w:outline/><w:shadow/><w:noProof/><w:spacing w:val="40"/><w:kern w:val="28"/><w:position w:val="6"/><w:w w:val="120"/><w:snapToGrid w:val="0"/><w:bCs/><w:iCs/><w:u w:val="double" w:color="FF0000"/><w:rStyle w:val="Strong"/><w:bdr w:val="single" w:sz="8" w:color="00FF00" w:space="2"/></w:rPr><w:t>Caps</w:t></w:r></w:p>"#;
    let bytes = sample(inner)?;
    let out = save(&bytes)?;
    let document = part(&out, "word/document.xml")?;
    for needle in [
        "<w:caps/>",
        "<w:smallCaps/>",
        "<w:emboss/>",
        "<w:imprint/>",
        "<w:outline/>",
        "<w:shadow/>",
        "<w:noProof/>",
        r#"<w:spacing w:val="40"/>"#,
        r#"<w:kern w:val="28"/>"#,
        r#"<w:position w:val="6"/>"#,
        r#"<w:w w:val="120"/>"#,
        r#"<w:snapToGrid w:val="0"/>"#,
        "<w:bCs/>",
        "<w:iCs/>",
        r#"<w:u w:val="double" w:color="FF0000"/>"#,
        r#"<w:rStyle w:val="Strong"/>"#,
        r#"<w:bdr w:val="single" w:sz="8" w:color="00FF00" w:space="2"/>"#,
    ] {
        has(&document, needle)?;
    }
    same_run(&bytes, &out, 0, 0)
}

fn char_unknown_kept() -> Test {
    let inner = r#"<w:p><w:r><w:rPr><w:b/><w:eastAsianLayout w:id="1"/><w:effect w:val="lights"/></w:rPr><w:t>X</w:t></w:r></w:p>"#;
    let out = save(&sample(inner)?)?;
    let document = part(&out, "word/document.xml")?;
    has(&document, "<w:eastAsianLayout")?;
    has(&document, "<w:effect")
}

fn paragraph_extended() -> Test {
    let inner = r#"<w:p><w:pPr><w:pBdr><w:top w:val="single" w:sz="4" w:space="1" w:color="auto"/></w:pBdr><w:tabs><w:tab w:val="right" w:leader="dot" w:pos="8640"/></w:tabs><w:contextualSpacing/><w:suppressLineNumbers/><w:ind w:left="720" w:leftChars="200" w:hanging="360" w:hangingChars="100"/><w:spacing w:before="120" w:after="240" w:line="360" w:lineRule="auto"/><w:framePr w:dropCap="drop" w:lines="3" w:wrap="around" w:hAnchor="text"/><w:jc w:val="both"/></w:pPr><w:r><w:t>Body</w:t></w:r></w:p>"#;
    let bytes = sample(inner)?;
    let out = save(&bytes)?;
    let document = part(&out, "word/document.xml")?;
    for needle in [
        r#"<w:top w:val="single" w:sz="4" w:space="1" w:color="auto"/>"#,
        r#"<w:tab w:val="right" w:pos="8640" w:leader="dot"/>"#,
        "<w:contextualSpacing/>",
        "<w:suppressLineNumbers/>",
        r#"w:leftChars="200""#,
        r#"w:hangingChars="100""#,
        r#"w:hanging="360""#,
        r#"w:before="120""#,
        r#"w:after="240""#,
        r#"w:lineRule="auto""#,
        r#"<w:framePr w:dropCap="drop" w:lines="3" w:wrap="around" w:hAnchor="text"/>"#,
        r#"<w:jc w:val="both"/>"#,
    ] {
        has(&document, needle)?;
    }
    same_paragraph(&bytes, &out, 0)
}

fn list_generated() -> Test {
    let doc = Document::builder()
        .paragraph(Paragraph::new().text("First").list(ListKind::Decimal, 0))
        .paragraph(Paragraph::new().text("Nested").list(ListKind::Decimal, 1))
        .paragraph(Paragraph::new().text("Bullet").list(ListKind::Bullet, 0))
        .build()
        .map_err(err)?;
    if doc.list_kind(0) != Some(ListKind::Decimal) || doc.list_kind(2) != Some(ListKind::Bullet) {
        return fail("the list kind is not on the model");
    }
    let out = doc.write_bytes().map_err(err)?;
    let numbering = part(&out, "word/numbering.xml")?;
    has(&numbering, r#"<w:numFmt w:val="decimal"/>"#)?;
    has(&numbering, r#"<w:numFmt w:val="bullet"/>"#)?;
    has(&numbering, r#"<w:lvlText w:val="%2."/>"#)?;
    let document = part(&out, "word/document.xml")?;
    has(
        &document,
        r#"<w:numPr><w:ilvl w:val="1"/><w:numId w:val="2"/></w:numPr>"#,
    )?;
    let reread = read(&out)?;
    if reread.plain_text(edgedoc::StoryRef::Body) != "First\nNested\nBullet" {
        return fail(reread.plain_text(edgedoc::StoryRef::Body));
    }
    Ok(())
}

fn list_generated_keeps_existing() -> Test {
    let numbering = br#"<?xml version="1.0"?><w:numbering xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:abstractNum w:abstractNumId="1"><w:lvl w:ilvl="0"><w:numFmt w:val="upperRoman"/><w:lvlText w:val="%1)"/></w:lvl></w:abstractNum><w:num w:numId="7"><w:abstractNumId w:val="1"/></w:num></w:numbering>"#;
    let bytes = office(
        &body(
            r#"<w:p><w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="7"/></w:numPr></w:pPr><w:r><w:t>Kept</w:t></w:r></w:p>"#,
        ),
        &rels(
            r#"<Relationship Id="rIdN" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering" Target="numbering.xml"/>"#,
        ),
        &[("word/numbering.xml", numbering)],
    )?;
    let mut doc = read(&bytes)?;
    doc.edit(EditOptions::default(), |edit| {
        edit.body()
            .push_paragraph(Paragraph::new().text("New").list(ListKind::LowerRoman, 0))
    })
    .map_err(err)?;
    let out = doc.write_bytes().map_err(err)?;
    let merged = part(&out, "word/numbering.xml")?;
    has(&merged, r#"<w:numFmt w:val="upperRoman"/>"#)?;
    has(&merged, r#"w:numId="7""#)?;
    has(&merged, r#"<w:numFmt w:val="lowerRoman"/>"#)?;
    // The new definition must not reuse the kept id, or the kept list changes.
    has(&merged, r#"<w:num w:numId="8">"#)?;
    has(&merged, r#"<w:abstractNum w:abstractNumId="2">"#)?;
    has(&merged, "</w:numbering>")
}

fn list_kept_numbering() -> Test {
    let numbering = br#"<?xml version="1.0"?><w:numbering xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:abstractNum w:abstractNumId="1"><w:lvl w:ilvl="0"><w:numFmt w:val="upperRoman"/><w:lvlText w:val="%1)"/></w:lvl></w:abstractNum><w:num w:numId="7"><w:abstractNumId w:val="1"/></w:num></w:numbering>"#;
    let bytes = office(
        &body(
            r#"<w:p><w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="7"/></w:numPr></w:pPr><w:r><w:t>Item</w:t></w:r></w:p>"#,
        ),
        &rels(
            r#"<Relationship Id="rIdN" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering" Target="numbering.xml"/>"#,
        ),
        &[("word/numbering.xml", numbering)],
    )?;
    let out = save(&bytes)?;
    let kept = part(&out, "word/numbering.xml")?;
    has(&kept, r#"<w:numFmt w:val="upperRoman"/>"#)?;
    has(&kept, r#"w:numId="7""#)
}

// ---------------------------------------------------------------- sections

fn section_extended() -> Test {
    let inner = r#"<w:p><w:r><w:t>Body</w:t></w:r></w:p><w:sectPr><w:pgBorders><w:top w:val="single" w:sz="6" w:space="24" w:color="000000"/></w:pgBorders><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1134" w:right="1134" w:bottom="1134" w:left="1134" w:header="426" w:footer="510" w:gutter="240"/><w:cols w:num="2" w:space="708" w:sep="1"/><w:titlePg/><w:lnNumType w:countBy="5" w:start="2" w:restart="continuous" w:distance="240"/><w:docGrid w:type="lines" w:linePitch="360"/><w:footnotePr><w:pos w:val="beneathText"/><w:numFmt w:val="lowerRoman"/><w:numStart w:val="3"/></w:footnotePr><w:bidi/><w:rtlGutter/><w:textDirection w:val="tbRl"/><w:type w:val="continuous"/></w:sectPr>"#;
    let bytes = sample(inner)?;
    let out = save(&bytes)?;
    let document = part(&out, "word/document.xml")?;
    for needle in [
        r#"<w:top w:val="single" w:sz="6" w:space="24" w:color="000000"/>"#,
        r#"w:header="426""#,
        r#"w:footer="510""#,
        r#"w:gutter="240""#,
        r#"<w:cols w:num="2" w:space="708" w:sep="1"/>"#,
        "<w:titlePg/>",
        r#"<w:lnNumType w:countBy="5" w:start="2" w:restart="continuous" w:distance="240"/>"#,
        r#"<w:docGrid w:type="lines" w:linePitch="360"/>"#,
        r#"<w:pos w:val="beneathText"/>"#,
        r#"<w:numFmt w:val="lowerRoman"/>"#,
        r#"<w:numStart w:val="3"/>"#,
        "<w:bidi/>",
        "<w:rtlGutter/>",
        r#"<w:textDirection w:val="tbRl"/>"#,
        r#"<w:type w:val="continuous"/>"#,
    ] {
        has(&document, needle)?;
    }
    let doc = read(&out)?;
    let section = doc.section();
    if section.header_dist != Twips(426) || section.columns != 2 || !section.title_page {
        return fail(format!("{section:?}"));
    }
    if section.line_numbering.as_ref().map(|item| item.count_by) != Some(5) {
        return fail("the line numbering is not on the model");
    }
    same_section(&bytes, &out)
}

fn section_builder() -> Test {
    let doc = Document::builder()
        .page(
            edgedoc::Page::a4()
                .landscape()
                .columns(2)
                .title_page()
                .header(Mm(15.0))
                .gutter(Mm(10.0)),
        )
        .paragraph(Paragraph::new().text("Wide"))
        .build()
        .map_err(err)?;
    let section = doc.section();
    if !section.landscape || section.columns != 2 || !section.title_page {
        return fail(format!("{section:?}"));
    }
    let out = doc.write_bytes().map_err(err)?;
    let document = part(&out, "word/document.xml")?;
    has(&document, r#"w:orient="landscape""#)?;
    has(&document, r#"<w:cols w:num="2"/>"#)?;
    has(&document, "<w:titlePg/>")?;
    let reread = read(&out)?;
    if !reread.section().landscape || reread.section().columns != 2 {
        return fail(format!("{:?}", reread.section()));
    }
    Ok(())
}

fn section_mid_document() -> Test {
    let inner = r#"<w:p><w:r><w:t>First</w:t></w:r></w:p><w:p><w:pPr><w:sectPr><w:pgSz w:w="16838" w:h="11906" w:orient="landscape"/><w:pgMar w:top="720" w:right="720" w:bottom="720" w:left="720" w:header="708" w:footer="708" w:gutter="0"/><w:cols w:num="2"/><w:type w:val="continuous"/></w:sectPr></w:pPr><w:r><w:t>Second</w:t></w:r></w:p>"#;
    let bytes = sample(inner)?;
    let doc = read(&bytes)?;
    if doc.section_break_count() != 1 {
        return fail(format!("{} section breaks", doc.section_break_count()));
    }
    let out = save(&bytes)?;
    let document = part(&out, "word/document.xml")?;
    has(
        &document,
        r#"<w:pgSz w:w="16838" w:h="11906" w:orient="landscape"/>"#,
    )?;
    has(&document, r#"<w:cols w:num="2"/>"#)?;
    has(&document, r#"<w:type w:val="continuous"/>"#)?;
    let reread = read(&out)?;
    if reread.section_break_count() != 1 {
        return fail("the section break was lost");
    }
    let mid = read(&out)?.section().clone();
    if mid.columns != 1 {
        return fail(format!("the last section changed: {mid:?}"));
    }
    Ok(())
}

// ---------------------------------------------------------------- tables

fn table_extended() -> Test {
    let inner = r#"<w:tbl><w:tblPr><w:tblStyle w:val="TableGrid"/><w:jc w:val="center"/><w:tblInd w:w="240" w:type="dxa"/><w:tblLayout w:type="fixed"/><w:tblCellSpacing w:w="20" w:type="dxa"/><w:tblOverlap w:val="never"/><w:tblpPr w:leftFromText="180" w:rightFromText="180" w:vertAnchor="text" w:horzAnchor="margin" w:tblpX="100" w:tblpY="200"/><w:bidiVisual/><w:tblBorders><w:top w:val="single" w:sz="4" w:color="auto"/></w:tblBorders><w:tblCellMar><w:top w:w="57" w:type="dxa"/></w:tblCellMar><w:tblLook w:val="04A0"/></w:tblPr><w:tblGrid><w:gridCol w:w="2000"/><w:gridCol w:w="2000"/></w:tblGrid><w:tr><w:trPr><w:cantSplit/><w:tblHeader/><w:trHeight w:val="300" w:hRule="exact"/></w:trPr><w:tc><w:tcPr><w:tcW w:w="4000" w:type="dxa"/><w:gridSpan w:val="2"/><w:vMerge w:val="restart"/><w:hMerge w:val="restart"/><w:tcBorders><w:left w:val="double" w:sz="8" w:color="FF0000"/></w:tcBorders><w:shd w:val="clear" w:fill="EEEEEE"/><w:noWrap/><w:tcMar><w:top w:w="100" w:type="dxa"/></w:tcMar><w:textDirection w:val="btLr"/><w:vAlign w:val="center"/></w:tcPr><w:p><w:r><w:t>Cell</w:t></w:r></w:p></w:tc></w:tr></w:tbl>"#;
    let bytes = sample(inner)?;
    let out = save(&bytes)?;
    let document = part(&out, "word/document.xml")?;
    for needle in [
        r#"<w:tblStyle w:val="TableGrid"/>"#,
        r#"<w:jc w:val="center"/>"#,
        r#"<w:tblInd w:w="240" w:type="dxa"/>"#,
        r#"<w:tblLayout w:type="fixed"/>"#,
        r#"<w:tblCellSpacing w:w="20" w:type="dxa"/>"#,
        r#"<w:tblOverlap w:val="never"/>"#,
        r#"w:tblpX="100""#,
        r#"w:horzAnchor="margin""#,
        "<w:bidiVisual/>",
        r#"<w:top w:val="single" w:sz="4" w:color="auto"/>"#,
        r#"<w:top w:w="57" w:type="dxa"/>"#,
        r#"<w:tblLook w:val="04A0"/>"#,
        "<w:cantSplit/>",
        "<w:tblHeader/>",
        r#"<w:trHeight w:val="300" w:hRule="exact"/>"#,
        r#"<w:gridSpan w:val="2"/>"#,
        r#"<w:vMerge w:val="restart"/>"#,
        r#"<w:hMerge w:val="restart"/>"#,
        r#"<w:left w:val="double" w:sz="8" w:color="FF0000"/>"#,
        r#"w:fill="EEEEEE""#,
        "<w:noWrap/>",
        r#"<w:top w:w="100" w:type="dxa"/>"#,
        r#"<w:textDirection w:val="btLr"/>"#,
        r#"<w:vAlign w:val="center"/>"#,
    ] {
        has(&document, needle)?;
    }
    let doc = read(&out)?;
    let table = doc.table(0).ok_or("the table is missing")?;
    if table.style.as_deref() != Some("TableGrid") || !table.bidi_visual {
        return fail(format!("{table:?}"));
    }
    same_table(&bytes, &out, 0)
}

fn table_builder() -> Test {
    let table = Table::new()
        .style("TableGrid")
        .widths([Mm(60.0), Mm(60.0)])
        .header(["Name", "Total"])
        .row(["Ada", "42"])
        .align("center")
        .floating(TablePosition {
            tblp_x: Some(1440),
            h_anchor: Some("margin".into()),
            ..TablePosition::default()
        });
    let doc = Document::builder().table(table).build().map_err(err)?;
    let out = doc.write_bytes().map_err(err)?;
    let document = part(&out, "word/document.xml")?;
    has(&document, r#"<w:tblStyle w:val="TableGrid"/>"#)?;
    has(&document, "<w:tblHeader/>")?;
    has(&document, r#"w:tblpX="1440""#)?;
    let reread = read(&out)?;
    if reread.plain_text(edgedoc::StoryRef::Body) != "Name\tTotal\nAda\t42" {
        return fail(reread.plain_text(edgedoc::StoryRef::Body));
    }
    Ok(())
}

fn cell_builder() -> Test {
    let mut cell = Cell::paragraph("Shaded")
        .span(2)
        .shading("#FFEE00")
        .valign("center");
    cell.borders = Borders {
        top: Some(LineBorder {
            val: "dashed".into(),
            size: Some(8),
            color: Some("0000FF".into()),
            space: None,
        }),
        ..Borders::default()
    };
    let table = Table::new().widths([Mm(40.0), Mm(40.0)]).row(["A", "B"]);
    let mut table = table;
    table.rows[0].cells[0] = cell;
    let doc = Document::builder().table(table).build().map_err(err)?;
    let out = doc.write_bytes().map_err(err)?;
    let document = part(&out, "word/document.xml")?;
    has(&document, r#"<w:gridSpan w:val="2"/>"#)?;
    has(&document, r#"w:fill="FFEE00""#)?;
    has(
        &document,
        r#"<w:top w:val="dashed" w:sz="8" w:color="0000FF"/>"#,
    )?;
    has(&document, r#"<w:vAlign w:val="center"/>"#)
}

// ---------------------------------------------------------------- drawings

fn picture_inline() -> Test {
    let doc = Document::builder()
        .paragraph(
            Paragraph::new().picture(
                Picture::png(b"PNG-BYTES".to_vec())
                    .size(Emu(914_400), Emu(457_200))
                    .alt("A logo"),
            ),
        )
        .build()
        .map_err(err)?;
    let out = doc.write_bytes().map_err(err)?;
    let document = part(&out, "word/document.xml")?;
    has(&document, "<wp:inline")?;
    has(&document, r#"<wp:extent cx="914400" cy="457200"/>"#)?;
    has(&document, r#"descr="A logo""#)?;
    let rels_xml = part(&out, "word/_rels/document.xml.rels")?;
    has(&rels_xml, "media/img1.png")?;
    let reread = read(&out)?;
    let pictures = reread.pictures();
    if pictures.len() != 1 {
        return fail(format!("{} pictures", pictures.len()));
    }
    if pictures[0].width != Some(Emu(914_400)) || pictures[0].alt.as_deref() != Some("A logo") {
        return fail(format!("{:?}", pictures[0]));
    }
    Ok(())
}

fn picture_floating() -> Test {
    let anchor = Anchor::inline_text_distance()
        .behind_text()
        .align("right", "top");
    let doc = Document::builder()
        .paragraph(
            Paragraph::new()
                .text("Wrapped ")
                .picture(Picture::jpeg(b"JPEG".to_vec()).floating(anchor)),
        )
        .build()
        .map_err(err)?;
    let out = doc.write_bytes().map_err(err)?;
    let document = part(&out, "word/document.xml")?;
    has(&document, "<wp:anchor")?;
    has(&document, r#"behindDoc="1""#)?;
    has(&document, "<wp:wrapSquare")?;
    has(&document, "<wp:align>right</wp:align>")?;
    let reread = read(&out)?;
    let pictures = reread.pictures();
    let anchor = pictures
        .first()
        .and_then(|picture| picture.anchor.as_ref())
        .ok_or("the anchor is missing")?;
    if !anchor.behind_text || anchor.h_align.as_deref() != Some("right") {
        return fail(format!("{anchor:?}"));
    }
    Ok(())
}

fn picture_offset() -> Test {
    let anchor = Anchor::inline_text_distance().offset(Emu(72_000), Emu(36_000));
    let doc = Document::builder()
        .paragraph(
            Paragraph::new()
                .text("Wrapped ")
                .picture(Picture::png(b"P".to_vec()).floating(anchor)),
        )
        .build()
        .map_err(err)?;
    let out = doc.write_bytes().map_err(err)?;
    let document = part(&out, "word/document.xml")?;
    has(&document, "<wp:posOffset>72000</wp:posOffset>")?;
    has(&document, "<wp:posOffset>36000</wp:posOffset>")?;
    let reread = read(&out)?;
    let anchor = reread
        .pictures()
        .first()
        .and_then(|picture| picture.anchor.clone())
        .ok_or("no picture")?;
    if anchor.x != Some(Emu(72_000)) {
        return fail(format!("{anchor:?}"));
    }
    Ok(())
}

fn picture_alt_round_trip() -> Test {
    let inner = r#"<w:p><w:r><w:drawing><wp:inline><wp:extent cx="1000000" cy="500000"/><wp:docPr id="1" name="Logo" descr="Brand mark"/><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:pic><pic:blipFill><a:blip r:embed="rIdP"/></pic:blipFill></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>"#;
    let bytes = office(
        &body(inner),
        &rels(
            r#"<Relationship Id="rIdP" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/logo.png"/>"#,
        ),
        &[("word/media/logo.png", b"LOGO")],
    )?;
    let doc = read(&bytes)?;
    let picture = doc.pictures().into_iter().next().ok_or("no picture")?;
    if picture.width != Some(Emu(1_000_000)) || picture.alt.as_deref() != Some("Brand mark") {
        return fail(format!("{picture:?}"));
    }
    let out = save(&bytes)?;
    has(&part(&out, "word/document.xml")?, r#"descr="Brand mark""#)
}

// ---------------------------------------------------------------- stories

fn footnote_story() -> Test {
    let footnotes = br#"<?xml version="1.0"?><w:footnotes xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:footnote w:type="separator" w:id="-1"><w:p><w:r><w:separator/></w:r></w:p></w:footnote><w:footnote w:id="2"><w:p><w:r><w:t>Source note</w:t></w:r></w:p></w:footnote></w:footnotes>"#;
    let bytes = office(
        &body(r#"<w:p><w:r><w:t>Text</w:t><w:footnoteReference w:id="2"/></w:r></w:p>"#),
        &rels(
            r#"<Relationship Id="rIdF" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/footnotes" Target="footnotes.xml"/>"#,
        ),
        &[("word/footnotes.xml", footnotes)],
    )?;
    let doc = read(&bytes)?;
    let ids = doc.parts(PartKind::Footnote);
    if ids.len() != 2 {
        return fail(format!("{ids:?} footnotes"));
    }
    let body = doc.plain_text(edgedoc::StoryRef::Part(ids[1]));
    if body != "Source note" {
        return fail(body);
    }
    if doc.part_meta(ids[1]).and_then(|meta| meta.id) != Some(2) {
        return fail("the note id is not on the model");
    }
    let out = save(&bytes)?;
    has(&part(&out, "word/footnotes.xml")?, "Source note")
}

fn footnote_edit() -> Test {
    let footnotes = br#"<?xml version="1.0"?><w:footnotes xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:footnote w:id="1"><w:p><w:r><w:t>Old note</w:t></w:r></w:p></w:footnote></w:footnotes>"#;
    let bytes = office(
        &body(r#"<w:p><w:r><w:t>Text</w:t><w:footnoteReference w:id="1"/></w:r></w:p>"#),
        &rels(
            r#"<Relationship Id="rIdF" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/footnotes" Target="footnotes.xml"/>"#,
        ),
        &[("word/footnotes.xml", footnotes)],
    )?;
    let mut doc = read(&bytes)?;
    let id = doc
        .parts(PartKind::Footnote)
        .first()
        .copied()
        .ok_or("no footnote")?;
    doc.edit(EditOptions::default(), |edit| {
        edit.body()
            .part_paragraph(id, Paragraph::new().text("Added note"))
    })
    .map_err(err)?;
    let out = doc.write_bytes().map_err(err)?;
    let notes = part(&out, "word/footnotes.xml")?;
    has(&notes, "Old note")?;
    has(&notes, "Added note")?;
    let reread = read(&out)?;
    let text = reread.plain_text(edgedoc::StoryRef::Part(reread.parts(PartKind::Footnote)[0]));
    has(&text, "Added note")
}

fn comment_story() -> Test {
    let comments = br#"<?xml version="1.0"?><w:comments xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:comment w:id="1" w:author="Ada" w:date="2026-01-01T00:00:00Z" w:initials="A"><w:p><w:r><w:t>Check the total</w:t></w:r></w:p></w:comment></w:comments>"#;
    let bytes = office(
        &body(
            r#"<w:p><w:commentRangeStart w:id="1"/><w:r><w:t>Total</w:t><w:commentReference w:id="1"/></w:r><w:commentRangeEnd w:id="1"/></w:p>"#,
        ),
        &rels(
            r#"<Relationship Id="rIdC" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments" Target="comments.xml"/>"#,
        ),
        &[("word/comments.xml", comments)],
    )?;
    let doc = read(&bytes)?;
    let ids = doc.parts(PartKind::Comment);
    if ids.len() != 1 {
        return fail(format!("{ids:?} comments"));
    }
    if doc
        .part_meta(ids[0])
        .and_then(|meta| meta.author.clone())
        .as_deref()
        != Some("Ada")
    {
        return fail("the comment author is not on the model");
    }
    let out = save(&bytes)?;
    let kept = part(&out, "word/comments.xml")?;
    has(&kept, "Check the total")?;
    has(&kept, r#"w:author="Ada""#)
}

fn header_picture() -> Test {
    let header = br#"<?xml version="1.0"?><w:hdr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture"><w:p><w:r><w:drawing><wp:inline><wp:extent cx="500000" cy="500000"/><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:pic><pic:blipFill><a:blip r:embed="rIdLogo"/></pic:blipFill></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p></w:hdr>"#;
    let header_rels = br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdLogo" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/logo.png"/></Relationships>"#;
    let bytes = office(
        &body(
            r#"<w:p><w:r><w:t>Body</w:t></w:r></w:p><w:sectPr><w:headerReference w:type="default" r:id="rIdH"/></w:sectPr>"#,
        ),
        &rels(
            r#"<Relationship Id="rIdH" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/header" Target="header1.xml"/>"#,
        ),
        &[
            ("word/header1.xml", header),
            ("word/_rels/header1.xml.rels", header_rels),
            ("word/media/logo.png", b"LOGO-BYTES"),
        ],
    )?;
    let doc = read(&bytes)?;
    let id = doc
        .parts(PartKind::Header)
        .first()
        .copied()
        .ok_or("no header")?;
    if doc.plain_text(edgedoc::StoryRef::Part(id)).is_empty()
        && doc.part_kind(id) != Some(PartKind::Header)
    {
        return fail("the header part is not readable");
    }
    let out = save(&bytes)?;
    let header_xml = part(&out, "word/header1.xml")?;
    let rels_xml = part(&out, "word/_rels/header1.xml.rels")?;
    has(&header_xml, "r:embed=")?;
    has(&rels_xml, "media/")?;
    let package = edgedoc_opc::read_zip(&out, &edgedoc::Budget::default()).map_err(err)?;
    if !package
        .parts
        .values()
        .any(|part| part.bytes == b"LOGO-BYTES")
    {
        return fail("the header picture bytes were dropped");
    }
    let reread = read(&out)?;
    let header_id = reread
        .parts(PartKind::Header)
        .first()
        .copied()
        .ok_or("no header after save")?;
    let header_text = part(&out, "word/header1.xml")?;
    has(&header_text, "r:embed")?;
    if reread.part_kind(header_id) != Some(PartKind::Header) {
        return fail("the header part changed kind");
    }
    Ok(())
}

fn footnote_picture() -> Test {
    let footnotes = br#"<?xml version="1.0"?><w:footnotes xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture"><w:footnote w:id="1"><w:p><w:r><w:drawing><wp:inline><wp:extent cx="300000" cy="300000"/><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:pic><pic:blipFill><a:blip r:embed="rIdNote"/></pic:blipFill></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p></w:footnote></w:footnotes>"#;
    let note_rels = br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdNote" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/note.png"/></Relationships>"#;
    let bytes = office(
        &body(r#"<w:p><w:r><w:t>Text</w:t><w:footnoteReference w:id="1"/></w:r></w:p>"#),
        &rels(
            r#"<Relationship Id="rIdF" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/footnotes" Target="footnotes.xml"/>"#,
        ),
        &[
            ("word/footnotes.xml", footnotes),
            ("word/_rels/footnotes.xml.rels", note_rels),
            ("word/media/note.png", b"NOTE-IMAGE"),
        ],
    )?;
    let out = save(&bytes)?;
    let rels_xml = part(&out, "word/_rels/footnotes.xml.rels")?;
    has(&rels_xml, "media/")?;
    let package = edgedoc_opc::read_zip(&out, &edgedoc::Budget::default()).map_err(err)?;
    if !package
        .parts
        .values()
        .any(|part| part.bytes == b"NOTE-IMAGE")
    {
        return fail("the note picture bytes were dropped");
    }
    Ok(())
}

fn vml_picture_kept() -> Test {
    let inner = r#"<w:p><w:r><w:pict><v:shape xmlns:v="urn:schemas-microsoft-com:vml"><v:imagedata r:id="rIdVml"/></v:shape></w:pict></w:r></w:p>"#;
    let bytes = office(
        &body(inner),
        &rels(
            r#"<Relationship Id="rIdVml" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/old.wmf"/>"#,
        ),
        &[("word/media/old.wmf", b"WMF-BYTES")],
    )?;
    let out = save(&bytes)?;
    let document = part(&out, "word/document.xml")?;
    has(&document, "rIdVml")?;
    let rels_xml = part(&out, "word/_rels/document.xml.rels")?;
    has(&rels_xml, "old.wmf")?;
    let package = edgedoc_opc::read_zip(&out, &edgedoc::Budget::default()).map_err(err)?;
    if !package
        .parts
        .values()
        .any(|part| part.bytes == b"WMF-BYTES")
    {
        return fail("the VML picture bytes were dropped");
    }
    Ok(())
}

fn header_edit() -> Test {
    let header = br#"<?xml version="1.0"?><w:hdr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:p><w:r><w:t>Old header</w:t></w:r></w:p></w:hdr>"#;
    let bytes = office(
        &body(
            r#"<w:p><w:r><w:t>Body</w:t></w:r></w:p><w:sectPr><w:headerReference w:type="default" r:id="rIdH"/></w:sectPr>"#,
        ),
        &rels(
            r#"<Relationship Id="rIdH" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/header" Target="header1.xml"/>"#,
        ),
        &[("word/header1.xml", header)],
    )?;
    let mut doc = read(&bytes)?;
    let id = doc
        .parts(PartKind::Header)
        .first()
        .copied()
        .ok_or("no header")?;
    if doc.plain_text(edgedoc::StoryRef::Part(id)) != "Old header" {
        return fail(doc.plain_text(edgedoc::StoryRef::Part(id)));
    }
    doc.edit(EditOptions::default(), |edit| {
        edit.body()
            .part_paragraph(id, Paragraph::new().text("New header"))
    })
    .map_err(err)?;
    let out = doc.write_bytes().map_err(err)?;
    let all = parts_text(&out)?;
    has(&all, "Old header")?;
    has(&all, "New header")
}

// ---------------------------------------------------------------- controls and fields

fn control_items() -> Test {
    let inner = r#"<w:sdt><w:sdtPr><w:alias w:val="Country"/><w:dropDownList><w:listItem w:displayText="France" w:value="FR"/><w:listItem w:displayText="Japan" w:value="JP"/></w:dropDownList></w:sdtPr><w:sdtContent><w:p><w:r><w:t>France</w:t></w:r></w:p></w:sdtContent></w:sdt><w:sdt><w:sdtPr><w:checkbox><w:checked w:val="1"/></w:checkbox></w:sdtPr><w:sdtContent><w:p><w:r><w:t>Yes</w:t></w:r></w:p></w:sdtContent></w:sdt>"#;
    let doc = read(&sample(inner)?)?;
    let controls = doc.controls();
    if controls.len() != 2 {
        return fail(format!("{} controls", controls.len()));
    }
    if controls[0].items.len() != 2
        || controls[0].items[1].value != "JP"
        || controls[0].items[1].display != "Japan"
    {
        return fail(format!("{:?}", controls[0].items));
    }
    if controls[1].checked != Some(true) {
        return fail(format!("{:?}", controls[1].checked));
    }
    let out = save(&sample(inner)?)?;
    let document = part(&out, "word/document.xml")?;
    has(
        &document,
        r#"<w:listItem w:displayText="Japan" w:value="JP"/>"#,
    )?;
    has(&document, r#"<w:checked w:val="1"/>"#)
}

fn control_builder() -> Test {
    let doc = Document::builder()
        .paragraph(Paragraph::new())
        .build()
        .map_err(err)?;
    let _ = doc;
    let control = ContentControl::drop_down("Country", "France", ["France", "Japan"]);
    if control.kind != "dropDownList" || control.items.len() != 2 {
        return fail(format!("{control:?}"));
    }
    let date = ContentControl::date("When", "2026-11-04", "yyyy-MM-dd");
    if date.date_format.as_deref() != Some("yyyy-MM-dd") {
        return fail("the date format is missing");
    }
    let checked = ContentControl::checked("Agree", true);
    if checked.checked != Some(true) {
        return fail("the checked state is missing");
    }
    Ok(())
}

fn simple_field_kept() -> Test {
    let inner = r#"<w:p><w:fldSimple w:instr=" MERGEFIELD Name "><w:r><w:t>Ada</w:t></w:r></w:fldSimple></w:p>"#;
    let out = save(&sample(inner)?)?;
    has(&part(&out, "word/document.xml")?, "MERGEFIELD Name")
}

fn page_field_kept() -> Test {
    let inner = r#"<w:p><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText> PAGE </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>7</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r></w:p>"#;
    let doc = read(&sample(inner)?)?;
    if !doc.plain_text(edgedoc::StoryRef::Body).contains('7') {
        return fail(doc.plain_text(edgedoc::StoryRef::Body));
    }
    Ok(())
}

// ---------------------------------------------------------------- properties and styles

fn core_props_round_trip() -> Test {
    let core = CoreProps::new()
        .title("Report")
        .creator("Ada")
        .subject("Quarterly")
        .keywords("finance, q3")
        .description("A short report")
        .last_modified_by("Grace")
        .modified("2026-11-04T09:00:00Z")
        .category("Finance")
        .content_status("Final")
        .language("en-GB");
    let doc = Document::builder()
        .core(core.clone())
        .paragraph(Paragraph::new().text("Body"))
        .build()
        .map_err(err)?;
    let out = doc.write_bytes().map_err(err)?;
    let reread = read(&out)?;
    let props = reread.core_props();
    if props.title != "Report" || props.subject != "Quarterly" || props.keywords != "finance, q3" {
        return fail(format!("{props:?}"));
    }
    if props.last_modified_by != "Grace"
        || props.content_status != "Final"
        || props.language != "en-GB"
    {
        return fail(format!("{props:?}"));
    }
    if props.modified != "2026-11-04T09:00:00Z" {
        return fail(format!("{:?}", props.modified));
    }
    Ok(())
}

fn style_table_preserved() -> Test {
    let styles = br#"<?xml version="1.0"?><w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:docDefaults><w:rPrDefault><w:rPr><w:sz w:val="22"/></w:rPr></w:rPrDefault></w:docDefaults><w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style><w:style w:type="table" w:styleId="TableGrid"><w:name w:val="Table Grid"/><w:tblPr><w:tblBorders><w:top w:val="single" w:sz="4"/></w:tblBorders></w:tblPr><w:tblStylePr w:type="firstRow"><w:rPr><w:b/></w:rPr></w:tblStylePr></w:style><w:style w:type="paragraph" w:styleId="Quote"><w:name w:val="Quote"/><w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:qFormat/><w:pPr><w:ind w:left="720"/></w:pPr><w:rPr><w:i/></w:rPr></w:style></w:styles>"#;
    let bytes = office(
        &body(r#"<w:p><w:pPr><w:pStyle w:val="Quote"/></w:pPr><w:r><w:t>Quote</w:t></w:r></w:p>"#),
        &rels(
            r#"<Relationship Id="rIdS" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>"#,
        ),
        &[("word/styles.xml", styles)],
    )?;
    let doc = read(&bytes)?;
    let quote = doc
        .styles()
        .by_id("Quote")
        .ok_or("the Quote style is missing")?;
    if quote.based_on.as_deref() != Some("Normal")
        || !quote.char_props.italic
        || quote.para_props.indent_left != Some(Twips(720))
    {
        return fail(format!("{quote:?}"));
    }
    if doc.styles().by_id("TableGrid").map(|style| style.kind) != Some(StyleKind::Table) {
        return fail("the table style is not typed as a table style");
    }
    let out = save(&bytes)?;
    let kept = part(&out, "word/styles.xml")?;
    has(&kept, r#"<w:tblStylePr w:type="firstRow">"#)?;
    has(&kept, "<w:docDefaults>")
}

fn style_builder() -> Test {
    let doc = Document::builder()
        .style(
            Style::paragraph("Heading1")
                .named("heading 1")
                .size(Pt(18.0))
                .bold()
                .color("#1F3864")
                .align("center"),
        )
        .style(Style::character("Emphasis").italic())
        .style(Style::table("Grid"))
        .paragraph(Paragraph::new().style("Heading1").text("Title"))
        .build()
        .map_err(err)?;
    let out = doc.write_bytes().map_err(err)?;
    let styles = part(&out, "word/styles.xml")?;
    has(
        &styles,
        r#"<w:style w:type="paragraph" w:styleId="Heading1">"#,
    )?;
    has(&styles, r#"<w:sz w:val="36"/>"#)?;
    has(&styles, r#"<w:color w:val="1F3864"/>"#)?;
    has(&styles, r#"<w:jc w:val="center"/>"#)?;
    has(
        &styles,
        r#"<w:style w:type="character" w:styleId="Emphasis">"#,
    )?;
    has(&styles, r#"<w:style w:type="table" w:styleId="Grid">"#)?;
    let reread = read(&out)?;
    if reread.style_outline("Heading1") != Some(1) {
        return fail(format!("{:?}", reread.style_outline("Heading1")));
    }
    Ok(())
}

fn run_builder_extended() -> Test {
    let doc = Document::builder()
        .paragraph(
            Paragraph::new()
                .run(RunBuilder::new().text("Small").small_caps())
                .run(RunBuilder::new().text("2").superscript())
                .run(RunBuilder::new().text("Spaced").spacing(60))
                .run(RunBuilder::new().text("Caps").caps()),
        )
        .build()
        .map_err(err)?;
    let out = doc.write_bytes().map_err(err)?;
    let document = part(&out, "word/document.xml")?;
    has(&document, "<w:smallCaps/>")?;
    has(&document, r#"<w:vertAlign w:val="superscript"/>"#)?;
    has(&document, r#"<w:spacing w:val="60"/>"#)?;
    has(&document, "<w:caps/>")?;
    let reread = read(&out)?;
    let props = reread.run_props(0, 1).ok_or("no second run")?;
    if props.vert_align.as_deref() != Some("superscript") {
        return fail(format!("{props:?}"));
    }
    Ok(())
}

fn paragraph_builder_extended() -> Test {
    let borders = Borders {
        bottom: Some(LineBorder {
            val: "single".into(),
            size: Some(6),
            color: Some("000000".into()),
            space: Some(1),
        }),
        ..Borders::default()
    };
    let doc = Document::builder()
        .paragraph(
            Paragraph::new()
                .text("Styled")
                .align("center")
                .indent_first(Mm(-5.0))
                .keep_next()
                .borders(borders)
                .tab_stop(TabStop {
                    val: "center".into(),
                    pos: Twips(4320),
                    leader: Some("dot".into()),
                }),
        )
        .build()
        .map_err(err)?;
    let out = doc.write_bytes().map_err(err)?;
    let document = part(&out, "word/document.xml")?;
    has(&document, r#"w:jcis"#).ok();
    has(&document, r#"<w:jc w:val="center"/>"#)?;
    has(&document, "w:hanging=")?;
    has(&document, "<w:keepNext/>")?;
    has(
        &document,
        r#"<w:tab w:val="center" w:pos="4320" w:leader="dot"/>"#,
    )?;
    let reread = read(&out)?;
    let props = reread.paragraph_props(0).ok_or("no paragraph props")?;
    if props.align.as_deref() != Some("center") || props.tabs.len() != 1 {
        return fail(format!("{props:?}"));
    }
    if props.indent_first.map(|value| value.0) != Some(Mm(-5.0).twips().0) {
        return fail(format!("{:?}", props.indent_first));
    }
    Ok(())
}

fn builder_with_props() -> Test {
    let props = ParaProps {
        align: Some("right".into()),
        ..ParaProps::default()
    };
    let para = Paragraph::with_props(props);
    if para.props.align.as_deref() != Some("right") {
        return fail("the properties were not set");
    }
    Ok(())
}

fn char_props_default_is_empty() -> Test {
    if !CharProps::default().is_empty() {
        return fail("the default character properties are not empty");
    }
    Ok(())
}

fn content_types_cover_media() -> Test {
    let doc = Document::builder()
        .paragraph(Paragraph::new().picture(Picture::png(b"P".to_vec())))
        .paragraph(Paragraph::new().text("List").list(ListKind::Bullet, 0))
        .build()
        .map_err(err)?;
    let out = doc.write_bytes().map_err(err)?;
    let types = part(&out, "[Content_Types].xml")?;
    has(&types, r#"Extension="png""#)?;
    has(&types, "wordprocessingml.numbering+xml")
}

fn block_helpers() -> Test {
    let doc = Document::builder()
        .table(Table::new().row(["A", "B"]))
        .paragraph(Paragraph::new().text("After"))
        .build()
        .map_err(err)?;
    let block = doc.block_at(0).ok_or("no block")?;
    if !matches!(block, Block::Table(_)) {
        return fail("the first block is not a table");
    }
    Ok(())
}

type Case = (&'static str, fn() -> Test);

fn coverage_cases() -> Vec<Case> {
    vec![
        ("char-extended", char_extended),
        ("char-unknown-kept", char_unknown_kept),
        ("paragraph-extended", paragraph_extended),
        ("list-generated", list_generated),
        (
            "list-generated-keeps-existing",
            list_generated_keeps_existing,
        ),
        ("list-kept-numbering", list_kept_numbering),
        ("section-extended", section_extended),
        ("section-builder", section_builder),
        ("section-mid-document", section_mid_document),
        ("table-extended", table_extended),
        ("table-builder", table_builder),
        ("cell-builder", cell_builder),
        ("picture-inline", picture_inline),
        ("picture-floating", picture_floating),
        ("picture-offset", picture_offset),
        ("picture-alt-round-trip", picture_alt_round_trip),
        ("footnote-story", footnote_story),
        ("footnote-edit", footnote_edit),
        ("footnote-picture", footnote_picture),
        ("comment-story", comment_story),
        ("header-edit", header_edit),
        ("vml-picture-kept", vml_picture_kept),
        ("header-picture", header_picture),
        ("control-items", control_items),
        ("control-builder", control_builder),
        ("simple-field-kept", simple_field_kept),
        ("page-field-kept", page_field_kept),
        ("core-props-round-trip", core_props_round_trip),
        ("style-table-preserved", style_table_preserved),
        ("style-builder", style_builder),
        ("run-builder-extended", run_builder_extended),
        ("paragraph-builder-extended", paragraph_builder_extended),
        ("builder-with-props", builder_with_props),
        ("char-props-default-is-empty", char_props_default_is_empty),
        ("content-types-cover-media", content_types_cover_media),
        ("block-helpers", block_helpers),
    ]
}

#[test]
fn word_features() -> Test {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/12-word-features.md");
    let doc = std::fs::read_to_string(path).map_err(err)?;
    let cases = coverage_cases();
    let mut seen = 0usize;
    for (id, _) in &cases {
        if !doc.contains(id) {
            return fail(format!("docs/12-word-features.md has no row for {id}"));
        }
    }
    for line in doc.lines() {
        // Each row is: area | feature | status | test id.
        let cells: Vec<&str> = line.split('|').map(str::trim).collect();
        if cells.len() < 6 {
            continue;
        }
        let id = cells[4].trim_matches('`');
        if id.is_empty() || id == "Test" || id.chars().all(|ch| ch == '-') {
            continue;
        }
        seen += 1;
        if id == "none" {
            continue;
        }
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
    for (id, check) in cases {
        check().map_err(|message| format!("{id}: {message}"))?;
    }
    Ok(())
}
