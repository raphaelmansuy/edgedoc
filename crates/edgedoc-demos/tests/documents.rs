//! The example documents must be valid packages and must keep the text they promise.
//!
//! Each case runs the same code that `make examples` runs, then reads the bytes back
//! through the public reader. A demo that builds but produces a broken package, or
//! loses a merge result, fails here. The cases need no files on disk.

use edgedoc::{
    CsvOptions, Document, InlineObject, Join, MergeKind, MissingField, Paragraph, Records, StoryRef,
};

type Test = Result<(), String>;

fn err(err: impl std::fmt::Display) -> String {
    err.to_string()
}

fn fail(message: impl Into<String>) -> Test {
    Err(message.into())
}

fn text_of(bytes: &[u8]) -> Result<String, String> {
    let doc = Document::read_bytes(bytes).map_err(err)?;
    let mut text = String::new();
    for story in doc.stories() {
        text.push_str(&doc.plain_text(story));
        text.push('\n');
    }
    Ok(text)
}

fn readable(bytes: &[u8], needles: &[&str]) -> Test {
    let text = text_of(bytes)?;
    for needle in needles {
        if !text.contains(needle) {
            return fail(format!(
                "missing {needle} in {}",
                text.chars().take(300).collect::<String>()
            ));
        }
    }
    Ok(())
}

fn find(files: &[edgedoc_demos::Rendered], name: &str) -> Result<Vec<u8>, String> {
    files
        .iter()
        .find(|file| file.name == name)
        .map(|file| file.bytes.clone())
        .ok_or_else(|| {
            format!(
                "no {name} in {:?}",
                files.iter().map(|file| &file.name).collect::<Vec<_>>()
            )
        })
}

fn invoice_template() -> Test {
    let files = edgedoc_demos::invoice::render().map_err(err)?;
    let bytes = find(&files, "invoice-template.docx")?;
    readable(&bytes, &["Invoice", "Billed to", "Description", "Terms"])?;
    let doc = Document::read_bytes(&bytes).map_err(err)?;
    // The seal is in the body. The logo is in the header, so it is not in `pictures`.
    if doc.pictures().is_empty() {
        return fail("the template body has no picture");
    }
    if doc.merge_field_names().len() < 6 {
        return fail(format!("{:?}", doc.merge_field_names()));
    }
    Ok(())
}

fn invoice_merged() -> Test {
    let files = edgedoc_demos::invoice::render().map_err(err)?;
    // One template, then one invoice per record.
    if files.len() != 5 {
        return fail(format!("{} files", files.len()));
    }
    let bytes = find(&files, "invoices/INV-2026-0418.docx")?;
    readable(
        &bytes,
        &[
            "Invoice INV-2026-0418",
            "Issued 2 October 2026",
            "due 1 November 2026",
            "Northwind Traders",
            "Amelia Hart",
            "PO-88213",
            "Brand audit",
            "4800.00",
            "8240.00",
            "1730.40",
            "9970.40",
            "Page 1 of 1",
        ],
    )?;
    let doc = Document::read_bytes(&bytes).map_err(err)?;
    if doc.section().page_w.0 != 11906 {
        return fail("the page is not A4");
    }
    // The line items must be table rows, not a placeholder.
    let table = (0..doc.body_len())
        .filter_map(|index| doc.table(index))
        .find(|table| table.rows.len() > 2);
    let table = table.ok_or("the invoice has no line-item table")?;
    if table.rows.len() != 4 {
        return fail(format!("{} rows", table.rows.len()));
    }
    Ok(())
}

fn invoice_totals_are_money() -> Test {
    let files = edgedoc_demos::invoice::render().map_err(err)?;
    let bytes = find(&files, "invoices/INV-2026-0421.docx")?;
    // 3600 + 540 * 2 = 4680; with 21% VAT it is 5662.80.
    readable(&bytes, &["4680.00", "982.80", "5662.80"])
}

