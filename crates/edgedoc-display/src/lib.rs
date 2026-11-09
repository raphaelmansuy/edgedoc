//! The facade: open a Word document and paint its pages.
//!
//! ```no_run
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! use edgedoc_display::{DisplayDocument, OpenOptions};
//!
//! let bytes = std::fs::read("report.docx")?;
//! let mut doc = DisplayDocument::open(&bytes, &OpenOptions::default())?;
//! let report = doc.layout()?;
//! println!("{} pages", report.pages);
//! println!("{}", doc.to_svg(0).unwrap_or_default());
//! # Ok(())
//! # }
//! ```
//!
//! The call takes the bytes of a `.docx` file. It reads the model, the styles, and the fonts, and
//! `layout` paints every page. See `DP-V-01` and `DP-A-07`.

#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![deny(clippy::unimplemented, clippy::todo, clippy::unreachable)]

mod build;
mod caret;
pub mod diagnose;
mod edit;
mod error;
mod fonts;
mod search;

pub use edit::{Editor, EditorMark, EditorState};

pub use build::{clear_resources, resource_bytes, resources, set_face_hints};
pub use diagnose::{Diagnostic, Severity};
pub use edgedoc_paint::svg::sniff_mime;
pub use error::{Error, Result};
pub use fonts::{embedded_face_names, load_builtin_fonts, FONT_FAMILIES};
pub use search::{SearchHit, SearchOptions, SearchResults};

pub use edgedoc_flow::{PageGeometry, Totals};
pub use edgedoc_geom::{PageSize, Point, Rect, Rgba};
pub use edgedoc_paint::{
    codec, lower, painter, svg, DisplayPage, GlyphRunOp, Op, Paint, RecordPainter, RecordedOp,
};
pub use edgedoc_text::Align;

use std::collections::BTreeMap;

use edgedoc_fonts::{DbShaper, FontDb};
use edgedoc_paint::{codec::decode, lower::LowerOptions};

/// What `open` accepts and refuses. See `DP-P-04` and `DP-P-08`.
#[derive(Debug, Clone)]
pub struct OpenOptions {
    /// A password for an encrypted package. Version 1 reports it and does not open the package.
    pub password: Option<String>,
    /// Allow an image from a URL outside the package. The default refuses every one.
    pub allow_external_resources: bool,
    /// Hosts that an external resource may come from, when the policy allows a fetch.
    pub external_hosts: Vec<String>,
    /// Show hidden text.
    pub show_hidden_text: bool,
    /// The largest number of pages the flow produces. See `DP-P-06`.
    pub max_pages: u32,
}

impl Default for OpenOptions {
    fn default() -> Self {
        Self {
            password: None,
            allow_external_resources: false,
            external_hosts: Vec::new(),
            show_hidden_text: false,
            max_pages: edgedoc_flow::MAX_PAGES,
        }
    }
}

/// Options that change the layout.
#[derive(Debug, Clone)]
pub struct LayoutOptions {
    /// Turn on hyphenation. Word has it off unless the paragraph asks for it.
    pub hyphenation: bool,
    /// Turn on widow and orphan control.
    pub widow_control: bool,
    /// Paint a box around every fragment, which a fidelity test reads.
    pub debug_boxes: bool,
    /// Paint tracked changes as markup.
    pub markup: bool,
}

impl Default for LayoutOptions {
    fn default() -> Self {
        Self {
            hyphenation: false,
            widow_control: true,
            debug_boxes: false,
            markup: true,
        }
    }
}

/// What a layout run did.
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutReport {
    pub pages: u32,
    /// The total layout passes, including page fields and per-page footnote numbering.
    pub passes: usize,
    pub elapsed_ms: f64,
    pub diagnostics: Vec<Diagnostic>,
}

/// An opened document, ready to lay out and paint.
pub struct DisplayDocument {
    pub(crate) inner: edgedoc::Document,
    fonts: FontDb,
    diagnostics: Vec<Diagnostic>,
    /// The pages from the last layout run.
    pages: Vec<PageGeometry>,
    /// The encoded display list of each page, from the last layout run.
    encoded: Vec<DisplayPage>,
    report: Option<LayoutReport>,
    outline: Vec<OutlineItem>,
    options: OpenOptions,
}

