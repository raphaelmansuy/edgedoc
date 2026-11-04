//! The README snippets, compiled and run.
//!
//! Each test holds one snippet from `README.md`. A snippet that stops compiling
//! or stops producing the stated output fails here, so the README cannot drift.

use edgedoc::{
    CsvOptions, Document, Join, MergeKind, MissingField, Mm, Paragraph, Pt, Records, StoryRef,
    Style, Table,
};

type Test = Result<(), String>;

fn err(err: impl std::fmt::Display) -> String {
    err.to_string()
}

fn fail(message: impl Into<String>) -> Test {
    Err(message.into())
}

/// The first snippet in the README: create a document.
fn create() -> Test {
    let mut doc = Document::builder()
        .core(edgedoc::CoreProps::new().title("Invoice"))
        .page(edgedoc::Page::a4().margins(Mm(20.0)))
        .style(Style::paragraph("Heading1").size(Pt(16.0)).bold())
        .paragraph(
            Paragraph::new()
                .style("Heading1")
                .text("Invoice ")
                .merge_field("Number"),
        )
        .table(
            Table::new()
                .widths([Mm(90.0), Mm(30.0), Mm(40.0)])
                .header(["Description", "Qty", "Amount"])
                .row(["Widget", "2", "40.00"]),
        )
        .build()
        .map_err(err)?;

    doc.edit(edgedoc::EditOptions::default(), |edit| {
        edit.body().paragraph_after_last(|para| {
            para.text("Total: ")?;
            para.merge_field("Total")?;
            Ok(())
        })
    })
    .map_err(err)?;

    let bytes = doc.write_bytes().map_err(err)?;
    if bytes.len() < 1000 {
        return fail(format!("{} bytes", bytes.len()));
    }
    let reread = Document::read_bytes(&bytes).map_err(err)?;
    let text = reread.plain_text(StoryRef::Body);
    if !text.contains("Widget") || !text.contains("Total") {
        return fail(text);
    }
    Ok(())
}

/// The merge snippet in the README.
fn merge() -> Test {
    let template = Document::builder()
        .paragraph(Paragraph::new().text("Dear ").merge_field("Name").text(","))
        .paragraph(
            Paragraph::new()
                .text("Your balance is ")
                .merge_field("Balance")
                .text("."),
        )
        .build()
        .map_err(err)?;

    let csv = b"Name,Balance\nAda Lovelace,120.00\nGrace Hopper,80.50\n";
    let records = Records::from_csv(csv, &CsvOptions::default()).map_err(err)?;
    let batch = template
        .merge()
        .records(records)
        .kind(MergeKind::Letters)
        .missing(MissingField::Error)
        .finish()
        .map_err(err)?;
    let letters = batch.join(Join::Auto).map_err(err)?;

    let text = letters.plain_text(StoryRef::Body);
    if !text.contains("Dear Ada Lovelace,") || !text.contains("120.00") {
        return fail(text);
    }
    if !text.contains("Dear Grace Hopper,") {
        return fail(text);
    }
    let bytes = letters.write_bytes().map_err(err)?;
    let reread = Document::read_bytes(&bytes).map_err(err)?;
    if reread.plain_text(StoryRef::Body) != text {
        return fail("the saved letters do not match the merged letters");
    }
    Ok(())
}

/// The inspection snippet in the README.
fn inspect() -> Test {
    let doc = Document::builder()
        .paragraph(Paragraph::new().text("Hello ").merge_field("Name"))
        .build()
        .map_err(err)?;
    let bytes = doc.write_bytes().map_err(err)?;
    let doc = Document::read_bytes(&bytes).map_err(err)?;
    if doc.merge_field_names() != vec!["Name".to_string()] {
        return fail(format!("{:?}", doc.merge_field_names()));
    }
    if doc.plain_text(StoryRef::Body) != "Hello «Name»" {
        return fail(doc.plain_text(StoryRef::Body));
    }
    Ok(())
}

/// The one-call merge in the README.
fn one_call() -> Test {
    let template = Document::builder()
        .paragraph(Paragraph::new().text("Hello ").merge_field("Name"))
        .build()
        .map_err(err)?
        .write_bytes()
        .map_err(err)?;
    let bytes = Document::merge_letters_bytes(&template, b"Name\nAda\n").map_err(err)?;
    let doc = Document::read_bytes(&bytes).map_err(err)?;
    if doc.plain_text(StoryRef::Body) != "Hello Ada" {
        return fail(doc.plain_text(StoryRef::Body));
    }
    Ok(())
}

#[test]
fn readme_snippets() -> Test {
    for (name, case) in [
        ("create", create as fn() -> Test),
        ("merge", merge),
        ("inspect", inspect),
        ("one-call", one_call),
    ] {
        case().map_err(|message| format!("{name}: {message}"))?;
    }
    Ok(())
}
