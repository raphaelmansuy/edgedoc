//! Fidelity tests: the behaviors that a screenshot cannot state.
//!
//! A test reads a real document, so a failure names the value it read. That is why this file
//! allows `expect` and `unwrap`: they turn a broken fixture into one clear message instead of an
//! index panic three frames later.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
//!
//! Each test names the requirement it locks, and it reads a real document through the facade, so a
//! regression in any crate shows here.

use edgedoc_display::{DisplayDocument, Op, OpenOptions};

/// Render one demo document. A helper is outside a `#[test]`, so it returns a clear `Result`.
fn sample(name: &str) -> Vec<u8> {
    let rendered = match name {
        "report" => edgedoc_demos::report::render(),
        "invoice" => edgedoc_demos::invoice::render(),
        "letter" => edgedoc_demos::letter::render(),
        "price-list" => edgedoc_demos::price_list::render_by_region(),
        other => Err(format!("unknown sample {other}")),
    };
    let Ok(files) = rendered else {
        return Vec::new();
    };
    // The merged document keeps the plain name, and the template carries `-template`.
    let wanted = format!("{name}.docx");
    if let Some(found) = files.iter().find(|r| r.name == wanted) {
        return found.bytes.clone();
    }
    let wanted_template = format!("{name}-template.docx");
    if let Some(found) = files.iter().find(|r| r.name == wanted_template) {
        return found.bytes.clone();
    }
    files.first().map(|r| r.bytes.clone()).unwrap_or_default()
}

/// Open and lay out one sample. A helper is outside a `#[test]`, so a failure returns an empty
/// document and the test asserts on that.
fn laid_out(name: &str) -> DisplayDocument {
    let bytes = sample(name);
    let mut doc = DisplayDocument::open(&bytes, &OpenOptions::default()).expect("the sample opens");
    doc.layout().expect("the sample lays out");
    doc
}

/// `DP-S-08`. A run with an explicit colour paints it, and the colour reaches the display list.
#[test]
fn a_run_paints_its_own_colour() {
    let doc = laid_out("report");
    let mut coloured = 0usize;
    let mut black = 0usize;
    for i in 0..doc.pages().len() {
        let Some(page) = doc.page(i) else { continue };
        for op in &page.ops {
            if let Op::Glyphs(g) = op {
                if g.text.trim().is_empty() {
                    continue;
                }
                if g.paint == edgedoc_geom::Rgba::BLACK {
                    black += 1;
                } else {
                    coloured += 1;
                }
            }
        }
    }
    assert!(black > 0, "the body text paints black");
    assert!(
        coloured > 0,
        "the headings and the muted notes paint their own colour, not black"
    );
}

/// `DP-S-08`. A heading style carries a colour into the run.
#[test]
fn a_heading_is_not_black() {
    let doc = laid_out("report");
    let page = doc.page(1).expect("page 2");
    let heading = page.ops.iter().find_map(|op| match op {
        Op::Glyphs(g) if g.size > 14.0 && !g.text.trim().is_empty() => Some(g),
        _ => None,
    });
    if let Some(g) = heading {
        assert_ne!(
            g.paint,
            edgedoc_geom::Rgba::BLACK,
            "a {:.1} pt heading paints {:?}, not black",
            g.size,
            g.paint
        );
    }
}

/// Shared cell edges follow Wordcraft: each adjoining cell paints the same centered rule.
#[test]
fn a_table_rule_paints_once_per_adjoining_cell() {
    let doc = laid_out("report");
    let page = doc.page(1).expect("page 2");
    // A vertical rule is a fill that is much taller than it is wide.
    let mut verticals = Vec::new();
    for op in &page.ops {
        if let Op::FillRect { rect, .. } = op {
            if rect.w > 0.0 && rect.h > rect.w * 20.0 && rect.h > 8.0 {
                verticals.push(*rect);
            }
        }
    }
    // The KPI strip has three columns and two shared dividers. Each adjoining cell paints
    // its edge, but the two strokes must occupy the same geometry rather than widen it.
    verticals.sort_by(|a, b| a.x.total_cmp(&b.x));
    assert_eq!(
        verticals.len(),
        4,
        "two cell edges per divider: {verticals:?}"
    );
    for pair in verticals.chunks_exact(2) {
        assert!((pair[0].x - pair[1].x).abs() < 0.01, "{pair:?}");
        assert!((pair[0].w - pair[1].w).abs() < 0.01, "{pair:?}");
        assert!((pair[0].y - pair[1].y).abs() < 0.01, "{pair:?}");
        assert!((pair[0].h - pair[1].h).abs() < 0.01, "{pair:?}");
    }
    assert!(verticals[2].x - verticals[0].x > 1.0);
}