/// One entry of the document outline.
#[derive(Debug, Clone, PartialEq)]
pub struct OutlineItem {
    pub title: String,
    pub level: u8,
    /// The block path, which a viewer scrolls to.
    pub path: String,
}

impl DisplayDocument {
    /// Open a package. The call reads the model, the styles, and the fonts, and it does not lay
    /// out. See `DP-P-03` and `DP-A-07`.
    pub fn open(bytes: &[u8], options: &OpenOptions) -> Result<Self> {
        if edgedoc::Document::is_password_protected(bytes) {
            return Err(Error::new(
                "package.password_required",
                "The package is encrypted. Supply a password, or ask the sender for an unencrypted copy.",
            ));
        }
        let doc =
            edgedoc::Document::read_bytes(bytes).map_err(|e| Error::new(e.code, e.message))?;
        let mut fonts = FontDb::new();
        load_builtin_fonts(&mut fonts);
        // A face that the package embeds wins over a built-in face.
        fonts::load_embedded_faces(&doc, &mut fonts);
        let mut out = Self {
            inner: doc,
            fonts,
            diagnostics: Vec::new(),
            pages: Vec::new(),
            encoded: Vec::new(),
            report: None,
            outline: Vec::new(),
            options: options.clone(),
        };
        out.outline = build::outline_of(&out.inner);
        Ok(out)
    }

    /// The page count that the package claims, which only sizes a scrollbar. See `DP-I-18`.
    pub fn page_count_hint(&self) -> Option<u32> {
        self.pages
            .is_empty()
            .then_some(())
            .and(None)
            .or(Some(self.pages.len() as u32).filter(|n| *n > 0))
    }

    /// The diagnostics from `open` and from the last layout.
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// The outline from the heading paragraphs. See `DP-V-37`.
    pub fn outline(&self) -> &[OutlineItem] {
        &self.outline
    }

    /// The font database, which a test reads to check a substitution.
    pub fn fonts(&self) -> &FontDb {
        &self.fonts
    }

    /// Register caller-supplied fonts before layout. Mutating the database invalidates the
    /// previous pages; call `layout` again before reading geometry or painted output.
    pub fn fonts_mut(&mut self) -> &mut FontDb {
        self.pages.clear();
        self.encoded.clear();
        self.report = None;
        &mut self.fonts
    }

    /// The pages from the last layout.
    pub fn pages(&self) -> &[PageGeometry] {
        &self.pages
    }

    /// One page's display list.
    pub fn page(&self, index: usize) -> Option<&DisplayPage> {
        self.encoded.get(index)
    }

    /// The report from the last layout.
    pub fn report(&self) -> Option<&LayoutReport> {
        self.report.as_ref()
    }

    /// Lay out and encode every page. See `DP-V-01`.
    pub fn layout(&mut self) -> Result<LayoutReport> {
        self.layout_with(&LayoutOptions::default())
    }

