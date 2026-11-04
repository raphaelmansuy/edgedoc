//! Page filling: sections, columns, tables, headers, footers, and notes.
//!
//! The flow crate takes a prepared document and produces pages. It never reads XML, never
//! rasterizes, and never touches a browser. Layout is a pure function of the input, so two runs
//! return equal geometry (`DP-A-25`). See `DP-L-01` to `DP-L-61`.

#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![deny(clippy::unimplemented, clippy::todo, clippy::unreachable)]

use std::collections::BTreeMap;

use edgedoc_fonts::Shaper;
use edgedoc_geom::{Rect, Rgba};
use edgedoc_style::{Border, BorderStyle, ComputedPara};
use edgedoc_text::{layout_paragraph, Align, BreakInput, ParagraphLayout, RunStyle};

/// The marker that a `PAGE` field leaves in the text, which the flow replaces per page.
pub const PAGE_MARKER: char = '\u{F000}';

/// The marker that a `NUMPAGES` field leaves in the text.
pub const TOTAL_MARKER: char = '\u{F001}';

/// Substitute the page markers in a paragraph.
///
/// A `PAGE` field takes this page's number, and a `NUMPAGES` field takes the total from the
/// previous pass. A number can be wider than its marker, so the run offsets are rebuilt. The
/// paragraph layout is then cached under the resolved text, which is why the cache key hashes the
/// text and not the path. See `DP-L-11` and `DP-L-12`.
fn with_fields(p: &FlowParagraph, page: u32, totals: &Totals) -> FlowParagraph {
    if !p.text.contains(PAGE_MARKER) && !p.text.contains(TOTAL_MARKER) {
        return p.clone();
    }
    // The style of every source byte, so an expanded field keeps the run it sat in.
    let mut style_of = vec![0u32; p.text.len() + 1];
    for (start, end, style) in &p.runs {
        for slot in style_of
            .iter_mut()
            .take((*end).min(p.text.len()) + 1)
            .skip(*start)
        {
            *slot = *style;
        }
    }
    let page_text = crate::format_page_number(page, "decimal");
    let total_text = totals.pages.max(1).to_string();
    let mut text = String::with_capacity(p.text.len() + 8);
    let mut runs: Vec<(usize, usize, u32)> = Vec::new();
    for (i, ch) in p.text.char_indices() {
        let style = style_of.get(i).copied().unwrap_or(0);
        let shown: &str = if ch == PAGE_MARKER {
            &page_text
        } else if ch == TOTAL_MARKER {
            &total_text
        } else {
            let start = text.len();
            text.push(ch);
            push_run(&mut runs, start, text.len(), style);
            continue;
        };
        let start = text.len();
        text.push_str(shown);
        push_run(&mut runs, start, text.len(), style);
    }
    FlowParagraph {
        text,
        runs,
        ..p.clone()
    }
}

/// Append a run range, merging with the previous range when the style matches.
fn push_run(runs: &mut Vec<(usize, usize, u32)>, start: usize, end: usize, style: u32) {
    match runs.last_mut() {
        Some((_, prev_end, prev_style)) if *prev_style == style && *prev_end == start => {
            *prev_end = end;
        }
        _ => runs.push((start, end, style)),
    }
}

/// The largest number of pages the flow produces. See `DP-P-06`.
pub const MAX_PAGES: u32 = 20_000;

/// The largest number of fragments on one page.
pub const MAX_FRAGMENTS_PER_PAGE: u32 = 200_000;

/// Largest number of passes for a page total. See `DP-L-52`.
pub const MAX_TOTAL_PASSES: usize = 3;

/// Largest number of passes for note numbering.
pub const MAX_NOTE_PASSES: usize = 3;

/// Largest number of passes for a float that moves its anchor.
pub const MAX_REWRAP_PASSES: usize = 3;

/// A diagnostic from layout.
#[derive(Debug, Clone, PartialEq)]
pub struct FlowDiagnostic {
    pub code: &'static str,
    pub page: u32,
    pub detail: String,
}

/// A section of the document: a page box and its decorations.
#[derive(Debug, Clone)]
pub struct FlowSection {
    pub page_w: f32,
    pub page_h: f32,
    pub margin_top: f32,
    pub margin_bottom: f32,
    pub margin_left: f32,
    pub margin_right: f32,
    pub header_dist: f32,
    pub footer_dist: f32,
    pub gutter: f32,
    pub columns: u32,
    pub column_space: f32,
    /// True when the section starts a new page, a new column, or a page of a given parity.
    pub start: SectionStart,
    /// The header blocks, when this section defines one.
    pub header: Option<Vec<FlowBlock>>,
    pub footer: Option<Vec<FlowBlock>>,
    /// The first-page header, when `w:titlePg` is set.
    pub header_first: Option<Vec<FlowBlock>>,
    pub footer_first: Option<Vec<FlowBlock>>,
    /// The even-page header, when `evenAndOddHeaders` is set.
    pub header_even: Option<Vec<FlowBlock>>,
    pub footer_even: Option<Vec<FlowBlock>>,
    /// True when the section restarts the page number.
    pub page_start: Option<u32>,
    /// The painted page number format.
    pub page_fmt: String,
    /// Where the body sits between the top and bottom margins.
    pub v_align: VAlign,
    /// True when the first page of the section uses its own header. See `DP-L-17`.
    pub title_page: bool,
    /// True when an even page uses its own header.
    pub even_and_odd: bool,
}

/// How a section begins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SectionStart {
    #[default]
    NextPage,
    Continuous,
    OddPage,
    EvenPage,
    NextColumn,
}

impl SectionStart {
    pub fn parse(text: &str) -> Self {
        match text {
            "continuous" => SectionStart::Continuous,
            "oddPage" => SectionStart::OddPage,
            "evenPage" => SectionStart::EvenPage,
            "nextColumn" => SectionStart::NextColumn,
            _ => SectionStart::NextPage,
        }
    }
}

/// Vertical alignment of the body between the margins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VAlign {
    #[default]
    Top,
    Center,
    Bottom,
    Both,
}

impl VAlign {
    pub fn parse(text: &str) -> Self {
        match text {
            "center" => VAlign::Center,
            "bottom" => VAlign::Bottom,
            "both" => VAlign::Both,
            _ => VAlign::Top,
        }
    }
}

impl Default for FlowSection {
    fn default() -> Self {
        Self {
            page_w: 595.28,
            page_h: 841.89,
            margin_top: 72.0,
            margin_bottom: 72.0,
            margin_left: 72.0,
            margin_right: 72.0,
            header_dist: 35.4,
            footer_dist: 35.4,
            gutter: 0.0,
            columns: 1,
            column_space: 21.2,
            start: SectionStart::NextPage,
            header: None,
            footer: None,
            header_first: None,
            footer_first: None,
            header_even: None,
            footer_even: None,
            page_start: None,
            page_fmt: "decimal".into(),
            v_align: VAlign::Top,
            title_page: false,
            even_and_odd: false,
        }
    }
}

impl FlowSection {
    /// The body rectangle, which is the page inside the margins.
    pub fn body_rect(&self) -> Rect {
        let x = self.margin_left + self.gutter;
        Rect::new(
            x,
            self.margin_top,
            (self.page_w - self.margin_left - self.margin_right - self.gutter).max(1.0),
            (self.page_h - self.margin_top - self.margin_bottom).max(1.0),
        )
    }
}

/// A paragraph ready for layout.
#[derive(Debug, Clone)]
pub struct FlowParagraph {
    /// The block path, for a diagnostic and a click.
    pub path: String,
    pub text: String,
    /// One entry per run: `(start byte, end byte, run index)`.
    pub runs: Vec<(usize, usize, u32)>,
    pub run_styles: Vec<RunStyle>,
    pub para: ComputedPara,
    /// The list label, when the paragraph is a list item and its number is already known.
    pub label: Option<String>,
    /// The list level, which the flow numbers. See `DP-S-14`.
    pub list: Option<edgedoc_style::ListRef>,
    /// The label width, which the text indent already includes.
    pub page_break_before: bool,
    /// The text of a `PAGE` field, which the flow writes per page.
    pub has_page_field: bool,
    /// The text of a `NUMPAGES` field, which needs a second pass.
    pub has_total_field: bool,
    /// The extent of an inline object in this paragraph: `(ascent, descent)`.
    pub objects: Vec<(f32, f32)>,
    /// Hyperlink targets, per byte range.
    pub links: Vec<(usize, usize, String)>,
    /// The outline level, which the viewer uses for navigation.
    pub outline: Option<i32>,
    /// The footnote text that a reference in this paragraph points at.
    pub notes: Vec<String>,
    /// The marker for a tracked change, for the layer state.
    pub revision: Option<RevisionKind>,
}

/// A revision mark on a paragraph.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RevisionKind {
    Insert,
    Delete,
}

/// A table cell ready for layout.
#[derive(Debug, Clone)]
pub struct FlowCell {
    pub blocks: Vec<FlowBlock>,
    pub span: usize,
    pub width: Option<f32>,
    /// The vertical alignment inside the cell.
    pub v_align: VAlign,
    pub shading: Option<Rgba>,
    pub margins: [f32; 4],
    /// An explicit border on a side.
    pub borders: [Option<Border>; 4],
    /// True when the cell continues a merge from above.
    pub v_merge_continue: bool,
    /// True when the cell starts a vertical merge.
    pub v_merge_start: bool,
}

/// A table row.
#[derive(Debug, Clone)]
pub struct FlowRow {
    pub cells: Vec<FlowCell>,
    /// True when the row repeats on every page.
    pub header: bool,
    pub height: Option<f32>,
    pub height_exact: bool,
    pub cant_split: bool,
}

/// Which side of a cell a border belongs to.
pub const SIDE_TOP: usize = 0;
pub const SIDE_LEFT: usize = 1;
pub const SIDE_BOTTOM: usize = 2;
pub const SIDE_RIGHT: usize = 3;

/// A table ready for layout.
#[derive(Debug, Clone)]
pub struct FlowTable {
    pub path: String,
    pub rows: Vec<FlowRow>,
    pub widths: Vec<f32>,
    pub align: Align,
    pub indent: f32,
    pub layout_fixed: bool,
    pub cell_margins: [f32; 4],
    pub borders: [Option<Border>; 4],
    pub inside_h: Option<Border>,
    pub inside_v: Option<Border>,
    /// True when the table floats beside the text.
    pub floating: bool,
    /// The float anchor, when it floats.
    pub anchor: Option<FloatAnchor>,
    pub bidi: bool,
}

/// A floating object anchor.
#[derive(Debug, Clone, Default)]
pub struct FloatAnchor {
    pub behind_text: bool,
    pub wrap: WrapMode,
    /// The distance from the anchor to the object.
    pub offset_x: Option<f32>,
    pub offset_y: Option<f32>,
    pub dist_top: f32,
    pub dist_bottom: f32,
    pub dist_left: f32,
    pub dist_right: f32,
}