/// `DP-F-23`. A line ends at a word boundary, never inside a word.
///
/// This is the check that a screenshot cannot make. The page looked correct while a line ended at
/// `smallest ba` and the next line began at `se.`, because both lines held the right characters.
///
/// The rule reads the character at the break point, which is the first cluster of the next line.
/// The previous character must be a space, a hyphen, or a character that ends a line by itself.
#[test]
fn a_line_ends_at_a_word_boundary() {
    let doc = laid_out("report");
    let mut checked = 0usize;
    for page in doc.pages() {
        for f in &page.fragments {
            let edgedoc_flow::Fragment::Paragraph(p) = f else {
                continue;
            };
            for line in &p.layout.lines {
                // A line that ends the paragraph or a hard break cannot break a word.
                if !matches!(line.end, edgedoc_text::LineEnd::Wrap) {
                    continue;
                }
                // The first cluster that paints after this line is where the next line starts.
                let Some(next_cluster) = p.layout.clusters.get(line.c1) else {
                    continue;
                };
                let at = next_cluster.start.min(p.text.len());
                let before = p.text.get(..at).and_then(|s| s.chars().next_back());
                let after = p.text.get(at..).and_then(|s| s.chars().next());
                // A break after a space, a hyphen, or between two East Asian characters is
                // correct. A break between two Latin letters is a word cut in two.
                let ok = match (before, after) {
                    (Some(b), Some(a)) => {
                        b.is_whitespace()
                            || b == '-'
                            || is_east_asian(b)
                            || is_east_asian(a)
                            || !a.is_alphanumeric()
                    }
                    _ => true,
                };
                assert!(
                    ok,
                    "page {} breaks a word: {:?} | {:?}",
                    page.index + 1,
                    p.text
                        .get(..at)
                        .unwrap_or("")
                        .chars()
                        .rev()
                        .take(18)
                        .collect::<String>(),
                    p.text
                        .get(at..)
                        .unwrap_or("")
                        .chars()
                        .take(18)
                        .collect::<String>()
                );
                checked += 1;
            }
        }
    }
    assert!(checked >= 3, "the check reads {checked} wrapped lines");
}

/// True for a character that a line may break next to without a space.
fn is_east_asian(c: char) -> bool {
    matches!(c as u32,
        0x2E80..=0x2FFF | 0x3001..=0x303F | 0x3040..=0x30FF | 0x3400..=0x4DBF
        | 0x4E00..=0x9FFF | 0xF900..=0xFAFF | 0xFF01..=0xFFEF | 0x20000..=0x3FFFF)
}

/// `DP-L-11`. A `PAGE` field paints the page number of the page that holds it.
#[test]
fn a_page_field_changes_per_page() {
    let doc = laid_out("report");
    let mut found = Vec::new();
    for i in 0..doc.pages().len().min(3) {
        let text = doc.page_text(i).unwrap_or_default();
        let n = i + 1;
        if text.contains(&format!("Page {n} ")) || text.contains(&format!("Page {n} of")) {
            found.push(n);
        }
    }
    assert!(
        found.len() >= 2,
        "at least two pages carry their own number: {found:?}"
    );
}

/// `DP-L-12`. A `NUMPAGES` field paints the real page count, after a second pass.
#[test]
fn a_total_field_matches_the_page_count() {
    let doc = laid_out("report");
    let pages = doc.pages().len();
    // The cover has its own first-page footer without a number. Check every numbered footer.
    let numbered: Vec<_> = (0..pages)
        .filter_map(|i| doc.page_text(i))
        .filter(|text| text.contains("Page "))
        .collect();
    assert!(numbered.len() >= 2, "the report has numbered footers");
    for text in numbered {
        assert!(
            text.contains(&format!("of {pages}")),
            "the footer shows {pages} pages: {text:?}"
        );
    }
    let report = doc.report().expect("a report");
    assert!(report.passes >= 2, "a total field needs a second pass");
}