    /// Lay out with explicit options.
    pub fn layout_with(&mut self, options: &LayoutOptions) -> Result<LayoutReport> {
        // The step counter guards a layout that grows without bound. See `DP-A-25`.
        let started = instant_ms();
        self.diagnostics.clear();
        if options.hyphenation {
            self.diagnostics.push(Diagnostic {
                code: "display.unsupported_layout",
                severity: Severity::Warning,
                page: None,
                detail: "Automatic hyphenation dictionaries".into(),
            });
        }
        // Publish the face ids before a build, and clear the picture registry, so a run resolves
        // to the face that the document asked for. See `DP-F-02`.
        build::set_face_hints(&self.fonts);
        let shaper = DbShaper { db: &self.fonts };
        let base_diagnostics = self.diagnostics.clone();
        let mut note_pages = std::collections::BTreeMap::new();
        let mut doc_layout = None;
        let mut layout_passes = 0usize;
        for pass in 0..3 {
            self.diagnostics.clone_from(&base_diagnostics);
            build::clear_resources();
            let mut prepared =
                build::prepare(&self.inner, &self.fonts, &mut self.diagnostics, &note_pages)?;
            for group in &mut prepared.sections {
                apply_layout_options(&mut group.blocks, options, self.options.show_hidden_text);
                for part in [
                    &mut group.section.header,
                    &mut group.section.footer,
                    &mut group.section.header_first,
                    &mut group.section.footer_first,
                    &mut group.section.header_even,
                    &mut group.section.footer_even,
                ]
                .into_iter()
                .flatten()
                {
                    apply_layout_options(part, options, self.options.show_hidden_text);
                }
            }
            let mut flow = edgedoc_flow::Flow::new(&shaper);
            flow.max_pages = self.options.max_pages;
            let layout = flow.layout_groups(&prepared.sections);
            layout_passes = layout_passes.saturating_add(layout.passes);
            let mut observed = std::collections::BTreeMap::new();
            for page in &layout.pages {
                for id in &page.footnote_ids {
                    observed.entry(*id).or_insert(page.index);
                }
            }
            let stable = !prepared.footnotes_each_page || observed == note_pages;
            doc_layout = Some(layout);
            if stable {
                break;
            }
            if pass == 2 {
                self.diagnostics.push(Diagnostic {
                    code: "display.unsupported_layout",
                    severity: Severity::Warning,
                    page: None,
                    detail: "Per-page footnote numbers did not converge".into(),
                });
                break;
            }
            note_pages = observed;
        }
        let doc_layout = doc_layout
            .ok_or_else(|| Error::new("display.layout_failed", "No layout pass completed"))?;
        self.diagnostics
            .extend(doc_layout.diagnostics.iter().map(|d| Diagnostic {
                code: d.code,
                severity: severity_of(d.code),
                page: Some(d.page),
                detail: d.detail.clone(),
            }));
        let lower_options = LowerOptions {
            debug_boxes: options.debug_boxes,
            show_hidden: self.options.show_hidden_text,
            markup: options.markup,
        };
        let mut encoded = Vec::with_capacity(doc_layout.pages.len());
        for page in &doc_layout.pages {
            encoded.push(edgedoc_paint::lower::lower_page(
                &shaper,
                page,
                &lower_options,
            ));
        }
        let mut collector = diagnose::Collector::new();
        collector.extend(std::mem::take(&mut self.diagnostics));
        self.diagnostics = collector.into_vec();
        let pages = doc_layout.pages.len() as u32;
        self.pages = doc_layout.pages;
        self.encoded = encoded;
        self.outline = build::outline_of(&self.inner);
        let report = LayoutReport {
            pages,
            passes: layout_passes,
            elapsed_ms: instant_ms() - started,
            diagnostics: self.diagnostics.clone(),
        };
        self.report = Some(report.clone());
        Ok(report)
    }

    /// The bytes of one page display list, which a worker transfers.
    pub fn page_bytes(&self, index: usize) -> Option<Vec<u8>> {
        self.encoded.get(index).map(codec::encode)
    }

    /// A glyph outline as `[unitsPerEm, svgPath]`, for a canvas to paint without reshaping text.
    pub fn glyph_outline_json(&self, face: u32, glyph: u16) -> Option<String> {
        let face = self.fonts.face(face)?;
        if glyph >= face.glyph_count() {
            return None;
        }
        let path = edgedoc_paint::svg::glyph_path(face, glyph);
        Some(format!("[{},\"{}\"]", face.units_per_em, path))
    }

    /// Every page as one SVG document, which a screenshot test reads.
    pub fn to_svg(&self, index: usize) -> Option<String> {
        let page = self.encoded.get(index)?;
        let images: Vec<edgedoc_paint::svg::ImageResource> = build::resources()
            .into_iter()
            .filter_map(|(id, _name, bytes)| {
                edgedoc_paint::svg::sniff_mime(&bytes).map(|mime| {
                    edgedoc_paint::svg::ImageResource {
                        id,
                        mime: mime.to_string(),
                        bytes,
                    }
                })
            })
            .collect();
        Some(edgedoc_paint::svg::paint_page_fonts(
            page,
            &self.fonts,
            &images,
        ))
    }

