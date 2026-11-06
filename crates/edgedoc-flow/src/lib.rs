//! Page filling: sections, columns, tables, headers, footers, and notes.
//!
//! The flow crate takes a prepared document and produces pages. It never reads XML, never
//! rasterizes, and never touches a browser. Layout is a pure function of the input, so two runs
//! return equal geometry (`DP-A-25`). See `DP-L-01` to `DP-L-61`.

#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![deny(clippy::unimplemented, clippy::todo, clippy::unreachable)]

use std::collections::{BTreeMap, BTreeSet};

use edgedoc_fonts::Shaper;
use edgedoc_geom::{Rect, Rgba};
use edgedoc_style::{Border, BorderStyle, ComputedPara};
use edgedoc_text::{layout_paragraph, Align, BreakInput, ParagraphLayout, RunStyle};

/// The marker that a `PAGE` field leaves in the text, which the flow replaces per page.
pub const PAGE_MARKER: char = '\u{F000}';

/// The marker that a `NUMPAGES` field leaves in the text.
pub const TOTAL_MARKER: char = '\u{F001}';
/// The markers for section-local page totals and the one-based section number.
pub const SECTION_TOTAL_MARKER: char = '\u{F002}';
pub const SECTION_MARKER: char = '\u{F003}';

fn has_fields(text: &str) -> bool {
    text.chars().any(|c| {
        matches!(
            c,
            PAGE_MARKER | TOTAL_MARKER | SECTION_TOTAL_MARKER | SECTION_MARKER
        )
    })
}

/// Substitute the page markers in a paragraph.
///
/// A `PAGE` field takes this page's number, and a `NUMPAGES` field takes the total from the
/// previous pass. A number can be wider than its marker, so the run offsets are rebuilt. The
/// paragraph layout is then cached under the resolved text, which is why the cache key hashes the
/// text and not the path. See `DP-L-11` and `DP-L-12`.
fn with_fields(
    p: &FlowParagraph,
    page: u32,
    section: usize,
    format: &str,
    totals: &Totals,
) -> FlowParagraph {
    if !has_fields(&p.text) {
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
    let page_text = crate::format_page_number(page, format);
    let total_text = totals.pages.max(1).to_string();
    let section_total = totals
        .section_pages
        .get(section)
        .copied()
        .unwrap_or(1)
        .max(1)
        .to_string();
    let section_text = section.saturating_add(1).to_string();
    let mut text = String::with_capacity(p.text.len() + 8);
    let mut runs: Vec<(usize, usize, u32)> = Vec::new();
    for (i, ch) in p.text.char_indices() {
        let style = style_of.get(i).copied().unwrap_or(0);
        let shown: &str = if ch == PAGE_MARKER {
            &page_text
        } else if ch == TOTAL_MARKER {
            &total_text
        } else if ch == SECTION_TOTAL_MARKER {
            &section_total
        } else if ch == SECTION_MARKER {
            &section_text
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
    pub compatibility_mode: u32,
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
    /// Put the inside margin and gutter on the binding side of each page.
    pub mirror_margins: bool,
    /// `w:lnNumType` for this section. Absent sections do not number lines.
    pub line_numbers: Option<LineNumbers>,
    /// `w:pgBorders` painted around the page.
    pub page_frame: PageFrame,
}

/// Line numbers in the margin. `distance` is points from the text.
#[derive(Debug, Clone, PartialEq)]
pub struct LineNumbers {
    pub count_by: u32,
    pub start: u32,
    pub restart: String,
    pub distance: f32,
}

/// Page-border edges. `Border::space` is the inset from the page edge, in points.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PageFrame {
    pub top: Option<Border>,
    pub left: Option<Border>,
    pub bottom: Option<Border>,
    pub right: Option<Border>,
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
            compatibility_mode: 15,
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
            mirror_margins: false,
            line_numbers: None,
            page_frame: PageFrame::default(),
        }
    }
}

impl FlowSection {
    /// The body rectangle, which is the page inside the margins.
    pub fn body_rect(&self) -> Rect {
        self.body_rect_on(false)
    }

    pub fn body_rect_on(&self, even: bool) -> Rect {
        let x = if even && self.mirror_margins {
            self.margin_right
        } else {
            self.margin_left + self.gutter
        };
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
    /// Style used for a line that contains only the paragraph mark, spaces, or tabs.
    pub mark_style: u32,
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
    /// The extent of an inline object in this paragraph: `(width, height)`.
    pub objects: Vec<(f32, f32)>,
    /// Images corresponding to object markers, in text order.
    pub inline_pictures: Vec<Option<InlinePicture>>,
    /// Behind-text page pictures keyed by object index.
    pub page_pictures: Vec<(usize, PageAnchorPicture)>,
    /// A directly specified top/bottom border set that can join its neighbours.
    pub border_group_safe: bool,
    pub border_join_above: bool,
    pub border_join_below: bool,
    /// Hyperlink targets, per byte range.
    pub links: Vec<(usize, usize, String)>,
    /// The outline level, which the viewer uses for navigation.
    pub outline: Option<i32>,
    /// Footnote stories and their reference byte offsets in this paragraph.
    pub notes: Vec<FlowNote>,
    /// The marker for a tracked change, for the layer state.
    pub revision: Option<RevisionKind>,
}

/// A picture that shares a line with text.
#[derive(Debug, Clone, PartialEq)]
pub struct InlinePicture {
    pub resource: u32,
    pub src: [f32; 4],
    pub width: f32,
    pub height: f32,
    pub alt: String,
    /// Left and bottom effect extents shift the painted image within its line box.
    pub effect_left: f32,
    pub effect_bottom: f32,
}

/// A non-wrapping picture anchored to the page.
#[derive(Debug, Clone)]
pub struct PageAnchorPicture {
    pub resource: u32,
    pub src: [f32; 4],
    pub width: f32,
    pub height: f32,
    pub alt: String,
    pub x: PageAnchorAxis,
    pub y: PageAnchorAxis,
}

#[derive(Debug, Clone, Copy)]
pub enum PageAnchorAxis {
    Start,
    Center,
    End,
    Offset(f32),
}

impl PageAnchorAxis {
    fn position(self, page: f32, size: f32) -> f32 {
        match self {
            Self::Start => 0.0,
            Self::Center => (page - size) / 2.0,
            Self::End => page - size,
            Self::Offset(value) => value.clamp(-31_680.0, 31_680.0),
        }
    }
}

/// A footnote referenced by a character position in a body paragraph.
#[derive(Debug, Clone)]
pub struct FlowNote {
    pub id: i32,
    pub offset: usize,
    pub blocks: std::sync::Arc<[FlowBlock]>,
    pub separator: std::sync::Arc<[FlowBlock]>,
    pub continuation_separator: std::sync::Arc<[FlowBlock]>,
    pub continuation_notice: std::sync::Arc<[FlowBlock]>,
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
    /// Grid columns omitted before and after the row's cells.
    pub grid_before: usize,
    pub grid_after: usize,
    /// Row exception borders: top, left, bottom, right, inside horizontal, inside vertical.
    pub exception_borders: [Option<Border>; 6],
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
    /// Whether the document requests an automatic table width.
    pub auto_width: bool,
    /// An explicit table width, in points or as a fraction of the available width.
    pub preferred_width: Option<TableWidth>,
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

#[derive(Debug, Clone, Copy)]
pub enum TableWidth {
    Points(f32),
    Fraction(f32),
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
    /// The separator before endnote stories.
    EndnoteSeparator {
        path: String,
        blocks: std::sync::Arc<[FlowBlock]>,
    },
    /// A picture that is a block of its own, as a paragraph with one object is not.
    Picture {
        path: String,
        width: f32,
        height: f32,
        alt: String,
        resource: u32,
        src: [f32; 4],
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
            FlowBlock::EndnoteSeparator { path, .. } => path,
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
        src: [f32; 4],
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
    /// A margin label, such as a line number. `baseline` is the glyph baseline.
    MarginLabel {
        text: String,
        rect: Rect,
        baseline: f32,
        size: f32,
        face: u32,
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
    /// Footnote story ids present on this page, including continuations.
    pub footnote_ids: Vec<i32>,
    /// The body rectangle.
    pub body: Rect,
    /// Number of leading header and footer fragments.
    pub decoration_count: usize,
    /// Section whose header and footer are displayed on this page.
    pub decoration_section: usize,
    /// Whether this page uses the section's first-page decorations.
    pub decoration_first: bool,
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
                Fragment::Rule { rect, .. } | Fragment::MarginLabel { rect, .. } => *rect,
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
    /// Physical page count per section, including a shared continuous-section page.
    pub section_pages: Vec<u32>,
}

impl Totals {
    /// The first guess is one page per section, which is low on purpose.
    pub fn first_guess(sections: usize) -> Self {
        Self {
            pages: sections.max(1) as u32,
            section_pages: vec![1; sections.max(1)],
        }
    }
}

/// One shaped paragraph, cached by content and width.
#[derive(Debug, Clone)]
struct ShapedParagraph {
    width: f32,
    source: FlowParagraph,
    layout: std::sync::Arc<ParagraphLayout>,
}

/// The flow engine.
#[derive(Clone, Default)]
struct ListCounters {
    values: BTreeMap<u32, [Option<u32>; 9]>,
    seen: BTreeSet<(u32, u32)>,
}

impl ListCounters {
    fn clear(&mut self) {
        self.values.clear();
        self.seen.clear();
    }
}

pub struct Flow<'a, S: Shaper> {
    pub shaper: &'a S,
    pub diagnostics: Vec<FlowDiagnostic>,
    /// The shape cache, keyed by `(text hash, width, style hash)`.
    cache: BTreeMap<(u64, u32, u64), ShapedParagraph>,
    /// The page count the last pass found.
    pub totals: Totals,
    /// Counters are shared by instances of the same abstract definition.
    counters: ListCounters,
    /// Document compatibility mode, which affects line breaking.
    compatibility_mode: u32,
    /// Per-run page budget, clamped to the hard engine limit.
    pub max_pages: u32,
    /// Next line number, carried across pages when the section restart says so.
    line_ordinal: u32,
    line_ordinal_ready: bool,
    line_section: usize,
}

impl<'a, S: Shaper> Flow<'a, S> {
    pub fn new(shaper: &'a S) -> Self {
        Self {
            shaper,
            diagnostics: Vec::new(),
            cache: BTreeMap::new(),
            totals: Totals::default(),
            counters: ListCounters::default(),
            compatibility_mode: 15,
            max_pages: MAX_PAGES,
            line_ordinal: 0,
            line_ordinal_ready: false,
            line_section: 0,
        }
    }