/// `DP-I-02`. A page has the size of its section, in points.
#[test]
fn a_page_is_a4() {
    let doc = laid_out("report");
    let page = doc.pages().first().expect("a page");
    assert!((page.w - 595.28).abs() < 2.0, "width {}", page.w);
    assert!((page.h - 841.89).abs() < 2.0, "height {}", page.h);
}

/// `DP-L-01`. Every fragment sits inside the page.
#[test]
fn every_fragment_is_inside_the_page() {
    let doc = laid_out("report");
    for page in doc.pages() {
        for f in &page.fragments {
            let rect = match f {
                edgedoc_flow::Fragment::Paragraph(p) => p.rect,
                edgedoc_flow::Fragment::Table(t) => t.rect,
                edgedoc_flow::Fragment::Picture { rect, .. } => *rect,
                edgedoc_flow::Fragment::Placeholder { rect, .. } => *rect,
                edgedoc_flow::Fragment::Rule { rect, .. }
                | edgedoc_flow::Fragment::MarginLabel { rect, .. } => *rect,
            };
            // A header and a footer sit in the margins, so the check allows the page, not the
            // body area.
            assert!(
                rect.x >= -2.0 && rect.x <= page.w + 2.0,
                "{} is outside the page width: {rect:?} of {}",
                f_kind(f),
                page.w
            );
            assert!(
                rect.y >= -40.0 && rect.y <= page.h + 40.0,
                "{} is outside the page height: {rect:?} of {}",
                f_kind(f),
                page.h
            );
        }
    }
}

fn f_kind(f: &edgedoc_flow::Fragment) -> &'static str {
    match f {
        edgedoc_flow::Fragment::Paragraph(_) => "paragraph",
        edgedoc_flow::Fragment::Table(_) => "table",
        edgedoc_flow::Fragment::Picture { .. } => "picture",
        edgedoc_flow::Fragment::Placeholder { .. } => "placeholder",
        edgedoc_flow::Fragment::Rule { .. } => "rule",
        edgedoc_flow::Fragment::MarginLabel { .. } => "margin",
    }
}

/// `DP-G-13`. The text index covers the characters that the page paints.
#[test]
fn the_text_index_covers_the_page_text() {
    let doc = laid_out("report");
    let page = doc.page(0).expect("page 1");
    assert!(!page.text.is_empty(), "the page holds text entries");
    for entry in &page.text {
        assert!(!entry.rect.is_empty(), "an entry covers a rectangle");
        assert!(entry.rect.y >= -20.0 && entry.rect.y <= page.h + 20.0);
    }
}

/// `DP-E-01`. A document lays out inside the time budget on this machine.
#[test]
fn layout_is_inside_the_budget() {
    let bytes = sample("report");
    let mut doc = DisplayDocument::open(&bytes, &OpenOptions::default()).expect("open");
    let report = doc.layout().expect("layout");
    // The native budget is generous: the browser target is the tight one, and CI records it.
    assert!(
        report.elapsed_ms < 30_000.0,
        "layout took {:.0} ms",
        report.elapsed_ms
    );
    assert!(report.pages >= 10, "{} pages", report.pages);
}

/// `DP-I-09`. A document that cannot lay out a part still shows every page.
#[test]
fn a_document_with_a_missing_style_still_pages() {
    let doc = laid_out("price-list");
    assert!(!doc.pages().is_empty());
    let text = doc.all_text();
    assert!(text.chars().filter(|c| c.is_alphabetic()).count() > 20);
}

/// `DP-A-25`. Two runs of one document return equal geometry.
#[test]
fn layout_is_deterministic() {
    let bytes = sample("invoice");
    let mut a = DisplayDocument::open(&bytes, &OpenOptions::default()).expect("open");
    let mut b = DisplayDocument::open(&bytes, &OpenOptions::default()).expect("open");
    a.layout().expect("a");
    b.layout().expect("b");
    assert_eq!(a.pages().len(), b.pages().len());
    for i in 0..a.pages().len() {
        assert_eq!(a.page_text_ops(i), b.page_text_ops(i), "page {i}");
    }
}

/// `DP-G-20`. The PDF painter writes one page per document page.
#[test]
fn every_page_writes_a_pdf_page() {
    let doc = laid_out("invoice");
    let mut pdf = edgedoc_paint::PdfPainter::new();
    for i in 0..doc.pages().len() {
        if let Some(page) = doc.page(i) {
            pdf.paint(page);
        }
    }
    assert_eq!(pdf.page_count(), doc.pages().len());
    assert!(pdf.pages[0].text.contains("Invoice") || pdf.pages[0].text.contains("invoice"));
}