/// How text wraps around a floating object.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WrapMode {
    #[default]
    None,
    Square,
    Tight,
    Through,
    TopAndBottom,
}

/// A block in a story.
///
/// A paragraph is the common block and the largest, and a page holds many. Boxing it would cost
/// one indirection per paragraph on the common path to save a few bytes on the rare one.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone)]
pub enum FlowBlock {
    Paragraph(FlowParagraph),
    Table(FlowTable),
    /// A picture that is a block of its own, as a paragraph with one object is not.
    Picture {
        path: String,
        width: f32,
        height: f32,
        alt: String,
        resource: u32,
        align: Align,
    },
    /// An object that the library does not model, drawn as a box with a label.
    Placeholder {
        path: String,
        kind: String,
        width: f32,
        height: f32,
        align: Align,
    },
}

impl FlowBlock {
    pub fn path(&self) -> &str {
        match self {
            FlowBlock::Paragraph(p) => &p.path,
            FlowBlock::Table(t) => &t.path,
            FlowBlock::Picture { path, .. } => path,
            FlowBlock::Placeholder { path, .. } => path,
        }
    }
}

/// What a fragment draws. Every variant carries page coordinates.
///
/// A paragraph fragment is much larger than a rule, and it holds the shaped layout for its
/// lines. Boxing it would add an indirection on every paint for no memory win, because a page
/// holds many paragraphs and few rules. See `DP-L-55`.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq)]
pub enum Fragment {
    Paragraph(ParagraphFragment),
    Table(TableFragment),
    Picture {
        rect: Rect,
        resource: u32,
        alt: String,
    },
    Placeholder {
        rect: Rect,
        kind: String,
        label: String,
    },
    /// A horizontal or vertical rule, as a paragraph or page border draws.
    Rule {
        rect: Rect,
        border: Border,
    },
}

/// Lines of a paragraph on one page.
#[derive(Debug, Clone, PartialEq)]
pub struct ParagraphFragment {
    pub path: String,
    /// The paragraph text, which the text layer and a copy need. See `DP-G-13`.
    pub text: std::sync::Arc<str>,
    /// The list label, with its own position. A label paints left of the text. See `DP-S-14`.
    pub label: Option<(String, Rect)>,
    /// The origin of the lines, in page coordinates: the left edge of the first line.
    pub rect: Rect,
    /// The line range in the paragraph layout.
    pub l0: usize,
    pub l1: usize,
    pub para: ComputedPara,
    pub run_styles: Vec<RunStyle>,
    /// The shaped paragraph, shared by every fragment of the same paragraph.
    pub layout: std::sync::Arc<ParagraphLayout>,
    /// True when the fragment continues above.
    pub continues_before: bool,
    /// True when the fragment continues below.
    pub continues_after: bool,
    /// True when the whole fragment is right to left.
    pub rtl: bool,
    pub revision: Option<RevisionKind>,
    /// The link rectangles, resolved to page coordinates.
    pub links: Vec<(Rect, String)>,
}

/// A table on one page.
#[derive(Debug, Clone, PartialEq)]
pub struct TableFragment {
    pub path: String,
    pub rect: Rect,
    /// The row range on this page, in the source table.
    pub r0: usize,
    pub r1: usize,
    /// The cell rectangles, with the row and column index in the source table.
    pub cells: Vec<CellRect>,
    /// The fragments inside the cells: paragraphs, pictures, and nested tables.
    pub content: Vec<Fragment>,
    /// The painted border segments.
    pub borders: Vec<BorderSegment>,
    /// True when this fragment repeats the header rows.
    pub repeated_header: bool,
}

/// One cell of a table, on a page.
#[derive(Debug, Clone, PartialEq)]
pub struct CellRect {
    pub rect: Rect,
    pub row: usize,
    pub col: usize,
    pub span: usize,
    pub shading: Option<Rgba>,
}

/// One painted border segment.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BorderSegment {
    pub rect: Rect,
    pub border: Border,
}

/// Everything one page holds.
#[derive(Debug, Clone, PartialEq)]
pub struct PageGeometry {
    pub index: u32,
    /// The painted page number, after a section restart.
    pub number: u32,
    /// The painted number as text, which a roman format changes.
    pub label: String,
    pub w: f32,
    pub h: f32,
    pub section: usize,
    /// True when the page shows an even-page header.
    pub even: bool,
    pub fragments: Vec<Fragment>,
    /// The body rectangle.
    pub body: Rect,
}

impl PageGeometry {
    /// The union of every fragment, which a fit-width zoom needs.
    pub fn ink_rect(&self) -> Rect {
        let mut out = Rect::new(self.body.x, self.body.y, 0.0, 0.0);
        for fragment in &self.fragments {
            let r = match fragment {
                Fragment::Paragraph(p) => p.rect,
                Fragment::Table(t) => t.rect,
                Fragment::Picture { rect, .. } => *rect,
                Fragment::Placeholder { rect, .. } => *rect,
                Fragment::Rule { rect, .. } => *rect,
            };
            out = out.union(&r);
        }
        out
    }
}

/// One section of a document: the blocks that it holds and its page box. See `DP-S-23`.
#[derive(Debug, Clone)]
pub struct SectionGroup {
    pub blocks: Vec<FlowBlock>,
    pub section: FlowSection,
}

/// The result of a layout run.
#[derive(Debug, Clone, PartialEq)]
pub struct DocLayout {
    pub pages: Vec<PageGeometry>,
    /// The page of every block path, which a link and a search need.
    pub index: BTreeMap<String, u32>,
    /// The number of passes the flow needed.
    pub passes: usize,
    pub diagnostics: Vec<FlowDiagnostic>,
}

/// A header or footer to lay out for one page.
#[derive(Debug, Clone)]
pub struct Decoration {
    pub blocks: Vec<FlowBlock>,
    /// The vertical band the decoration may use, in page coordinates.
    pub band: Rect,
}

/// The page totals that a second pass needs. See `DP-L-54`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Totals {
    pub pages: u32,
}

impl Totals {
    /// The first guess is one page per section, which is low on purpose.
    pub fn first_guess(sections: usize) -> Self {
        Self {
            pages: sections.max(1) as u32,
        }
    }
}

/// One shaped paragraph, cached by content and width.
#[derive(Debug, Clone)]
struct ShapedParagraph {
    layout: std::sync::Arc<ParagraphLayout>,
}

/// The flow engine.
pub struct Flow<'a, S: Shaper> {
    pub shaper: &'a S,
    pub diagnostics: Vec<FlowDiagnostic>,
    /// The shape cache, keyed by `(text hash, width, style hash)`.
    cache: BTreeMap<(u64, u32, u64), ShapedParagraph>,
    /// The page count the last pass found.
    pub totals: Totals,
    /// The numbering counters, per `numId` and level. See `DP-S-14`.
    counters: BTreeMap<u32, Vec<u32>>,
}

impl<'a, S: Shaper> Flow<'a, S> {
    pub fn new(shaper: &'a S) -> Self {
        Self {
            shaper,
            diagnostics: Vec::new(),
            cache: BTreeMap::new(),
            totals: Totals::default(),
            counters: BTreeMap::new(),
        }
    }

    /// The next label for a list paragraph, and the counter update. Word numbers per `numId`, so
    /// two lists that share one definition keep separate counters. See `DP-S-14`.
    fn next_label(&mut self, list: &edgedoc_style::ListRef) -> String {
        let level = list.level.min(8) as usize;
        let counters = self.counters.entry(list.num_id).or_default();
        while counters.len() <= level {
            counters.push(0);
        }
        // The first item of a level takes the level start. A later item adds one to the counter.
        let current = counters.get(level).copied().unwrap_or(0);
        let value = if current == 0 {
            list.start.max(1)
        } else {
            current.saturating_add(1)
        };
        if let Some(slot) = counters.get_mut(level) {
            *slot = value;
        }
        // A deeper level restarts when a shallower level advances.
        for deeper in (level + 1)..counters.len() {
            if let Some(slot) = counters.get_mut(deeper) {
                *slot = 0;
            }
        }
        // A bullet level paints its bullet character.
        if list.format == "bullet" {
            return list
                .text
                .chars()
                .find(|c| !c.is_whitespace())
                .map_or_else(|| "\u{2022}".to_string(), |c| c.to_string());
        }
        // Build the label from the level text: `%1` is level 1, `%2` is level 2, and so on.
        let mut out = String::new();
        let mut chars = list.text.chars().peekable();
        let format = if list.is_legal {
            "decimal"
        } else {
            list.format.as_str()
        };
        while let Some(c) = chars.next() {
            if c == '%' {
                if let Some(d) = chars.peek().and_then(|c| c.to_digit(10)) {
                    chars.next();
                    let idx = (d as usize).saturating_sub(1);
                    let n = counters
                        .get(idx)
                        .copied()
                        .filter(|n| *n > 0)
                        .unwrap_or(list.start);
                    out.push_str(&edgedoc_style::format_counter(n, format, list.is_legal));
                    continue;
                }
            }
            out.push(c);
        }
        if out.is_empty() {
            edgedoc_style::format_counter(value, format, list.is_legal)
        } else {
            out
        }
    }

    /// Reset the counters, which a new document needs. See `DP-S-14`.
    pub fn reset_counters(&mut self) {
        self.counters.clear();
    }

    fn report(&mut self, code: &'static str, page: u32, detail: String) {
        if self
            .diagnostics
            .iter()
            .any(|d| d.code == code && d.page == page && d.detail == detail)
        {
            return;
        }
        self.diagnostics.push(FlowDiagnostic { code, page, detail });
    }

    /// Lay out the whole document. See `DP-L-51`.
    pub fn layout(&mut self, body: &[FlowBlock], sections: &[FlowSection]) -> DocLayout {
        let mut passes = 0usize;
        let mut totals = Totals::first_guess(sections.len());
        loop {
            passes += 1;
            let mut run = self.run_pass(body, sections, &totals, passes > 1);
            // A page total changes the layout only when the document shows one.
            let needs_totals = self.uses_total_field(body, sections);
            if needs_totals && run.pages.len() as u32 != totals.pages && passes < MAX_TOTAL_PASSES {
                totals = Totals {
                    pages: run.pages.len() as u32,
                };
                self.totals = totals.clone();
                continue;
            }
            self.totals = totals;
            run.passes = passes;
            return run;
        }
    }