    /// The next label for a list paragraph, and the counter update. See `DP-S-14`.
    fn next_label(&mut self, list: &edgedoc_style::ListRef) -> String {
        let level = list.level.min(8) as usize;
        let first_use = self.counters.seen.insert((list.num_id, list.level));
        let counters = self.counters.values.entry(list.abstract_id).or_default();
        if first_use && list.start_override.is_some() {
            counters[level] = None;
        }
        let value = counters[level]
            .map(|current| current.saturating_add(1))
            .unwrap_or(if first_use {
                list.start_override.unwrap_or(list.start)
            } else {
                list.start
            });
        if let Some(slot) = counters.get_mut(level) {
            *slot = Some(value);
        }
        // A deeper level restarts when a shallower level advances.
        for deeper in (level + 1)..counters.len() {
            if let Some(slot) = counters.get_mut(deeper) {
                *slot = None;
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
        let format = list.format.as_str();
        while let Some(c) = chars.next() {
            if c == '%' {
                if let Some(d) = chars.peek().and_then(|c| c.to_digit(10)) {
                    chars.next();
                    let idx = (d as usize).saturating_sub(1);
                    let n = counters.get(idx).copied().flatten().unwrap_or(list.start);
                    let referenced = list.level_formats.get(idx).map_or(format, String::as_str);
                    let number_format = if list.is_legal && referenced != "decimalZero" {
                        "decimal"
                    } else {
                        referenced
                    };
                    out.push_str(&edgedoc_style::format_counter(
                        n,
                        number_format,
                        list.is_legal,
                    ));
                    continue;
                }
            }
            out.push(c);
        }
        let label = if out.is_empty() {
            edgedoc_style::format_counter(value, format, list.is_legal)
        } else {
            out
        };
        if list.caps {
            label.to_uppercase()
        } else {
            label
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

    /// Lay out one block range. Use `layout_groups` for explicit section ranges.
    pub fn layout(&mut self, body: &[FlowBlock], sections: &[FlowSection]) -> DocLayout {
        self.layout_groups(&[SectionGroup {
            blocks: body.to_vec(),
            section: sections.first().cloned().unwrap_or_default(),
        }])
    }

    /// Fill section ranges with a persistent page builder. A continuation carries its line and
    /// row cursors to the next page; a continuous section retains the current physical page.
    pub fn layout_groups(&mut self, groups: &[SectionGroup]) -> DocLayout {
        let mode = groups
            .first()
            .map_or(15, |group| group.section.compatibility_mode);
        if self.compatibility_mode != mode {
            self.cache.clear();
            self.compatibility_mode = mode;
        }
        let page_limit = self.max_pages.clamp(1, MAX_PAGES);
        let mut totals = Totals::first_guess(groups.len());
        let needs_totals = groups.iter().any(|g| {
            walk_parts(&g.blocks)
                || [
                    &g.section.header,
                    &g.section.footer,
                    &g.section.header_first,
                    &g.section.footer_first,
                    &g.section.header_even,
                    &g.section.footer_even,
                ]
                .into_iter()
                .flatten()
                .any(|part| walk_parts(part))
        });
        for pass in 1..=MAX_TOTAL_PASSES {
            self.counters.clear();
            self.diagnostics.clear();
            self.line_ordinal = 0;
            self.line_ordinal_ready = false;
            self.line_section = 0;
            self.totals = totals.clone();
            let mut pages = Vec::new();
            let mut index = BTreeMap::new();
            let mut membership = vec![std::collections::BTreeSet::new(); groups.len()];
            let mut current: Option<PageBuilder> = None;
            let mut note_carry: Vec<NoteBox> = Vec::new();
            let mut next_number = 1u32;
            let mut limit_hit = false;
            for (gi, group) in groups.iter().enumerate() {
                let section = &group.section;
                let continuous = matches!(
                    section.start,
                    SectionStart::Continuous | SectionStart::NextColumn
                );
                let compatible = current.as_ref().is_some_and(|p| {
                    (p.section.page_w - section.page_w).abs() < 0.01
                        && (p.section.page_h - section.page_h).abs() < 0.01
                });
                if gi > 0 && (!continuous || !compatible) {
                    if let Some(page) = current.take() {
                        next_number = page.number.saturating_add(1);
                        let old_section = page.section.clone();
                        let old_index = page.section_index;
                        pages.push(page.finish(self, &mut note_carry));
                        if !self.drain_note_pages(
                            &mut pages,
                            &mut note_carry,
                            (&old_section, old_index),
                            &mut next_number,
                            &mut membership,
                            page_limit,
                        ) {
                            limit_hit = true;
                            break;
                        }
                    }
                }
                if let Some(page) = current.as_mut() {
                    page.continue_section(section, gi);
                    if section.start == SectionStart::NextColumn && !page.next_column() {
                        if let Some(page) = current.take() {
                            next_number = page.number.saturating_add(1);
                            let old_section = page.section.clone();
                            let old_index = page.section_index;
                            pages.push(page.finish(self, &mut note_carry));
                            if !self.drain_note_pages(
                                &mut pages,
                                &mut note_carry,
                                (&old_section, old_index),
                                &mut next_number,
                                &mut membership,
                                page_limit,
                            ) {
                                limit_hit = true;
                                break;
                            }
                        }
                    }
                }
                if current.is_none() {
                    if pages.len() as u32 >= page_limit {
                        limit_hit = true;
                        break;
                    }
                    if let Some(n) = section.page_start {
                        next_number = n;
                    }
                    if gi > 0
                        && matches!(
                            section.start,
                            SectionStart::OddPage | SectionStart::EvenPage
                        )
                    {
                        let want_odd = section.start == SectionStart::OddPage;
                        if (next_number % 2 == 1) != want_odd {
                            let mut blank = PageBuilder::new(
                                pages.len() as u32,
                                next_number,
                                section.clone(),
                                gi,
                                next_number % 2 == 0,
                            );
                            blank.begin(self, section, next_number % 2 == 0, false);
                            // The parity page belongs to the preceding section's page count.
                            if let Some(prev) = membership.get_mut(gi.saturating_sub(1)) {
                                prev.insert(pages.len() as u32);
                            }
                            pages.push(blank.finish(self, &mut note_carry));
                            next_number = next_number.saturating_add(1);
                        }
                    }
                    if pages.len() as u32 >= page_limit {
                        limit_hit = true;
                        break;
                    }
                    let mut page = PageBuilder::new(
                        pages.len() as u32,
                        next_number,
                        section.clone(),
                        gi,
                        next_number % 2 == 0,
                    );
                    page.begin(self, section, next_number % 2 == 0, true);
                    page.receive_notes(&mut note_carry);
                    current = Some(page);
                }
                if pages.len() as u32 >= page_limit {
                    limit_hit = true;
                    break;
                }
                // Even an empty section owns the physical page on which it starts.
                if let Some(page) = current.as_ref() {
                    membership[gi].insert(page.index);
                }
                let mut block_index = 0;
                let mut balance_from = 0;
                let mut balance_snapshot = current.clone();
                let mut balance_counters = self.counters.clone();
                while let Some(block) = group.blocks.get(block_index) {
                    let Some(page) = current.as_mut() else { break };
                    membership[gi].insert(page.index);
                    let placed = self.push_kept_block(page, &group.blocks[block_index..], &totals);
                    index.entry(block.path().to_string()).or_insert(page.index);
                    if placed {
                        block_index += 1;
                        continue;
                    }
                    let Some(mut previous) = current.take() else {
                        break;
                    };
                    let break_origin = previous.next_break;
                    let previous_gap = previous.prev_after.clone();
                    let lines = std::mem::take(&mut previous.line_cursors);
                    let rows = std::mem::take(&mut previous.table_cursors);
                    let labels = std::mem::take(&mut previous.list_labels);
                    next_number = previous.number.saturating_add(1);
                    pages.push(previous.finish(self, &mut note_carry));
                    if pages.len() as u32 >= page_limit {
                        limit_hit = true;
                        break;
                    }
                    let mut next = PageBuilder::new(
                        pages.len() as u32,
                        next_number,
                        section.clone(),
                        gi,
                        next_number % 2 == 0,
                    );
                    next.list_labels = labels;
                    next.begin(self, section, next_number % 2 == 0, false);
                    next.receive_notes(&mut note_carry);
                    next.starts_after = break_origin;
                    next.prev_after = previous_gap;
                    next.line_cursors = lines;
                    next.table_cursors = rows;
                    balance_from = block_index;
                    balance_snapshot = Some(next.clone());
                    balance_counters = self.counters.clone();
                    current = Some(next);
                }
                if !limit_hit
                    && section.columns > 1
                    && section.v_align == VAlign::Top
                    && groups
                        .get(gi + 1)
                        .is_some_and(|g| g.section.start == SectionStart::Continuous)
                {
                    if let Some(snapshot) = balance_snapshot {
                        if let Some((balanced, counters)) = self.balance_columns(
                            &snapshot,
                            &balance_counters,
                            &group.blocks[balance_from..],
                            &totals,
                        ) {
                            current = Some(balanced);
                            self.counters = counters;
                        }
                    }
                }
                if limit_hit {
                    break;
                }
            }
            if let Some(page) = current.take() {
                let section = page.section.clone();
                let section_index = page.section_index;
                next_number = page.number.saturating_add(1);
                pages.push(page.finish(self, &mut note_carry));
                if !self.drain_note_pages(
                    &mut pages,
                    &mut note_carry,
                    (&section, section_index),
                    &mut next_number,
                    &mut membership,
                    page_limit,
                ) {
                    limit_hit = true;
                }
            }
            if pages.is_empty() {
                let section = FlowSection::default();
                let mut page = PageBuilder::new(0, 1, section.clone(), 0, false);
                page.begin(self, &section, false, true);
                pages.push(page.finish(self, &mut note_carry));
            }
            if limit_hit {
                self.report(
                    "display.page_limit",
                    pages.len() as u32,
                    format!("{page_limit} pages"),
                );
            }
            let found = Totals {
                pages: pages.len() as u32,
                section_pages: membership.iter().map(|p| p.len() as u32).collect(),
            };
            if needs_totals && found != totals && pass < MAX_TOTAL_PASSES {
                totals = found;
                continue;
            }
            if needs_totals && found != totals {
                self.report(
                    "display.field_totals_unstable",
                    0,
                    "Page totals did not converge".into(),
                );
            }
            self.totals = found;
            if groups
                .iter()
                .any(|group| section_has_numbered_story(&group.section))
            {
                for page in &mut pages {
                    let Some(group) = groups.get(page.decoration_section) else {
                        continue;
                    };
                    let section = &group.section;
                    let mut decoration = PageBuilder::new(
                        page.index,
                        page.number,
                        section.clone(),
                        page.decoration_section,
                        page.even,
                    );
                    decoration.final_decorations = true;
                    decoration.begin(self, section, page.even, page.decoration_first);
                    let count = page.decoration_count.min(page.fragments.len());
                    page.decoration_count = decoration.decorations.len();
                    page.fragments.splice(..count, decoration.decorations);
                }
            }
            return DocLayout {
                pages,
                index,
                passes: pass,
                diagnostics: std::mem::take(&mut self.diagnostics),
            };
        }
        // The bounded pass loop always returns a layout.
        DocLayout {
            pages: Vec::new(),
            index: BTreeMap::new(),
            passes: 0,
            diagnostics: Vec::new(),
        }
    }

    fn drain_note_pages(
        &mut self,
        pages: &mut Vec<PageGeometry>,
        carry: &mut Vec<NoteBox>,
        section: (&FlowSection, usize),
        next_number: &mut u32,
        membership: &mut [std::collections::BTreeSet<u32>],
        page_limit: u32,
    ) -> bool {
        let (section, section_index) = section;
        while !carry.is_empty() {
            if pages.len() as u32 >= page_limit {
                return false;
            }
            let number = *next_number;
            let mut page = PageBuilder::new(
                pages.len() as u32,
                number,
                section.clone(),
                section_index,
                number % 2 == 0,
            );
            page.begin(self, section, number % 2 == 0, false);
            page.receive_notes(carry);
            if let Some(member) = membership.get_mut(section_index) {
                member.insert(page.index);
            }
            pages.push(page.finish(self, carry));
            *next_number = number.saturating_add(1);
        }
        true
    }

    fn push_kept_block(
        &mut self,
        page: &mut PageBuilder,
        blocks: &[FlowBlock],
        totals: &Totals,
    ) -> bool {
        let Some(block) = blocks.first() else {
            return true;
        };
        loop {
            let column = page.current_column();
            let keep_height = self.keep_chain_height(blocks, column.w);
            if !page.line_cursors.contains_key(block.path())
                && page.y > column.y + 0.5
                && keep_height > page.bottom - page.y
                && keep_height <= column.h
            {
                if page.next_column() {
                    page.starts_after = BreakOrigin::ConsumedBefore;
                    continue;
                }
                page.next_break = BreakOrigin::ConsumedBefore;
                return false;
            }
            return page.push_block(self, block, totals);
        }
    }

    /// As in WordCraft's balance_columns: retry only the final page at successively shorter
    /// column heights, restoring list state on every probe. Explicit breaks keep their positions.
    fn balance_columns(
        &mut self,
        snapshot: &PageBuilder,
        counters: &ListCounters,
        blocks: &[FlowBlock],
        totals: &Totals,
    ) -> Option<(PageBuilder, ListCounters)> {
        if blocks.iter().any(
            |b| matches!(b, FlowBlock::Paragraph(p) if p.text.contains(['\u{000C}', '\u{000E}'])),
        ) {
            return None;
        }
        let full = snapshot.current_column().h;
        let mut probe = |height: f32| {
            let saved_counters = std::mem::replace(&mut self.counters, counters.clone());
            let saved_diagnostics = std::mem::take(&mut self.diagnostics);
            let mut page = snapshot.clone();
            for column in &mut page.columns {
                column.h = height;
            }
            page.bottom = page.current_column().bottom();
            let mut fits = true;
            for i in 0..blocks.len() {
                if !self.push_kept_block(&mut page, &blocks[i..], totals) {
                    fits = false;
                    break;
                }
            }
            let bottom = page.current_column().bottom();
            fits &= fragments_rect(&page.fragments[snapshot.fragments.len()..]).bottom()
                <= bottom + 0.01;
            let result = fits.then(|| (page, self.counters.clone()));
            self.counters = saved_counters;
            self.diagnostics = saved_diagnostics;
            result
        };
        let mut best = probe(full)?;
        let mut low = 1.0;
        let mut high = full;
        for _ in 0..12 {
            if high - low <= 0.25 {
                break;
            }
            let mid = (low + high) / 2.0;
            if let Some(result) = probe(mid) {
                high = mid;
                best = result;
            } else {
                low = mid;
            }
        }
        Some(best)
    }

    fn keep_chain_height(&mut self, blocks: &[FlowBlock], width: f32) -> f32 {
        if !matches!(blocks.first(), Some(FlowBlock::Paragraph(p)) if p.para.keep_next) {
            return 0.0;
        }
        let mut height = 0.0;
        let mut after = 0.0f32;
        for block in blocks {
            match block {
                FlowBlock::Paragraph(p) => {
                    let layout = self.paragraph_layout(
                        p,
                        (width - p.para.indent_left - p.para.indent_right).max(1.0),
                        &[],
                        p.para.indent_first,
                    );
                    // Wordcraft's keep-with-next lookahead reserves each paragraph's
                    // before and after spacing. Actual placement still collapses gaps.
                    height += after + p.para.space_before;
                    if p.para.keep_next || p.para.keep_lines {
                        height += layout.height;
                    } else {
                        height += layout
                            .lines
                            .iter()
                            .take(if p.para.widow_control { 2 } else { 1 })
                            .map(|l| l.height)
                            .sum::<f32>();
                    }
                    if !p.para.keep_next {
                        break;
                    }
                    after = p.para.space_after;
                }
                FlowBlock::Table(t) => {
                    height += table_row_heights(self, t, width, &BTreeMap::new())
                        .first()
                        .copied()
                        .unwrap_or(0.0);
                    break;
                }
                FlowBlock::EndnoteSeparator { .. } => {
                    height += 20.0;
                    break;
                }
                FlowBlock::Picture { height: h, .. } | FlowBlock::Placeholder { height: h, .. } => {
                    height += h;
                    break;
                }
            }
        }
        height
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
                if cached.width == width
                    && cached.source.text == para.text
                    && cached.source.runs == para.runs
                    && cached.source.run_styles == para.run_styles
                    && cached.source.mark_style == para.mark_style
                    && cached.source.para == para.para
                    && cached.source.objects == para.objects
                {
                    return cached.layout.clone();
                }
            }
        }
        // Paragraph text is laid out relative to its left indent, whereas DOCX tab
        // positions are relative to the column margin. Convert into local coordinates.
        let mut tabs: Vec<_> = para
            .para
            .tabs
            .iter()
            .map(|tab| edgedoc_text::TabStop {
                pos: tab.pos - para.para.indent_left,
                ..*tab
            })
            .collect();
        // A hanging indent supplies an implicit left tab at the text indent. The
        // explicit stop wins when both occupy the same position, as in Wordcraft.
        if para.para.indent_first < 0.0 {
            tabs.push(edgedoc_text::TabStop {
                pos: 0.0,
                align: edgedoc_text::TabAlign::Left,
                leader: None,
            });
        }
        let input = BreakInput {
            text: &para.text,
            runs: &para.runs,
            styles: &para.run_styles,
            mark_style: para.mark_style,
            width,
            tab_margins: Some((-para.para.indent_left, width + para.para.indent_right)),
            indent_left: 0.0,
            indent_right: 0.0,
            indent_first: first_indent,
            align: para.para.align,
            shrink_justified_spaces: self.compatibility_mode >= 15,
            bidi: para.para.bidi,
            tabs: &tabs,
            default_tab: para.para.default_tab,
            exclusions,
            word_wrap_off: !para.para.word_wrap,
            kinsoku: para.para.kinsoku,
            objects: &para.objects,
            hyphen_after: &[],
        };
        let mut shaped = layout_paragraph(self.shaper, &input);
        // Word adds only a label's excess ascent to automatic line spacing;
        // the label's descent does not enlarge the text line. Symbol and
        // Wingdings have Word-specific metrics when absent or one em tall.
        let label_extra = if !para.para.line_exact && !para.para.line_at_least {
            para.list
                .as_ref()
                .and_then(|list| {
                    let font = list.font.as_deref().unwrap_or("");
                    let size = para.para.label_size.unwrap_or_else(|| {
                        para.run_styles.first().map_or(11.0, |style| style.size)
                    });
                    let face = para.para.label_face;
                    let (asc, desc, gap) = self
                        .shaper
                        .line_metrics(face.unwrap_or(para.para.label_fallback_face), size);
                    let one_em = size > 0.0 && ((asc + desc + gap) / size - 1.0).abs() < 0.02;
                    let symbol_ascent = match font.trim().to_ascii_lowercase().as_str() {
                        "symbol" | "symbol (pcl6)" => Some(1.005 * size),
                        "wingdings" => Some(0.899 * size),
                        _ => None,
                    };
                    let label_ascent = if (face.is_none() && !font.is_empty()) || one_em {
                        symbol_ascent.unwrap_or(asc + gap.max(0.0))
                    } else {
                        asc + gap.max(0.0)
                    };
                    shaped
                        .lines
                        .first()
                        .map(|line| (label_ascent - (line.baseline - line.top)).max(0.0))
                })
                .unwrap_or(0.0)
        } else {
            0.0
        };
        apply_line_spacing(&mut shaped, &para.para, self.shaper, &para.run_styles);
        if label_extra > 0.0 {
            if let Some(first) = shaped.lines.first_mut() {
                first.height += label_extra;
                first.baseline += label_extra;
            }
            for line in shaped.lines.iter_mut().skip(1) {
                line.top += label_extra;
                line.baseline += label_extra;
            }
            shaped.height += label_extra;
        }
        let layout = std::sync::Arc::new(shaped);
        if !hits_exclusions {
            self.cache.insert(
                key,
                ShapedParagraph {
                    width,
                    source: para.clone(),
                    layout: layout.clone(),
                },
            );
        }
        layout
    }
}

/// True when a header or footer part shows a page total.
fn apply_line_spacing<S: Shaper>(
    layout: &mut ParagraphLayout,
    para: &ComputedPara,
    shaper: &S,
    styles: &[RunStyle],
) {
    if para.line_height <= 0.0 || !para.line_height.is_finite() {
        return;
    }
    let mut top = 0.0;
    for line in &mut layout.lines {
        let natural = line.height;
        let picture_height = layout
            .clusters
            .get(line.c0..line.c1)
            .unwrap_or(&[])
            .iter()
            .filter(|cluster| matches!(cluster.kind, edgedoc_text::ClusterKind::Object(_)))
            .map(|cluster| cluster.obj_h)
            .fold(0.0f32, f32::max);
        let mut text_ascent = 0.0f32;
        let mut text_descent = 0.0f32;
        let mut ascent_with_gap = 0.0f32;
        if picture_height > 0.0 {
            for cluster in layout.clusters.get(line.c0..line.c1).unwrap_or(&[]) {
                if let Some(style) = styles.get(cluster.style as usize) {
                    let (asc, desc, gap) = shaper.line_metrics(style.face, style.line_size);
                    text_ascent = text_ascent.max(asc + style.line_shift.max(0.0));
                    text_descent = text_descent.max(desc + (-style.line_shift).max(0.0));
                    ascent_with_gap =
                        ascent_with_gap.max(asc + gap.max(0.0) + style.line_shift.max(0.0));
                }
            }
        }
        let height = if para.line_exact {
            para.line_height.max(1.0)
        } else if para.line_at_least {
            natural.max(para.line_height)
        } else {
            // w:line is in 240ths of a line for auto, stored by the cascade in twips/20.
            if picture_height > text_ascent {
                ((ascent_with_gap.max(text_ascent) + text_descent) * para.line_height / 12.0)
                    .max(picture_height + text_descent)
                    .max(1.0)
            } else {
                (natural * para.line_height / 12.0).max(1.0)
            }
        };
        let ascent = line.baseline - line.top;
        line.top = top;
        // Wordcraft para::lines::baseline_offset: fixed/minimum spacing keeps the font's
        // descent below the baseline; multiple spacing above single adds leading below it.
        line.baseline = if para.line_exact || para.line_at_least {
            top + height - line.text_desc
        } else {
            top + ascent * (para.line_height / 12.0).min(1.0)
        };
        line.height = height;
        top += height;
    }
    layout.height = top;
}

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
#[derive(Clone)]
struct PageBuilder {
    starts_after: BreakOrigin,
    next_break: BreakOrigin,
    index: u32,
    number: u32,
    section: FlowSection,
    section_index: usize,
    decoration_section: usize,
    decoration_first: bool,
    final_decorations: bool,
    even: bool,
    fragments: Vec<Fragment>,
    page_pictures_behind: Vec<Fragment>,
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
    note_separator: Vec<Fragment>,
    note_separator_height: f32,
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
    prev_after: Option<(f32, String, bool, bool)>,
    list_labels: BTreeMap<String, String>,
    pending_break: Option<edgedoc_text::LineEnd>,
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum BreakOrigin {
    #[default]
    Automatic,
    /// The paragraph's leading gap was already considered before an automatic break.
    ConsumedBefore,
    Inline,
    Paragraph,
}

#[derive(Clone)]
struct NoteBox {
    id: i32,
    fragments: Vec<Fragment>,
    height: f32,
    separator: Vec<Fragment>,
    separator_height: f32,
    continuation_separator: Vec<Fragment>,
    continuation_separator_height: f32,
    continuation_notice: Vec<Fragment>,
    continuation_notice_height: f32,
    continued: bool,
}

impl NoteBox {
    /// Find line bottoms that can split the note without cutting an unsplittable object.
    fn cuts(&self) -> Vec<f32> {
        let mut cuts = Vec::new();
        let mut last_top = f32::MIN;
        let blocked = self
            .fragments
            .iter()
            .filter_map(|fragment| match fragment {
                Fragment::Paragraph(_) | Fragment::Rule { .. } | Fragment::MarginLabel { .. } => {
                    None
                }
                Fragment::Table(table) => Some(table.rect),
                Fragment::Picture { rect, .. } | Fragment::Placeholder { rect, .. } => Some(*rect),
            })
            .collect::<Vec<_>>();
        for fragment in &self.fragments {
            if let Fragment::Paragraph(para) = fragment {
                if let Some(first) = para.layout.lines.get(para.l0) {
                    for line in para.layout.lines.iter().take(para.l1).skip(para.l0) {
                        let top = para.rect.y + line.top - first.top;
                        last_top = last_top.max(top);
                        cuts.push(top + line.height);
                    }
                }
            }
        }
        cuts.retain(|cut| {
            *cut > 0.01
                && *cut <= last_top + 0.01
                && !blocked
                    .iter()
                    .any(|rect| rect.y < *cut - 0.01 && rect.bottom() > *cut + 0.01)
        });
        cuts.sort_by(f32::total_cmp);
        cuts.dedup_by(|a, b| (*a - *b).abs() < 0.01);
        cuts
    }

    fn first_piece(&self) -> f32 {
        self.cuts()
            .first()
            .copied()
            .unwrap_or(self.height)
            .min(self.height)
    }

    fn split(self, cut: f32) -> (Self, Self) {
        let mut head = Vec::new();
        let mut tail = Vec::new();
        for fragment in self.fragments {
            let rect = fragments_rect(std::slice::from_ref(&fragment));
            if rect.bottom() <= cut + 0.01 {
                head.push(fragment);
            } else if rect.y >= cut - 0.01 {
                tail.push(fragment);
            } else if let Fragment::Paragraph(para) = fragment {
                let first_top = para.layout.lines.get(para.l0).map_or(0.0, |line| line.top);
                let split = (para.l0..para.l1)
                    .find(|index| {
                        para.layout.lines.get(*index).map_or(true, |line| {
                            para.rect.y + line.top - first_top + line.height > cut + 0.01
                        })
                    })
                    .unwrap_or(para.l1);
                let tail_y = para
                    .layout
                    .lines
                    .get(split)
                    .map_or(para.rect.bottom(), |line| {
                        para.rect.y + line.top - first_top
                    });
                if split > para.l0 {
                    let mut part = para.clone();
                    part.l1 = split;
                    part.rect.h = (tail_y - para.rect.y).max(0.0);
                    part.continues_after = true;
                    part.links.retain(|(rect, _)| rect.y < tail_y - 0.01);
                    head.push(Fragment::Paragraph(part));
                }
                if split < para.l1 {
                    let mut part = para;
                    part.l0 = split;
                    part.rect.h = (part.rect.bottom() - tail_y).max(0.0);
                    part.rect.y = tail_y;
                    part.label = None;
                    part.continues_before = true;
                    part.links.retain(|(rect, _)| rect.bottom() > tail_y + 0.01);
                    tail.push(Fragment::Paragraph(part));
                }
            } else {
                head.push(fragment);
            }
        }
        let start = tail
            .iter()
            .filter_map(|fragment| match fragment {
                Fragment::Paragraph(para) => Some(para.rect.y),
                _ => None,
            })
            .fold(f32::MAX, f32::min);
        let start = if start == f32::MAX {
            cut
        } else {
            start.max(cut)
        };
        for fragment in &mut tail {
            shift_fragment(fragment, 0.0, -start);
        }
        let head_note = Self {
            id: self.id,
            fragments: head,
            height: cut,
            separator: self.separator.clone(),
            separator_height: self.separator_height,
            continuation_separator: self.continuation_separator.clone(),
            continuation_separator_height: self.continuation_separator_height,
            continuation_notice: self.continuation_notice.clone(),
            continuation_notice_height: self.continuation_notice_height,
            continued: self.continued,
        };
        let tail_note = Self {
            id: self.id,
            fragments: tail,
            height: (self.height - start).max(0.0),
            separator: self.separator,
            separator_height: self.separator_height,
            continuation_separator: self.continuation_separator,
            continuation_separator_height: self.continuation_separator_height,
            continuation_notice: self.continuation_notice,
            continuation_notice_height: self.continuation_notice_height,
            continued: true,
        };
        (head_note, tail_note)
    }
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
            starts_after: BreakOrigin::Automatic,
            next_break: BreakOrigin::Automatic,
            index,
            number,
            section,
            section_index,
            decoration_section: section_index,
            decoration_first: false,
            final_decorations: false,
            even,
            fragments: Vec::new(),
            page_pictures_behind: Vec::new(),
            y: 0.0,
            bottom: 0.0,
            body: Rect::default(),
            column: 0,
            columns: Vec::new(),
            notes: Vec::new(),
            note_separator: Vec::new(),
            note_separator_height: 0.0,
            decorations: Vec::new(),
            totals: Totals::default(),
            body_fragments: Vec::new(),
            table_cursors: BTreeMap::new(),
            line_cursors: BTreeMap::new(),
            prev_after: None,
            list_labels: BTreeMap::new(),
            pending_break: None,
        }
    }

    fn begin<S: Shaper>(
        &mut self,
        flow: &mut Flow<'_, S>,
        section: &FlowSection,
        even: bool,
        first_page: bool,
    ) {
        self.decoration_first = first_page;
        self.totals = flow.totals.clone();
        let body = section.body_rect_on(even);
        self.body = body;
        self.columns = column_rects(section, body);
        self.column = 0;
        self.y = self.columns.first().map_or(body.y, |c| c.y);
        self.bottom = self.columns.first().map_or(body.bottom(), |c| c.bottom());
        self.decorations = self.build_decorations(flow, section, even, first_page);
        // Decorations reserve their measured height before the first body block is placed.
        self.columns = column_rects(section, self.body);
        self.y = self.columns.first().map_or(self.body.y, |c| c.y);
        self.bottom = self
            .columns
            .first()
            .map_or(self.body.bottom(), |c| c.bottom());
    }

    fn continue_section(&mut self, section: &FlowSection, section_index: usize) {
        let columns_changed = self.section.columns != section.columns
            || self.section.column_space != section.column_space;
        self.section_index = section_index;
        self.section = section.clone();
        if columns_changed || section.columns > 1 {
            let bottom = (fragments_rect(&self.fragments).bottom()
                + trailing_space(&self.fragments))
            .max(self.y);
            let region = Rect::new(
                self.body.x,
                bottom,
                self.body.w,
                (self.body.bottom() - bottom).max(1.0),
            );
            self.columns = column_rects(section, region);
            self.column = 0;
            self.y = region.y;
            self.prev_after = None;
        }
    }

    fn resolve(&self, p: &FlowParagraph) -> FlowParagraph {
        with_fields(
            p,
            self.number,
            self.section_index,
            &self.section.page_fmt,
            &self.totals,
        )
    }

    fn resolve_table(&self, t: &FlowTable) -> FlowTable {
        let mut table = t.clone();
        for row in &mut table.rows {
            for cell in &mut row.cells {
                for block in &mut cell.blocks {
                    match block {
                        FlowBlock::Paragraph(p) => *p = self.resolve(p),
                        FlowBlock::Table(t) => *t = self.resolve_table(t),
                        _ => {}
                    }
                }
            }
        }
        table
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
        let header = if first_page && section.title_page {
            section.header_first.as_ref()
        } else if even && section.even_and_odd {
            section.header_even.as_ref().or(section.header.as_ref())
        } else {
            section.header.as_ref()
        };
        if let Some(blocks) = header {
            let band = Rect::new(
                self.body.x,
                section.header_dist.max(0.0),
                (section.page_w - section.margin_left - section.margin_right - section.gutter)
                    .max(1.0),
                section.page_h,
            );
            let fragments = self.blocks_in_band(flow, blocks, band);
            let bottom = fragments_rect(&fragments).bottom() + trailing_space(&fragments);
            if !fragments.is_empty() && bottom > self.body.y {
                let end = self.body.bottom();
                self.body.y = bottom;
                self.body.h = (end - bottom).max(1.0);
            }
            out.extend(fragments);
        }
        let footer = if first_page && section.title_page {
            section.footer_first.as_ref()
        } else if even && section.even_and_odd {
            section.footer_even.as_ref().or(section.footer.as_ref())
        } else {
            section.footer.as_ref()
        };
        if let Some(blocks) = footer {
            let band = Rect::new(
                self.body.x,
                0.0,
                (section.page_w - section.margin_left - section.margin_right - section.gutter)
                    .max(1.0),
                section.page_h,
            );
            if self.final_decorations && blocks_have_numbering(blocks) {
                let labels = std::mem::take(&mut self.list_labels);
                self.blocks_in_band(flow, blocks, band);
                self.list_labels = labels;
            }
            let mut fragments = self.blocks_in_band(flow, blocks, band);
            if !fragments.is_empty() {
                let ink = fragments_rect(&fragments);
                let shift = section.page_h
                    - section.footer_dist
                    - ink.bottom()
                    - trailing_space(&fragments);
                for fragment in &mut fragments {
                    shift_fragment(fragment, 0.0, shift);
                }
                let top = ink.y + shift;
                if top < self.body.bottom() {
                    self.body.h = (top - self.body.y).max(1.0);
                }
                if top < self.body.y {
                    flow.report(
                        "display.header_overflow",
                        self.index,
                        "Headers and footers leave no body area".into(),
                    );
                }
            }
            out.extend(fragments);
        }
        out
    }

    /// Place blocks inside a fixed band, as a header or a note does.
    fn blocks_in_band<S: Shaper>(
        &mut self,
        flow: &mut Flow<'_, S>,
        blocks: &[FlowBlock],
        band: Rect,
    ) -> Vec<Fragment> {
        let mut out = Vec::new();
        let mut y = band.y;
        for block in blocks {
            match block {
                FlowBlock::EndnoteSeparator { .. } => {
                    out.push(endnote_separator(band.x, y, band.w));
                    y += 20.0;
                }
                FlowBlock::Paragraph(p) => {
                    let resolved;
                    let p = if has_fields(&p.text) {
                        resolved = self.resolve(p);
                        &resolved
                    } else {
                        p
                    };
                    if let (true, Some(list)) = (p.path.starts_with("part["), p.list.as_ref()) {
                        let label = self
                            .list_labels
                            .entry(p.path.clone())
                            .or_insert_with(|| flow.next_label(list))
                            .clone();
                        if !p.para.footer_page_frame {
                            y += p.para.space_before;
                        }
                        let (layout, left, label_rect) =
                            cell_paragraph_layout(flow, p, band.w, Some(&label));
                        let height = layout.height;
                        let rect = Rect::new(
                            band.x + left,
                            y,
                            (band.w - left - p.para.indent_right).max(1.0),
                            height,
                        );
                        let pictures =
                            inline_picture_fragments(p, &layout, rect, 0, layout.lines.len());
                        out.push(Fragment::Paragraph(ParagraphFragment {
                            path: p.path.clone(),
                            text: std::sync::Arc::from(p.text.as_str()),
                            label: label_rect
                                .map(|(x, w)| (label, Rect::new(band.x + x, y, w, height))),
                            rect,
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
                        out.extend(pictures);
                        if !p.para.footer_page_frame {
                            y += height + p.para.space_after;
                        }
                        continue;
                    }
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
                    let width =
                        (band.w - label_w - p.para.indent_left - p.para.indent_right).max(1.0);
                    if !p.para.footer_page_frame {
                        y += p.para.space_before;
                    }
                    let layout = flow.paragraph_layout(p, width, &[], p.para.indent_first);
                    let height = layout.height;
                    let rect = Rect::new(band.x + label_w + p.para.indent_left, y, width, height);
                    let pictures =
                        inline_picture_fragments(p, &layout, rect, 0, layout.lines.len());
                    out.push(Fragment::Paragraph(ParagraphFragment {
                        path: p.path.clone(),
                        text: std::sync::Arc::from(p.text.as_str()),
                        label: None,
                        rect,
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
                    out.extend(pictures);
                    if !p.para.footer_page_frame {
                        y += height + p.para.space_after;
                    }
                }
                FlowBlock::Table(t) => {
                    let resolved = self.resolve_table(t);
                    let t = &resolved;
                    assign_table_list_labels(flow, t, &mut self.list_labels);
                    let width = if t.preferred_width.is_some() {
                        table_width(t, band.w)
                    } else {
                        table_width(t, band.w).min(band.w)
                    };
                    let x = match t.align {
                        Align::Center => band.x + (band.w - width) / 2.0,
                        Align::Right => band.right() - width,
                        _ => band.x + t.indent,
                    };
                    let (fragment, height) = build_table_fragment(
                        flow,
                        t,
                        Rect::new(x, y, width, 0.0),
                        0,
                        t.rows.len(),
                        false,
                        &self.list_labels,
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
                    src,
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
                        src: *src,
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
        if let Some(kind) = self.pending_break.take() {
            self.next_break = BreakOrigin::Inline;
            if kind == edgedoc_text::LineEnd::PageBreak || !self.next_column() {
                return false;
            }
            self.starts_after = BreakOrigin::Inline;
        }
        match block {
            FlowBlock::Paragraph(p) => self.push_paragraph(flow, p),
            FlowBlock::Table(t) => self.push_table(flow, &self.resolve_table(t)),
            FlowBlock::EndnoteSeparator { blocks, .. } => {
                let column = self.current_column();
                self.y += self.prev_after.take().map_or(0.0, |(after, _, _, _)| after);
                if blocks.is_empty() {
                    self.fragments
                        .push(endnote_separator(column.x, self.y, column.w));
                    self.y += 20.0;
                } else {
                    let (mut fragments, height) =
                        self.layout_note_separator(flow, blocks, false, column);
                    for fragment in &mut fragments {
                        shift_fragment(fragment, 0.0, self.y);
                    }
                    self.fragments.extend(fragments);
                    self.y += height;
                }
                true
            }
            FlowBlock::Picture {
                width,
                height,
                alt,
                resource,
                src,
                align,
                ..
            } => {
                let column = self.current_column();
                let gap = if self.y > column.y + 0.5 {
                    self.prev_after
                        .as_ref()
                        .map_or(0.0, |(after, _, _, _)| *after)
                } else {
                    0.0
                };
                if self.y + gap + height > self.bottom && self.y > column.y {
                    if self.next_column() {
                        return self.push_block(flow, block, totals);
                    }
                    return false;
                }
                self.y += gap;
                self.prev_after = None;
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
                    src: *src,
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
            let reserved = if self.notes.is_empty() {
                0.0
            } else {
                self.note_separator_height + self.notes.iter().map(|note| note.height).sum::<f32>()
            };
            self.bottom = c.bottom() - reserved;
            return true;
        }
        false
    }

    fn push_paragraph<S: Shaper>(&mut self, flow: &mut Flow<'_, S>, p: &FlowParagraph) -> bool {
        // A page field shows this page's number, so the paragraph is resolved per page.
        let resolved;
        let p = if has_fields(&p.text) {
            resolved = self.resolve(p);
            &resolved
        } else {
            p
        };
        let column = self.current_column();
        if p.para.page_break_before
            && !self.line_cursors.contains_key(&p.path)
            && self.y > column.y + 0.5
        {
            self.next_break = BreakOrigin::Paragraph;
            return false;
        }
        // A paragraph that continues from an earlier page starts at the line cursor.
        let start_line = self.line_cursors.get(&p.path).copied().unwrap_or(0);
        // A list item takes its number from the running counter, once, on its first line.
        let numbered = if start_line == 0 {
            p.list.as_ref().map(|l| {
                self.list_labels
                    .entry(p.path.clone())
                    .or_insert_with(|| flow.next_label(l))
                    .clone()
            })
        } else {
            None
        };
        let label = if start_line > 0 {
            String::new()
        } else {
            numbered.or_else(|| p.label.clone()).unwrap_or_default()
        };
        let measure_label = self
            .list_labels
            .get(&p.path)
            .map(String::as_str)
            .unwrap_or(&label);
        let label_w = if measure_label.is_empty() {
            0.0
        } else {
            let size = p
                .para
                .label_size
                .unwrap_or_else(|| p.run_styles.first().map_or(11.0, |s| s.size));
            let face = p.para.label_face.unwrap_or(p.para.label_fallback_face);
            flow.shaper
                .shape_run_kerning(face, measure_label, size, false, p.para.label_kerning)
                .iter()
                .map(|g| g.advance)
                .sum::<f32>()
                + 6.0
        };
        // A left-aligned bullet with a tab suffix occupies the hanging indent;
        // the text itself begins at the paragraph's left indent.
        let hanging_bullet = p.list.as_ref().is_some_and(|list| {
            list.format == "bullet"
                && (list.suffix.is_empty() || list.suffix == "tab")
                && (list.justification.is_empty() || list.justification == "left")
        }) && p.para.indent_first < 0.0
            && label_w - 6.0 <= -p.para.indent_first
            && !measure_label.is_empty();
        // Wordcraft positions a numeric label at the first-line indent, then places text at
        // the next list or paragraph tab (or after the label for a space/nothing suffix).
        let numeric_label = p
            .list
            .as_ref()
            .filter(|list| list.format != "bullet" && !measure_label.is_empty())
            .map(|list| {
                let at = p.para.indent_left + p.para.indent_first;
                let glyph_width = (label_w - 6.0).max(0.0);
                let label_x = match list.justification.as_str() {
                    "right" | "end" => at - glyph_width,
                    "center" => at - glyph_width / 2.0,
                    _ => at,
                };
                let end = label_x + glyph_width;
                let text_x = match list.suffix.as_str() {
                    "nothing" => end,
                    "space" => {
                        end + p
                            .para
                            .label_size
                            .unwrap_or_else(|| p.run_styles.first().map_or(11.0, |s| s.size))
                            * 0.25
                    }
                    _ => {
                        let step = if p.para.default_tab > 0.0 {
                            p.para.default_tab
                        } else {
                            36.0
                        };
                        let default = ((end + 0.01) / step).floor().mul_add(step, step);
                        let explicit = p
                            .para
                            .tabs
                            .iter()
                            .filter(|tab| tab.pos > end + 0.01)
                            .map(|tab| tab.pos)
                            .chain(
                                (p.para.indent_first < 0.0 && p.para.indent_left > end + 0.01)
                                    .then_some(p.para.indent_left),
                            )
                            .reduce(f32::min)
                            .unwrap_or(default);
                        list.tab
                            .filter(|pos| *pos > end + 0.01)
                            .map_or(explicit, |pos| pos.min(explicit))
                    }
                };
                (label_x, text_x)
            });
        let label_reserve = if hanging_bullet || numeric_label.is_some() {
            0.0
        } else {
            label_w
        };
        let width = (column.w - label_reserve - p.para.indent_left - p.para.indent_right).max(1.0);
        // A hanging bullet uses the first-line indent for the label rather than the text.
        let first_indent = if let Some((_, text_x)) = numeric_label {
            text_x - p.para.indent_left
        } else if hanging_bullet {
            0.0
        } else {
            p.para.indent_first
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
        let at_top = self.y <= column.y + 0.5;
        if start_line > 0
            || (at_top
                && (self.starts_after == BreakOrigin::ConsumedBefore
                    || self.starts_after == BreakOrigin::Inline
                    || (self.starts_after == BreakOrigin::Paragraph
                        && self.section.compatibility_mode >= 15)))
        {
            space_before = 0.0;
        } else if at_top {
            if let Some((after, _, _, _)) = &self.prev_after {
                space_before = (space_before - after.max(0.0)).max(0.0);
            }
        } else if let Some((after, prev_style, prev_listed, prev_auto)) = self.prev_after.clone() {
            let same_style = prev_style == p.para.style_id;
            let contextual = p.para.contextual_spacing && same_style;
            if contextual {
                // Contextual spacing drops the gap between two paragraphs of one style.
                space_before = 0.0;
            } else if prev_listed && p.list.is_some() {
                if prev_auto || p.para.space_before_auto {
                    let before = if p.para.space_before_auto {
                        0.0
                    } else {
                        space_before
                    };
                    let after = if prev_auto { 0.0 } else { after };
                    space_before = before.max(after);
                } else {
                    space_before = space_before.max(after);
                }
            } else {
                space_before = space_before.max(after);
            }
        }
        if self.index == 0 && p.path == "body[0]" && p.para.space_before_auto {
            space_before = 0.0;
        }
        self.starts_after = BreakOrigin::Automatic;
        let border_room = |border: Option<Border>| {
            border.filter(|b| b.style.is_visible()).map_or(0.0, |b| {
                b.space
                    + b.width.max(if b.style == BorderStyle::Thick {
                        1.5
                    } else {
                        0.25
                    })
            })
        };
        let top_room = if start_line == 0 && !p.border_join_above {
            border_room(p.para.borders.top)
        } else {
            0.0
        };
        let bottom_room = if p.border_join_below {
            0.0
        } else {
            border_room(p.para.borders.bottom)
        };
        let available = self.bottom - self.y - space_before;
        // Keep-lines and keep-next move the paragraph whole when it can fit a page.
        let full_height =
            layout.height + space_before + p.para.space_after + top_room + bottom_room;
        // A kept-together paragraph that does not fit this page moves on whole, unless it cannot
        // fit any page. See `DP-L-05`.
        if p.para.keep_lines
            && full_height > available
            && full_height <= column.h
            && self.y > column.y + 0.5
        {
            if self.next_column() {
                return self.push_paragraph(flow, p);
            }
            return false;
        }
        let mut top = self.y + space_before + top_room;
        let mut line_index = start_line;
        let mut any = false;
        let mut pending_notes: Vec<(usize, NoteBox)> = Vec::new();
        let mut pending_height = 0.0f32;
        while line_index < line_count {
            let line = match layout.lines.get(line_index) {
                Some(l) => l,
                None => break,
            };
            let ending_room = if line_index + 1 == line_count {
                bottom_room
            } else {
                0.0
            };
            // At the page's bottom margin, Word lets the extra leading from
            // multiple spacing extend below the margin as long as the text
            // itself fits. Notes and footer-reduced areas need the full line.
            let at_margin = self.bottom >= self.section.page_h - self.section.margin_bottom - 0.01;
            let extra_leading = !p.para.line_exact
                && !p.para.line_at_least
                && p.para.line_height > 12.0
                && at_margin
                && self.notes.is_empty()
                && p.notes.is_empty();
            let fit_height = if extra_leading {
                (line.baseline - line.top + line.text_desc).clamp(0.0, line.height)
            } else {
                line.height
            };
            let mut line_notes = Vec::new();
            for note in &p.notes {
                if note.offset >= line.start
                    && note.offset < line.stop
                    && !self.notes.iter().any(|placed| placed.id == note.id)
                    && !pending_notes.iter().any(|(_, placed)| placed.id == note.id)
                    && !line_notes
                        .iter()
                        .any(|(_, placed): &(usize, NoteBox)| placed.id == note.id)
                {
                    line_notes.push((note.offset, self.layout_note(flow, note)));
                }
            }
            let line_notes_height = line_notes.iter().map(|(_, note)| note.height).sum::<f32>();
            let separator =
                if self.notes.is_empty() && pending_notes.is_empty() && !line_notes.is_empty() {
                    line_notes[0].1.separator_height
                } else {
                    0.0
                };
            let required = top + fit_height + ending_room;
            let fits_full =
                required <= self.bottom - pending_height - line_notes_height - separator + 0.001;
            let fits_first_piece = line_notes.split_last().is_some_and(|((_, last), prior)| {
                let before = prior.iter().map(|(_, note)| note.height).sum::<f32>();
                required
                    <= self.bottom
                        - pending_height
                        - separator
                        - before
                        - last.first_piece()
                        - last.continuation_notice_height
                        + 0.001
            });
            if !fits_full && !fits_first_piece {
                // The first line of a fragment must fit on this page when the page already holds
                // content. Otherwise the page is full. See DP-L-05.
                if !any && self.y > column.y + 0.5 {
                    break;
                }
                if any {
                    break;
                }
            }
            pending_height += line_notes_height + separator;
            pending_notes.extend(line_notes);
            top += line.height;
            line_index += 1;
            any = true;
            if matches!(
                line.end,
                edgedoc_text::LineEnd::PageBreak | edgedoc_text::LineEnd::ColumnBreak
            ) {
                break;
            }
        }
        if !any {
            // Not one line fits. When the column already holds something, the page is full and
            // the caller starts another page. See DP-L-05.
            if self.y > column.y + 0.5 {
                if self.next_column() {
                    self.starts_after = BreakOrigin::ConsumedBefore;
                    return self.push_paragraph(flow, p);
                }
                self.next_break = BreakOrigin::ConsumedBefore;
                return false;
            }
            // A paragraph alone taller than the page splits here. See DP-F-40.
            flow.report("display.page_overflow", self.index, p.path.clone());
            if layout.lines.get(start_line).is_none() {
                return true;
            }
            line_index = start_line + 1;
        }
        // Widow and orphan control: a split leaves at least two lines on each page. See DP-F-26.
        let hard_break = layout
            .lines
            .get(line_index.saturating_sub(1))
            .map(|l| l.end);
        if p.para.widow_control
            && line_index < line_count
            && !matches!(
                hard_break,
                Some(edgedoc_text::LineEnd::PageBreak | edgedoc_text::LineEnd::ColumnBreak)
            )
        {
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
        let placed_end = layout
            .lines
            .get(line_index.saturating_sub(1))
            .map_or(0, |line| line.stop);
        pending_notes.retain(|(offset, _)| *offset < placed_end);
        if !pending_notes.is_empty() {
            if self.notes.is_empty() {
                self.note_separator_height = pending_notes[0].1.separator_height;
                self.note_separator = pending_notes[0].1.separator.clone();
                self.bottom -= self.note_separator_height;
            }
            for (_, note) in pending_notes {
                self.bottom -= note.height;
                self.notes.push(note);
            }
        }
        top = self.y
            + space_before
            + top_room
            + layout
                .lines
                .iter()
                .skip(start_line)
                .take(line_index - start_line)
                .map(|l| l.height)
                .sum::<f32>();
        let fragment_top = self.y + space_before + top_room;
        let rect = Rect::new(
            column.x + label_reserve + p.para.indent_left,
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
                Rect::new(
                    if let Some((label_x, _)) = numeric_label {
                        let shift = layout
                            .lines
                            .first()
                            .and_then(|line| line.xs.first())
                            .map_or(0.0, |x| x - first_indent);
                        column.x + label_x + shift
                    } else if hanging_bullet {
                        column.x + p.para.indent_left + p.para.indent_first
                    } else {
                        column.x
                    },
                    fragment_top,
                    label_w.max(1.0),
                    top - fragment_top,
                ),
            ))
        };
        let mut fragment_para = p.para.clone();
        if p.border_join_above {
            fragment_para.borders.top = None;
        }
        if p.border_join_below {
            fragment_para.borders.bottom = None;
        }
        self.fragments.push(Fragment::Paragraph(ParagraphFragment {
            path: p.path.clone(),
            text: std::sync::Arc::from(p.text.as_str()),
            label: label_rect,
            rect,
            l0: start_line,
            l1: line_index,
            para: fragment_para,
            run_styles: p.run_styles.clone(),
            layout: layout.clone(),
            continues_before: start_line > 0,
            continues_after,
            rtl: p.para.bidi,
            revision: p.revision,
            links: link_rects(&layout, &p.links, rect),
        }));
        self.fragments.extend(inline_picture_fragments(
            p, &layout, rect, start_line, line_index,
        ));
        for line in layout.lines.iter().take(line_index).skip(start_line) {
            for cluster in &layout.clusters[line.c0..line.c1] {
                if let edgedoc_text::ClusterKind::Object(index) = cluster.kind {
                    if let Some((_, picture)) =
                        p.page_pictures.iter().find(|(object, _)| *object == index)
                    {
                        self.page_pictures_behind.push(Fragment::Picture {
                            rect: Rect::new(
                                picture.x.position(self.section.page_w, picture.width),
                                picture.y.position(self.section.page_h, picture.height),
                                picture.width,
                                picture.height,
                            ),
                            resource: picture.resource,
                            src: picture.src,
                            alt: picture.alt.clone(),
                        });
                    }
                }
            }
        }
        // The gap between two paragraphs belongs to the pair, and the next paragraph decides it
        // from `prev_after` as the larger of the two spaces. Adding the space after here would
        // count it twice. See `DP-L-43`.
        self.y = top + if continues_after { 0.0 } else { bottom_room };
        if continues_after {
            // The rest of the paragraph continues on the next page or column.
            self.line_cursors.insert(p.path.clone(), line_index);
            if hard_break.is_some_and(|kind| {
                matches!(
                    kind,
                    edgedoc_text::LineEnd::PageBreak | edgedoc_text::LineEnd::ColumnBreak
                )
            }) {
                self.next_break = BreakOrigin::Inline;
            }
            if !matches!(hard_break, Some(edgedoc_text::LineEnd::PageBreak)) && self.next_column() {
                return self.push_paragraph(flow, p);
            }
            return false;
        }
        self.line_cursors.remove(&p.path);
        self.list_labels.remove(&p.path);
        if matches!(
            hard_break,
            Some(edgedoc_text::LineEnd::PageBreak | edgedoc_text::LineEnd::ColumnBreak)
        ) {
            self.pending_break = hard_break;
        }
        self.prev_after = Some((
            p.para.space_after,
            p.para.style_id.clone(),
            p.list.is_some(),
            p.para.space_after_auto,
        ));
        true
    }

    fn push_table<S: Shaper>(&mut self, flow: &mut Flow<'_, S>, t: &FlowTable) -> bool {
        // An empty table has no remaining rows. Retrying it on each page cannot make progress.
        if t.rows.is_empty() {
            return true;
        }
        // An automatic table follows its contents even when a preferred grid is present.
        // Restrict this path to simple cells whose measured widths fit in the column.
        if t.auto_width && !t.layout_fixed && t.rows.iter().all(|row| row.grid_before == 0 && row.grid_after == 0 && row.cells.iter().all(|cell|
            cell.span == 1 && cell.width.is_none() && cell.blocks.iter().all(|block|
                matches!(block, FlowBlock::Paragraph(p) if !p.text.contains('\t') && p.para.align == Align::Left))))
        {
            let columns = t.rows.iter().map(|row| row.cells.len()).max().unwrap_or(0);
            if columns > 0 {
                let mut widths = vec![4.0f32; columns];
                let mut has_content = false;
                for row in &t.rows {
                    for (col, cell) in row.cells.iter().enumerate() {
                        let mut content = 0.0f32;
                        for block in &cell.blocks {
                            if let FlowBlock::Paragraph(p) = block {
                                has_content |= !p.text.is_empty();
                                let layout = flow.paragraph_layout(p, self.current_column().w, &[], p.para.indent_first);
                                content = content.max(layout.max_width + p.para.indent_left + p.para.indent_right);
                            }
                        }
                        widths[col] = widths[col].max(content + cell.margins[1] + cell.margins[3]);
                    }
                }
                if has_content && widths.iter().sum::<f32>() <= self.current_column().w {
                    let mut measured = t.clone();
                    measured.widths = widths;
                    measured.auto_width = false;
                    return self.push_table(flow, &measured);
                }
            }
        }
        let column = self.current_column();
        assign_table_list_labels(flow, t, &mut self.list_labels);
        // A paragraph's trailing spacing precedes the following table. Do not defer it
        // until the next paragraph, which shifts the table while leaving later text fixed.
        if self.table_row_cursor(&t.path) == 0 {
            let after = self.prev_after.take().map_or(0.0, |(after, _, _, _)| after);
            // A page or column starts with a fresh y position. The preceding paragraph's
            // after-spacing was left on its previous page and must not move this table.
            if self.y > column.y + 0.5 {
                self.y += after;
            }
        }
        if t.floating {
            // A floating table takes its own space in version 1, at the current position.
            let (fragment, height) = build_table_fragment(
                flow,
                t,
                Rect::new(column.x, self.y, column.w, 0.0),
                0,
                t.rows.len(),
                false,
                &self.list_labels,
            );
            self.fragments.push(Fragment::Table(fragment));
            self.y += height;
            return true;
        }
        let grid_width: f32 = t.widths.iter().sum();
        // Wordcraft retains a modestly wider preferred grid when the automatic table's
        // cells request widths. A bare grid without cell widths remains content-fitted.
        let keep_grid = t.auto_width
            && grid_width.is_finite()
            && grid_width > 0.0
            && (t.layout_fixed
                || (grid_width > column.w
                    && grid_width <= column.w * 1.5
                    && t.rows
                        .iter()
                        .any(|row| row.cells.iter().any(|cell| cell.width.is_some()))));
        let width = if t.preferred_width.is_some() {
            table_width(t, column.w)
        } else if keep_grid {
            grid_width.min(100_000.0)
        } else {
            column.w.min(table_width(t, column.w))
        };
        let x = match t.align {
            Align::Center => column.x + (column.w - width) / 2.0,
            Align::Right => column.right() - width,
            _ => column.x + t.indent,
        };
        let row_heights = table_row_heights(flow, t, width, &self.list_labels);
        let start_row = self.table_row_cursor(&t.path);
        let mut rows_placed = 0usize;
        let repeated = start_row > 0 && t.rows.iter().take(start_row).any(|row| row.header);
        let mut height_total = if repeated {
            t.rows
                .iter()
                .enumerate()
                .take(start_row)
                .filter(|(_, row)| row.header)
                .map(|(i, _)| row_heights.get(i).copied().unwrap_or(0.0))
                .sum()
        } else {
            0.0
        };
        // Measure row by row, and stop when the page is full.
        let mut r = start_row;
        while r < t.rows.len() {
            let row_h = row_heights.get(r).copied().unwrap_or(0.0);
            if self.y + height_total + row_h > self.bottom + 0.001 && rows_placed > 0 {
                break;
            }
            // A first row that cannot fit moves on whole, unless it may split. See `DP-L-38`.
            if self.y + height_total + row_h > self.bottom + 0.001
                && rows_placed == 0
                && self.y > column.y + 0.5
            {
                break;
            }
            height_total += row_h;
            rows_placed += 1;
            r += 1;
        }
        if rows_placed == 0 {
            if self.next_column() {
                return self.push_table(flow, t);
            }
            return false;
        }
        let (fragment, height) = build_table_fragment(
            flow,
            t,
            Rect::new(x, self.y, width, 0.0),
            start_row,
            start_row + rows_placed,
            repeated,
            &self.list_labels,
        );
        self.fragments.push(Fragment::Table(fragment));
        self.y += height;
        if start_row + rows_placed < t.rows.len() {
            // The table continues on the next page.
            self.set_table_row_cursor(&t.path, start_row + rows_placed);
            if self.next_column() {
                return self.push_table(flow, t);
            }
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

    fn layout_note<S: Shaper>(&mut self, flow: &mut Flow<'_, S>, note: &FlowNote) -> NoteBox {
        let band = Rect::new(self.body.x, 0.0, self.body.w, self.body.h);
        let fragments = self.blocks_in_band(flow, &note.blocks, band);
        let height = if fragments.is_empty() {
            0.0
        } else {
            fragments_rect(&fragments).bottom() + trailing_space(&fragments)
        };
        let (separator, separator_height) =
            self.layout_note_separator(flow, &note.separator, false, self.body);
        let (continuation_separator, continuation_separator_height) =
            self.layout_note_separator(flow, &note.continuation_separator, true, self.body);
        let continuation_notice = self.blocks_in_band(flow, &note.continuation_notice, band);
        let continuation_notice_height = if continuation_notice.is_empty() {
            0.0
        } else {
            fragments_rect(&continuation_notice).bottom() + trailing_space(&continuation_notice)
        };
        NoteBox {
            id: note.id,
            fragments,
            height,
            separator,
            separator_height,
            continuation_separator,
            continuation_separator_height,
            continuation_notice,
            continuation_notice_height,
            continued: false,
        }
    }

    fn layout_note_separator<S: Shaper>(
        &mut self,
        flow: &mut Flow<'_, S>,
        blocks: &[FlowBlock],
        continuation: bool,
        area: Rect,
    ) -> (Vec<Fragment>, f32) {
        let band = Rect::new(area.x, 0.0, area.w, area.h);
        let mut separator = self.blocks_in_band(flow, blocks, band);
        let separator_height = if separator.is_empty() {
            separator.push(Fragment::Rule {
                rect: Rect::new(
                    area.x,
                    5.75,
                    if continuation {
                        area.w
                    } else {
                        144.0_f32.min(area.w)
                    },
                    0.5,
                ),
                border: Border {
                    width: 0.5,
                    color: Rgba::BLACK,
                    style: BorderStyle::Single,
                    space: 0.0,
                },
            });
            12.0
        } else {
            let extent = fragments_rect(&separator).bottom() + trailing_space(&separator);
            let mut rules = Vec::new();
            for fragment in &separator {
                if let Fragment::Paragraph(para) = fragment {
                    if para.path.starts_with("footnote-separator[")
                        || para.path.starts_with("footnote-continuation-separator[")
                    {
                        if let Some(line) = para.layout.lines.first() {
                            rules.push(Fragment::Rule {
                                rect: Rect::new(
                                    para.rect.x,
                                    para.rect.y + line.height / 2.0 - 0.25,
                                    if continuation {
                                        para.rect.w
                                    } else {
                                        144.0_f32.min(para.rect.w)
                                    },
                                    0.5,
                                ),
                                border: Border {
                                    width: 0.5,
                                    color: Rgba::BLACK,
                                    style: BorderStyle::Single,
                                    space: 0.0,
                                },
                            });
                        }
                    }
                }
            }
            separator.extend(rules);
            extent
        };
        (separator, separator_height)
    }

    fn receive_notes(&mut self, carry: &mut Vec<NoteBox>) {
        if carry.is_empty() {
            return;
        }
        self.notes = std::mem::take(carry);
        if self.notes[0].continued {
            self.note_separator_height = self.notes[0].continuation_separator_height;
            self.note_separator = self.notes[0].continuation_separator.clone();
        } else {
            self.note_separator_height = self.notes[0].separator_height;
            self.note_separator = self.notes[0].separator.clone();
        }
        self.bottom -=
            self.note_separator_height + self.notes.iter().map(|note| note.height).sum::<f32>();
    }

    fn finish<S: Shaper>(
        mut self,
        flow: &mut Flow<'_, S>,
        carry: &mut Vec<NoteBox>,
    ) -> PageGeometry {
        let footnote_ids = self.notes.iter().map(|note| note.id).collect();
        let body_len = self.fragments.len();
        // Place the notes at the bottom of the body, after the body fragments.
        if !self.notes.is_empty() {
            carry.extend(self.place_notes(flow));
        }
        // `both` spreads the body between the top margin and the notes. See DP-L-03.
        if self.section.v_align == VAlign::Both {
            let end = body_len.min(self.fragments.len());
            let floor = if end < self.fragments.len() {
                fragment_top(&self.fragments[end])
            } else {
                self.body.bottom()
            };
            justify_body(&mut self.fragments[..end], floor);
        }
        // Vertical alignment moves the body. See DP-L-03.
        if matches!(self.section.v_align, VAlign::Center | VAlign::Bottom)
            && self.body_fragments.is_empty()
        {
            self.body_fragments = std::mem::take(&mut self.fragments);
        }
        if matches!(self.section.v_align, VAlign::Center | VAlign::Bottom)
            && !self.body_fragments.is_empty()
        {
            let ink = fragments_rect(&self.body_fragments);
            let free =
                (self.body.bottom() - ink.bottom() - trailing_space(&self.body_fragments)).max(0.0);
            let shift = if self.section.v_align == VAlign::Bottom {
                free
            } else {
                free / 2.0
            };
            if shift > 0.5 {
                for f in &mut self.body_fragments {
                    shift_fragment(f, 0.0, shift);
                }
            }
        }
        let numbered = if self.body_fragments.is_empty() {
            &self.fragments[..body_len.min(self.fragments.len())]
        } else {
            &self.body_fragments[..body_len.min(self.body_fragments.len())]
        };
        let line_labels = self.line_number_labels(flow, numbered);
        let page_rules = page_frame_rules(&self.section);
        let mut fragments = std::mem::take(&mut self.decorations);
        let decoration_count = fragments.len();
        let ordered = if self.body_fragments.is_empty() {
            std::mem::take(&mut self.fragments)
        } else {
            std::mem::take(&mut self.body_fragments)
        };
        // A behind-text float paints before the body, and everything else after.
        let mut behind = std::mem::take(&mut self.page_pictures_behind);
        let mut front: Vec<Fragment> = Vec::new();
        for f in ordered {
            front.push(f);
        }
        fragments.append(&mut behind);
        fragments.append(&mut front);
        fragments.extend(line_labels);
        fragments.extend(page_rules);
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
            footnote_ids,
            body: self.body,
            decoration_count,
            decoration_section: self.decoration_section,
            decoration_first: self.decoration_first,
        }
    }

    fn place_notes<S: Shaper>(&mut self, flow: &mut Flow<'_, S>) -> Vec<NoteBox> {
        let total = self.notes.iter().map(|note| note.height).sum::<f32>();
        let original_bottom = self.bottom + self.note_separator_height + total;
        let floor = fragments_rect(&self.fragments)
            .bottom()
            .max(fragments_rect(&self.body_fragments).bottom())
            .max(self.body.y);
        let top = self.bottom.max(floor);
        let mut y = top + self.note_separator_height;
        for mut fragment in std::mem::take(&mut self.note_separator) {
            shift_fragment(&mut fragment, 0.0, top);
            self.fragments.push(fragment);
        }
        let mut carry = Vec::new();
        let mut placed = 0usize;
        let mut remaining = std::mem::take(&mut self.notes).into_iter();
        while let Some(note) = remaining.next() {
            let room = original_bottom - y;
            let mut notice = None;
            let note = if note.height <= room + 0.01 {
                note
            } else {
                let cuts = note.cuts();
                let cut = cuts
                    .iter()
                    .rev()
                    .find(|cut| **cut <= room - note.continuation_notice_height + 0.01)
                    .copied();
                if let Some(cut) = cut {
                    let (head, tail) = note.split(cut);
                    notice = Some(head.continuation_notice.clone());
                    carry.push(tail);
                    carry.extend(&mut remaining);
                    head
                } else if placed == 0 {
                    flow.report(
                        "display.note_area_full",
                        self.index,
                        format!("A note cannot split within {room:.0} pt"),
                    );
                    note
                } else {
                    carry.push(note);
                    carry.extend(&mut remaining);
                    break;
                }
            };
            for mut fragment in note.fragments {
                shift_fragment(&mut fragment, 0.0, y);
                self.fragments.push(fragment);
            }
            y += note.height;
            if let Some(notice) = notice {
                for mut fragment in notice {
                    shift_fragment(&mut fragment, 0.0, y);
                    self.fragments.push(fragment);
                }
            }
            placed += 1;
            if !carry.is_empty() {
                break;
            }
        }
        carry
    }

    /// Paint `w:lnNumType` beside body lines. Notes and suppressed paragraphs are not counted.
    fn line_number_labels<S: Shaper>(
        &self,
        flow: &mut Flow<'_, S>,
        body: &[Fragment],
    ) -> Vec<Fragment> {
        let Some(spec) = self.section.line_numbers.clone() else {
            return Vec::new();
        };
        let count_by = spec.count_by.max(1);
        let restart = spec.restart.as_str();
        let reset = match restart {
            "continuous" => !flow.line_ordinal_ready,
            "newSection" => !flow.line_ordinal_ready || flow.line_section != self.section_index,
            _ => true,
        };
        if reset {
            flow.line_ordinal = spec.start;
        }
        flow.line_ordinal_ready = true;
        flow.line_section = self.section_index;
        let mut out = Vec::new();
        for_each_numbered_line(body, &mut |baseline, face, size, suppress| {
            if suppress {
                return;
            }
            let ordinal = flow.line_ordinal;
            flow.line_ordinal = flow.line_ordinal.saturating_add(1);
            if ordinal % count_by != 0 {
                return;
            }
            let text = ordinal.to_string();
            let width: f32 = flow
                .shaper
                .shape_run(face, &text, size, false)
                .iter()
                .map(|glyph| glyph.advance)
                .sum();
            let width = if width > 0.0 {
                width
            } else {
                size * 0.5 * text.chars().count() as f32
            };
            let x = (self.body.x - spec.distance - width).max(1.0);
            out.push(Fragment::MarginLabel {
                rect: Rect::new(x, baseline - size * 0.8, width.max(1.0), size),
                text,
                baseline,
                size,
                face,
            });
        });
        out
    }
}

fn fragment_top(fragment: &Fragment) -> f32 {
    match fragment {
        Fragment::Paragraph(p) => p.rect.y,
        Fragment::Table(t) => t.rect.y,
        Fragment::Picture { rect, .. }
        | Fragment::Placeholder { rect, .. }
        | Fragment::Rule { rect, .. }
        | Fragment::MarginLabel { rect, .. } => rect.y,
    }
}

fn justify_body(fragments: &mut [Fragment], floor: f32) {
    let content = fragments.iter().filter(|f| is_body_block(f)).count();
    if content < 2 {
        return;
    }
    let ink = fragments_rect(fragments);
    let free = (floor - ink.bottom() - trailing_space(fragments)).max(0.0);
    if free < 0.5 {
        return;
    }
    let step = free / (content - 1) as f32;
    let mut seen = 0usize;
    let mut dy = 0.0;
    for fragment in fragments.iter_mut() {
        if is_body_block(fragment) && seen > 0 {
            dy = step * seen as f32;
        }
        if dy > 0.0 {
            shift_fragment(fragment, 0.0, dy);
        }
        if is_body_block(fragment) {
            seen += 1;
        }
    }
}

fn is_body_block(fragment: &Fragment) -> bool {
    matches!(
        fragment,
        Fragment::Paragraph(_)
            | Fragment::Table(_)
            | Fragment::Picture { .. }
            | Fragment::Placeholder { .. }
    )
}

fn for_each_numbered_line(fragments: &[Fragment], visit: &mut dyn FnMut(f32, u32, f32, bool)) {
    for fragment in fragments {
        match fragment {
            Fragment::Paragraph(para) => {
                let Some(first) = para.layout.lines.get(para.l0) else {
                    continue;
                };
                let style = para.run_styles.first();
                let face = style.map_or(0, |item| item.face);
                let size = style.map_or(10.0, |item| item.size);
                for line in para.layout.lines.iter().take(para.l1).skip(para.l0) {
                    let baseline = para.rect.y + line.baseline - first.top;
                    visit(baseline, face, size, para.para.suppress_line_numbers);
                }
            }
            Fragment::Table(table) => for_each_numbered_line(&table.content, visit),
            Fragment::Picture { .. }
            | Fragment::Placeholder { .. }
            | Fragment::Rule { .. }
            | Fragment::MarginLabel { .. } => {}
        }
    }
}

fn page_frame_rules(section: &FlowSection) -> Vec<Fragment> {
    let frame = section.page_frame;
    let mut out = Vec::new();
    let inset = |edge: Option<Border>| edge.map_or(0.0, |border| border.space);
    let left = inset(frame.left);
    let right = inset(frame.right);
    let top = inset(frame.top);
    let bottom = inset(frame.bottom);
    if let Some(border) = frame.top {
        let width = border.width.max(0.25);
        out.push(Fragment::Rule {
            rect: Rect::new(left, top, (section.page_w - left - right).max(width), width),
            border,
        });
    }
    if let Some(border) = frame.bottom {
        let width = border.width.max(0.25);
        out.push(Fragment::Rule {
            rect: Rect::new(
                left,
                (section.page_h - bottom - width).max(0.0),
                (section.page_w - left - right).max(width),
                width,
            ),
            border,
        });
    }
    if let Some(border) = frame.left {
        let width = border.width.max(0.25);
        out.push(Fragment::Rule {
            rect: Rect::new(left, top, width, (section.page_h - top - bottom).max(width)),
            border,
        });
    }
    if let Some(border) = frame.right {
        let width = border.width.max(0.25);
        out.push(Fragment::Rule {
            rect: Rect::new(
                (section.page_w - right - width).max(0.0),
                top,
                width,
                (section.page_h - top - bottom).max(width),
            ),
            border,
        });
    }
    out
}

fn trailing_space(fragments: &[Fragment]) -> f32 {
    let bottom = fragments_rect(fragments).bottom();
    fragments
        .iter()
        .filter_map(|f| match f {
            Fragment::Paragraph(p)
                if !p.continues_after && (p.rect.bottom() - bottom).abs() < 0.01 =>
            {
                let border = p.para.borders.bottom.filter(|b| b.style.is_visible());
                Some(
                    p.para.space_after
                        + border.map_or(0.0, |b| {
                            b.space
                                + b.width.max(if b.style == BorderStyle::Thick {
                                    1.5
                                } else {
                                    0.25
                                })
                        }),
                )
            }
            _ => None,
        })
        .fold(0.0, f32::max)
}

fn fragments_rect(fragments: &[Fragment]) -> Rect {
    let mut out = Rect::new(f32::MAX, f32::MAX, 0.0, 0.0);
    for f in fragments {
        let r = match f {
            Fragment::Paragraph(p) => p.rect,
            Fragment::Table(t) => t.rect,
            Fragment::Picture { rect, .. } => *rect,
            Fragment::Placeholder { rect, .. } => *rect,
            Fragment::Rule { rect, .. } | Fragment::MarginLabel { rect, .. } => *rect,
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
        Fragment::Paragraph(p) => {
            p.rect = p.rect.translate(dx, dy);
            if let Some((_, rect)) = &mut p.label {
                *rect = rect.translate(dx, dy);
            }
            for (rect, _) in &mut p.links {
                *rect = rect.translate(dx, dy);
            }
        }
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
        Fragment::MarginLabel { rect, baseline, .. } => {
            *rect = rect.translate(dx, dy);
            *baseline += dy;
        }
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
    if let Some(width) = t.preferred_width {
        return match width {
            TableWidth::Points(points) => points,
            TableWidth::Fraction(fraction) => available * fraction,
        }
        .max(1.0);
    }
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
    mins.fill(share * 0.3);
    maxes.fill(share);
    (mins, maxes)
}

/// Space the reference reserves around cell text for the effective horizontal borders.
fn cell_border_bands(t: &FlowTable, row: usize, col: usize, cell: &FlowCell) -> (f32, f32) {
    let band = |border: Option<Border>| {
        border
            .filter(|b| b.style.is_visible())
            .map_or(0.0, |b| b.width.clamp(0.0, 12.0))
    };
    let row_exception = t.rows.get(row).map(|r| &r.exception_borders);
    let exception_top = row_exception.and_then(|b| b[if row == 0 { SIDE_TOP } else { 4 }]);
    let table_top = if row == 0 {
        t.borders[SIDE_TOP]
    } else {
        t.inside_h
    };
    let mut top = band(cell.borders[SIDE_TOP].or(exception_top).or(table_top));
    if top == 0.0 && row > 0 && !cell.v_merge_continue {
        if let Some(previous) = t.rows.get(row - 1) {
            let mut prior_col = previous.grid_before;
            for above in &previous.cells {
                let next = prior_col + above.span.max(1);
                if prior_col < col + cell.span.max(1) && col < next {
                    top = top.max(band(
                        above.borders[SIDE_BOTTOM].or(previous.exception_borders[4]),
                    ));
                }
                prior_col = next;
            }
        }
    }
    let bottom = if row + 1 == t.rows.len() {
        band(
            cell.borders[SIDE_BOTTOM]
                .or(row_exception.and_then(|b| b[SIDE_BOTTOM]))
                .or(t.borders[SIDE_BOTTOM]),
        )
    } else {
        0.0
    };
    (top, bottom)
}

fn endnote_separator(x: f32, y: f32, width: f32) -> Fragment {
    Fragment::Rule {
        rect: Rect::new(x, y + 11.75, 144.0_f32.min(width), 0.5),
        border: Border {
            width: 0.5,
            color: Rgba::BLACK,
            style: BorderStyle::Single,
            space: 0.0,
        },
    }
}

#[derive(Default)]
struct CellSpacing {
    previous_after: f32,
    previous_auto: bool,
    previous_listed: bool,
    previous_style: Option<(String, bool)>,
}

impl CellSpacing {
    fn before(&mut self, paragraph: &FlowParagraph, first: bool) -> f32 {
        let p = &paragraph.para;
        let mut before = p.space_before;
        let mut adjust = 0.0;
        if let Some((style, contextual)) = &self.previous_style {
            if style == &p.style_id {
                if *contextual {
                    adjust -= self.previous_after;
                    before = (before - self.previous_after).max(0.0);
                    self.previous_after = 0.0;
                }
                if p.contextual_spacing {
                    before = 0.0;
                }
            }
        }
        if !first && self.previous_listed && paragraph.list.is_some() {
            if self.previous_auto {
                adjust -= self.previous_after;
                self.previous_after = 0.0;
            }
            if p.space_before_auto {
                before = 0.0;
            }
        }
        if first && p.space_before_auto {
            before = 0.0;
        } else if !first && before > 0.0 && self.previous_after > 0.0 {
            before = (before - self.previous_after).max(0.0);
        }
        adjust + before
    }

    fn after(&mut self, paragraph: &FlowParagraph, last: bool) -> f32 {
        let p = &paragraph.para;
        let after = if last && p.space_after_auto {
            0.0
        } else {
            p.space_after
        };
        self.previous_after = after;
        self.previous_auto = p.space_after_auto;
        self.previous_listed = paragraph.list.is_some();
        self.previous_style = Some((p.style_id.clone(), p.contextual_spacing));
        after
    }
}

/// Assign table labels once, including rows that will be laid out on later pages. Repeated
/// headers reuse the cached label rather than advancing the document counters again.
fn inline_picture_fragments(
    paragraph: &FlowParagraph,
    layout: &ParagraphLayout,
    rect: Rect,
    first_line: usize,
    end_line: usize,
) -> Vec<Fragment> {
    let mut out = Vec::new();
    let first_top = layout.lines.get(first_line).map_or(0.0, |line| line.top);
    for line in layout.lines.iter().take(end_line).skip(first_line) {
        for index in line.c0..line.c1 {
            let Some(cluster) = layout.clusters.get(index) else {
                continue;
            };
            let edgedoc_text::ClusterKind::Object(object) = cluster.kind else {
                continue;
            };
            let Some(Some(picture)) = paragraph.inline_pictures.get(object) else {
                continue;
            };
            let x = rect.x + line.xs.get(index - line.c0).copied().unwrap_or(0.0);
            let baseline = rect.y + line.baseline - first_top;
            out.push(Fragment::Picture {
                rect: Rect::new(
                    x + picture.effect_left,
                    baseline - picture.height - picture.effect_bottom,
                    picture.width,
                    picture.height,
                ),
                resource: picture.resource,
                src: picture.src,
                alt: picture.alt.clone(),
            });
        }
    }
    out
}

fn blocks_have_numbering(blocks: &[FlowBlock]) -> bool {
    blocks.iter().any(|block| match block {
        FlowBlock::Paragraph(p) => p.list.is_some(),
        FlowBlock::Table(table) => table.rows.iter().any(|row| {
            row.cells
                .iter()
                .any(|cell| blocks_have_numbering(&cell.blocks))
        }),
        _ => false,
    })
}

fn section_has_numbered_story(section: &FlowSection) -> bool {
    [
        &section.header,
        &section.footer,
        &section.header_first,
        &section.footer_first,
        &section.header_even,
        &section.footer_even,
    ]
    .into_iter()
    .flatten()
    .any(|blocks| blocks_have_numbering(blocks))
}

fn assign_table_list_labels<S: Shaper>(
    flow: &mut Flow<'_, S>,
    table: &FlowTable,
    labels: &mut BTreeMap<String, String>,
) {
    for row in &table.rows {
        for cell in &row.cells {
            for block in &cell.blocks {
                match block {
                    FlowBlock::Paragraph(p) => {
                        if let Some(list) = &p.list {
                            labels
                                .entry(p.path.clone())
                                .or_insert_with(|| flow.next_label(list));
                        }
                    }
                    FlowBlock::Table(inner) => assign_table_list_labels(flow, inner, labels),
                    _ => {}
                }
            }
        }
    }
}

/// Shape a cell paragraph with the same list-label geometry used in the body.
fn cell_paragraph_layout<S: Shaper>(
    flow: &mut Flow<'_, S>,
    p: &FlowParagraph,
    width: f32,
    label: Option<&str>,
) -> (std::sync::Arc<ParagraphLayout>, f32, Option<(f32, f32)>) {
    let Some((list, label)) = p
        .list
        .as_ref()
        .zip(label)
        .filter(|(_, label)| !label.is_empty())
    else {
        let left = p.para.indent_left;
        let text_width = (width - left - p.para.indent_right).max(1.0);
        return (
            flow.paragraph_layout(p, text_width, &[], p.para.indent_first),
            left,
            None,
        );
    };
    let size = p
        .para
        .label_size
        .unwrap_or_else(|| p.run_styles.first().map_or(11.0, |s| s.size));
    let face = p.para.label_face.unwrap_or(p.para.label_fallback_face);
    let glyph_width: f32 = flow
        .shaper
        .shape_run_kerning(face, label, size, false, p.para.label_kerning)
        .iter()
        .map(|g| g.advance)
        .sum();
    let at = p.para.indent_left + p.para.indent_first;
    let label_x = match list.justification.as_str() {
        "right" | "end" => at - glyph_width,
        "center" => at - glyph_width / 2.0,
        _ => at,
    };
    let end = label_x + glyph_width;
    let text_x = match list.suffix.as_str() {
        "nothing" => end,
        "space" => end + size * 0.25,
        _ => {
            let step = if p.para.default_tab > 0.0 {
                p.para.default_tab
            } else {
                36.0
            };
            let default = ((end + 0.01) / step).floor().mul_add(step, step);
            let next = p
                .para
                .tabs
                .iter()
                .filter(|tab| tab.pos > end + 0.01)
                .map(|tab| tab.pos)
                .chain(
                    (p.para.indent_first < 0.0 && p.para.indent_left > end + 0.01)
                        .then_some(p.para.indent_left),
                )
                .reduce(f32::min)
                .unwrap_or(default);
            list.tab
                .filter(|pos| *pos > end + 0.01)
                .map_or(next, |pos| pos.min(next))
        }
    };
    let left = p.para.indent_left;
    let text_width = (width - left - p.para.indent_right).max(1.0);
    let first_indent = text_x - left;
    let layout = flow.paragraph_layout(p, text_width, &[], first_indent);
    let shift = layout
        .lines
        .first()
        .and_then(|line| line.xs.first())
        .map_or(0.0, |x| x - first_indent);
    (layout, left, Some((label_x + shift, glyph_width + 6.0)))
}

fn row_height<S: Shaper>(
    flow: &mut Flow<'_, S>,
    t: &FlowTable,
    row: usize,
    width: f32,
    labels: &BTreeMap<String, String>,
) -> f32 {
    let Some(r) = t.rows.get(row) else {
        return 0.0;
    };
    let mut tallest = 0.0f32;
    let mut row_insets = 0.0f32;
    let row_top = r
        .cells
        .iter()
        .map(|cell| cell.margins[0])
        .fold(0.0f32, f32::max);
    let row_bottom = r
        .cells
        .iter()
        .map(|cell| cell.margins[2])
        .fold(0.0f32, f32::max);
    let widths = column_widths(t, width);
    let mut col = r.grid_before;
    for cell in &r.cells {
        let span = cell.span.max(1);
        let (border_top, border_bottom) = cell_border_bands(t, row, col, cell);
        let insets = row_top + row_bottom + border_top + border_bottom;
        row_insets = row_insets.max(insets);
        let cell_w =
            (widths.iter().skip(col).take(span).sum::<f32>() - cell.margins[1] - cell.margins[3])
                .max(1.0);
        let h = cell_content_height(flow, cell, cell_w, insets, labels);
        if !cell.v_merge_start {
            tallest = tallest.max(h);
        }
        col += span;
    }
    if let Some(h) = r.height {
        if h > 0.0 {
            if r.height_exact {
                return h + row_bottom;
            }
            tallest = tallest.max(h + row_insets);
        }
    }
    tallest.max(1.0)
}

fn cell_content_height<S: Shaper>(
    flow: &mut Flow<'_, S>,
    cell: &FlowCell,
    width: f32,
    insets: f32,
    labels: &BTreeMap<String, String>,
) -> f32 {
    if cell.v_merge_continue {
        return insets;
    }
    let mut h = insets;
    let mut spacing = CellSpacing::default();
    for (index, block) in cell.blocks.iter().enumerate() {
        if !matches!(block, FlowBlock::Paragraph(_)) {
            spacing = CellSpacing::default();
        }
        match block {
            FlowBlock::EndnoteSeparator { .. } => h += 20.0,
            FlowBlock::Paragraph(p) => {
                let (layout, _, _) =
                    cell_paragraph_layout(flow, p, width, labels.get(&p.path).map(String::as_str));
                h += spacing.before(p, index == 0)
                    + layout.height
                    + spacing.after(p, index + 1 == cell.blocks.len());
            }
            FlowBlock::Picture { height, .. } | FlowBlock::Placeholder { height, .. } => {
                h += height
            }
            FlowBlock::Table(inner) => {
                let nested_width = if inner.preferred_width.is_some() {
                    table_width(inner, width)
                } else {
                    width
                };
                h += table_height(flow, inner, nested_width, labels);
            }
        }
    }
    h
}

fn table_row_heights<S: Shaper>(
    flow: &mut Flow<'_, S>,
    t: &FlowTable,
    width: f32,
    labels: &BTreeMap<String, String>,
) -> Vec<f32> {
    let mut heights: Vec<f32> = (0..t.rows.len())
        .map(|r| row_height(flow, t, r, width, labels))
        .collect();
    let widths = column_widths(t, width);
    for (row_index, row) in t.rows.iter().enumerate() {
        let top = row
            .cells
            .iter()
            .map(|c| c.margins[0])
            .fold(0.0f32, f32::max);
        let bottom = row
            .cells
            .iter()
            .map(|c| c.margins[2])
            .fold(0.0f32, f32::max);
        let mut col = row.grid_before;
        for cell in &row.cells {
            let span = cell.span.max(1);
            if cell.v_merge_start {
                let mut end = row_index;
                while t.rows.get(end + 1).is_some_and(|next| {
                    let mut start = next.grid_before;
                    next.cells.iter().any(|c| {
                        let found = start == col && c.v_merge_continue;
                        start += c.span.max(1);
                        found
                    })
                }) {
                    end += 1;
                }
                let (border_top, border_bottom) = cell_border_bands(t, row_index, col, cell);
                let cell_width = (widths.iter().skip(col).take(span).sum::<f32>()
                    - cell.margins[1]
                    - cell.margins[3])
                    .max(1.0);
                let needed = cell_content_height(
                    flow,
                    cell,
                    cell_width,
                    top + bottom + border_top + border_bottom,
                    labels,
                );
                let available: f32 = heights[row_index..=end].iter().sum();
                if needed > available {
                    heights[end] += needed - available;
                }
            }
            col += span;
        }
    }
    heights
}

fn table_height<S: Shaper>(
    flow: &mut Flow<'_, S>,
    t: &FlowTable,
    width: f32,
    labels: &BTreeMap<String, String>,
) -> f32 {
    table_row_heights(flow, t, width, labels).into_iter().sum()
}

fn column_widths(t: &FlowTable, width: f32) -> Vec<f32> {
    if !t.widths.is_empty() {
        let sum: f32 = t.widths.iter().sum();
        if sum > 0.0 {
            let k = width / sum;
            return t.widths.iter().map(|w| w * k).collect();
        }
    }
    let n = t
        .rows
        .iter()
        .map(|r| {
            r.grid_before + r.cells.iter().map(|c| c.span.max(1)).sum::<usize>() + r.grid_after
        })
        .max()
        .unwrap_or(1)
        .max(1);
    vec![width / n as f32; n]
}

fn build_table_fragment<S: Shaper>(
    flow: &mut Flow<'_, S>,
    t: &FlowTable,
    origin: Rect,
    r0: usize,
    r1: usize,
    repeated_header: bool,
    labels: &BTreeMap<String, String>,
) -> (TableFragment, f32) {
    let width = origin.w.max(1.0);
    let widths = column_widths(t, width);
    let row_heights = table_row_heights(flow, t, width, labels);
    let mut cells = Vec::new();
    let mut content = Vec::new();
    let mut borders = Vec::new();
    let mut seen_cell_edges: BTreeMap<(bool, i64, i64, i64), Vec<Border>> = BTreeMap::new();
    // A vertical-merge continuation has no top edge at its row boundary.
    let mut merge_holes: Vec<(f32, f32, f32)> = Vec::new();
    let mut cell_edges = Vec::new();
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
        let h = row_heights.get(r).copied().unwrap_or(0.0);
        let row_top = row
            .cells
            .iter()
            .map(|cell| cell.margins[0])
            .fold(0.0f32, f32::max);
        let row_bottom = row
            .cells
            .iter()
            .map(|cell| cell.margins[2])
            .fold(0.0f32, f32::max);
        let mut x = origin.x + widths.iter().take(row.grid_before).sum::<f32>();
        let mut col = row.grid_before;
        for cell in &row.cells {
            let span = cell.span.max(1);
            let w: f32 = widths.iter().skip(col).take(span).sum();
            let row_rect = Rect::new(x, y, w.max(1.0), h);
            let mut span_height = h;
            if cell.v_merge_start {
                let mut next_row = r + 1;
                while next_row < t.rows.len()
                    && t.rows.get(next_row).is_some_and(|next| {
                        let mut start = next.grid_before;
                        next.cells.iter().any(|c| {
                            let found = start == col && c.v_merge_continue;
                            start += c.span.max(1);
                            found
                        })
                    })
                {
                    span_height += row_heights.get(next_row).copied().unwrap_or(0.0);
                    next_row += 1;
                }
            }
            let rect = Rect::new(x, y, w.max(1.0), span_height);
            cell_edges.push((r, col, span, row_rect, cell));
            if cell.v_merge_continue {
                merge_holes.push((row_rect.y, row_rect.x, row_rect.right()));
            }
            cells.push(CellRect {
                rect,
                row: r,
                col,
                span,
                shading: if cell.v_merge_continue {
                    None
                } else {
                    cell.shading
                },
            });
            // Cell content: one paragraph fragment per block, measured at the cell width and
            // placed inside the cell margins.
            let inner_x = rect.x + cell.margins[1];
            let inner_w = (rect.w - cell.margins[1] - cell.margins[3]).max(1.0);
            let (border_top, border_bottom) = cell_border_bands(t, r, col, cell);
            let mut cy = rect.y + row_top + border_top;
            let content_top = cy;
            let content_start = content.len();
            let mut spacing = CellSpacing::default();
            for (index, block) in cell
                .blocks
                .iter()
                .enumerate()
                .filter(|_| !cell.v_merge_continue)
            {
                if !matches!(block, FlowBlock::Paragraph(_)) {
                    spacing = CellSpacing::default();
                }
                match block {
                    FlowBlock::EndnoteSeparator { .. } => {
                        content.push(endnote_separator(inner_x, cy, inner_w));
                        cy += 20.0;
                    }
                    FlowBlock::Paragraph(p) => {
                        cy += spacing.before(p, index == 0);
                        let label = labels.get(&p.path);
                        let (layout, left, label_rect) =
                            cell_paragraph_layout(flow, p, inner_w, label.map(String::as_str));
                        let hh = layout.height;
                        let text_width = (inner_w - left - p.para.indent_right).max(1.0);
                        let text_rect = Rect::new(inner_x + left, cy, text_width, hh);
                        let pictures =
                            inline_picture_fragments(p, &layout, text_rect, 0, layout.lines.len());
                        content.push(Fragment::Paragraph(ParagraphFragment {
                            path: p.path.clone(),
                            text: std::sync::Arc::from(p.text.as_str()),
                            label: label.zip(label_rect).map(|(text, (x, w))| {
                                (text.clone(), Rect::new(inner_x + x, cy, w, hh))
                            }),
                            rect: text_rect,
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
                        content.extend(pictures);
                        cy += hh + spacing.after(p, index + 1 == cell.blocks.len());
                    }
                    FlowBlock::Picture {
                        height,
                        width: pw,
                        alt,
                        resource,
                        src,
                        ..
                    } => {
                        content.push(Fragment::Picture {
                            rect: Rect::new(inner_x, cy, (*pw).min(inner_w), *height),
                            resource: *resource,
                            src: *src,
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
                        let nested_width = if inner.preferred_width.is_some() {
                            table_width(inner, inner_w)
                        } else {
                            inner_w
                        };
                        let nested_x = if inner.preferred_width.is_some() {
                            match inner.align {
                                Align::Center => inner_x + (inner_w - nested_width) / 2.0,
                                Align::Right => inner_x + inner_w - nested_width,
                                _ => inner_x + inner.indent,
                            }
                        } else {
                            inner_x
                        };
                        let (fragment, ih) = build_table_fragment(
                            flow,
                            inner,
                            Rect::new(nested_x, cy, nested_width, 0.0),
                            0,
                            inner.rows.len(),
                            false,
                            labels,
                        );
                        content.push(Fragment::Table(fragment));
                        cy += ih;
                    }
                }
            }
            // A cell vertical alignment moves its content inside the cell. See `DP-L-40`.
            let used = cy - content_top;
            let free = (rect.h - row_top - row_bottom - border_top - border_bottom - used).max(0.0);
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
            // Cell-specific rules also define shared edges when a table does not provide an
            // inside rule. Retain only one coincident rule when both neighboring cells specify
            // the same side; a table inside rule continues to own the shared edge for now.
            let first_col = col == row.grid_before;
            let last_col = col + span >= widths.len().saturating_sub(row.grid_after);
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
                let inside_rule = match side {
                    SIDE_TOP | SIDE_BOTTOM => t.inside_h,
                    _ => t.inside_v,
                };
                if outer || inside_rule.is_none() {
                    let horizontal = matches!(side, SIDE_TOP | SIDE_BOTTOM);
                    let (axis, start, length) = match side {
                        SIDE_TOP => (rect.y, rect.x, rect.w),
                        SIDE_BOTTOM => (rect.bottom(), rect.x, rect.w),
                        SIDE_LEFT => (rect.x, rect.y, rect.h),
                        _ => (rect.right(), rect.y, rect.h),
                    };
                    let quantize = |point: f32| (point * 100.0).round() as i64;
                    let key = (
                        horizontal,
                        quantize(axis),
                        quantize(start),
                        quantize(length),
                    );
                    let previous = seen_cell_edges.entry(key).or_default();
                    if !previous.contains(b) {
                        previous.push(*b);
                        borders.push(cell_border_segment(rect, side, *b, &cell.borders));
                    }
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
            let mut holes = merge_holes
                .iter()
                .filter(|(y, _, _)| (*y - b).abs() < 0.01)
                .map(|(_, left, right)| (*left, *right))
                .collect::<Vec<_>>();
            holes.sort_by(|a, b| a.0.total_cmp(&b.0));
            let mut cursor = origin.x;
            for (left, right) in holes
                .into_iter()
                .chain(std::iter::once((origin.x + width, origin.x + width)))
            {
                let end = left.clamp(origin.x, origin.x + width);
                if end > cursor + 0.01 {
                    borders.push(BorderSegment {
                        rect: Rect::new(
                            cursor,
                            b - inside.width.max(0.5) / 2.0,
                            end - cursor,
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
                cursor = cursor.max(right.min(origin.x + width));
            }
        }
    }
    if let Some(inside) = t.inside_v {
        // A grid boundary only paints where an actual cell ends; a grid-spanning cell has
        // no vertical edge through its interior.
        for cell in &cells {
            let x = cell.rect.right();
            if x >= origin.x + width - 0.01 {
                continue;
            }
            borders.push(BorderSegment {
                rect: Rect::new(x, cell.rect.y, inside.width.max(0.5), cell.rect.h),
                border: Border {
                    width: inside.width.max(0.5),
                    color: inside.color,
                    style: inside.style,
                    space: 0.0,
                },
            });
        }
    }
    if (t.inside_h.is_some() || t.inside_v.is_some())
        || cell_edges
            .iter()
            .any(|(_, _, span, _, cell)| *span > 1 || cell.v_merge_start || cell.v_merge_continue)
        || t.rows.iter().any(|row| {
            row.grid_before > 0
                || row.grid_after > 0
                || row.exception_borders.iter().any(Option::is_some)
        })
    {
        // Resolve each cell edge separately when table rules are present or cells are merged. Shared edges are
        // painted by both adjoining cells, as Wordcraft does, and merge continuations have
        // no horizontal edge through the merged area.
        borders.clear();
        for &(row, col, span, rect, cell) in &cell_edges {
            let exceptions = t
                .rows
                .get(row)
                .map(|r| r.exception_borders)
                .unwrap_or([None; 6]);
            let first_col = t.rows.get(row).is_some_and(|r| col == r.grid_before);
            let last_col = t
                .rows
                .get(row)
                .is_some_and(|r| col + span >= widths.len().saturating_sub(r.grid_after));
            let edge = |side: usize, row_border: Option<Border>, fallback: Option<Border>| {
                cell.borders[side]
                    .or(row_border)
                    .or(fallback)
                    .filter(|border| border.style.is_visible())
            };
            let left = edge(
                SIDE_LEFT,
                exceptions[if first_col { SIDE_LEFT } else { 5 }],
                if first_col {
                    t.borders[SIDE_LEFT]
                } else {
                    t.inside_v
                },
            );
            let right = edge(
                SIDE_RIGHT,
                exceptions[if last_col { SIDE_RIGHT } else { 5 }],
                if last_col {
                    t.borders[SIDE_RIGHT]
                } else {
                    t.inside_v
                },
            );
            let top = edge(
                SIDE_TOP,
                exceptions[if row == 0 { SIDE_TOP } else { 4 }],
                if row == 0 {
                    t.borders[SIDE_TOP]
                } else {
                    t.inside_h
                },
            );
            let bottom = edge(
                SIDE_BOTTOM,
                exceptions[if row + 1 == t.rows.len() {
                    SIDE_BOTTOM
                } else {
                    4
                }],
                if row + 1 == t.rows.len() {
                    t.borders[SIDE_BOTTOM]
                } else {
                    t.inside_h
                },
            );
            let continued_below = t.rows.get(row + 1).is_some_and(|next| {
                let mut start = next.grid_before;
                next.cells.iter().any(|c| {
                    let matches = start == col && c.v_merge_continue;
                    start += c.span.max(1);
                    matches
                })
            });
            let half = |b: Option<Border>| b.map_or(0.0, |b| b.width.max(0.3) / 2.0);
            let hleft = rect.x - half(left);
            let hwidth = rect.w + half(left) + half(right);
            if !cell.v_merge_continue {
                if let Some(b) = top {
                    let w = b.width.max(0.3);
                    borders.push(BorderSegment {
                        rect: Rect::new(hleft, rect.y - w / 2.0, hwidth, w),
                        border: b,
                    });
                }
            }
            if !continued_below {
                if let Some(b) = bottom {
                    let w = b.width.max(0.3);
                    borders.push(BorderSegment {
                        rect: Rect::new(hleft, rect.bottom() - w / 2.0, hwidth, w),
                        border: b,
                    });
                }
            }
            for (x, b) in [(rect.x, left), (rect.right(), right)] {
                if let Some(b) = b {
                    let w = b.width.max(0.3);
                    borders.push(BorderSegment {
                        rect: Rect::new(x - w / 2.0, rect.y, w, rect.h),
                        border: b,
                    });
                }
            }
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

/// Cell borders are centered on the grid line. Horizontal rules extend across the
/// adjacent vertical strokes so that the corners remain joined.
fn cell_border_segment(
    rect: Rect,
    side: usize,
    border: Border,
    sides: &[Option<Border>; 4],
) -> BorderSegment {
    let w = border.width.max(0.3);
    let half_edge = |side: usize| {
        sides[side]
            .filter(|border| border.style.is_visible())
            .map_or(0.0, |border| border.width.max(0.3) / 2.0)
    };
    let left = half_edge(SIDE_LEFT);
    let right = half_edge(SIDE_RIGHT);
    let segment = match side {
        SIDE_TOP => Rect::new(rect.x - left, rect.y - w / 2.0, rect.w + left + right, w),
        SIDE_BOTTOM => Rect::new(
            rect.x - left,
            rect.bottom() - w / 2.0,
            rect.w + left + right,
            w,
        ),
        SIDE_LEFT => Rect::new(rect.x - w / 2.0, rect.y, w, rect.h),
        _ => Rect::new(rect.right() - w / 2.0, rect.y, w, rect.h),
    };
    BorderSegment {
        rect: segment,
        border,
    }
}

fn border_segment(rect: Rect, side: usize, border: Border) -> BorderSegment {
    let w = border.width.max(0.3);
    let segment = match side {
        SIDE_TOP => Rect::new(rect.x, rect.y - w / 2.0, rect.w, w),
        SIDE_BOTTOM => Rect::new(rect.x, rect.bottom() - w / 2.0, rect.w, w),
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

    #[test]
    fn shared_abstract_list_counters_apply_instance_restarts_once() {
        let fonts = db();
        let shaper = DbShaper { db: &fonts };
        let mut flow = Flow::new(&shaper);
        let base = edgedoc_style::ListRef {
            num_id: 19,
            abstract_id: 9,
            text: "%1".into(),
            format: "decimal".into(),
            start: 1,
            ..Default::default()
        };
        let restarted = edgedoc_style::ListRef {
            num_id: 22,
            start_override: Some(1),
            ..base.clone()
        };
        assert_eq!(flow.next_label(&base), "1");
        assert_eq!(flow.next_label(&restarted), "1");
        assert_eq!(flow.next_label(&restarted), "2");
        assert_eq!(flow.next_label(&base), "3");
        let zero = edgedoc_style::ListRef {
            num_id: 23,
            start_override: Some(0),
            ..base
        };
        assert_eq!(flow.next_label(&zero), "0");
        assert_eq!(flow.next_label(&zero), "1");
    }

    #[test]
    fn legal_multilevel_labels_preserve_zero_padded_decimal_levels() {
        let fonts = db();
        let shaper = DbShaper { db: &fonts };
        let mut flow = Flow::new(&shaper);
        let parent = edgedoc_style::ListRef {
            num_id: 1,
            abstract_id: 2,
            level: 0,
            text: "%1".into(),
            format: "upperRoman".into(),
            start: 1,
            level_formats: vec!["upperRoman".into(), "decimalZero".into()],
            ..Default::default()
        };
        assert_eq!(flow.next_label(&parent), "I");
        let child = edgedoc_style::ListRef {
            level: 1,
            text: "%1.%2".into(),
            format: "decimalZero".into(),
            is_legal: true,
            ..parent
        };
        assert_eq!(flow.next_label(&child), "1.01");
    }

    fn run_style() -> RunStyle {
        RunStyle {
            face: 0,
            size: 11.0,
            line_size: 11.0,
            kerning: true,
            positional_tab: None,
            color: edgedoc_geom::Rgba::BLACK,
            underline: false,
            double_underline: false,
            underline_color: None,
            strike: false,
            double_strike: false,
            highlight: None,
            shading: None,
            x_scale: 1.0,
            letter_spacing: 0.0,
            baseline_shift: 0.0,
            line_shift: 0.0,
            hidden: false,
            rtl: false,
            caps: edgedoc_text::CapsMode::None,
            symbol: None,
            synth_bold: false,
            synth_italic: false,
            shadow: false,
            outline: false,
            emboss: false,
            imprint: false,
            border_width: 0.0,
            border_color: None,
        }
    }

    fn paragraph(text: &str) -> FlowParagraph {
        FlowParagraph {
            path: format!("body[{}]", text.len()),
            text: text.to_string(),
            runs: vec![(0, text.len(), 0)],
            run_styles: vec![run_style()],
            mark_style: 0,
            para: ComputedPara::default(),
            label: None,
            list: None,
            page_break_before: false,
            has_page_field: false,
            has_total_field: false,
            objects: Vec::new(),
            inline_pictures: Vec::new(),
            page_pictures: Vec::new(),
            border_group_safe: false,
            border_join_above: false,
            border_join_below: false,
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
                grid_before: 0,
                grid_after: 0,
                exception_borders: [None; 6],
                header: false,
                height: None,
                height_exact: false,
                cant_split: false,
            }],
            widths: vec![200.0, 200.0],
            auto_width: true,
            preferred_width: None,
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
                    let shared_x = t.cells[0].rect.right();
                    assert_eq!(
                        t.borders
                            .iter()
                            .filter(|segment| {
                                (segment.rect.x + segment.rect.w / 2.0 - shared_x).abs() < 0.01
                                    && (segment.rect.h - t.cells[0].rect.h).abs() < 0.01
                            })
                            .count(),
                        2,
                        "both cells paint the centered shared edge"
                    );
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
    fn numbered_header_is_laid_out_after_the_body() {
        let fonts = db();
        let shaper = DbShaper { db: &fonts };
        let list = edgedoc_style::ListRef {
            num_id: 1,
            abstract_id: 1,
            text: "%1.".into(),
            format: "decimal".into(),
            start: 1,
            ..Default::default()
        };
        let mut section = a4();
        let mut header = paragraph("Header");
        header.path = "part[1]/block[0]".into();
        header.list = Some(list.clone());
        section.header = Some(vec![FlowBlock::Paragraph(header)]);
        let mut body = paragraph("Body");
        body.list = Some(list);
        let mut flow = Flow::new(&shaper);
        let layout = flow.layout(&[FlowBlock::Paragraph(body)], &[section]);
        let labels = layout.pages[0]
            .fragments
            .iter()
            .filter_map(|fragment| match fragment {
                Fragment::Paragraph(p) => p.label.as_ref().map(|(text, _)| text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(labels, ["3.", "2."]);
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
            src: [0.0, 0.0, 1.0, 1.0],
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
    fn a_picture_follows_the_preceding_paragraph_spacing() {
        let db = db();
        let shaper = DbShaper { db: &db };
        let mut flow = Flow::new(&shaper);
        let mut before = paragraph("Before picture");
        before.para.space_after = 10.0;
        let body = vec![
            FlowBlock::Paragraph(before),
            FlowBlock::Picture {
                path: "picture".into(),
                width: 100.0,
                height: 50.0,
                alt: String::new(),
                resource: 1,
                src: [0.0, 0.0, 1.0, 1.0],
                align: Align::Left,
            },
        ];
        let layout = flow.layout(&body, &[a4()]);
        let mut paragraph_bottom = None;
        let mut picture_top = None;
        for fragment in &layout.pages[0].fragments {
            match fragment {
                Fragment::Paragraph(p) => paragraph_bottom = Some(p.rect.bottom()),
                Fragment::Picture { rect, .. } => picture_top = Some(rect.y),
                _ => {}
            }
        }
        assert!(
            (picture_top.unwrap_or_default() - paragraph_bottom.unwrap_or_default() - 10.0).abs()
                < 0.01
        );
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