/// `DP-L-43`. The gap between two paragraphs is the larger of the space after and the space
/// before, never their sum. Two paragraphs set with 12 pt after and 12 pt before leave 12 pt.
///
/// A builder document gives the two paragraphs the same style, so the gap reads from one rule.
#[test]
fn paragraph_spacing_is_the_larger_of_after_and_before() {
    use edgedoc::{Document, Paragraph, Twips};
    let mut doc = Document::builder()
        .paragraph(Paragraph::new().text("First paragraph"))
        .paragraph(Paragraph::new().text("Second paragraph"))
        .build()
        .expect("build");
    doc.edit(edgedoc::EditOptions::default(), |edit| {
        for index in 0..2 {
            edit.body().paragraph_at(index, |p| {
                p.paragraph_mut().props.spacing_after = Some(Twips(240));
                p.paragraph_mut().props.spacing_before = Some(Twips(240));
                p.paragraph_mut().props.spacing_before = Some(Twips(240));
                p.paragraph_mut().props.line = Some(Twips(240));
                Ok(())
            })?;
        }
        Ok(())
    })
    .expect("edit");
    let bytes = doc.write_bytes().expect("write");
    let mut display = DisplayDocument::open(&bytes, &OpenOptions::default()).expect("open");
    display.layout().expect("layout");
    let page = display.pages().first().expect("a page");
    let mut tops: Vec<(f32, f32)> = Vec::new();
    for f in &page.fragments {
        if let edgedoc_flow::Fragment::Paragraph(p) = f {
            if p.text.contains("paragraph") {
                // The last line's bottom, which is where the gap starts.
                let last = p.l1.saturating_sub(1);
                let line = p.layout.lines.get(last);
                let ink_bottom = line.map_or(p.rect.y + p.rect.h, |l| {
                    p.rect.y + (l.top - p.layout.lines.first().map_or(0.0, |f| f.top)) + l.height
                });
                tops.push((p.rect.y, ink_bottom));
            }
        }
    }
    tops.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    assert_eq!(tops.len(), 2, "two paragraphs on the page");
    let gap = tops[1].0 - tops[0].1;
    // 12 pt after and 12 pt before leave 12 pt. The sum would be 24, and no space is 0.
    assert!(
        (gap - 12.0).abs() < 1.5,
        "the gap between the paragraphs is {gap:.1} pt, not 12 pt"
    );
}

/// `DP-S-31`. An indent in characters is a multiple of the paragraph mark's size.
#[test]
fn a_character_indent_moves_the_text_right() {
    use edgedoc::{Document, Paragraph};
    let mut doc = Document::builder()
        .paragraph(Paragraph::new().text("An indented paragraph"))
        .build()
        .expect("build");
    doc.edit(edgedoc::EditOptions::default(), |edit| {
        edit.body().paragraph_at(0, |p| {
            p.paragraph_mut().props.indent_left_chars = Some(200);
            Ok(())
        })
    })
    .expect("edit");
    let bytes = doc.write_bytes().expect("write");
    let mut display = DisplayDocument::open(&bytes, &OpenOptions::default()).expect("open");
    display.layout().expect("layout");
    let page = display.page(0).expect("page");
    let first = page.ops.iter().find_map(|op| match op {
        Op::Glyphs(g) if !g.text.trim().is_empty() => Some(g.x),
        _ => None,
    });
    // Two characters at the 11 pt default is 22 pt, so the text starts right of the margin.
    assert!(
        first.unwrap_or(0.0) > 72.0,
        "the paragraph starts right of the margin: {:?}",
        first
    );
}

/// `DP-U-24`. The invoice holds a shaded table header and a right-aligned total.
#[test]
fn an_invoice_paints_a_shaded_header() {
    let doc = laid_out("invoice");
    let page = doc.page(0).expect("page 1");
    let shaded = page.ops.iter().filter(|op| match op {
        Op::FillRect { paint, .. } => {
            matches!(paint, edgedoc_paint::Paint::Solid(c) if c.a > 0 && c.r < 100)
        }
        _ => false,
    });
    assert!(shaded.count() >= 1, "the table header paints a dark fill");
}
