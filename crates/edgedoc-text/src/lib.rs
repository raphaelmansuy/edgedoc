//! Line breaking, bidi, justification, and tabs.
//!
//! A paragraph becomes a list of [`Cluster`] values, one per grapheme, then a list of [`Line`]
//! values. The model follows WordCraft's measured behavior: a cluster owns its glyph range, so a
//! ligature is one cluster of several glyphs, and the breaker is first-fit, not a global optimum.
//! See `DP-F-08` to `DP-F-52`.

#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![deny(clippy::unimplemented, clippy::todo, clippy::unreachable)]

use edgedoc_fonts::Shaper;
use edgedoc_geom::Rect;
use unicode_bidi::BidiInfo;
use unicode_segmentation::UnicodeSegmentation;

/// Narrowest width the breaker accepts, in points. A zero-width column makes progress instead of
/// looping. See `DP-F-51`.
pub const MIN_LINE_WIDTH: f32 = 1.0;

/// What a cluster draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClusterKind {
    Text,
    Space,
    Tab,
    /// A line break inside a paragraph.
    LineBreak,
    /// A page break inside a paragraph.
    PageBreak,
    /// A column break.
    ColumnBreak,
    /// A soft hyphen, which paints only at a break.
    SoftHyphen,
    /// An inline object: an index into the paragraph object list.
    Object(usize),
    /// A zero-width marker: a bookmark, a note reference, hidden text.
    Marker,
}

/// One drawing segment of a grapheme. A font or style change can split the drawing, but the
/// edit and wrap boundary stays at the cluster.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Glyph {
    pub gid: u16,
    /// Offset from the cluster x, in points.
    pub dx: f32,
    pub dy: f32,
}

/// One grapheme, with its glyphs and its advance.
#[derive(Debug, Clone, PartialEq)]
pub struct Cluster {
    /// Byte offset in the paragraph text.
    pub start: usize,
    pub end: usize,
    /// Advance in points, with character scaling and letter spacing applied.
    pub adv: f32,
    pub kind: ClusterKind,
    /// Index into the style list.
    pub style: u32,
    pub g0: u32,
    pub g1: u32,
    /// True when a line may break after this cluster.
    pub break_after: bool,
    /// Height above the baseline for an inline object.
    pub obj_h: f32,
    /// Depth below the baseline for an inline object.
    pub obj_d: f32,
    /// True when the cluster is a decimal separator, which a decimal tab aligns on.
    pub dot: bool,
    /// Bidi embedding level, from UAX #9.
    pub level: u8,
}

impl Cluster {
    /// True when the cluster takes no room on a line: a marker, a break, or a soft hyphen.
    pub fn is_zero_width(&self) -> bool {
        matches!(
            self.kind,
            ClusterKind::Marker
                | ClusterKind::LineBreak
                | ClusterKind::PageBreak
                | ClusterKind::ColumnBreak
                | ClusterKind::SoftHyphen
        )
    }

    /// True when justification may stretch this cluster.
    pub fn is_stretchable(&self) -> bool {
        matches!(self.kind, ClusterKind::Space | ClusterKind::Tab)
    }
}

/// How a line ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineEnd {
    /// A wrap at the right edge, or the last line of the paragraph.
    #[default]
    Wrap,
    LineBreak,
    PageBreak,
    ColumnBreak,
    /// The end of the paragraph.
    Para,
}

/// Where text sits horizontally.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Align {
    #[default]
    Left,
    Center,
    Right,
    Justify,
    Distribute,
}

impl Align {
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "left" | "start" => Some(Align::Left),
            "center" => Some(Align::Center),
            "right" | "end" => Some(Align::Right),
            "both" | "justify" | "lowKashida" | "mediumKashida" | "highKashida" | "kashida" => {
                Some(Align::Justify)
            }
            "distribute" | "thaiDistribute" => Some(Align::Distribute),
            _ => None,
        }
    }
}

/// A tab stop alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TabAlign {
    #[default]
    Left,
    Center,
    Right,
    Decimal,
    Bar,
}

impl TabAlign {
    pub fn parse(text: &str) -> Self {
        match text {
            "center" => TabAlign::Center,
            "right" | "end" => TabAlign::Right,
            "decimal" => TabAlign::Decimal,
            "bar" => TabAlign::Bar,
            _ => TabAlign::Left,
        }
    }
}

/// A tab stop.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TabStop {
    pub pos: f32,
    pub align: TabAlign,
    /// The leader character, or `None`.
    pub leader: Option<char>,
}

/// One line of a paragraph.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Line {
    /// Top of the line box, relative to the paragraph top.
    pub top: f32,
    pub height: f32,
    /// Baseline, relative to the paragraph top.
    pub baseline: f32,
    /// The text's own ascent on this line, which is the caret extent.
    pub text_asc: f32,
    pub text_desc: f32,
    /// Cluster index range.
    pub c0: usize,
    pub c1: usize,
    /// The x of every cluster, plus the end x. Length is `c1 - c0 + 1`.
    pub xs: Vec<f32>,
    /// For a tab cluster: the leader to draw across it, with the character and its count.
    pub leaders: Vec<(usize, char, u32)>,
    pub end: LineEnd,
    /// Byte range in the paragraph text.
    pub start: usize,
    pub stop: usize,
    /// The left and right edges the line was laid out in, relative to the paragraph.
    pub left: f32,
    pub right: f32,
    /// The hyphen that paints at a soft-hyphen break: style and advance.
    pub hyphen: Option<(u32, f32)>,
    /// The line continues the previous line's row on the far side of a float.
    pub beside: bool,
}

impl Line {
    pub fn width(&self) -> f32 {
        self.xs.last().copied().unwrap_or(0.0) - self.xs.first().copied().unwrap_or(0.0)
    }

    /// The horizontal extent of the text, without the trailing gap.
    pub fn ink_width(&self) -> f32 {
        let last = self.xs.last().copied().unwrap_or(0.0);
        let first = self.xs.first().copied().unwrap_or(0.0);
        (last - self.left).max(last - first)
    }
}