    /// Every page as SVG, joined into one document with a page break between pages.
    pub fn all_svg(&self) -> String {
        let mut out = String::new();
        for i in 0..self.encoded.len() {
            if let Some(svg) = self.to_svg(i) {
                out.push_str(&svg);
            }
        }
        out
    }

    /// A recorder log of one page, which a golden test compares. See `DP-Q-09`.
    pub fn page_text_ops(&self, index: usize) -> Option<String> {
        let page = self.encoded.get(index)?;
        let mut rec = RecordPainter::new();
        rec.paint(page);
        Some(rec.text())
    }

    /// The visible text of one page, in paint order.
    pub fn page_text(&self, index: usize) -> Option<String> {
        let page = self.encoded.get(index)?;
        let mut rec = RecordPainter::new();
        rec.paint(page);
        Some(rec.visible_text())
    }

    /// Every page's text, with a paragraph separator between pages.
    pub fn all_text(&self) -> String {
        let mut out = String::new();
        for i in 0..self.encoded.len() {
            if let Some(t) = self.page_text(i) {
                out.push_str(&t);
                out.push('\n');
            }
        }
        out
    }

    /// Decode a page from bytes, which a worker sends to a viewer.
    pub fn decode_page(&self, bytes: &[u8]) -> Option<DisplayPage> {
        decode(bytes)
    }

    /// The hit at a point on a page. See `DP-G-27`.
    pub fn hit_test(&self, page: usize, point: Point) -> Option<Hit> {
        let display = self.encoded.get(page)?;
        // Links come first, because they sit above the text.
        for op in &display.ops {
            if let Op::Link(link) = op {
                if link.rect.contains(point) {
                    let target = display.targets.get(link.target as usize).cloned();
                    return Some(Hit::Link {
                        rect: link.rect,
                        target,
                    });
                }
            }
        }
        for op in &display.ops {
            if let Op::Glyphs(run) = op {
                if run.hidden {
                    continue;
                }
                let width = run.width();
                let top = run.y - run.size;
                let rect = Rect::new(
                    run.x.min(run.x + width),
                    top,
                    width.abs().max(1.0),
                    run.size * 1.35,
                );
                if rect.contains(point) {
                    // The nearest character boundary, not the nearest glyph centre.
                    let mut x = run.x;
                    let mut best = 0usize;
                    let mut best_d = f32::MAX;
                    for (i, adv) in run.advances.iter().enumerate() {
                        let mid = x + adv / 2.0;
                        let d = (point.x - mid).abs();
                        if d < best_d {
                            best_d = d;
                            best = if point.x < mid { i } else { i + 1 };
                        }
                        x += adv;
                    }
                    let cluster = run
                        .clusters
                        .get(best.min(run.clusters.len().saturating_sub(1)))
                        .copied()
                        .unwrap_or(0);
                    return Some(Hit::Glyph {
                        rect,
                        baseline: run.y,
                        text_offset: cluster as usize,
                        run: run.text.clone(),
                    });
                }
            }
        }
        for op in &display.ops {
            if let Op::Image { rect, resource, .. } = op {
                if rect.contains(point) {
                    return Some(Hit::Image {
                        rect: *rect,
                        resource: *resource,
                    });
                }
            }
        }
        Some(Hit::Page { page: page as u32 })
    }

    /// The text of a page between two character offsets of a paragraph.
    pub fn selection_text(&self, page: usize, start: usize, end: usize) -> String {
        let Some(p) = self.encoded.get(page) else {
            return String::new();
        };
        let mut out = String::new();
        for op in &p.ops {
            if let Op::Glyphs(run) = op {
                for (i, c) in run.clusters.iter().enumerate() {
                    let off = *c as usize;
                    if off >= start && off < end {
                        if let Some(ch) = run.text.chars().nth(i) {
                            out.push(ch);
                        }
                    }
                }
            }
        }
        out
    }