fn report() -> Test {
    let files = edgedoc_demos::report::render().map_err(err)?;
    let bytes = find(&files, "report.docx")?;
    readable(
        &bytes,
        &[
            "Quarterly business review",
            "Northern Europe",
            "North America",
            "Asia Pacific",
            "€7,960,000",
            "How we measure",
        ],
    )?;
    let doc = Document::read_bytes(&bytes).map_err(err)?;
    // One section per region keeps the footer field meaningful.
    if doc.section_break_count() < 4 {
        return fail(format!("{} section breaks", doc.section_break_count()));
    }
    // The footer of the first section names its own region.
    let header = doc.parts(edgedoc::PartKind::Footer);
    let mut footers = String::new();
    for id in header {
        footers.push_str(&doc.plain_text(StoryRef::Part(id)));
    }
    if !footers.contains("Region:") {
        return fail(footers);
    }
    Ok(())
}

fn report_sections_carry_their_region() -> Test {
    let files = edgedoc_demos::report::render().map_err(err)?;
    let bytes = find(&files, "report.docx")?;
    let doc = Document::read_bytes(&bytes).map_err(err)?;
    let mut named = 0;
    for id in doc.parts(edgedoc::PartKind::Footer) {
        let text = doc.plain_text(StoryRef::Part(id));
        if text.contains("Region: ") && !text.contains("«Region»") {
            named += 1;
        }
    }
    if named < 2 {
        return fail(format!("{named} footers name a region"));
    }
    Ok(())
}

fn letter() -> Test {
    let files = edgedoc_demos::letter::render().map_err(err)?;
    let bytes = find(&files, "letters/Hart-Amelia.docx")?;
    readable(
        &bytes,
        &[
            "Dear Amelia Hart,",
            "Northwind Traders",
            "48200",
            "As an enterprise account",
            "Ask about the multi-year terms",
            "Yours sincerely",
        ],
    )
}

fn letter_filter() -> Test {
    let files = edgedoc_demos::letter::render().map_err(err)?;
    // Three of the six records are enterprise accounts.
    let letters = files
        .iter()
        .filter(|file| file.name.starts_with("letters/"))
        .count();
    if letters != 3 {
        return fail(format!("{letters} letters"));
    }
    // An enterprise account gets the conditional paragraph.
    let text = text_of(&find(&files, "letters/Hart-Amelia.docx")?)?;
    if !text.contains("As an enterprise account") {
        return fail(text);
    }
    Ok(())
}

fn directory() -> Test {
    let files = edgedoc_demos::letter::render().map_err(err)?;
    let bytes = find(&files, "directory.docx")?;
    readable(&bytes, &["Amelia", "Wei", "Sofia"])?;
    let doc = Document::read_bytes(&bytes).map_err(err)?;
    // A directory is one table, not a stack of pages.
    if doc.section_break_count() != 0 {
        return fail(format!("{} section breaks", doc.section_break_count()));
    }
    Ok(())
}

/// A field result longer than its placeholder must resize the run, or the save truncates it.
fn field_result_length() -> Test {
    let doc = Document::builder()
        .paragraph(Paragraph::new().text("Invoice ").merge_field("Number"))
        .build()
        .map_err(err)?;
    let records =
        Records::from_csv(b"Number\nINV-2026-0001\n", &CsvOptions::default()).map_err(err)?;
    let merged = doc
        .merge()
        .records(records)
        .kind(MergeKind::Letters)
        .missing(MissingField::Error)
        .finish()
        .map_err(err)?
        .join(Join::Auto)
        .map_err(err)?;
    if merged.plain_text(StoryRef::Body) != "Invoice INV-2026-0001" {
        return fail(merged.plain_text(StoryRef::Body));
    }
    let out = merged.write_bytes().map_err(err)?;
    let reread = Document::read_bytes(&out).map_err(err)?;
    if reread.plain_text(StoryRef::Body) != "Invoice INV-2026-0001" {
        return fail(reread.plain_text(StoryRef::Body));
    }
    Ok(())
}

/// Text after a field must not join the field run.
fn text_after_a_field() -> Test {
    let mut para = Paragraph::new();
    let props = edgedoc::CharProps::default();
    para.push_text("Page ", &props);
    para.push_object(
        InlineObject::Field(edgedoc::Field::new(" PAGE ", "3")),
        &props,
    );
    para.push_text(" of 9", &props);
    let doc = Document::builder().paragraph(para).build().map_err(err)?;
    let out = doc.write_bytes().map_err(err)?;
    let reread = Document::read_bytes(&out).map_err(err)?;
    if reread.plain_text(StoryRef::Body) != "Page 3 of 9" {
        return fail(reread.plain_text(StoryRef::Body));
    }
    Ok(())
}