/// One style of run, as the layout sees it.
#[derive(Debug, Clone, PartialEq)]
pub struct RunStyle {
    pub face: u32,
    pub size: f32,
    /// The text colour, resolved through the cascade. Word's default is black, not a theme
    /// colour, so a run with no `w:color` paints black.
    pub color: edgedoc_geom::Rgba,
    /// True when the run shows an underline.
    pub underline: bool,
    /// True when the run shows a single strikethrough.
    pub strike: bool,
    /// True when the run shows a double strikethrough.
    pub double_strike: bool,
    /// The highlight behind the run, when it has one.
    pub highlight: Option<edgedoc_geom::Rgba>,
    /// The character shading behind the run.
    pub shading: Option<edgedoc_geom::Rgba>,
    /// Horizontal scale from `w:w`, 1.0 = 100%.
    pub x_scale: f32,
    /// Letter spacing from `w:spacing`, in points.
    pub letter_spacing: f32,
    /// Baseline offset from `w:position`, in points.
    pub baseline_shift: f32,
    /// True when the run is hidden text.
    pub hidden: bool,
    /// True when the run reads right to left.
    pub rtl: bool,
    /// Text to draw instead of the source, as small caps or a symbol font gives.
    pub caps: CapsMode,
    /// The symbol font that the run names, when it names one. Its codes map to Unicode.
    pub symbol: Option<String>,
    /// True when the face has no bold sibling, so the painter emboldens the glyphs.
    pub synth_bold: bool,
    /// True when the face has no italic sibling, so the painter slants the glyphs.
    pub synth_italic: bool,
}

/// A plain 11 pt run, which a missing style falls back to.
fn default_style() -> RunStyle {
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
        caps: CapsMode::None,
        symbol: None,
        synth_bold: false,
        synth_italic: false,
    }
}

/// How a run renders its letters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CapsMode {
    #[default]
    None,
    All,
    Small,
}

/// What the breaker needs to break one paragraph.
#[derive(Debug, Clone)]
pub struct BreakInput<'a> {
    /// The paragraph text.
    pub text: &'a str,
    /// One entry per byte range of a run: `(start, end, style index)`.
    pub runs: &'a [(usize, usize, u32)],
    /// The style table.
    pub styles: &'a [RunStyle],
    /// Available width in points.
    pub width: f32,
    /// Left indent, in points.
    pub indent_left: f32,
    /// Right indent, in points, which reduces the width.
    pub indent_right: f32,
    /// First-line indent, in points. It applies to the first line only.
    pub indent_first: f32,
    /// Alignment.
    pub align: Align,
    /// True when the paragraph reads right to left.
    pub bidi: bool,
    /// Tab stops.
    pub tabs: &'a [TabStop],
    /// The default tab stop interval.
    pub default_tab: f32,
    /// Exclusions from floating objects, in paragraph coordinates.
    pub exclusions: &'a [Rect],
    /// True when a line may break between any two East Asian characters.
    pub word_wrap_off: bool,
    /// True when kinsoku applies: no line starts with a closing mark.
    pub kinsoku: bool,
    /// Inline object extents: `(ascent, descent)` in points.
    pub objects: &'a [(f32, f32)],
    /// Soft-hyphen break positions, as cluster byte offsets, already computed.
    pub hyphen_after: &'a [usize],
}

/// The result of a break.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ParagraphLayout {
    pub clusters: Vec<Cluster>,
    pub lines: Vec<Line>,
    /// Sum of the line heights, without paragraph spacing.
    pub height: f32,
    /// The widest line, which a table fit needs.
    pub max_width: f32,
}

/// True when a character is East Asian text, as UAX #11 and the kinsoku tables define it.
pub fn is_east_asian(c: char) -> bool {
    matches!(c as u32,
        0x2E80..=0x2FFF
        | 0x3001..=0x303F
        | 0x3040..=0x3098
        | 0x309B..=0x30FF
        | 0x3100..=0x312F
        | 0x3190..=0x31FF
        | 0x3400..=0x4DBF
        | 0x4E00..=0x9FFF
        | 0xF900..=0xFAFF
        | 0xFE30..=0xFE4F
        | 0xFF01..=0xFFEF
        | 0x20000..=0x3FFFF)
}

/// Characters a line may not start with. See `DP-F-23`.
const NO_START: &str = "、。，．・：；？！゛゜ヽヾゝゞ々〻〉》」』】〕〗〙〛〞〟）］｝｠｡｣､･ﾞﾟ％￠";

/// Characters a line may not end with.
const NO_END: &str = "〈《「『【〔〖〘〚〝（［｛｟｢＄￡￥";

/// The bidi level of every byte, plus the paragraph direction.
pub fn bidi_levels(text: &str) -> (Vec<u8>, bool) {
    if text.is_ascii() {
        return (vec![0; text.len()], false);
    }
    let info = BidiInfo::new(text, None);
    let mut levels = vec![0u8; text.len()];
    let mut rtl_paragraph = false;
    for (i, para) in info.paragraphs.iter().enumerate() {
        let levels_para = &info.levels[para.range.clone()];
        for (j, level) in levels_para.iter().enumerate() {
            if let Some(slot) = levels.get_mut(para.range.start + j) {
                *slot = level.number();
            }
        }
        if i == 0 {
            rtl_paragraph = para.level.is_rtl();
        }
    }
    (levels, rtl_paragraph)
}

/// Split a string into grapheme clusters with their byte ranges.
pub fn graphemes(text: &str) -> Vec<(usize, usize)> {
    text.grapheme_indices(true)
        .map(|(i, g)| (i, i + g.len()))
        .collect()
}

/// Apply the caps mode to a grapheme.
pub fn caps_text(grapheme: &str, mode: CapsMode) -> String {
    match mode {
        CapsMode::None => grapheme.to_string(),
        CapsMode::All => grapheme.to_uppercase(),
        CapsMode::Small => grapheme.to_string(),
    }
}