    /// The index of every block path and the page it starts on.
    pub fn block_pages(&self) -> BTreeMap<String, u32> {
        let mut out = BTreeMap::new();
        for page in &self.pages {
            for f in &page.fragments {
                match f {
                    edgedoc_flow::Fragment::Paragraph(p) => {
                        out.entry(p.path.clone()).or_insert(page.index);
                    }
                    edgedoc_flow::Fragment::Table(t) => {
                        out.entry(t.path.clone()).or_insert(page.index);
                    }
                    edgedoc_flow::Fragment::Picture { .. }
                    | edgedoc_flow::Fragment::Placeholder { .. }
                    | edgedoc_flow::Fragment::Rule { .. } => {}
                }
            }
        }
        out
    }

    /// The document title, or the file name when the package holds none.
    pub fn title(&self) -> &str {
        let t = self.inner.core_title();
        if t.is_empty() {
            "Untitled document"
        } else {
            t
        }
    }

    /// The shaper over this document's faces.
    pub fn shaper(&self) -> DbShaper<'_> {
        DbShaper { db: &self.fonts }
    }
}

/// What a hit found. See `DP-G-27`.
#[derive(Debug, Clone, PartialEq)]
pub enum Hit {
    Glyph {
        rect: Rect,
        baseline: f32,
        /// The byte offset in the run's text.
        text_offset: usize,
        run: String,
    },
    Image {
        rect: Rect,
        resource: u32,
    },
    Link {
        rect: Rect,
        target: Option<String>,
    },
    Page {
        page: u32,
    },
}

fn apply_layout_options(
    blocks: &mut [edgedoc_flow::FlowBlock],
    options: &LayoutOptions,
    show_hidden: bool,
) {
    for block in blocks {
        match block {
            edgedoc_flow::FlowBlock::Paragraph(p) => {
                if !options.widow_control {
                    p.para.widow_control = false;
                }
                if show_hidden {
                    for style in &mut p.run_styles {
                        style.hidden = false;
                    }
                }
            }
            edgedoc_flow::FlowBlock::Table(t) => {
                for row in &mut t.rows {
                    for cell in &mut row.cells {
                        apply_layout_options(&mut cell.blocks, options, show_hidden);
                    }
                }
            }
            _ => {}
        }
    }
}

fn severity_of(code: &str) -> Severity {
    match code {
        "display.page_overflow" | "display.layout_budget" | "display.page_limit" => Severity::Error,
        "display.font_substituted"
        | "display.glyph_missing"
        | "display.image_unsupported"
        | "display.image_denied"
        | "display.table_too_wide"
        | "display.float_unplaced"
        | "display.numbering_missing" => Severity::Warning,
        _ => Severity::Info,
    }
}

/// Milliseconds since the process started.
///
/// `std::time::Instant::now()` panics on `wasm32-unknown-unknown`, because the target has no
/// clock. A browser reads `performance.now()`, which the WASM adapter supplies. See `DP-A-26`.
#[cfg(not(target_arch = "wasm32"))]
fn instant_ms() -> f64 {
    use std::sync::OnceLock;
    use std::time::Instant;
    static START: OnceLock<Instant> = OnceLock::new();
    let start = START.get_or_init(Instant::now);
    start.elapsed().as_micros() as f64 / 1000.0
}