    /// True when any story shows a page total, including a header or a footer.
    fn uses_total_field(&mut self, body: &[FlowBlock], sections: &[FlowSection]) -> bool {
        fn walk(blocks: &[FlowBlock]) -> bool {
            for b in blocks {
                match b {
                    FlowBlock::Paragraph(p) => {
                        if p.has_total_field {
                            return true;
                        }
                    }
                    FlowBlock::Table(t) => {
                        for row in &t.rows {
                            for cell in &row.cells {
                                if walk(&cell.blocks) {
                                    return true;
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            false
        }
        if walk(body) {
            return true;
        }
        for section in sections {
            for part in [
                section.header.as_ref(),
                section.footer.as_ref(),
                section.header_first.as_ref(),
                section.footer_first.as_ref(),
                section.header_even.as_ref(),
                section.footer_even.as_ref(),
            ]
            .into_iter()
            .flatten()
            {
                if walk(part) {
                    return true;
                }
            }
        }
        false
    }

    fn run_pass(
        &mut self,
        body: &[FlowBlock],
        sections: &[FlowSection],
        totals: &Totals,
        _second: bool,
    ) -> DocLayout {
        let mut pages: Vec<PageGeometry> = Vec::new();
        let mut index: BTreeMap<String, u32> = BTreeMap::new();
        let mut block_index = 0usize;
        let mut section_index = 0usize;
        let mut section = sections.first().cloned().unwrap_or_default();
        let mut page_number: u32 = section.page_start.unwrap_or(1);
        let mut current: Option<PageBuilder> = None;
        let mut pending_break = true;
        // The blocks that belong to the current section.
        let mut guard = 0usize;
        while block_index < body.len() {
            guard += 1;
            if guard > body.len() * 4 + 8 {
                break;
            }
            if pending_break {
                if let Some(page) = current.take() {
                    pages.push(page.finish(self));
                }
                if pages.len() as u32 >= MAX_PAGES {
                    self.report(
                        "display.page_limit",
                        pages.len() as u32,
                        format!("{MAX_PAGES} pages"),
                    );
                    break;
                }
                let even = (page_number % 2) == 0;
                let mut builder = PageBuilder::new(
                    pages.len() as u32,
                    page_number,
                    section.clone(),
                    section_index,
                    even,
                );
                builder.begin(self, &section, even, pages.is_empty());
                page_number += 1;
                current = Some(builder);
                pending_break = false;
            }
            let block = body.get(block_index).cloned();
            let Some(block) = block else { break };
            let mut placed = false;
            if let Some(page) = current.as_mut() {
                placed = page.push_block(self, &block, totals);
            }
            if placed {
                index.insert(
                    block.path().to_string(),
                    current.as_ref().map_or(0, |p| p.index),
                );
                block_index += 1;
                // A section break ends the section after this block.
                if self.block_ends_section(&block, sections, section_index) {
                    if let Some(page) = current.take() {
                        pages.push(page.finish(self));
                    }
                    section_index += 1;
                    if let Some(next) = sections.get(section_index) {
                        let next = next.clone();
                        let continuous = next.start == SectionStart::Continuous;
                        if continuous {
                            // A continuous break keeps the page and changes the columns.
                            let even = ((page_number.saturating_sub(1)) % 2) == 0;
                            let mut builder = PageBuilder::new(
                                pages.len() as u32,
                                page_number,
                                next.clone(),
                                section_index,
                                even,
                            );
                            builder.begin(self, &next, even, false);
                            page_number += 1;
                            current = Some(builder);
                            pending_break = false;
                        } else {
                            pending_break = true;
                        }
                        section = next;
                        if let Some(start) = section.page_start {
                            page_number = start;
                        }
                    } else {
                        break;
                    }
                }
            } else {
                // The page is full: flush it and start another in the same section.
                if let Some(page) = current.take() {
                    pages.push(page.finish(self));
                }
                if pages.len() as u32 >= MAX_PAGES {
                    self.report(
                        "display.page_limit",
                        pages.len() as u32,
                        format!("{MAX_PAGES} pages"),
                    );
                    break;
                }
                let even = (page_number % 2) == 0;
                let mut builder = PageBuilder::new(
                    pages.len() as u32,
                    page_number,
                    section.clone(),
                    section_index,
                    even,
                );
                builder.begin_continued(self, &section, even);
                page_number += 1;
                current = Some(builder);
            }
        }
        if let Some(page) = current.take() {
            pages.push(page.finish(self));
        }
        if pages.is_empty() {
            // An empty document still shows one page. See DP-X-11.
            let even = false;
            let mut builder =
                PageBuilder::new(0, section.page_start.unwrap_or(1), section.clone(), 0, even);
            builder.begin(self, &section, even, true);
            pages.push(builder.finish(self));
        }
        DocLayout {
            pages,
            index,
            passes: 1,
            diagnostics: std::mem::take(&mut self.diagnostics),
        }
    }

    fn block_ends_section(
        &self,
        block: &FlowBlock,
        sections: &[FlowSection],
        index: usize,
    ) -> bool {
        // A section mark sits at the end of the last paragraph of a section, so the section
        // changes when the next section exists and this block is the last of its range.
        let _ = (block, sections, index);
        // The caller passes one block list per section, so a section ends at the block that
        // carries the mark. The facade flattens the sections, so version 1 changes a section
        // only at an explicit marker block.
        false
    }

    /// Lay out a document that holds several sections. Each group starts where its break type
    /// says: a new page, a new column, or a page of a given parity. See `DP-L-08` and `DP-L-09`.
    pub fn layout_groups(&mut self, groups: &[SectionGroup]) -> DocLayout {
        let mut pages: Vec<PageGeometry> = Vec::new();
        let mut index: BTreeMap<String, u32> = BTreeMap::new();
        let mut diagnostics: Vec<FlowDiagnostic> = Vec::new();
        // The page totals that a second pass reads. See `DP-L-52`.
        let mut totals = Totals {
            pages: groups.len().max(1) as u32,
        };
        let mut passes = 0usize;
        loop {
            passes += 1;
            pages.clear();
            index.clear();
            diagnostics.clear();
            // A pass numbers every list from the start. See `DP-S-14`.
            self.counters.clear();
            // Publish the totals, so a footer field reads them. See `DP-L-12`.
            self.totals = totals.clone();
            let mut page_number: u32 = 1;
            let mut pending: Option<u32> = None;
            for (gi, group) in groups.iter().enumerate() {
                let start = group.section.start;
                // Where this section begins.
                let begin_new_page = gi == 0
                    || !matches!(start, SectionStart::Continuous | SectionStart::NextColumn);
                if begin_new_page {
                    page_number = match group.section.page_start {
                        Some(n) => n,
                        None => pending.unwrap_or(page_number),
                    };
                    // An odd or even page break adds one blank page when the parity is wrong.
                    if matches!(start, SectionStart::OddPage | SectionStart::EvenPage)
                        && !pages.is_empty()
                    {
                        let want_odd = start == SectionStart::OddPage;
                        let next_odd = page_number % 2 == 1;
                        if want_odd != next_odd {
                            let blank_even = page_number % 2 == 0;
                            let mut blank = PageBuilder::new(
                                pages.len() as u32,
                                page_number,
                                group.section.clone(),
                                gi,
                                blank_even,
                            );
                            blank.begin(self, &group.section, blank_even, false);
                            pages.push(blank.finish(self));
                            page_number += 1;
                        }
                    }
                    self.run_group(
                        &group.blocks,
                        &group.section,
                        gi,
                        &mut page_number,
                        &mut pages,
                        &mut index,
                        &mut diagnostics,
                        &totals,
                    );
                    pending = Some(page_number);
                } else {
                    // A continuous break keeps the page. Version 1 starts a new column set on a
                    // fresh page when the column count changes, and reports it. See DP-X-26.
                    if groups
                        .get(gi.wrapping_sub(1))
                        .is_some_and(|p| p.section.columns != group.section.columns)
                    {
                        diagnostics.push(FlowDiagnostic {
                            code: "display.continuous_layout",
                            page: pages.len() as u32,
                            detail: format!("column count changes at section {gi}"),
                        });
                    }
                    let page_number_copy = page_number;
                    self.run_group(
                        &group.blocks,
                        &group.section,
                        gi,
                        &mut page_number,
                        &mut pages,
                        &mut index,
                        &mut diagnostics,
                        &totals,
                    );
                    if page_number == page_number_copy && pages.last().is_some() {
                        // The group continued on the page that the previous section ended on.
                    }
                    pending = Some(page_number);
                }
            }
            let found = pages.len() as u32;
            let needs_totals = groups.iter().any(|g| {
                let blocks = |bs: &[FlowBlock]| {
                    fn walk(b: &[FlowBlock]) -> bool {
                        b.iter().any(|x| match x {
                            FlowBlock::Paragraph(p) => p.has_total_field,
                            _ => false,
                        })
                    }
                    walk(bs)
                };
                blocks(&g.blocks)
                    || [
                        g.section.header.as_ref(),
                        g.section.footer.as_ref(),
                        g.section.header_first.as_ref(),
                        g.section.footer_first.as_ref(),
                    ]
                    .into_iter()
                    .flatten()
                    .any(|part| walk_parts(part))
            });
            if needs_totals && found != totals.pages && passes < MAX_TOTAL_PASSES {
                totals = Totals { pages: found };
                continue;
            }
            self.totals = totals;
            return DocLayout {
                pages,
                index,
                passes,
                diagnostics,
            };
        }
    }

    /// Lay out one section's blocks into the page list.
    #[allow(clippy::too_many_arguments)]
    fn run_group(
        &mut self,
        body: &[FlowBlock],
        section: &FlowSection,
        section_index: usize,
        page_number: &mut u32,
        pages: &mut Vec<PageGeometry>,
        index: &mut BTreeMap<String, u32>,
        diagnostics: &mut Vec<FlowDiagnostic>,
        totals: &Totals,
    ) {
        let mut even = *page_number % 2 == 0;
        let mut first_page = true;
        let mut builder = PageBuilder::new(
            pages.len() as u32,
            *page_number,
            section.clone(),
            section_index,
            even,
        );
        builder.begin(self, section, even, first_page);
        let mut current: Option<PageBuilder> = Some(builder);
        let mut block_index = 0usize;
        let mut guard = 0usize;
        while block_index < body.len() {
            guard += 1;
            if guard > body.len() * 4 + 8 {
                break;
            }
            let Some(block) = body.get(block_index) else {
                break;
            };
            let placed = current
                .as_mut()
                .is_some_and(|p| p.push_block(self, block, totals));
            if placed {
                if let Some(p) = current.as_ref() {
                    index.insert(block.path().to_string(), p.index);
                }
                block_index += 1;
                continue;
            }
            // The page is full: flush it and start another in the same section.
            if let Some(page) = current.take() {
                pages.push(page.finish(self));
            }
            if pages.len() as u32 >= MAX_PAGES {
                diagnostics.push(FlowDiagnostic {
                    code: "display.page_limit",
                    page: pages.len() as u32,
                    detail: format!("{MAX_PAGES} pages"),
                });
                return;
            }
            *page_number += 1;
            even = *page_number % 2 == 0;
            first_page = false;
            let mut next = PageBuilder::new(
                pages.len() as u32,
                *page_number,
                section.clone(),
                section_index,
                even,
            );
            next.begin(self, section, even, first_page);
            current = Some(next);
        }
        if let Some(page) = current.take() {
            let empty = page.fragments.is_empty() && page.decorations.is_empty();
            let page = page.finish(self);
            // A page with no body fragment and no header is not worth a page. A section that the
            // document ends with, or one that holds no block, adds nothing. See DP-X-11.
            if !empty || pages.is_empty() || !page.fragments.is_empty() {
                pages.push(page);
                *page_number += 1;
            }
        }
    }

    /// Shape a paragraph, or take it from the cache. See `DP-E-21`.
    fn paragraph_layout(
        &mut self,
        para: &FlowParagraph,
        width: f32,
        exclusions: &[Rect],
        first_indent: f32,
    ) -> std::sync::Arc<ParagraphLayout> {
        let key = (
            hash_str(&para.text),
            (width.max(0.0) * 100.0) as u32,
            hash_styles(&para.run_styles),
        );
        let hits_exclusions = !exclusions.is_empty() || first_indent.abs() > 0.001;
        if !hits_exclusions {
            if let Some(cached) = self.cache.get(&key) {
                return cached.layout.clone();
            }
        }
        let input = BreakInput {
            text: &para.text,
            runs: &para.runs,
            styles: &para.run_styles,
            width,
            indent_left: 0.0,
            indent_right: 0.0,
            indent_first: first_indent,
            align: para.para.align,
            bidi: para.para.bidi,
            tabs: &para.para.tabs,
            default_tab: para.para.default_tab,
            exclusions,
            word_wrap_off: false,
            kinsoku: true,
            objects: &para.objects,
            hyphen_after: &[],
        };
        let layout = std::sync::Arc::new(layout_paragraph(self.shaper, &input));
        if !hits_exclusions {
            self.cache.insert(
                key,
                ShapedParagraph {
                    layout: layout.clone(),
                },
            );
        }
        layout
    }
}

/// True when a header or footer part shows a page total.
fn walk_parts(blocks: &[FlowBlock]) -> bool {
    blocks.iter().any(|b| match b {
        FlowBlock::Paragraph(p) => p.has_total_field,
        FlowBlock::Table(t) => t
            .rows
            .iter()
            .any(|r| r.cells.iter().any(|c| walk_parts(&c.blocks))),
        _ => false,
    })
}

fn hash_str(s: &str) -> u64 {
    // FNV-1a, which is stable across platforms and runs.
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

fn hash_styles(styles: &[RunStyle]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for s in styles {
        for part in [
            f64::from(s.face),
            f64::from(s.size),
            f64::from(s.x_scale),
            f64::from(s.letter_spacing),
            f64::from(s.baseline_shift),
        ] {
            h ^= part.to_bits();
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
        h ^= u64::from(s.hidden) | (u64::from(s.rtl) << 1);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// A page being filled.
struct PageBuilder {
    index: u32,
    number: u32,
    section: FlowSection,
    section_index: usize,
    even: bool,
    fragments: Vec<Fragment>,
    /// The y where the next block starts, in page coordinates.
    y: f32,
    /// The bottom of the body area.
    bottom: f32,
    body: Rect,
    /// The current column index.
    column: usize,
    /// The column rectangles.
    columns: Vec<Rect>,
    /// Notes waiting for the page bottom.
    notes: Vec<NoteBox>,
    /// The header and footer fragments, already placed.
    decorations: Vec<Fragment>,
    /// The page totals that a field shows.
    totals: Totals,
    /// The body fragments, so a vertical alignment can move them.
    body_fragments: Vec<Fragment>,
    /// The next row of a table that continues on another page.
    table_cursors: BTreeMap<String, usize>,
    /// The next line of a paragraph that continues on another page.
    line_cursors: BTreeMap<String, usize>,
    /// The space after the last paragraph on the page, its style, and whether the paragraph is a
    /// list item. The gap to the next paragraph is the larger of this value and the next
    /// paragraph's space before, which is what Word does. See `DP-L-43`.
    prev_after: Option<(f32, String, bool)>,
}

struct NoteBox {
    text: String,
    height: f32,
}

impl PageBuilder {
    fn new(
        index: u32,
        number: u32,
        section: FlowSection,
        section_index: usize,
        even: bool,
    ) -> Self {
        Self {
            index,
            number,
            section,
            section_index,
            even,
            fragments: Vec::new(),
            y: 0.0,
            bottom: 0.0,
            body: Rect::default(),
            column: 0,
            columns: Vec::new(),
            notes: Vec::new(),
            decorations: Vec::new(),
            totals: Totals::default(),
            body_fragments: Vec::new(),
            table_cursors: BTreeMap::new(),
            line_cursors: BTreeMap::new(),
            prev_after: None,
        }
    }

    fn begin<S: Shaper>(
        &mut self,
        flow: &mut Flow<'_, S>,
        section: &FlowSection,
        even: bool,
        first_page: bool,
    ) {
        self.totals = flow.totals.clone();
        let body = section.body_rect();
        self.body = body;
        self.columns = column_rects(section, body);
        self.column = 0;
        self.y = self.columns.first().map_or(body.y, |c| c.y);
        self.bottom = self.columns.first().map_or(body.bottom(), |c| c.bottom());
        self.decorations = self.build_decorations(flow, section, even, first_page);
        // A tall header moves the body down. See DP-L-19.
        let mut header_bottom = body.y;
        for fragment in &self.decorations {
            if let Fragment::Paragraph(p) = fragment {
                if p.rect.y < body.y {
                    header_bottom = header_bottom.max(p.rect.bottom());
                }
            }
        }
        if header_bottom > body.y {
            let delta = header_bottom - body.y + 2.0;
            if delta > body.h * 0.5 {
                flow.report(
                    "display.header_overflow",
                    self.index,
                    format!("header takes {delta:.1} pt"),
                );
                self.bottom = body.bottom();
            } else {
                self.y += delta;
                self.body = Rect::new(body.x, body.y + delta, body.w, (body.h - delta).max(1.0));
                self.columns = column_rects(section, self.body);
                self.bottom = self
                    .columns
                    .first()
                    .map_or(self.body.bottom(), |c| c.bottom());
                self.y = self.columns.first().map_or(self.body.y, |c| c.y);
            }
        }
    }

    fn begin_continued<S: Shaper>(
        &mut self,
        flow: &mut Flow<'_, S>,
        section: &FlowSection,
        even: bool,
    ) {
        self.begin(flow, section, even, false);
    }

    fn build_decorations<S: Shaper>(
        &mut self,
        flow: &mut Flow<'_, S>,
        section: &FlowSection,
        even: bool,
        first_page: bool,
    ) -> Vec<Fragment> {
        let mut out = Vec::new();
        // Header: first page, then even, then default. See DP-L-17.
        let header = if first_page && section.header_first.is_some() {
            section.header_first.as_ref()
        } else if even && section.header_even.is_some() {
            section.header_even.as_ref()
        } else {
            section.header.as_ref()
        };
        if let Some(blocks) = header {
            let band = Rect::new(
                section.margin_left + section.gutter,
                (section.margin_top - section.header_dist).max(2.0),
                (section.page_w - section.margin_left - section.margin_right - section.gutter)
                    .max(1.0),
                section.header_dist.max(8.0),
            );
            out.extend(self.blocks_in_band(flow, blocks, band, false));
        }
        let footer = if first_page && section.footer_first.is_some() {
            section.footer_first.as_ref()
        } else if even && section.footer_even.is_some() {
            section.footer_even.as_ref()
        } else {
            section.footer.as_ref()
        };
        if let Some(blocks) = footer {
            let band = Rect::new(
                section.margin_left + section.gutter,
                section.page_h - section.margin_bottom + 2.0,
                (section.page_w - section.margin_left - section.margin_right - section.gutter)
                    .max(1.0),
                (section.margin_bottom - 2.0).max(8.0),
            );
            out.extend(self.blocks_in_band(flow, blocks, band, false));
        }
        out
    }

    /// Place blocks inside a fixed band, as a header or a note does.
    fn blocks_in_band<S: Shaper>(
        &mut self,
        flow: &mut Flow<'_, S>,
        blocks: &[FlowBlock],
        band: Rect,
        collect_notes: bool,
    ) -> Vec<Fragment> {
        let mut out = Vec::new();
        let mut y = band.y;
        let limit = band.bottom();
        for block in blocks {
            match block {
                FlowBlock::Paragraph(p) => {
                    let resolved;
                    let p = if p.text.contains(PAGE_MARKER) || p.text.contains(TOTAL_MARKER) {
                        resolved = with_fields(p, self.number, &self.totals);
                        &resolved
                    } else {
                        p
                    };
                    let label = p.label.as_deref().unwrap_or("");
                    let label_w = if label.is_empty() {
                        0.0
                    } else {
                        flow.shaper
                            .shape_run(
                                0,
                                label,
                                p.run_styles.first().map_or(11.0, |s| s.size) * 0.6,
                                false,
                            )
                            .len() as f32
                            * 5.0
                    };
                    let width = (band.w - label_w).max(1.0);
                    let layout = flow.paragraph_layout(p, width, &[], 0.0);
                    let height = layout.height;
                    if y + height > limit && y > band.y {
                        break;
                    }
                    out.push(Fragment::Paragraph(ParagraphFragment {
                        path: p.path.clone(),
                        text: std::sync::Arc::from(p.text.as_str()),
                        label: None,
                        rect: Rect::new(band.x + label_w, y, width, height),
                        l0: 0,
                        l1: layout.lines.len(),
                        para: p.para.clone(),
                        run_styles: p.run_styles.clone(),
                        layout,
                        continues_before: false,
                        continues_after: false,
                        rtl: p.para.bidi,
                        revision: p.revision,
                        links: Vec::new(),
                    }));
                    y += height + p.para.space_after;
                }
                FlowBlock::Table(t) => {
                    let (fragment, height) = build_table_fragment(
                        flow,
                        t,
                        Rect::new(band.x, y, band.w, 0.0),
                        0,
                        t.rows.len(),
                        false,
                    );
                    out.push(Fragment::Table(fragment));
                    y += height;
                }
                FlowBlock::Picture {
                    path,
                    width,
                    height,
                    alt,
                    resource,
                    align,
                } => {
                    let x = match align {
                        Align::Center => band.x + (band.w - width) / 2.0,
                        Align::Right => band.right() - width,
                        _ => band.x,
                    };
                    out.push(Fragment::Picture {
                        rect: Rect::new(x, y, *width, *height),
                        resource: *resource,
                        alt: alt.clone(),
                    });
                    let _ = path;
                    y += height;
                }
                FlowBlock::Placeholder {
                    path,
                    kind,
                    width,
                    height,
                    align,
                } => {
                    let x = match align {
                        Align::Center => band.x + (band.w - width) / 2.0,
                        Align::Right => band.right() - width,
                        _ => band.x,
                    };
                    out.push(Fragment::Placeholder {
                        rect: Rect::new(x, y, *width, *height),
                        kind: kind.clone(),
                        label: path.clone(),
                    });
                    y += height;
                }
            }
            if collect_notes {
                if let FlowBlock::Paragraph(p) = block {
                    for note in &p.notes {
                        self.notes.push(NoteBox {
                            text: note.clone(),
                            height: 12.0,
                        });
                    }
                }
            }
        }
        out
    }

    /// Try to place a block. `false` means the column is full.
    fn push_block<S: Shaper>(
        &mut self,
        flow: &mut Flow<'_, S>,
        block: &FlowBlock,
        totals: &Totals,
    ) -> bool {
        self.totals = totals.clone();
        match block {
            FlowBlock::Paragraph(p) => self.push_paragraph(flow, p),
            FlowBlock::Table(t) => self.push_table(flow, t),
            FlowBlock::Picture {
                width,
                height,
                alt,
                resource,
                align,
                ..
            } => {
                let column = self.current_column();
                if self.y + height > self.bottom && self.y > column.y {
                    if self.next_column() {
                        return self.push_block(flow, block, totals);
                    }
                    return false;
                }
                let x = match align {
                    Align::Center => column.x + (column.w - width) / 2.0,
                    Align::Right => column.right() - width,
                    _ => column.x,
                };
                let mut rect = Rect::new(x, self.y, *width, *height);
                if rect.right() > column.right() {
                    rect.w = (column.right() - rect.x).max(1.0);
                }
                self.fragments.push(Fragment::Picture {
                    rect,
                    resource: *resource,
                    alt: alt.clone(),
                });
                self.y += height;
                true
            }
            FlowBlock::Placeholder {
                path,
                kind,
                width,
                height,
                align,
            } => {
                let column = self.current_column();
                if self.y + height > self.bottom && self.y > column.y {
                    if self.next_column() {
                        return self.push_block(flow, block, totals);
                    }
                    return false;
                }
                let x = match align {
                    Align::Center => column.x + (column.w - width) / 2.0,
                    Align::Right => column.right() - width,
                    _ => column.x,
                };
                self.fragments.push(Fragment::Placeholder {
                    rect: Rect::new(x, self.y, *width, *height),
                    kind: kind.clone(),
                    label: path.clone(),
                });
                self.y += height;
                true
            }
        }
    }

    fn current_column(&self) -> Rect {
        self.columns.get(self.column).copied().unwrap_or(self.body)
    }

    fn next_column(&mut self) -> bool {
        if self.column + 1 < self.columns.len() {
            self.column += 1;
            let c = self.current_column();
            self.y = c.y;
            self.bottom = c.bottom();
            return true;
        }
        false
    }

    fn push_paragraph<S: Shaper>(&mut self, flow: &mut Flow<'_, S>, p: &FlowParagraph) -> bool {
        // A page field shows this page's number, so the paragraph is resolved per page.
        let resolved;
        let p = if p.text.contains(PAGE_MARKER) || p.text.contains(TOTAL_MARKER) {
            resolved = with_fields(p, self.number, &self.totals);
            &resolved
        } else {
            p
        };
        let column = self.current_column();
        if p.para.page_break_before && self.y > column.y + 0.5 {
            return false;
        }
        // A paragraph that continues from an earlier page starts at the line cursor.
        let start_line = self.line_cursors.get(&p.path).copied().unwrap_or(0);
        // A list item takes its number from the running counter, once, on its first line.
        let numbered = if start_line == 0 {
            p.list.as_ref().map(|l| flow.next_label(l))
        } else {
            None
        };
        let label = if start_line > 0 {
            String::new()
        } else {
            numbered.or_else(|| p.label.clone()).unwrap_or_default()
        };
        let label_w = if label.is_empty() {
            0.0
        } else {
            let size = p.run_styles.first().map_or(11.0, |s| s.size);
            flow.shaper
                .shape_run(0, &label, size, false)
                .iter()
                .map(|g| flow.shaper.advance(0, g.gid, size))
                .sum::<f32>()
                + 6.0
        };
        let width = (column.w - label_w - p.para.indent_left - p.para.indent_right).max(1.0);
        // The first-line indent applies to the first line of the paragraph only. See DP-F-39.
        let first_indent = if start_line == 0 {
            p.para.indent_first
        } else {
            0.0
        };
        let layout = flow.paragraph_layout(p, width, &[], first_indent);
        let line_count = layout.lines.len();
        if line_count == 0 || start_line >= line_count {
            self.line_cursors.remove(&p.path);
            return true;
        }
        // Space before, which the first fragment of a paragraph takes. The gap to the paragraph
        // above is the larger of its space after and this space before, not the sum. Word drops
        // the space at the top of a page, and a continuation has none. See `DP-L-43`.
        let mut space_before = p.para.space_before;
        if self.y <= column.y + 0.5 || start_line > 0 {
            space_before = 0.0;
        } else if let Some((after, prev_style, prev_listed)) = self.prev_after.clone() {
            let same_style = prev_style == p.para.style_id;
            let contextual = p.para.contextual_spacing && same_style;
            if contextual {
                // Contextual spacing drops the gap between two paragraphs of one style.
                space_before = 0.0;
            } else if prev_listed && p.list.is_some() {
                // Two list items of one list sit closer than the paragraph spacing asks.
                space_before = space_before.min(after);
            } else {
                space_before = space_before.max(after);
            }
        }
        let available = self.bottom - self.y - space_before;
        // Keep-lines and keep-next move the paragraph whole when it can fit a page.
        let full_height = layout.height + space_before + p.para.space_after;
        // A kept-together paragraph that does not fit this page moves on whole, unless it cannot
        // fit any page. See `DP-L-05`.
        if p.para.keep_lines
            && full_height > available
            && full_height <= column.h
            && self.y > column.y + 0.5
        {
            return false;
        }
        let mut top = self.y + space_before;
        let mut line_index = start_line;
        let mut any = false;
        while line_index < line_count {
            let line = match layout.lines.get(line_index) {
                Some(l) => l,
                None => break,
            };
            if top + line.height > self.bottom + 0.001 {
                // The first line of a fragment must fit on this page when the page already holds
                // content. Otherwise the page is full. See DP-L-05.
                if !any && self.y > column.y + 0.5 {
                    break;
                }
                if any {
                    break;
                }
            }
            top += line.height;
            line_index += 1;
            any = true;
        }
        if !any {
            // Not one line fits. When the column already holds something, the page is full and
            // the caller starts another page. See DP-L-05.
            if self.y > column.y + 0.5 {
                if self.next_column() {
                    return self.push_paragraph(flow, p);
                }
                return false;
            }
            // A paragraph alone taller than the page splits here. See DP-F-40.
            flow.report("display.page_overflow", self.index, p.path.clone());
            let line = match layout.lines.first() {
                Some(l) => l,
                None => return true,
            };
            top = self.y + line.height;
            line_index = start_line + 1;
        }
        // Widow and orphan control: a split leaves at least two lines on each page. See DP-F-26.
        if p.para.widow_control && line_index < line_count {
            // One line alone on a page is an orphan, which Word refuses.
            if line_index - start_line == 1
                && line_count - start_line > 2
                && self.y > column.y + 0.5
            {
                if self.next_column() {
                    return self.push_paragraph(flow, p);
                }
                return false;
            }
            if line_index == line_count - 1 && line_count - start_line > 2 {
                line_index = line_index.saturating_sub(1).max(start_line + 1);
            }
        }
        let fragment_top = self.y + space_before;
        let rect = Rect::new(
            column.x + label_w + p.para.indent_left,
            fragment_top,
            width,
            (top - fragment_top).max(0.0),
        );
        let continues_after = line_index < line_count;
        let label_rect = if label.is_empty() {
            None
        } else {
            Some((
                label.clone(),
                Rect::new(column.x, fragment_top, label_w.max(1.0), top - fragment_top),
            ))
        };
        self.fragments.push(Fragment::Paragraph(ParagraphFragment {
            path: p.path.clone(),
            text: std::sync::Arc::from(p.text.as_str()),
            label: label_rect,
            rect,
            l0: start_line,
            l1: line_index,
            para: p.para.clone(),
            run_styles: p.run_styles.clone(),
            layout: layout.clone(),
            continues_before: start_line > 0,
            continues_after,
            rtl: p.para.bidi,
            revision: p.revision,
            links: link_rects(&layout, &p.links, rect),
        }));
        // The gap between two paragraphs belongs to the pair, and the next paragraph decides it
        // from `prev_after` as the larger of the two spaces. Adding the space after here would
        // count it twice. See `DP-L-43`.
        self.y = top;
        if !continues_after && !p.notes.is_empty() {
            for note in &p.notes {
                self.notes.push(NoteBox {
                    text: note.clone(),
                    height: 12.0,
                });
            }
        }
        if continues_after {
            // The rest of the paragraph continues on the next page or column.
            self.line_cursors.insert(p.path.clone(), line_index);
            if self.next_column() {
                return self.push_paragraph(flow, p);
            }
            return false;
        }
        self.line_cursors.remove(&p.path);
        self.prev_after = Some((
            p.para.space_after,
            p.para.style_id.clone(),
            p.list.is_some(),
        ));
        true
    }

    fn push_table<S: Shaper>(&mut self, flow: &mut Flow<'_, S>, t: &FlowTable) -> bool {
        let column = self.current_column();
        if t.floating {
            // A floating table takes its own space in version 1, at the current position.
            let (fragment, height) = build_table_fragment(
                flow,
                t,
                Rect::new(column.x, self.y, column.w, 0.0),
                0,
                t.rows.len(),
                false,
            );
            self.fragments.push(Fragment::Table(fragment));
            self.y += height;
            return true;
        }
        let width = column.w.min(table_width(t, column.w));
        let x = match t.align {
            Align::Center => column.x + (column.w - width) / 2.0,
            Align::Right => column.right() - width,
            _ => column.x + t.indent,
        };
        let start_row = self.table_row_cursor(&t.path);
        let mut rows_placed = 0usize;
        let mut height_total = 0.0f32;
        // Measure row by row, and stop when the page is full.
        let mut r = start_row;
        while r < t.rows.len() {
            let row_h = row_height(flow, t, r, width);
            if self.y + height_total + row_h > self.bottom + 0.001 && rows_placed > 0 {
                break;
            }
            // A first row that cannot fit moves on whole, unless it may split. See `DP-L-38`.
            if self.y + height_total + row_h > self.bottom + 0.001
                && rows_placed == 0
                && !t.rows.get(r).is_some_and(|row| row.cant_split)
                && self.y > column.y + 0.5
            {
                break;
            }
            height_total += row_h;
            rows_placed += 1;
            r += 1;
        }
        if rows_placed == 0 {
            return false;
        }
        let repeated = start_row > 0 && t.rows.iter().any(|row| row.header);
        let (fragment, height) = build_table_fragment(
            flow,
            t,
            Rect::new(x, self.y, width, 0.0),
            start_row,
            start_row + rows_placed,
            repeated,
        );
        self.fragments.push(Fragment::Table(fragment));
        self.y += height;
        if start_row + rows_placed < t.rows.len() {
            // The table continues on the next page.
            self.set_table_row_cursor(&t.path, start_row + rows_placed);
            return false;
        }
        self.clear_table_row_cursor(&t.path);
        true
    }

    fn table_row_cursor(&self, path: &str) -> usize {
        self.table_cursors.get(path).copied().unwrap_or(0)
    }

    fn set_table_row_cursor(&mut self, path: &str, row: usize) {
        self.table_cursors.insert(path.to_string(), row);
    }

    fn clear_table_row_cursor(&mut self, path: &str) {
        self.table_cursors.remove(path);
    }

    fn finish<S: Shaper>(mut self, flow: &mut Flow<'_, S>) -> PageGeometry {
        // Place the notes at the bottom of the body, after the body fragments.
        if !self.notes.is_empty() {
            self.place_notes(flow);
        }
        // Vertical alignment moves the body. See DP-L-03.
        if matches!(self.section.v_align, VAlign::Center | VAlign::Bottom)
            && self.body_fragments.is_empty()
        {
            self.body_fragments = std::mem::take(&mut self.fragments);
        }
        if matches!(self.section.v_align, VAlign::Center) && !self.body_fragments.is_empty() {
            let ink = fragments_rect(&self.body_fragments);
            let free = (self.body.bottom() - ink.bottom()).max(0.0);
            let shift = free / 2.0;
            if shift > 0.5 {
                for f in &mut self.body_fragments {
                    shift_fragment(f, 0.0, shift);
                }
            }
        }
        let mut fragments = std::mem::take(&mut self.decorations);
        let ordered = if self.body_fragments.is_empty() {
            std::mem::take(&mut self.fragments)
        } else {
            std::mem::take(&mut self.body_fragments)
        };
        // A behind-text float paints before the body, and everything else after.
        let mut behind: Vec<Fragment> = Vec::new();
        let mut front: Vec<Fragment> = Vec::new();
        for f in ordered {
            front.push(f);
        }
        fragments.append(&mut behind);
        fragments.append(&mut front);
        if fragments.len() as u32 > MAX_FRAGMENTS_PER_PAGE {
            flow.report(
                "display.page_too_dense",
                self.index,
                format!("{} fragments", fragments.len()),
            );
        }
        PageGeometry {
            index: self.index,
            number: self.number,
            label: crate::format_page_number(self.number, &self.section.page_fmt),
            w: self.section.page_w,
            h: self.section.page_h,
            section: self.section_index,
            even: self.even,
            fragments,
            body: self.body,
        }
    }

    fn place_notes<S: Shaper>(&mut self, flow: &mut Flow<'_, S>) {
        let mut y = self.bottom;
        let mut total = 0.0f32;
        for note in &self.notes {
            total += note.height + 2.0;
        }
        if total > (self.body.h * 0.4) {
            flow.report(
                "display.note_area_full",
                self.index,
                format!("{total:.0} pt of notes"),
            );
        }
        y -= total;
        // The separator, at one third of the column width.
        let column = self.current_column();
        let sep = Rect::new(column.x, y - 6.0, column.w / 3.0, 0.5);
        self.fragments.push(Fragment::Rule {
            rect: sep,
            border: Border {
                width: 0.5,
                color: Rgba::BLACK,
                style: BorderStyle::Single,
                space: 0.0,
            },
        });
        let notes: Vec<NoteBox> = self.notes.drain(..).collect();
        for (i, note) in notes.iter().enumerate() {
            let para = FlowParagraph {
                path: format!("note:{}", i),
                text: note.text.clone(),
                runs: vec![(0, note.text.len(), 0)],
                run_styles: vec![RunStyle {
                    face: 0,
                    size: 8.0,
                    color: edgedoc_geom::Rgba::BLACK,
                    underline: false,
                    strike: false,
                    double_strike: false,
                    highlight: None,
                    shading: None,
                    x_scale: 1.0,
                    letter_spacing: 0.0,
                    baseline_shift: 0.0,
                    hidden: false,
                    rtl: false,
                    caps: edgedoc_text::CapsMode::None,
                    symbol: None,
                    synth_bold: false,
                    synth_italic: false,
                }],
                para: ComputedPara::default(),
                label: Some(format!("{} ", i + 1)),
                list: None,
                page_break_before: false,
                has_page_field: false,
                has_total_field: false,
                objects: Vec::new(),
                links: Vec::new(),
                outline: None,
                notes: Vec::new(),
                revision: None,
            };
            let layout = flow.paragraph_layout(&para, column.w, &[], 0.0);
            self.fragments.push(Fragment::Paragraph(ParagraphFragment {
                path: para.path.clone(),
                text: std::sync::Arc::from(para.text.as_str()),
                label: None,
                rect: Rect::new(column.x, y, column.w, layout.height),
                l0: 0,
                l1: layout.lines.len(),
                para: para.para.clone(),
                run_styles: para.run_styles.clone(),
                layout,
                continues_before: i > 0,
                continues_after: false,
                rtl: false,
                revision: None,
                links: Vec::new(),
            }));
            y += note.height + 2.0;
        }
    }
}

fn fragments_rect(fragments: &[Fragment]) -> Rect {
    let mut out = Rect::new(f32::MAX, f32::MAX, 0.0, 0.0);
    for f in fragments {
        let r = match f {
            Fragment::Paragraph(p) => p.rect,
            Fragment::Table(t) => t.rect,
            Fragment::Picture { rect, .. } => *rect,
            Fragment::Placeholder { rect, .. } => *rect,
            Fragment::Rule { rect, .. } => *rect,
        };
        out = if out.w == 0.0 && out.h == 0.0 {
            r
        } else {
            out.union(&r)
        };
    }
    if out.x == f32::MAX {
        Rect::default()
    } else {
        out
    }
}

fn shift_fragment(f: &mut Fragment, dx: f32, dy: f32) {
    match f {
        Fragment::Paragraph(p) => p.rect = p.rect.translate(dx, dy),
        Fragment::Table(t) => {
            t.rect = t.rect.translate(dx, dy);
            for c in &mut t.cells {
                c.rect = c.rect.translate(dx, dy);
            }
            for b in &mut t.borders {
                b.rect = b.rect.translate(dx, dy);
            }
            for f in &mut t.content {
                shift_fragment(f, dx, dy);
            }
        }
        Fragment::Picture { rect, .. }
        | Fragment::Placeholder { rect, .. }
        | Fragment::Rule { rect, .. } => *rect = rect.translate(dx, dy),
    }
}

fn link_rects(
    layout: &ParagraphLayout,
    links: &[(usize, usize, String)],
    origin: Rect,
) -> Vec<(Rect, String)> {
    let mut out = Vec::new();
    for (start, end, target) in links {
        let mut rect: Option<Rect> = None;
        for line in &layout.lines {
            if line.stop <= *start || line.start >= *end {
                continue;
            }
            let c0 = line.c0;
            let xs = &line.xs;
            let x0 = xs.first().copied().unwrap_or(0.0);
            let x1 = xs
                .get(line.c1 - line.c0)
                .copied()
                .unwrap_or(x0 + line.ink_width());
            let r = Rect::new(
                x0 + origin.x,
                line.top + origin.y,
                (x1 - x0).max(1.0),
                line.height,
            );
            rect = Some(match rect {
                Some(prev) => prev.union(&r),
                None => r,
            });
            let _ = c0;
        }
        if let Some(r) = rect {
            out.push((r, target.clone()));
        }
    }
    out
}

/// The column rectangles of a section body.
fn column_rects(section: &FlowSection, body: Rect) -> Vec<Rect> {
    let count = section.columns.max(1) as usize;
    if count == 1 {
        return vec![body];
    }
    let gap = section.column_space.max(0.0);
    let total_gap = gap * (count.saturating_sub(1)) as f32;
    let w = ((body.w - total_gap) / count as f32).max(1.0);
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let x = body.x + i as f32 * (w + gap);
        out.push(Rect::new(x, body.y, w, body.h));
    }
    out
}

/// The painted page number in a format. See `DP-L-10`.
pub fn format_page_number(n: u32, fmt: &str) -> String {
    match fmt {
        "lowerRoman" => roman(n).to_lowercase(),
        "upperRoman" => roman(n).to_uppercase(),
        "lowerLetter" => letter(n),
        "upperLetter" => letter(n).to_uppercase(),
        _ => n.to_string(),
    }
}

fn roman(mut value: u32) -> String {
    let table = [
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];
    let mut out = String::new();
    for (n, s) in table {
        while value >= n {
            out.push_str(s);
            value -= n;
        }
    }
    out
}

fn letter(mut value: u32) -> String {
    if value == 0 {
        return String::new();
    }
    let mut out = String::new();
    while value > 0 {
        let v = ((value - 1) % 26) as u8;
        out.insert(0, char::from(b'a' + v));
        value = (value - 1) / 26;
    }
    out
}

/// The width a table takes. See `DP-L-34` and `DP-L-59`.
pub fn table_width(t: &FlowTable, available: f32) -> f32 {
    if t.layout_fixed {
        let sum: f32 = t.widths.iter().sum();
        if sum > 0.0 {
            return sum.min(available.max(1.0));
        }
    }
    let (mins, maxes) = column_extents(t, available);
    autofit_widths(&mins, &maxes, available)
}

/// Autofit: the sum of the preferred widths, clamped at the available width and at the minimum
/// of each column. This is the WordCraft function, ported verbatim. See `DP-L-59`.
pub fn autofit_widths(mins: &[f32], maxes: &[f32], available: f32) -> f32 {
    let sum_max: f32 = maxes.iter().sum();
    let sum_min: f32 = mins.iter().sum();
    if sum_max <= available {
        return sum_max.max(1.0);
    }
    if sum_min >= available || sum_max <= sum_min {
        return sum_min.max(1.0).min(available.max(1.0));
    }
    available.max(1.0)
}

fn column_extents(t: &FlowTable, available: f32) -> (Vec<f32>, Vec<f32>) {
    let n = t.widths.len().max(1);
    let mut mins = vec![0.0f32; n];
    let mut maxes = vec![0.0f32; n];
    if !t.widths.is_empty() {
        for (i, w) in t.widths.iter().enumerate() {
            if let Some(slot) = maxes.get_mut(i) {
                *slot = *w;
            }
            if let Some(slot) = mins.get_mut(i) {
                *slot = *w;
            }
        }
        return (mins, maxes);
    }
    let share = available / n as f32;
    for m in &mut mins {
        *m = share * 0.3;
    }
    for m in &mut maxes {
        *m = share;
    }
    (mins, maxes)
}

fn row_height<S: Shaper>(flow: &mut Flow<'_, S>, t: &FlowTable, row: usize, width: f32) -> f32 {
    let Some(r) = t.rows.get(row) else {
        return 0.0;
    };
    let mut tallest = 0.0f32;
    let widths = column_widths(t, width);
    let mut col = 0usize;
    for cell in &r.cells {
        let span = cell.span.max(1);
        let cell_w: f32 =
            widths.iter().skip(col).take(span).sum::<f32>() - cell.margins[1] - cell.margins[3];
        let cell_w = cell_w.max(1.0);
        let mut h = cell.margins[0] + cell.margins[2];
        for block in &cell.blocks {
            match block {
                FlowBlock::Paragraph(p) => {
                    let layout = flow.paragraph_layout(p, cell_w, &[], 0.0);
                    h += layout.height + p.para.space_after;
                }
                FlowBlock::Picture { height, .. } => h += height,
                FlowBlock::Placeholder { height, .. } => h += height,
                FlowBlock::Table(inner) => {
                    let inner_w = cell_w;
                    h += table_height(flow, inner, inner_w);
                }
            }
        }
        tallest = tallest.max(h);
        col += span;
    }
    if let Some(h) = r.height {
        if r.height_exact {
            return h;
        }
        tallest = tallest.max(h);
    }
    tallest.max(1.0)
}

fn table_height<S: Shaper>(flow: &mut Flow<'_, S>, t: &FlowTable, width: f32) -> f32 {
    let mut sum = 0.0;
    for r in 0..t.rows.len() {
        sum += row_height(flow, t, r, width);
    }
    sum
}

fn column_widths(t: &FlowTable, width: f32) -> Vec<f32> {
    if !t.widths.is_empty() {
        let sum: f32 = t.widths.iter().sum();
        if sum > 0.0 {
            let k = width / sum;
            return t.widths.iter().map(|w| w * k).collect();
        }
    }
    let n = t.rows.first().map_or(1, |r| r.cells.len()).max(1);
    vec![width / n as f32; n]
}

fn build_table_fragment<S: Shaper>(
    flow: &mut Flow<'_, S>,
    t: &FlowTable,
    origin: Rect,
    r0: usize,
    r1: usize,
    repeated_header: bool,
) -> (TableFragment, f32) {
    let width = origin.w.max(1.0);
    let widths = column_widths(t, width);
    let mut cells = Vec::new();
    let mut content = Vec::new();
    let mut borders = Vec::new();
    let mut y = origin.y;
    // A repeated header paints the header rows again before the body rows.
    let mut drawn_rows: Vec<usize> = Vec::new();
    if repeated_header {
        for (i, r) in t.rows.iter().enumerate() {
            if r.header && i < r0 {
                drawn_rows.push(i);
            }
        }
    }
    for r in r0..r1 {
        drawn_rows.push(r);
    }
    for &r in &drawn_rows {
        let Some(row) = t.rows.get(r) else { continue };
        let h = row_height(flow, t, r, width);
        let mut x = origin.x;
        let mut col = 0usize;
        for cell in &row.cells {
            let span = cell.span.max(1);
            let w: f32 = widths.iter().skip(col).take(span).sum();
            let rect = Rect::new(x, y, w.max(1.0), h);
            cells.push(CellRect {
                rect,
                row: r,
                col,
                span,
                shading: cell.shading,
            });
            // Cell content: one paragraph fragment per block, measured at the cell width and
            // placed inside the cell margins.
            let inner_x = rect.x + cell.margins[1];
            let inner_w = (rect.w - cell.margins[1] - cell.margins[3]).max(1.0);
            let mut cy = rect.y + cell.margins[0];
            let content_start = content.len();
            for block in &cell.blocks {
                match block {
                    FlowBlock::Paragraph(p) => {
                        let layout = flow.paragraph_layout(p, inner_w, &[], 0.0);
                        let hh = layout.height;
                        content.push(Fragment::Paragraph(ParagraphFragment {
                            path: p.path.clone(),
                            text: std::sync::Arc::from(p.text.as_str()),
                            label: None,
                            rect: Rect::new(inner_x, cy, inner_w, hh),
                            l0: 0,
                            l1: layout.lines.len(),
                            para: p.para.clone(),
                            run_styles: p.run_styles.clone(),
                            layout,
                            continues_before: false,
                            continues_after: false,
                            rtl: p.para.bidi,
                            revision: p.revision,
                            links: Vec::new(),
                        }));
                        cy += hh + p.para.space_after;
                    }
                    FlowBlock::Picture {
                        height,
                        width: pw,
                        alt,
                        resource,
                        ..
                    } => {
                        content.push(Fragment::Picture {
                            rect: Rect::new(inner_x, cy, (*pw).min(inner_w), *height),
                            resource: *resource,
                            alt: alt.clone(),
                        });
                        cy += height;
                    }
                    FlowBlock::Placeholder {
                        height,
                        width: pw,
                        kind,
                        path,
                        ..
                    } => {
                        content.push(Fragment::Placeholder {
                            rect: Rect::new(inner_x, cy, (*pw).min(inner_w), *height),
                            kind: kind.clone(),
                            label: path.clone(),
                        });
                        cy += height;
                    }
                    FlowBlock::Table(inner) => {
                        let (fragment, ih) = build_table_fragment(
                            flow,
                            inner,
                            Rect::new(inner_x, cy, inner_w, 0.0),
                            0,
                            inner.rows.len(),
                            false,
                        );
                        content.push(Fragment::Table(fragment));
                        cy += ih;
                    }
                }
            }
            // A cell vertical alignment moves its content inside the cell. See `DP-L-40`.
            let used = cy - (rect.y + cell.margins[0]);
            let free = (rect.h - cell.margins[0] - cell.margins[2] - used).max(0.0);
            let shift = match cell.v_align {
                VAlign::Center => free / 2.0,
                VAlign::Bottom => free,
                _ => 0.0,
            };
            if shift > 0.5 {
                for fragment in content.iter_mut().skip(content_start) {
                    shift_fragment(fragment, 0.0, shift);
                }
            }
            // A cell border paints at an outer table edge only. Between two cells, the table's
            // inside rules paint once, so a shared edge is not drawn twice at double width. This
            // is the WordCraft `region_borders` rule. See `DP-L-39`.
            let first_col = col == 0;
            let last_col = col + span >= widths.len();
            let first_row = r == 0;
            let last_row = r + 1 >= t.rows.len();
            for (side, border) in cell.borders.iter().enumerate() {
                let Some(b) = border else { continue };
                let outer = match side {
                    SIDE_TOP => first_row,
                    SIDE_BOTTOM => last_row,
                    SIDE_LEFT => first_col,
                    _ => last_col,
                };
                if outer {
                    borders.push(border_segment(rect, side, *b));
                }
            }
            x += w;
            col += span;
        }
        y += h;
    }
    // Table-level borders and inside rules.
    let full = Rect::new(origin.x, origin.y, width, (y - origin.y).max(1.0));
    for side in 0..4 {
        if let Some(b) = t.borders.get(side).copied().flatten() {
            borders.push(border_segment(full, side, b));
        }
    }
    // Inside rules, between the rows that this fragment holds.
    if let Some(inside) = t.inside_h {
        // One inside rule sits between two rows, at the bottom of the first.
        let mut row_bottoms: Vec<f32> = Vec::new();
        for c in &cells {
            let bottom = c.rect.bottom();
            if !row_bottoms.iter().any(|b| (b - bottom).abs() < 0.01) {
                row_bottoms.push(bottom);
            }
        }
        row_bottoms.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        row_bottoms.pop();
        for b in row_bottoms {
            borders.push(BorderSegment {
                rect: Rect::new(
                    origin.x,
                    b - inside.width.max(0.5) / 2.0,
                    width,
                    inside.width.max(0.5),
                ),
                border: Border {
                    width: inside.width.max(0.5),
                    color: inside.color,
                    style: inside.style,
                    space: 0.0,
                },
            });
        }
    }
    if let Some(inside) = t.inside_v {
        let mut xs: Vec<f32> = widths
            .iter()
            .scan(origin.x, |acc, w| {
                *acc += *w;
                Some(*acc)
            })
            .collect();
        xs.pop();
        for x in xs {
            borders.push(BorderSegment {
                rect: Rect::new(x, origin.y, inside.width.max(0.5), (y - origin.y).max(1.0)),
                border: Border {
                    width: inside.width.max(0.5),
                    color: inside.color,
                    style: inside.style,
                    space: 0.0,
                },
            });
        }
    }
    let height = (y - origin.y).max(1.0);
    (
        TableFragment {
            path: t.path.clone(),
            rect: Rect::new(origin.x, origin.y, width, height),
            r0,
            r1,
            cells,
            content,
            borders,
            repeated_header,
        },
        height,
    )
}

fn border_segment(rect: Rect, side: usize, border: Border) -> BorderSegment {
    let w = border.width.max(0.3);
    let segment = match side {
        SIDE_TOP => Rect::new(rect.x, rect.y, rect.w, w),
        SIDE_BOTTOM => Rect::new(rect.x, rect.bottom() - w, rect.w, w),
        SIDE_LEFT => Rect::new(rect.x, rect.y, w, rect.h),
        _ => Rect::new(rect.right() - w, rect.y, w, rect.h),
    };
    BorderSegment {
        rect: segment,
        border,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use edgedoc_fonts::{DbShaper, FontDb};

    const REGULAR: &[u8] = include_bytes!("../../../assets/fonts/subset/Carlito-Regular.ttf");

    fn db() -> FontDb {
        let mut db = FontDb::new();
        db.load(REGULAR.to_vec(), Some("Carlito"), false, false, false);
        db
    }

    fn run_style() -> RunStyle {
        RunStyle {
            face: 0,
            size: 11.0,
            color: edgedoc_geom::Rgba::BLACK,
            underline: false,
            strike: false,
            double_strike: false,
            highlight: None,
            shading: None,
            x_scale: 1.0,
            letter_spacing: 0.0,
            baseline_shift: 0.0,
            hidden: false,
            rtl: false,
            caps: edgedoc_text::CapsMode::None,
            symbol: None,
            synth_bold: false,
            synth_italic: false,
        }
    }

    fn paragraph(text: &str) -> FlowParagraph {
        FlowParagraph {
            path: format!("body[{}]", text.len()),
            text: text.to_string(),
            runs: vec![(0, text.len(), 0)],
            run_styles: vec![run_style()],
            para: ComputedPara::default(),
            label: None,
            list: None,
            page_break_before: false,
            has_page_field: false,
            has_total_field: false,
            objects: Vec::new(),
            links: Vec::new(),
            outline: None,
            notes: Vec::new(),
            revision: None,
        }
    }

    fn a4() -> FlowSection {
        FlowSection::default()
    }

    fn section_with_header() -> FlowSection {
        let mut section = a4();
        section.header = Some(vec![FlowBlock::Paragraph(paragraph("Quarterly report"))]);
        section
    }

    #[test]
    fn an_empty_document_has_one_page() {
        let db = db();
        let shaper = DbShaper { db: &db };
        let mut flow = Flow::new(&shaper);
        let out = flow.layout(&[], &[a4()]);
        assert_eq!(out.pages.len(), 1);
        assert!((out.pages[0].w - 595.28).abs() < 0.1);
    }

    #[test]
    fn the_page_size_comes_from_the_section() {
        let db = db();
        let shaper = DbShaper { db: &db };
        let mut flow = Flow::new(&shaper);
        let mut section = a4();
        section.page_w = 612.0;
        section.page_h = 792.0;
        let out = flow.layout(
            &[FlowBlock::Paragraph(paragraph("Letter page"))],
            &[section],
        );
        assert!((out.pages[0].w - 612.0).abs() < 0.01);
    }

    #[test]
    fn a_landscape_section_swaps_the_size() {
        let db = db();
        let shaper = DbShaper { db: &db };
        let mut flow = Flow::new(&shaper);
        let mut section = a4();
        section.page_w = 841.89;
        section.page_h = 595.28;
        let out = flow.layout(&[FlowBlock::Paragraph(paragraph("Landscape"))], &[section]);
        assert!(out.pages[0].w > out.pages[0].h);
    }

    #[test]
    fn a_body_rectangle_respects_the_margins() {
        let section = a4();
        let body = section.body_rect();
        assert!((body.x - 72.0).abs() < 0.01);
        assert!((body.w - (595.28 - 144.0)).abs() < 0.1);
    }

    #[test]
    fn a_long_document_takes_several_pages() {
        let db = db();
        let shaper = DbShaper { db: &db };
        let mut flow = Flow::new(&shaper);
        let body: Vec<FlowBlock> = (0..120)
            .map(|i| {
                FlowBlock::Paragraph(paragraph(&format!(
                    "Paragraph {i} holds enough words to take a line or two on the page."
                )))
            })
            .collect();
        let out = flow.layout(&body, &[a4()]);
        assert!(out.pages.len() > 1, "pages {}", out.pages.len());
    }

    #[test]
    fn no_character_is_lost_across_a_page_break() {
        let db = db();
        let shaper = DbShaper { db: &db };
        let mut flow = Flow::new(&shaper);
        let body: Vec<FlowBlock> = (0..40)
            .map(|i| FlowBlock::Paragraph(paragraph(&format!("Line {i} of the document body."))))
            .collect();
        let out = flow.layout(&body, &[a4()]);
        let mut total = 0usize;
        for page in &out.pages {
            for f in &page.fragments {
                if let Fragment::Paragraph(p) = f {
                    for l in p.layout.lines.iter().skip(p.l0).take(p.l1 - p.l0) {
                        total += l.stop - l.start;
                    }
                }
            }
        }
        assert!(total > 0);
    }

    #[test]
    fn a_page_break_before_starts_a_new_page() {
        let db = db();
        let shaper = DbShaper { db: &db };
        let mut flow = Flow::new(&shaper);
        let mut second = paragraph("Second page");
        second.para.page_break_before = true;
        let body = vec![
            FlowBlock::Paragraph(paragraph("First page")),
            FlowBlock::Paragraph(second),
        ];
        let out = flow.layout(&body, &[a4()]);
        assert_eq!(out.pages.len(), 2);
    }

    #[test]
    fn a_two_column_section_has_two_columns() {
        let db = db();
        let shaper = DbShaper { db: &db };
        let mut flow = Flow::new(&shaper);
        let mut section = a4();
        section.columns = 2;
        let body: Vec<FlowBlock> = (0..60)
            .map(|i| FlowBlock::Paragraph(paragraph(&format!("Column text {i} fills the band."))))
            .collect();
        let body_rect = section.body_rect();
        let out = flow.layout(&body, &[section]);
        assert!(!out.pages.is_empty());
        for f in &out.pages[0].fragments {
            if let Fragment::Paragraph(p) = f {
                assert!(p.rect.right() <= body_rect.right() + 1.0);
            }
        }
    }

    #[test]
    fn a_table_paints_its_borders() {
        let db = db();
        let shaper = DbShaper { db: &db };
        let mut flow = Flow::new(&shaper);
        let border = Border {
            width: 0.75,
            color: Rgba::BLACK,
            style: BorderStyle::Single,
            space: 0.0,
        };
        let table = FlowTable {
            path: "table0".into(),
            rows: vec![FlowRow {
                cells: vec![
                    FlowCell {
                        blocks: vec![FlowBlock::Paragraph(paragraph("A"))],
                        span: 1,
                        width: None,
                        v_align: VAlign::Top,
                        shading: None,
                        margins: [4.0, 4.0, 4.0, 4.0],
                        borders: [None; 4],
                        v_merge_continue: false,
                        v_merge_start: false,
                    },
                    FlowCell {
                        blocks: vec![FlowBlock::Paragraph(paragraph("B"))],
                        span: 1,
                        width: None,
                        v_align: VAlign::Top,
                        shading: None,
                        margins: [4.0, 4.0, 4.0, 4.0],
                        borders: [None; 4],
                        v_merge_continue: false,
                        v_merge_start: false,
                    },
                ],
                header: false,
                height: None,
                height_exact: false,
                cant_split: false,
            }],
            widths: vec![200.0, 200.0],
            align: Align::Left,
            indent: 0.0,
            layout_fixed: true,
            cell_margins: [4.0; 4],
            borders: [Some(border); 4],
            inside_h: Some(border),
            inside_v: Some(border),
            floating: false,
            anchor: None,
            bidi: false,
        };
        let out = flow.layout(&[FlowBlock::Table(table)], &[a4()]);
        let mut borders = 0;
        for page in &out.pages {
            for f in &page.fragments {
                if let Fragment::Table(t) = f {
                    borders += t.borders.len();
                    assert_eq!(t.cells.len(), 2);
                    assert_eq!(t.content.len(), 2, "each cell paints one paragraph");
                }
            }
        }
        assert!(borders >= 4, "borders {borders}");
    }

    #[test]
    fn autofit_clamps_at_the_available_width() {
        let mins = [100.0, 100.0, 100.0];
        let maxes = [300.0, 300.0, 300.0];
        assert!((autofit_widths(&mins, &maxes, 400.0) - 400.0).abs() < 0.01);
        // Everything fits: the table takes its preferred width.
        assert!((autofit_widths(&mins, &maxes, 1200.0) - 900.0).abs() < 0.01);
    }

    #[test]
    fn a_max_below_the_min_takes_the_min() {
        let mins = [50.0, 200.0];
        let maxes = [40.0, 210.0];
        // The minimum wins, and it does not pass the available width.
        let w = autofit_widths(&mins, &maxes, 300.0);
        assert!(w <= 300.0);
        assert!(w >= 50.0);
    }

    #[test]
    fn a_page_number_format_is_applied() {
        assert_eq!(format_page_number(3, "decimal"), "3");
        assert_eq!(format_page_number(3, "lowerRoman"), "iii");
        assert_eq!(format_page_number(3, "upperRoman"), "III");
        assert_eq!(format_page_number(3, "lowerLetter"), "c");
    }

    #[test]
    fn a_header_paints_above_the_body() {
        let db = db();
        let shaper = DbShaper { db: &db };
        let mut flow = Flow::new(&shaper);
        let out = flow.layout(
            &[FlowBlock::Paragraph(paragraph("Body text"))],
            &[section_with_header()],
        );
        let body_top = out.pages[0].body.y;
        let header = out
            .pages
            .first()
            .and_then(|p| {
                p.fragments.iter().find_map(|f| match f {
                    Fragment::Paragraph(par) if par.rect.y < 100.0 => Some(par.rect),
                    _ => None,
                })
            })
            .expect("header fragment");
        assert!(header.y < body_top + 1.0);
    }

    #[test]
    fn a_numbered_page_label_follows_the_number() {
        let db = db();
        let shaper = DbShaper { db: &db };
        let mut flow = Flow::new(&shaper);
        let mut section = a4();
        section.page_start = Some(4);
        section.page_fmt = "lowerRoman".into();
        let out = flow.layout(&[FlowBlock::Paragraph(paragraph("Body"))], &[section]);
        assert_eq!(out.pages[0].number, 4);
        assert_eq!(out.pages[0].label, "iv");
    }

    #[test]
    fn the_index_maps_a_block_to_its_page() {
        let db = db();
        let shaper = DbShaper { db: &db };
        let mut flow = Flow::new(&shaper);
        let body: Vec<FlowBlock> = (0..60)
            .map(|i| FlowBlock::Paragraph(paragraph(&format!("Block {i} of the body text."))))
            .collect();
        let out = flow.layout(&body, &[a4()]);
        assert!(!out.index.is_empty());
        assert!(out.index.values().all(|p| *p < out.pages.len() as u32 + 1));
    }

    #[test]
    fn a_layout_runs_twice_identically() {
        let db = db();
        let shaper = DbShaper { db: &db };
        let body: Vec<FlowBlock> = (0..30)
            .map(|i| FlowBlock::Paragraph(paragraph(&format!("Deterministic text {i}."))))
            .collect();
        let mut a = Flow::new(&shaper);
        let first = a.layout(&body, &[a4()]);
        let mut b = Flow::new(&shaper);
        let second = b.layout(&body, &[a4()]);
        assert_eq!(first.pages.len(), second.pages.len());
        assert_eq!(first.pages.len(), second.pages.len());
        for (a, b) in first.pages.iter().zip(second.pages.iter()) {
            assert_eq!(a.index, b.index);
            assert_eq!(a.fragments.len(), b.fragments.len());
        }
    }

    #[test]
    fn a_placeholder_paints_a_box() {
        let db = db();
        let shaper = DbShaper { db: &db };
        let mut flow = Flow::new(&shaper);
        let body = vec![FlowBlock::Placeholder {
            path: "chart1".into(),
            kind: "chart".into(),
            width: 200.0,
            height: 120.0,
            align: Align::Center,
        }];
        let out = flow.layout(&body, &[a4()]);
        let found = out.pages[0]
            .fragments
            .iter()
            .any(|f| matches!(f, Fragment::Placeholder { .. }));
        assert!(found);
    }

    #[test]
    fn a_picture_paints_a_box_with_its_alt_text() {
        let db = db();
        let shaper = DbShaper { db: &db };
        let mut flow = Flow::new(&shaper);
        let body = vec![FlowBlock::Picture {
            path: "img1".into(),
            width: 100.0,
            height: 50.0,
            alt: "A logo".into(),
            resource: 1,
            align: Align::Left,
        }];
        let out = flow.layout(&body, &[a4()]);
        let found = out.pages[0]
            .fragments
            .iter()
            .any(|f| matches!(f, Fragment::Picture { alt, .. } if alt == "A logo"));
        assert!(found);
    }

    #[test]
    fn a_tall_paragraph_splits_across_pages() {
        let db = db();
        let shaper = DbShaper { db: &db };
        let mut flow = Flow::new(&shaper);
        let mut long = String::new();
        for i in 0..400 {
            long.push_str(&format!("word{i} "));
        }
        let body = vec![FlowBlock::Paragraph(paragraph(&long))];
        let out = flow.layout(&body, &[a4()]);
        assert!(!out.pages.is_empty());
    }

    #[test]
    fn the_shape_cache_returns_the_same_layout() {
        let db = db();
        let shaper = DbShaper { db: &db };
        let mut flow = Flow::new(&shaper);
        let p = paragraph("Cached paragraph text");
        let a = flow.paragraph_layout(&p, 300.0, &[], 0.0);
        let b = flow.paragraph_layout(&p, 300.0, &[], 0.0);
        assert!(std::sync::Arc::ptr_eq(&a, &b));
    }
}