fn style_at(runs: &[(usize, usize, u32)], offset: usize) -> u32 {
    for (start, end, style) in runs {
        if offset >= *start && offset < *end {
            return *style;
        }
    }
    runs.last().map_or(0, |(_, _, s)| *s)
}

/// Shape a grapheme into glyphs at its style. See `DP-F-09` and `DP-F-10`.
fn shape_grapheme<S: Shaper>(shaper: &S, style: &RunStyle, grapheme: &str) -> (Vec<Glyph>, f32) {
    let text = caps_text(grapheme, style.caps);
    let shaped = shaper.shape_run(style.face, &text, style.size, style.rtl);
    let mut glyphs = Vec::with_capacity(shaped.len());
    let mut adv = 0.0f32;
    for g in &shaped {
        glyphs.push(Glyph {
            gid: g.gid,
            dx: g.dx,
            dy: g.dy + style.baseline_shift,
        });
        adv += shaper.advance(style.face, g.gid, style.size) + g.dx;
    }
    if glyphs.is_empty() {
        // A missing glyph still takes room, so the line does not collapse.
        adv = style.size * 0.5;
    }
    adv = adv * style.x_scale + style.letter_spacing;
    (glyphs, adv.max(0.0))
}

/// Build the cluster list for a paragraph.
pub fn build_clusters<S: Shaper>(shaper: &S, input: &BreakInput<'_>) -> Vec<Cluster> {
    let (levels, _) = bidi_levels(input.text);
    let mut clusters = Vec::new();
    let mut g0 = 0u32;
    let mut object_index = 0usize;
    for (start, end) in graphemes(input.text) {
        let slice = input.text.get(start..end).unwrap_or("");
        let style_index = style_at(input.runs, start);
        let style = input.styles.get(style_index as usize);
        let Some(style) = style else {
            continue;
        };
        let first = slice.chars().next();
        let kind = if slice.contains('\u{000C}') {
            ClusterKind::PageBreak
        } else if slice.contains('\n') || slice.contains('\u{000B}') {
            ClusterKind::LineBreak
        } else if slice.contains('\t') {
            ClusterKind::Tab
        } else if slice.contains('\u{00AD}') {
            ClusterKind::SoftHyphen
        } else if slice.contains('\u{FFFC}') {
            // An inline object takes the next extent from the list, in text order.
            let idx = object_index;
            object_index += 1;
            ClusterKind::Object(idx)
        } else if first.is_some_and(char::is_whitespace) && slice != "\u{00A0}" {
            ClusterKind::Space
        } else {
            ClusterKind::Text
        };
        // An inline object takes its own width and its own extent.
        let object_extent = match kind {
            ClusterKind::Object(i) => input.objects.get(i).copied().unwrap_or((12.0, 0.0)),
            _ => (0.0, 0.0),
        };
        // A hidden run and a bookmark take no room.
        let (glyphs, adv) = if style.hidden
            || slice.starts_with('\u{200B}')
            || matches!(
                kind,
                ClusterKind::PageBreak | ClusterKind::LineBreak | ClusterKind::SoftHyphen
            ) {
            // Hidden text, a marker, and a break take no room and draw nothing.
            (Vec::new(), 0.0)
        } else if matches!(kind, ClusterKind::Object(_)) {
            // The object paints itself; the cluster only takes its advance.
            (Vec::new(), object_extent.0)
        } else {
            shape_grapheme(shaper, style, slice)
        };
        let glyph_count = glyphs.len() as u32;
        let cluster = Cluster {
            start,
            end,
            adv,
            kind: if style.hidden {
                ClusterKind::Marker
            } else {
                kind
            },
            style: style_index,
            g0,
            g1: g0 + glyph_count,
            break_after: false,
            obj_h: object_extent.0,
            obj_d: object_extent.1,
            dot: matches!(first, Some('.')) || matches!(first, Some(',')),
            level: levels.get(start).copied().unwrap_or(0),
        };
        g0 += glyph_count;
        clusters.push(cluster);
    }
    // Break opportunities from UAX #14, then the Word exceptions.
    let mut break_points: Vec<usize> = unicode_linebreak::linebreaks(input.text)
        .map(|(i, _)| i)
        .filter(|i| *i > 0)
        .collect();
    break_points.sort_unstable();
    break_points.dedup();
    // A break opportunity at byte `i` means the break falls *between* the cluster that ends at
    // `i` and the next one. So a cluster may break after it when a point equals its end.
    let break_at: std::collections::BTreeSet<usize> = break_points.iter().copied().collect();
    let count = clusters.len();
    for index in 0..count {
        let Some(cluster) = clusters.get(index).cloned() else {
            continue;
        };
        let mut breakable = break_at.contains(&cluster.end);
        // Word's exceptions: no break before a closing East Asian mark, and no break after an
        // opening one. See DP-F-23 and DP-F-51.
        if breakable && input.kinsoku {
            if let Some(next) = clusters.get(index + 1) {
                let next_char = input.text.get(next.start..next.end).unwrap_or("");
                let cur_char = input.text.get(cluster.start..cluster.end).unwrap_or("");
                if NO_START.contains(next_char) || NO_END.contains(cur_char) {
                    breakable = false;
                }
            }
        }
        // A CJK pair breaks anywhere when the paragraph allows it.
        if !breakable && input.word_wrap_off {
            if let Some(next) = clusters.get(index + 1) {
                let cur_char = input.text.get(cluster.start..cluster.end).unwrap_or("");
                let next_char = input.text.get(next.start..next.end).unwrap_or("");
                if cur_char
                    .chars()
                    .next()
                    .is_some_and(|c| is_east_asian(c) && !NO_END.contains(c))
                    && next_char
                        .chars()
                        .next()
                        .is_some_and(|c| is_east_asian(c) && !NO_START.contains(c))
                {
                    breakable = true;
                }
            }
        }
        // A space is always a break opportunity, and the space stays on the previous line.
        if matches!(cluster.kind, ClusterKind::Space) {
            breakable = true;
        }
        // Word's exceptions, measured in WordCraft: a break does not follow a slash, and a break
        // does follow a hyphen that comes before a digit. See `DP-F-23`.
        if breakable {
            let cur_char = input.text.get(cluster.start..cluster.end).unwrap_or("");
            let cur = cur_char.chars().next();
            if cur == Some('/') {
                breakable = false;
            }
            if cur == Some('-') {
                if let Some(next) = clusters.get(index + 1) {
                    let next_char = input.text.get(next.start..next.end).unwrap_or("");
                    if !next_char.chars().next().is_some_and(|c| c.is_ascii_digit()) {
                        breakable = false;
                    }
                }
            }
        }
        // A soft hyphen is a break opportunity of its own.
        if matches!(cluster.kind, ClusterKind::SoftHyphen) {
            breakable = true;
        }
        if let Some(c) = clusters.get_mut(index) {
            c.break_after = breakable;
        }
    }
    // A soft hyphen marks the break before it, not after it.
    for i in 0..clusters.len() {
        if matches!(
            clusters.get(i).map(|c| c.kind),
            Some(ClusterKind::SoftHyphen)
        ) && i > 0
        {
            if let Some(prev) = clusters.get_mut(i - 1) {
                prev.break_after = true;
            }
        }
    }
    clusters
}