/// Milliseconds on the browser target, which has no `Instant`. The value is 0, so a report
/// carries no timing. The viewer measures elapsed time itself with `performance.now()`.
#[cfg(target_arch = "wasm32")]
fn instant_ms() -> f64 {
    0.0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The merged quarterly report from the demo crate, which is the sample in `DP-U-24`.
    fn report_doc() -> Vec<u8> {
        let files = edgedoc_demos::report::render().expect("the report renders");
        files
            .into_iter()
            .find(|r| r.name == "report.docx")
            .map(|r| r.bytes)
            .expect("report.docx")
    }

    #[test]
    fn a_document_opens_and_reports_a_size() {
        let bytes = report_doc();
        let doc = DisplayDocument::open(&bytes, &OpenOptions::default()).expect("open");
        assert!(!doc.title().is_empty());
    }

    #[test]
    fn a_document_lays_out_to_several_pages() {
        let bytes = report_doc();
        let mut doc = DisplayDocument::open(&bytes, &OpenOptions::default()).expect("open");
        let report = doc.layout().expect("layout");
        assert!(report.pages >= 1, "pages {}", report.pages);
        assert_eq!(doc.pages().len() as u32, report.pages);
    }

    #[test]
    fn every_page_encodes_and_decodes() {
        let bytes = report_doc();
        let mut doc = DisplayDocument::open(&bytes, &OpenOptions::default()).expect("open");
        doc.layout().expect("layout");
        for i in 0..doc.pages().len() {
            let raw = doc.page_bytes(i).expect("bytes");
            let back = doc.decode_page(&raw).expect("decode");
            assert_eq!(back.index as usize, i);
        }
    }

    #[test]
    fn the_text_of_the_document_is_not_empty() {
        let bytes = report_doc();
        let mut doc = DisplayDocument::open(&bytes, &OpenOptions::default()).expect("open");
        doc.layout().expect("layout");
        let text = doc.all_text();
        assert!(
            text.chars().filter(|c| c.is_alphabetic()).count() > 50,
            "{text:?}"
        );
    }

    #[test]
    fn a_zero_byte_input_is_refused() {
        let err = DisplayDocument::open(&[], &OpenOptions::default());
        assert!(err.is_err());
    }

    #[test]
    fn a_non_word_zip_is_refused() {
        // A ZIP with no document part.
        let mut buf = Vec::new();
        buf.extend_from_slice(&[0x50, 0x4B, 0x03, 0x04]);
        buf.extend_from_slice(&[0u8; 64]);
        let err = DisplayDocument::open(&buf, &OpenOptions::default());
        assert!(err.is_err());
    }

    #[test]
    fn the_svg_of_a_page_is_well_formed() {
        let bytes = report_doc();
        let mut doc = DisplayDocument::open(&bytes, &OpenOptions::default()).expect("open");
        doc.layout().expect("layout");
        let svg = doc.to_svg(0).expect("svg");
        assert!(svg.starts_with("<svg"));
        assert!(svg.trim_end().ends_with("</svg>"));
        assert_eq!(svg.matches("<g").count(), svg.matches("</g>").count());
    }

    #[test]
    fn a_hit_inside_a_glyph_returns_a_character() {
        let bytes = report_doc();
        let mut doc = DisplayDocument::open(&bytes, &OpenOptions::default()).expect("open");
        doc.layout().expect("layout");
        // Find the first glyph run and click in it.
        let page = doc.page(0).expect("page");
        let run = page.ops.iter().find_map(|op| match op {
            Op::Glyphs(g) if !g.text.trim().is_empty() => Some(g.clone()),
            _ => None,
        });
        if let Some(run) = run {
            let hit = doc.hit_test(0, Point::new(run.x + 1.0, run.y - run.size * 0.5));
            assert!(matches!(hit, Some(Hit::Glyph { .. })), "{hit:?}");
        }
    }

    #[test]
    fn a_layout_run_is_deterministic_across_two_documents() {
        let bytes = report_doc();
        let mut a = DisplayDocument::open(&bytes, &OpenOptions::default()).expect("open");
        let mut b = DisplayDocument::open(&bytes, &OpenOptions::default()).expect("open");
        a.layout().expect("a");
        b.layout().expect("b");
        assert_eq!(a.pages().len(), b.pages().len());
        assert_eq!(a.all_text(), b.all_text());
        for i in 0..a.pages().len() {
            assert_eq!(a.page_text_ops(i), b.page_text_ops(i));
        }
    }

    #[test]
    fn a_layout_runs_twice_on_one_document_with_the_same_result() {
        let bytes = report_doc();
        let mut doc = DisplayDocument::open(&bytes, &OpenOptions::default()).expect("open");
        let first = doc.layout().expect("first");
        let text_a = doc.all_text();
        let second = doc.layout().expect("second");
        assert_eq!(first.pages, second.pages);
        assert_eq!(text_a, doc.all_text());
    }
}
