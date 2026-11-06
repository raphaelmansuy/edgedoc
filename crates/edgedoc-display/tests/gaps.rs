//! Typography regressions retained from the original gap audit.
//!
//! Section fields, mirrored margins, and pagination have exercised OOXML fixtures in
//! `rendering_parity.rs`. The old checks used documents without the features they claimed
//! to verify; those checks have been removed. See `docs/display/20-rendering-parity.md`.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use edgedoc_display::{DisplayDocument, Op, OpenOptions};

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