/// The available band at a vertical position, after exclusions.
fn band_at(exclusions: &[Rect], y: f32, height: f32, left: f32, right: f32) -> Vec<(f32, f32)> {
    let mut blocked: Vec<(f32, f32)> = exclusions
        .iter()
        .filter(|r| y < r.bottom() && r.y < y + height)
        .map(|r| (r.x, r.right()))
        .collect();
    if blocked.is_empty() {
        return vec![(left, right)];
    }
    blocked.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let mut out = Vec::new();
    let mut cursor = left;
    for (bl, br) in blocked {
        if bl > cursor {
            out.push((cursor, bl.min(right)));
        }
        cursor = cursor.max(br);
        if cursor >= right {
            return out;
        }
    }
    if cursor < right {
        out.push((cursor, right));
    }
    out.retain(|(a, b)| b - a > 0.5);
    out
}

/// The line height of a cluster, from its face metrics and its style.
fn cluster_metrics<S: Shaper>(shaper: &S, style: &RunStyle) -> (f32, f32) {
    let (asc, desc, _) = shaper.line_metrics(style.face, style.size);
    (asc, desc)
}

/// Break a paragraph into lines. See `DP-F-47` and `DP-F-50`.
pub fn layout_paragraph<S: Shaper>(shaper: &S, input: &BreakInput<'_>) -> ParagraphLayout {
    let clusters = build_clusters(shaper, input);
    let content_width = (input.width - input.indent_left - input.indent_right).max(MIN_LINE_WIDTH);
    let mut lines: Vec<Line> = Vec::new();
    let mut index = 0usize;
    let mut y = 0.0f32;
    let mut first_line = true;
    let count = clusters.len();
    if count == 0 {
        // An empty paragraph still takes one line, which holds the paragraph mark.
        let style = input.styles.first().cloned().unwrap_or(RunStyle {
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
            caps: CapsMode::None,
            symbol: None,
            synth_bold: false,
            synth_italic: false,
        });
        let (asc, desc) = cluster_metrics(shaper, &style);
        let (ascent, descent, gap) = shaper.line_metrics(style.face, style.size);
        let height = ascent + descent + gap;
        let _ = (asc, desc);
        lines.push(Line {
            top: y,
            height,
            baseline: y + ascent,
            text_asc: ascent,
            text_desc: descent,
            c0: 0,
            c1: 0,
            xs: vec![input.indent_left],
            end: LineEnd::Para,
            start: 0,
            stop: 0,
            left: input.indent_left,
            right: input.indent_left + content_width,
            hyphen: None,
            beside: false,
            leaders: Vec::new(),
        });
        return ParagraphLayout {
            clusters,
            height,
            lines,
            max_width: 0.0,
        };
    }
    let mut guard = 0usize;
    while index < count {
        guard += 1;
        if guard > count + 4 {
            break;
        }
        let indent = if first_line {
            input.indent_left + input.indent_first
        } else {
            input.indent_left
        };
        let line_left = indent;
        let line_right = input.indent_left + content_width;
        // The band is the full width unless an exclusion crosses the line.
        let bands = band_at(input.exclusions, y, 1.0, line_left, line_right);
        let (band_left, band_right) = bands.first().copied().unwrap_or((line_left, line_right));
        let mut xs: Vec<f32> = Vec::new();
        let mut pen = band_left;
        let mut used = 0.0f32;
        let mut ascent = 0.0f32;
        let mut descent = 0.0f32;
        let mut line_gap = 0.0f32;
        let mut leaders: Vec<(usize, char, u32)> = Vec::new();
        let mut last_fit = index;
        let mut end_kind = LineEnd::Wrap;
        let mut hyphen: Option<(u32, f32)> = None;
        let start_index = index;
        let mut cursor = band_left;
        let mut j = index;
        while j < count {
            let Some(cluster) = clusters.get(j) else {
                break;
            };
            let style = input
                .styles
                .get(cluster.style as usize)
                .cloned()
                .unwrap_or_else(default_style);
            let (asc, desc, gap) = shaper.line_metrics(style.face, style.size);
            ascent = ascent.max(asc + style.baseline_shift.abs());
            descent = descent.max(desc + style.baseline_shift.abs());
            line_gap = line_gap.max(gap);
            match cluster.kind {
                ClusterKind::PageBreak => {
                    end_kind = LineEnd::PageBreak;
                    j += 1;
                    break;
                }
                ClusterKind::ColumnBreak => {
                    end_kind = LineEnd::ColumnBreak;
                    j += 1;
                    break;
                }
                ClusterKind::LineBreak => {
                    end_kind = LineEnd::LineBreak;
                    j += 1;
                    break;
                }
                ClusterKind::SoftHyphen => {
                    // A soft hyphen takes no room unless the line ends there.
                    xs.push(cursor);
                    j += 1;
                    continue;
                }
                ClusterKind::Tab => {
                    let (target, leader) = next_tab(input, cursor);
                    let gap_adv = (target - cursor).max(0.0);
                    if cursor + gap_adv > band_right + 0.001 && j > start_index {
                        end_kind = LineEnd::Wrap;
                        break;
                    }
                    xs.push(cursor);
                    if let Some(ch) = leader {
                        let dot_w = shaper
                            .advance(style.face, shaper.glyph(style.face, ch), style.size)
                            .max(1.0);
                        let n = (gap_adv / dot_w).floor() as u32;
                        leaders.push((xs.len() - 1, ch, n));
                    }
                    cursor = target;
                    pen = cursor;
                    used = cursor - band_left;
                    j += 1;
                    last_fit = j;
                    continue;
                }
                ClusterKind::Object(_) => {
                    let (oh, od) = input
                        .objects
                        .get(match cluster.kind {
                            ClusterKind::Object(i) => i,
                            _ => 0,
                        })
                        .copied()
                        .unwrap_or((0.0, 0.0));
                    ascent = ascent.max(oh);
                    descent = descent.max(od);
                }
                _ => {}
            }
            if cursor + cluster.adv > band_right + 0.001 && j > start_index {
                // The cluster does not fit. A space hangs past the edge in Word. Any other
                // cluster ends the line at the last break opportunity, so a line never splits a
                // word. See DP-F-23 and DP-F-27.
                if !matches!(cluster.kind, ClusterKind::Space) {
                    let mut cut = None;
                    for k in (start_index..j).rev() {
                        if clusters.get(k).is_some_and(|c| c.break_after)
                            && !matches!(
                                clusters.get(k).map(|c| c.kind),
                                Some(ClusterKind::SoftHyphen)
                            )
                        {
                            cut = Some(k + 1);
                            break;
                        }
                    }
                    // With no earlier opportunity, one word is wider than the band, so it
                    // overflows. See DP-F-27.
                    if let Some(k) = cut {
                        j = k;
                    }
                    break;
                }
            }
            xs.push(cursor);
            cursor += cluster.adv;
            pen = cursor;
            used = cursor - band_left;
            j += 1;
            last_fit = j;
        }
        // If the line overran, fall back to the last break opportunity that fits.
        if used > (band_right - band_left) + 0.001 {
            let mut best = None;
            for k in start_index..last_fit {
                if let Some(c) = clusters.get(k) {
                    if c.break_after && !matches!(c.kind, ClusterKind::SoftHyphen) {
                        best = Some(k + 1);
                    }
                }
            }
            if let Some(b) = best {
                j = b;
                xs.truncate(j - start_index);
                cursor = xs.last().copied().unwrap_or(band_left)
                    + clusters.get(j - 1).map_or(0.0, |c| c.adv);
                pen = cursor;
                used = cursor - band_left;
            }
        }
        // A soft hyphen at the break paints a hyphen.
        if let Some(next) = clusters.get(j) {
            if matches!(next.kind, ClusterKind::SoftHyphen) {
                let style = input.styles.get(next.style as usize);
                if let Some(style) = style {
                    let gid = shaper.glyph(style.face, '-');
                    hyphen = Some((next.style, shaper.advance(style.face, gid, style.size)));
                }
            }
        }
        if j == start_index {
            // Always make progress, even when one cluster is wider than the band. See DP-F-27.
            j = start_index + 1;
            if let Some(c) = clusters.get(start_index) {
                xs = vec![band_left];
                cursor = band_left + c.adv;
                pen = cursor;
                used = c.adv;
                ascent = ascent.max(c.obj_h);
                descent = descent.max(c.obj_d);
            }
        }
        // Trailing spaces hang past the right edge, so they leave the line measure.
        // See DP-F-49.
        let trailing = clusters
            .get(start_index..j)
            .unwrap_or(&[])
            .iter()
            .rev()
            .take_while(|c| matches!(c.kind, ClusterKind::Space))
            .count();
        let visible_end = j - trailing;
        let is_last = j >= count;
        let end = if is_last && matches!(end_kind, LineEnd::Wrap) {
            LineEnd::Para
        } else {
            end_kind
        };
        let text_width = if trailing > 0 {
            xs.get(visible_end - start_index)
                .copied()
                .unwrap_or(band_left)
                - band_left
        } else {
            used
        };
        // A line reports the band it was laid out in, which a float can narrow.
        let line_left_edge = band_left;
        let line_right_edge = band_right;
        // Alignment moves the clusters inside the band.
        let free = (band_right - band_left) - text_width;
        let is_justified = matches!(input.align, Align::Justify | Align::Distribute)
            && !is_last
            && !matches!(end, LineEnd::LineBreak)
            && free > 0.01;
        let mut shift = 0.0f32;
        match input.align {
            Align::Center => shift = (free / 2.0).max(0.0),
            Align::Right => shift = free.max(0.0),
            _ => {}
        }
        if is_justified {
            // Stretch spaces and tabs. See DP-F-28 and DP-F-31.
            let stretchable: Vec<usize> = clusters
                .get(start_index..visible_end)
                .unwrap_or(&[])
                .iter()
                .enumerate()
                .filter(|(_, c)| c.is_stretchable())
                .map(|(i, _)| i)
                .collect();
            let mut extra = free;
            let max_stretch = 3.0 * stretchable.len() as f32;
            if max_stretch > 0.0 && extra > max_stretch {
                extra = max_stretch;
            }
            let per = if stretchable.is_empty() {
                0.0
            } else {
                extra / stretchable.len() as f32
            };
            let mut out: Vec<f32> = Vec::with_capacity(xs.len() + 1);
            let mut added = 0.0f32;
            let mut idx = 0usize;
            for (i, x) in xs.iter().enumerate() {
                while stretchable.get(idx).is_some_and(|s| *s < i) {
                    added += per;
                    idx += 1;
                }
                out.push(x + added);
            }
            if !out.is_empty() {
                while idx < stretchable.len() {
                    added += per;
                    idx += 1;
                }
            }
            xs = out;
            cursor += added;
            pen = cursor;
        } else if shift > 0.0 {
            for x in &mut xs {
                *x += shift;
            }
            cursor += shift;
            pen = cursor;
        }
        // The last entry is the end x, so `xs.len() == c1 - c0 + 1`. See DP-F-49. A trailing
        // space hangs past the right edge, so the end x stops at the last visible cluster.
        let visible_pos = xs
            .get(visible_end.saturating_sub(start_index))
            .copied()
            .unwrap_or(pen);
        xs.push(visible_pos.max(xs.first().copied().unwrap_or(band_left)));
        if xs.len() < 2 {
            xs = vec![band_left, band_left];
        }
        let _ = cursor;
        let text_asc = ascent.max(1.0);
        let text_desc = descent.max(0.0);
        let height = text_asc + text_desc + line_gap;
        let baseline = y + text_asc + line_gap / 2.0;
        let start_byte = clusters.get(start_index).map_or(0, |c| c.start);
        let stop_byte = clusters
            .get(visible_end.saturating_sub(1))
            .map_or(start_byte, |c| c.end);

        lines.push(Line {
            top: y,
            height,
            baseline,
            text_asc,
            text_desc,
            c0: start_index,
            c1: j,
            xs,
            end,
            start: start_byte,
            stop: stop_byte,
            left: line_left_edge,
            right: line_right_edge,
            hyphen,
            beside: false,
            leaders,
        });
        y += height;
        index = j;
        first_line = false;
        if matches!(end, LineEnd::PageBreak | LineEnd::ColumnBreak) {
            break;
        }
    }
    let max_width = lines.iter().map(|l| l.ink_width()).fold(0.0f32, f32::max);
    ParagraphLayout {
        clusters,
        height: y,
        lines,
        max_width,
    }
}