/// Two headers that each hold a picture must not write the same media part twice.
fn media_parts_are_unique() -> Test {
    let files = edgedoc_demos::invoice::render().map_err(err)?;
    let bytes = find(&files, "invoices/INV-2026-0418.docx")?;
    let doc = Document::read_bytes(&bytes).map_err(err)?;
    let mut names = doc.part_names();
    names.sort();
    let before = names.len();
    names.dedup();
    if names.len() != before {
        return fail("a part name appears twice");
    }
    Ok(())
}

/// Every part of every demo must be well-formed XML, use only declared prefixes, and
/// point every relationship at a part that exists. A package that fails this opens as
/// a corrupt file in Word.
fn packages_are_well_formed() -> Test {
    let mut documents: Vec<edgedoc_demos::Rendered> = Vec::new();
    documents.extend(edgedoc_demos::invoice::render().map_err(err)?);
    documents.extend(edgedoc_demos::report::render().map_err(err)?);
    documents.extend(edgedoc_demos::letter::render().map_err(err)?);

    for file in documents {
        let package = edgedoc_opc::read_zip(&file.bytes, &edgedoc::Budget::default())
            .map_err(|err| format!("{}: {err}", file.name))?;
        for (name, part) in &package.parts {
            if !name.ends_with(".xml") && !name.ends_with(".rels") {
                continue;
            }
            // A parse proves the XML is well-formed and every prefix is bound.
            edgedoc_opc::xml::parse(&part.bytes, &edgedoc::Budget::default())
                .map_err(|err| format!("{}: {name}: {}", file.name, err.message))?;
        }
        // Every internal relationship target must resolve.
        for (name, part) in &package.parts {
            if !name.ends_with(".rels") {
                continue;
            }
            let base = name.replace("_rels/", "").replace(".rels", "");
            let directory = base.rsplit_once('/').map(|(dir, _)| dir).unwrap_or("");
            let root =
                edgedoc_opc::xml::parse(&part.bytes, &edgedoc::Budget::default()).map_err(err)?;
            for rel in edgedoc_opc::rels_from_el(&root) {
                if rel.external {
                    continue;
                }
                let target = if rel.target.starts_with('/') {
                    rel.target.trim_start_matches('/').to_string()
                } else if directory.is_empty() {
                    rel.target.clone()
                } else {
                    format!("{directory}/{}", rel.target)
                };
                if package.part(&target).is_none() {
                    return fail(format!("{}: {name} points at missing {target}", file.name));
                }
            }
        }
    }
    Ok(())
}

/// A header that holds a picture must declare the drawing namespaces on its root.
/// Without them Word reports a corrupt file, even though the bytes are well-formed
/// for a parser that ignores prefixes.
fn header_picture_namespaces() -> Test {
    let files = edgedoc_demos::invoice::render().map_err(err)?;
    let bytes = find(&files, "invoice-template.docx")?;
    let package = edgedoc_opc::read_zip(&bytes, &edgedoc::Budget::default()).map_err(err)?;
    let header = package
        .part("word/header1.xml")
        .ok_or("the invoice has no header part")?;
    let text = String::from_utf8_lossy(&header.bytes).into_owned();
    for prefix in ["xmlns:wp=", "xmlns:a=", "xmlns:pic=", "xmlns:r="] {
        if !text.contains(prefix) {
            return fail(format!("the header root lacks {prefix}"));
        }
    }
    Ok(())
}

type Case = (&'static str, fn() -> Test);

fn cases() -> Vec<Case> {
    vec![
        ("invoice-template", invoice_template),
        ("invoice-merged", invoice_merged),
        ("invoice-totals-are-money", invoice_totals_are_money),
        ("report", report),
        (
            "report-sections-carry-their-region",
            report_sections_carry_their_region,
        ),
        ("letter", letter),
        ("letter-filter", letter_filter),
        ("directory", directory),
        ("field-result-length", field_result_length),
        ("text-after-a-field", text_after_a_field),
        ("media-parts-are-unique", media_parts_are_unique),
        ("packages-are-well-formed", packages_are_well_formed),
        ("header-picture-namespaces", header_picture_namespaces),
    ]
}

#[test]
fn example_documents() -> Test {
    for (name, case) in cases() {
        case().map_err(|message| format!("{name}: {message}"))?;
    }
    Ok(())
}
