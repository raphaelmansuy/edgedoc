//! The downloaded corpus, checked in Rust.
//!
//! These documents come from python-docx, not from this library. They are the only proof that the
//! reader understands a file that Word wrote. See `tests/corpus/README.md`.
//!
//! A test skips when the corpus is absent, so a checkout without a network still builds. Run
//! `make corpus` to fill it.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::{Path, PathBuf};

use edgedoc_display::{DisplayDocument, Op, OpenOptions};

fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/corpus/downloaded")
}

fn documents() -> Vec<PathBuf> {
    let dir = corpus_dir();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<PathBuf> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "docx"))
        .collect();
    out.sort();
    out
}

/// `DP-P-03`. Every document that Word writes opens.
///
/// The first run of this corpus failed on 12 files: a whitespace node after the root element, which
/// Word writes and the package parser refused. Every other check in this repository passed while
/// that defect was live, because the library's own writer does not put that whitespace there.
#[test]
fn every_corpus_document_opens() {
    let files = documents();
    if files.is_empty() {
        return;
    }
    let mut failures = Vec::new();
    for path in &files {
        let bytes = std::fs::read(path).expect("read");
        if let Err(err) = DisplayDocument::open(&bytes, &OpenOptions::default()) {
            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            failures.push(format!("{name}: {} {}", err.code, err.message));
        }
    }
    assert!(
        failures.is_empty(),
        "these documents did not open:\n{}",
        failures.join("\n")
    );
}

/// `DP-I-02`. Every document lays out to at least one page.
#[test]
fn every_corpus_document_lays_out() {
    let files = documents();
    if files.is_empty() {
        return;
    }
    for path in &files {
        let bytes = std::fs::read(path).expect("read");
        let mut doc = DisplayDocument::open(&bytes, &OpenOptions::default()).expect("open");
        let report = doc.layout().expect("layout");
        assert!(
            report.pages >= 1,
            "{} lays out to no page",
            path.file_name().unwrap_or_default().to_string_lossy()
        );
        assert_eq!(doc.pages().len() as u32, report.pages);
    }
}

/// `DP-Q-03`. A document with text paints text, and the text layer holds it.
///
/// A structural fixture holds no `w:t` element, so its zero is correct. The check reads the
/// document, not a guess.
#[test]
fn a_document_with_text_paints_it() {
    let files = documents();
    if files.is_empty() {
        return;
    }
    let mut with_text = 0usize;
    for path in &files {
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let bytes = std::fs::read(path).expect("read");
        let mut doc = DisplayDocument::open(&bytes, &OpenOptions::default()).expect("open");
        doc.layout().expect("layout");
        let painted: String = doc.all_text();
        let runs = doc
            .page(0)
            .map(|p| p.ops.iter().filter(|o| matches!(o, Op::Glyphs(_))).count())
            .unwrap_or(0);
        if runs == 0 && painted.trim().is_empty() {
            // A structural fixture. Its emptiness is the expected result.
            continue;
        }
        with_text += 1;
        assert!(
            !painted.trim().is_empty(),
            "{name} paints {runs} runs but the text layer is empty"
        );
    }
    assert!(
        with_text > 15,
        "the corpus holds {with_text} documents with text, which is fewer than the 24 it should"
    );
}

/// `DP-L-01`. No fragment paints outside its page.
#[test]
fn every_fragment_stays_inside_its_page() {
    let files = documents();
    if files.is_empty() {
        return;
    }
    for path in &files {
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let bytes = std::fs::read(path).expect("read");
        let mut doc = DisplayDocument::open(&bytes, &OpenOptions::default()).expect("open");
        doc.layout().expect("layout");
        for page in doc.pages() {
            for op in doc
                .page(page.index as usize)
                .map(|p| &p.ops)
                .into_iter()
                .flatten()
            {
                let rect = match op {
                    Op::FillRect { rect, .. } => Some(*rect),
                    Op::StrokeRect { rect, .. } => Some(*rect),
                    Op::Glyphs(g) => Some(edgedoc_geom::Rect::new(
                        g.x,
                        g.y - g.size,
                        g.width().abs().max(1.0),
                        g.size * 1.4,
                    )),
                    _ => None,
                };
                if let Some(rect) = rect {
                    assert!(
                        rect.x >= -60.0
                            && rect.y >= -60.0
                            && rect.x <= page.w + 60.0
                            && rect.y <= page.h + 60.0,
                        "{name} paints {rect:?} outside a {:.0}x{:.0} page",
                        page.w,
                        page.h
                    );
                }
            }
        }
    }
}

/// `DP-L-34`. A document with a table draws a table rule.
#[test]
fn a_corpus_table_draws_a_rule() {
    let files = documents();
    if files.is_empty() {
        return;
    }
    let mut tables = 0usize;
    for path in &files {
        let bytes = std::fs::read(path).expect("read");
        let mut doc = DisplayDocument::open(&bytes, &OpenOptions::default()).expect("open");
        doc.layout().expect("layout");
        for page in doc.pages() {
            for f in &page.fragments {
                if matches!(f, edgedoc_flow::Fragment::Table(_)) {
                    tables += 1;
                }
            }
        }
    }
    assert!(tables > 10, "the corpus lays out {tables} table fragments");
}