/// The next tab stop after `x`, with the leader character of that stop. See `DP-F-33`.
pub fn next_tab(input: &BreakInput<'_>, x: f32) -> (f32, Option<char>) {
    let mut best: Option<(f32, Option<char>)> = None;
    for stop in input.tabs {
        if stop.pos > x + 0.01 {
            best = Some(match best {
                Some((b, _)) if b <= stop.pos => (b, None),
                _ => (stop.pos, stop.leader),
            });
        }
    }
    if let Some((pos, leader)) = best {
        return (pos, leader);
    }
    let step = if input.default_tab > 0.0 {
        input.default_tab
    } else {
        36.0
    };
    (((x / step).floor() + 1.0) * step, None)
}

/// The next tab stop after `x`.
pub fn next_tab_stop(input: &BreakInput<'_>, x: f32) -> f32 {
    next_tab(input, x).0
}

/// The text of a line, for a selection or a copy.
pub fn line_text<'a>(text: &'a str, line: &Line) -> &'a str {
    text.get(line.start..line.stop).unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::*;
    use edgedoc_fonts::{DbShaper, FontDb};

    const REGULAR: &[u8] = include_bytes!("../../../assets/fonts/subset/Carlito-Regular.ttf");
    const BOLD: &[u8] = include_bytes!("../../../assets/fonts/subset/Carlito-Bold.ttf");

    fn db() -> FontDb {
        let mut db = FontDb::new();
        db.load(REGULAR.to_vec(), Some("Carlito"), false, false, false);
        db.load(BOLD.to_vec(), Some("Carlito"), true, false, false);
        db
    }

    fn style(size: f32) -> RunStyle {
        RunStyle {
            face: 0,
            size,
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
            caps: CapsMode::None,
            symbol: None,
            synth_bold: false,
            synth_italic: false,
        }
    }

    fn input<'a>(
        text: &'a str,
        styles: &'a [RunStyle],
        runs: &'a [(usize, usize, u32)],
    ) -> BreakInput<'a> {
        BreakInput {
            text,
            runs,
            styles,
            width: 300.0,
            indent_left: 0.0,
            indent_right: 0.0,
            indent_first: 0.0,
            align: Align::Left,
            bidi: false,
            tabs: &[],
            default_tab: 36.0,
            exclusions: &[],
            word_wrap_off: false,
            kinsoku: true,
            objects: &[],
            hyphen_after: &[],
        }
    }

    #[test]
    fn one_cluster_per_grapheme() {
        let db = db();
        let shaper = DbShaper { db: &db };
        let styles = [style(11.0)];
        let runs = [(0usize, 5usize, 0u32)];
        let inp = input("Hello", &styles, &runs);
        let clusters = build_clusters(&shaper, &inp);
        assert_eq!(clusters.len(), 5);
        assert_eq!(clusters[0].start, 0);
        assert_eq!(clusters[0].end, 1);
    }

    #[test]
    fn a_combining_mark_is_one_cluster() {
        let db = db();
        let shaper = DbShaper { db: &db };
        let styles = [style(11.0)];
        let text = "e\u{0301}";
        let runs = [(0usize, text.len(), 0u32)];
        let inp = input(text, &styles, &runs);
        let clusters = build_clusters(&shaper, &inp);
        assert_eq!(clusters.len(), 1, "the mark joins the base letter");
    }

    #[test]
    fn a_space_is_a_break_opportunity() {
        let db = db();
        let shaper = DbShaper { db: &db };
        let styles = [style(11.0)];
        let text = "one two";
        let runs = [(0usize, text.len(), 0u32)];
        let inp = input(text, &styles, &runs);
        let clusters = build_clusters(&shaper, &inp);
        let space_index = 3;
        assert!(clusters[space_index].break_after);
    }

    #[test]
    fn text_wraps_at_the_width() {
        let db = db();
        let shaper = DbShaper { db: &db };
        let styles = [style(11.0)];
        let text = "The quick brown fox jumps over the lazy dog and keeps running past the edge of the page";
        let runs = [(0usize, text.len(), 0u32)];
        let mut inp = input(text, &styles, &runs);
        inp.width = 150.0;
        let layout = layout_paragraph(&shaper, &inp);
        assert!(layout.lines.len() > 1);
        for line in &layout.lines {
            assert!(
                line.ink_width() <= 150.0 + 0.5,
                "width {}",
                line.ink_width()
            );
        }
    }

    #[test]
    fn every_line_makes_progress_even_when_a_word_is_too_wide() {
        let db = db();
        let shaper = DbShaper { db: &db };
        let styles = [style(11.0)];
        let text = "Supercalifragilisticexpialidocious";
        let runs = [(0usize, text.len(), 0u32)];
        let mut inp = input(text, &styles, &runs);
        inp.width = 12.0;
        let layout = layout_paragraph(&shaper, &inp);
        assert!(!layout.lines.is_empty());
        // Every cluster lands on some line, so no character is lost.
        let covered: usize = layout.lines.iter().map(|l| l.c1 - l.c0).sum();
        assert_eq!(covered, layout.clusters.len());
    }

    #[test]
    fn a_zero_width_column_still_makes_progress() {
        let db = db();
        let shaper = DbShaper { db: &db };
        let styles = [style(11.0)];
        let text = "abc";
        let runs = [(0usize, 3usize, 0u32)];
        let mut inp = input(text, &styles, &runs);
        inp.width = 0.0;
        let layout = layout_paragraph(&shaper, &inp);
        assert!(!layout.lines.is_empty());
        assert!(layout.clusters.len() == 3);
    }

    #[test]
    fn an_empty_paragraph_takes_one_line() {
        let db = db();
        let shaper = DbShaper { db: &db };
        let styles = [style(11.0)];
        let runs: [(usize, usize, u32); 0] = [];
        let inp = input("", &styles, &runs);
        let layout = layout_paragraph(&shaper, &inp);
        assert_eq!(layout.lines.len(), 1);
        assert!(layout.height > 10.0);
    }

    #[test]
    fn a_hard_line_break_ends_the_line() {
        let db = db();
        let shaper = DbShaper { db: &db };
        let styles = [style(11.0)];
        let text = "one\ntwo";
        let runs = [(0usize, text.len(), 0u32)];
        let inp = input(text, &styles, &runs);
        let layout = layout_paragraph(&shaper, &inp);
        assert_eq!(layout.lines.len(), 2);
        assert_eq!(layout.lines[0].end, LineEnd::LineBreak);
        assert_eq!(layout.lines[1].end, LineEnd::Para);
    }

    #[test]
    fn justification_stretches_spaces_only() {
        let db = db();
        let shaper = DbShaper { db: &db };
        let styles = [style(11.0)];
        let text = "one two three four five six seven";
        let runs = [(0usize, text.len(), 0u32)];
        let mut inp = input(text, &styles, &runs);
        inp.width = 90.0;
        inp.align = Align::Justify;
        let layout = layout_paragraph(&shaper, &inp);
        assert!(layout.lines.len() > 1, "the paragraph takes several lines");
        let first = &layout.lines[0];
        // A justified line reaches the right edge, less the trailing space that hangs past it.
        let right = first.xs.get(first.c1 - first.c0).copied().unwrap_or(0.0);
        assert!(
            right > 80.0 && right <= 91.0,
            "a justified line stops at the right edge, not at {right}"
        );
        // The line ends at a word boundary: the next character is a space, so no word is split.
        let end = first.stop.min(text.len());
        let next = text.get(end..).unwrap_or("").chars().next();
        assert!(
            next.map_or(true, |c| c == ' '),
            "a justified line breaks before a space, not inside a word: {:?}",
            text.get(end..)
                .unwrap_or("")
                .chars()
                .take(8)
                .collect::<String>()
        );
    }

    #[test]
    fn the_last_line_is_not_justified() {
        let db = db();
        let shaper = DbShaper { db: &db };
        let styles = [style(11.0)];
        let text = "short line here";
        let runs = [(0usize, text.len(), 0u32)];
        let mut inp = input(text, &styles, &runs);
        inp.width = 400.0;
        inp.align = Align::Justify;
        let layout = layout_paragraph(&shaper, &inp);
        assert_eq!(layout.lines.len(), 1);
        assert_eq!(layout.lines[0].end, LineEnd::Para);
    }

    #[test]
    fn a_page_break_ends_the_line_and_the_paragraph() {
        let db = db();
        let shaper = DbShaper { db: &db };
        let styles = [style(11.0)];
        let text = "before\u{000C}after";
        let runs = [(0usize, text.len(), 0u32)];
        let inp = input(text, &styles, &runs);
        let layout = layout_paragraph(&shaper, &inp);
        assert_eq!(layout.lines[0].end, LineEnd::PageBreak);
    }

    #[test]
    fn bidi_levels_are_zero_for_latin() {
        let (levels, rtl) = bidi_levels("Hello");
        assert!(!rtl);
        assert!(levels.iter().all(|l| *l == 0));
    }

    #[test]
    fn a_hebrew_paragraph_is_right_to_left() {
        let (_, rtl) = bidi_levels("שלום");
        assert!(rtl);
    }

    #[test]
    fn a_decimal_separator_is_marked() {
        let db = db();
        let shaper = DbShaper { db: &db };
        let styles = [style(11.0)];
        let text = "12.50";
        let runs = [(0usize, text.len(), 0u32)];
        let inp = input(text, &styles, &runs);
        let clusters = build_clusters(&shaper, &inp);
        assert!(clusters[2].dot);
    }

    #[test]
    fn a_tab_reaches_the_next_stop() {
        let db = db();
        let shaper = DbShaper { db: &db };
        let styles = [style(11.0)];
        let text = "a\tb";
        let runs = [(0usize, text.len(), 0u32)];
        let stops = [TabStop {
            pos: 100.0,
            align: TabAlign::Left,
            leader: None,
        }];
        let mut inp = input(text, &styles, &runs);
        inp.tabs = &stops;
        let layout = layout_paragraph(&shaper, &inp);
        assert_eq!(layout.lines.len(), 1);
        // The cluster after the tab starts at the stop.
        assert!((layout.lines[0].xs[2] - 100.0).abs() < 0.01);
    }

    #[test]
    fn a_tab_past_the_last_stop_uses_the_default() {
        let db = db();
        let shaper = DbShaper { db: &db };
        let styles = [style(11.0)];
        // `aaaa` is wider than the 20 pt stop, so the tab takes the default interval.
        let text = "aaaa\tb";
        let runs = [(0usize, text.len(), 0u32)];
        let stops = [TabStop {
            pos: 20.0,
            align: TabAlign::Left,
            leader: None,
        }];
        let mut inp = input(text, &styles, &runs);
        inp.tabs = &stops;
        inp.default_tab = 36.0;
        let layout = layout_paragraph(&shaper, &inp);
        let x = layout.lines[0].xs[5];
        assert!(x >= 36.0, "the tab passed the default stop, x={x}");
        assert!((x - 36.0).abs() < 0.01, "x={x}");
    }

    #[test]
    fn a_bar_tab_leaves_a_leader() {
        let db = db();
        let shaper = DbShaper { db: &db };
        let styles = [style(11.0)];
        let text = "a\tb";
        let runs = [(0usize, text.len(), 0u32)];
        let stops = [TabStop {
            pos: 100.0,
            align: TabAlign::Left,
            leader: Some('.'),
        }];
        let mut inp = input(text, &styles, &runs);
        inp.tabs = &stops;
        let layout = layout_paragraph(&shaper, &inp);
        assert!(!layout.lines[0].leaders.is_empty());
    }

    #[test]
    fn kinsoku_keeps_a_closing_mark_off_the_line_start() {
        let db = db();
        let shaper = DbShaper { db: &db };
        let styles = [style(11.0)];
        let text = "文字。";
        let runs = [(0usize, text.len(), 0u32)];
        let inp = input(text, &styles, &runs);
        let clusters = build_clusters(&shaper, &inp);
        // The cluster before the closing mark may not break, so the mark stays with its text.
        assert!(!clusters[1].break_after);
    }

    #[test]
    fn hidden_text_takes_no_room() {
        let db = db();
        let shaper = DbShaper { db: &db };
        let mut hidden = style(11.0);
        hidden.hidden = true;
        let styles = [hidden];
        let text = "abc";
        let runs = [(0usize, 3usize, 0u32)];
        let inp = input(text, &styles, &runs);
        let clusters = build_clusters(&shaper, &inp);
        assert!(clusters.iter().all(|c| c.adv == 0.0));
    }

    #[test]
    fn width_excludes_the_indents() {
        let db = db();
        let shaper = DbShaper { db: &db };
        let styles = [style(11.0)];
        let text = "The quick brown fox jumps over the lazy dog";
        let runs = [(0usize, text.len(), 0u32)];
        let mut inp = input(text, &styles, &runs);
        inp.width = 300.0;
        inp.indent_left = 40.0;
        inp.indent_right = 40.0;
        let layout = layout_paragraph(&shaper, &inp);
        for line in &layout.lines {
            assert!(line.ink_width() <= 220.5, "ink {}", line.ink_width());
        }
    }

    #[test]
    fn an_exclusion_narrows_the_lines_it_covers() {
        let db = db();
        let shaper = DbShaper { db: &db };
        let styles = [style(11.0)];
        let text = "The quick brown fox jumps over the lazy dog and keeps running";
        let runs = [(0usize, text.len(), 0u32)];
        let exclusions = [Rect::new(0.0, 0.0, 200.0, 30.0)];
        let mut inp = input(text, &styles, &runs);
        inp.width = 400.0;
        inp.exclusions = &exclusions;
        let layout = layout_paragraph(&shaper, &inp);
        assert!(layout.lines.len() > 1);
        // The first lines sit beside the float, so they are narrower than the full width.
        assert!(layout.lines[0].ink_width() <= 200.5);
    }

    #[test]
    fn all_caps_affects_the_glyphs_not_the_text() {
        let db = db();
        let shaper = DbShaper { db: &db };
        let mut caps = style(11.0);
        caps.caps = CapsMode::All;
        let styles = [caps];
        let text = "ab";
        let runs = [(0usize, 2usize, 0u32)];
        let inp = input(text, &styles, &runs);
        let clusters = build_clusters(&shaper, &inp);
        assert_eq!(clusters.len(), 2);
        // The source text is unchanged; the glyph for "A" differs from the glyph for "a".
        assert_eq!(&text[clusters[0].start..clusters[0].end], "a");
    }

    #[test]
    fn letter_spacing_increases_the_advance() {
        let db = db();
        let shaper = DbShaper { db: &db };
        let plain = style(11.0);
        let mut spaced = style(11.0);
        spaced.letter_spacing = 2.0;
        let styles = [plain, spaced];
        let text = "aaa";
        let runs_a = [(0usize, 3usize, 0u32)];
        let runs_b = [(0usize, 3usize, 1u32)];
        let a = layout_paragraph(&shaper, &input(text, &styles, &runs_a));
        let b = layout_paragraph(&shaper, &input(text, &styles, &runs_b));
        assert!(b.max_width > a.max_width + 5.0);
    }

    #[test]
    fn x_scale_changes_the_advance() {
        let db = db();
        let shaper = DbShaper { db: &db };
        let plain = style(11.0);
        let mut scaled = style(11.0);
        scaled.x_scale = 0.5;
        let styles = [plain, scaled];
        let text = "aaaa";
        let runs_a = [(0usize, 4usize, 0u32)];
        let runs_b = [(0usize, 4usize, 1u32)];
        let a = layout_paragraph(&shaper, &input(text, &styles, &runs_a));
        let b = layout_paragraph(&shaper, &input(text, &styles, &runs_b));
        assert!((b.max_width - a.max_width * 0.5).abs() < 0.5);
    }
}
