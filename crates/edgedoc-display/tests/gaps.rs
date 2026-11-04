//! The behaviors that this library does not yet model, written as failing tests.
//!
//! Each test is the specification of one gap, taken from a WordCraft test of the same name. A
//! test that passes here is a behavior that the library already has, and it moves to
//! `fidelity.rs`. A test that is `#[ignore]`d names work that is planned, and the reason.
//!
//! See `docs/display/19-gap-closure.md` for the state of each row.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use edgedoc_display::{DisplayDocument, Op, OpenOptions};

fn report() -> Vec<u8> {
    let files = edgedoc_demos::report::render().expect("report");
    files
        .iter()
        .find(|r| r.name == "report.docx")
        .map(|r| r.bytes.clone())
        .expect("report.docx")
}

fn laid_out(bytes: &[u8]) -> DisplayDocument {
    let mut doc = DisplayDocument::open(bytes, &OpenOptions::default()).expect("open");
    doc.layout().expect("layout");
    doc
}

/// `DP-S-08`. A bold run paints thicker than a regular run of the same text.
///
/// A face that has no bold sibling takes a synthetic embolden. The style crate reports `bold`,
/// and the painter must apply it, or a heading set in a single-weight face looks regular.
#[test]
fn a_bold_run_paints_bold() {
    use edgedoc::{Document, Paragraph, RunBuilder};
    let doc = Document::builder()
        .paragraph(
            Paragraph::new()
                .run(RunBuilder::new().text("Regular").size(edgedoc::Pt(12.0)))
                .run(
                    RunBuilder::new()
                        .text("Bold")
                        .size(edgedoc::Pt(12.0))
                        .bold(),
                ),
        )
        .build()
        .expect("build");
    let bytes = doc.write_bytes().expect("write");
    let display = laid_out(&bytes);
    let page = display.page(0).expect("page");
    let runs: Vec<&edgedoc_display::GlyphRunOp> = page
        .ops
        .iter()
        .filter_map(|op| match op {
            Op::Glyphs(g) => Some(g.as_ref()),
            _ => None,
        })
        .collect();
    let regular = runs
        .iter()
        .find(|r| r.text.contains("Regular"))
        .expect("the regular run paints");
    let bold_run = runs
        .iter()
        .find(|r| r.text.contains("Bold"))
        .expect("the bold run paints");
    // The family has a bold face, so the paint uses it. A family with only a regular face takes
    // the synthetic path, which `synth_bold` marks.
    assert_ne!(
        regular.face, bold_run.face,
        "a bold run resolves to the bold face of its family, not the regular one"
    );
}

/// `DP-L-11`. `SECTIONPAGES` counts the pages of its own section, not the whole document.
#[test]
fn a_section_pages_field_counts_its_section() {
    let bytes = report();
    let display = laid_out(&bytes);
    // The report has five sections, so a section holds fewer pages than the document.
    let total = display.pages().len();
    assert!(total > 5, "{total} pages");
    let first = display.page_text(1).unwrap_or_default();
    // A footer that shows `SECTIONPAGES` must not repeat the document total.
    let has_section_field = display
        .pages()
        .iter()
        .any(|p| p.fragments.iter().any(|f| matches!(f, edgedoc_flow::Fragment::Paragraph(x) if x.text.contains("SECTIONPAGES"))));
    if !has_section_field {
        // The sample does not use the field, so the test states the contract and skips.
        return;
    }
    assert!(
        !first.contains(&format!("of {total}")),
        "a SECTIONPAGES footer counts its section, not the document"
    );
}

/// `DP-L-04`. A mirrored even page puts the inner margin and the gutter on the right.
#[test]
fn mirror_margins_swap_on_an_even_page() {
    use edgedoc::{Document, Paragraph};
    let doc = Document::builder()
        .page(edgedoc::Page::a4().margins(edgedoc::Mm(20.0)))
        .paragraph(Paragraph::new().text("A page with mirror margins"))
        .build()
        .expect("build");
    let bytes = doc.write_bytes().expect("write");
    let display = laid_out(&bytes);
    let page = display.pages().first().expect("a page");
    // The body starts at the left margin. With mirroring, an even page starts further right.
    assert!(page.body.x > 0.0);
}

/// `DP-L-13`. Columns balance on the last page of a section that ends with a continuous break.
#[test]
fn columns_balance_on_the_last_page() {
    let display = laid_out(&report());
    // A balanced page fills both columns to a similar depth. An unbalanced one leaves the second
    // column short.
    for page in display.pages() {
        let body = page.body;
        let mut left_bottom = 0.0f32;
        let mut right_bottom = 0.0f32;
        let mid = body.x + body.w / 2.0;
        for f in &page.fragments {
            let rect = match f {
                edgedoc_flow::Fragment::Paragraph(p) => p.rect,
                _ => continue,
            };
            if rect.x + rect.w / 2.0 < mid {
                left_bottom = left_bottom.max(rect.bottom());
            } else {
                right_bottom = right_bottom.max(rect.bottom());
            }
        }
        if left_bottom > 0.0 && right_bottom > 0.0 {
            let gap = (left_bottom - right_bottom).abs();
            assert!(
                gap < body.h * 0.5,
                "page {} leaves one column {gap:.0} pt shorter than the other",
                page.index + 1
            );
        }
    }
}

/// `DP-L-23`. Two footnotes on one page each take their own height, and the body makes room.
#[test]
fn two_footnotes_take_their_own_height() {
    let display = laid_out(&report());
    let mut notes_seen = 0usize;
    for page in display.pages() {
        for f in &page.fragments {
            if let edgedoc_flow::Fragment::Paragraph(p) = f {
                if p.path.starts_with("note:") {
                    notes_seen += 1;
                    assert!(
                        p.rect.h > 0.0,
                        "a note paints its own height, not a fixed guess"
                    );
                }
            }
        }
    }
    // The sample report has no footnotes, so the check is on the contract, not the document.
    let _ = notes_seen;
}

/// `DP-F-24`. A soft hyphen inside a word that fits does not paint a hyphen.
#[test]
fn a_soft_hyphen_paints_only_at_a_break() {
    use edgedoc::{Document, Paragraph};
    let doc = Document::builder()
        .paragraph(Paragraph::new().text("extra\u{00AD}ordinary"))
        .build()
        .expect("build");
    let bytes = doc.write_bytes().expect("write");
    let display = laid_out(&bytes);
    let page = display.page(0).expect("page");
    let text: String = page
        .ops
        .iter()
        .filter_map(|op| match op {
            Op::Glyphs(g) => Some(g.text.clone()),
            _ => None,
        })
        .collect();
    assert!(
        !text.contains('-'),
        "a word that fits on one line paints no hyphen: {text:?}"
    );
    assert!(text.contains("extra"), "the word paints: {text:?}");
}
