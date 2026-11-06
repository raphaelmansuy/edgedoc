//! Line breaking, bidi, justification, and tabs.
//!
//! A paragraph becomes a list of [`Cluster`] values, then a list of [`Line`] values. Contextual
//! shaping may join graphemes into a ligature cluster. Each cluster owns its actual glyphs and
//! advances, and the breaker uses first fit.
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
    pub advance: f32,
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
    /// The shaping result used by both line layout and paint, relative to this cluster.
    pub glyphs: Vec<Glyph>,
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PositionalTab {
    pub align: TabAlign,
    pub margin_relative: bool,
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
    /// Font size used to measure the line; super/subscript glyphs draw smaller.
    pub line_size: f32,
    pub kerning: bool,
    pub positional_tab: Option<PositionalTab>,
    /// The text colour, resolved through the cascade. Word's default is black, not a theme
    /// colour, so a run with no `w:color` paints black.
    pub color: edgedoc_geom::Rgba,
    /// True when the run shows an underline.
    pub underline: bool,
    /// Draw two underline strokes when the document requests a double underline.
    pub double_underline: bool,
    /// Explicit underline colour; absent uses the text colour.
    pub underline_color: Option<edgedoc_geom::Rgba>,
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
    /// The explicit position shift used for line extents, excluding super/subscript alignment.
    pub line_shift: f32,
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
    /// Style of the paragraph mark, used when a line has only spaces and tabs.
    pub mark_style: u32,
    /// Available width in points.
    pub width: f32,
    /// Page-margin edges relative to the paragraph origin. None uses the paragraph edges.
    pub tab_margins: Option<(f32, f32)>,
    /// Left indent, in points.
    pub indent_left: f32,
    /// Right indent, in points, which reduces the width.
    pub indent_right: f32,
    /// First-line indent, in points. It applies to the first line only.
    pub indent_first: f32,
    /// Alignment.
    pub align: Align,
    /// Word 2013 and newer may fit another word by slightly shrinking spaces.
    pub shrink_justified_spaces: bool,
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
const NO_START: &str = "ぁぃぅぇぉっゃゅょゎゕゖァィゥェォッャュョヮヵヶㇰㇱㇲㇳㇴㇵㇶㇷㇸㇹㇺㇻㇼㇽㇾㇿｧｨｩｪｫｬｭｮｯーｰ゛゜ヽヾゝゞ々〻ﾞﾟ、。，．・：；？！〉》」』】〕〗〙〛〞〟）］｝｠｡｣､･％￠";

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
    let shaped = shaper.shape_run_kerning(style.face, &text, style.size, style.rtl, style.kerning);
    let mut glyphs = Vec::with_capacity(shaped.len());
    let mut adv = 0.0f32;
    for g in &shaped {
        glyphs.push(Glyph {
            gid: g.gid,
            advance: g.advance * style.x_scale,
            dx: g.dx * style.x_scale,
            dy: g.dy,
        });
        adv += g.advance;
    }
    if glyphs.is_empty() {
        // A missing glyph still takes room, so the line does not collapse.
        adv = style.size * 0.5;
    }
    adv = adv * style.x_scale + style.letter_spacing;
    (glyphs, adv.max(0.0))
}

/// Shape adjacent text together so kerning, ligatures and joining survive grapheme boundaries.
/// Control characters, bidi levels and style changes remain shaping boundaries.
fn contextual_shape<S: Shaper>(shaper: &S, input: &BreakInput<'_>, clusters: &mut Vec<Cluster>) {
    let mut out = Vec::with_capacity(clusters.len());
    let mut i = 0;
    while i < clusters.len() {
        let first = &clusters[i];
        let Some(style) = input.styles.get(first.style as usize) else {
            break;
        };
        if style.caps != CapsMode::None
            || !matches!(first.kind, ClusterKind::Text | ClusterKind::Space)
        {
            out.push(first.clone());
            i += 1;
            continue;
        }
        let mut j = i + 1;
        while j < clusters.len()
            && input.styles.get(clusters[j].style as usize) == Some(style)
            && clusters[j].level == first.level
            && matches!(clusters[j].kind, ClusterKind::Text | ClusterKind::Space)
        {
            j += 1;
        }
        let start = first.start;
        let stop = clusters[j - 1].end;
        let shaped = shaper.shape_run_kerning(
            style.face,
            &input.text[start..stop],
            style.size,
            style.rtl || first.level % 2 == 1,
            style.kerning,
        );
        let mut groups: std::collections::BTreeMap<usize, Vec<edgedoc_fonts::ShapedGlyph>> =
            std::collections::BTreeMap::new();
        for g in shaped {
            let source = start + g.cluster as usize;
            let boundary = clusters[i..j].partition_point(|c| c.start <= source);
            if let Some(c) = boundary
                .checked_sub(1)
                .and_then(|index| clusters[i..j].get(index))
            {
                groups.entry(c.start).or_default().push(g);
            }
        }
        if groups.is_empty() {
            for source in &clusters[i..j] {
                let mut c = source.clone();
                let (glyphs, advance) = shape_grapheme(shaper, style, &input.text[c.start..c.end]);
                c.glyphs = glyphs;
                c.adv = advance;
                out.push(c);
            }
            i = j;
            continue;
        }
        let boundaries: Vec<_> = groups
            .keys()
            .copied()
            .chain(std::iter::once(stop))
            .collect();
        for (k, (source, glyphs)) in groups.into_iter().enumerate() {
            let Some(template) = clusters[i..j]
                .binary_search_by_key(&source, |c| c.start)
                .ok()
                .and_then(|index| clusters[i..j].get(index))
            else {
                continue;
            };
            let mut c = template.clone();
            c.end = boundaries[k + 1];
            c.adv = (glyphs.iter().map(|g| g.advance).sum::<f32>() * style.x_scale
                + style.letter_spacing)
                .max(0.0);
            c.glyphs = glyphs
                .into_iter()
                .map(|g| Glyph {
                    gid: g.gid,
                    advance: g.advance * style.x_scale,
                    dx: g.dx * style.x_scale,
                    dy: g.dy,
                })
                .collect();
            out.push(c);
        }
        i = j;
    }
    let mut g0 = 0;
    for c in &mut out {
        c.g0 = g0;
        g0 += c.glyphs.len() as u32;
        c.g1 = g0;
    }
    *clusters = out;
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
        } else if slice.contains('\u{000E}') {
            ClusterKind::ColumnBreak
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
                ClusterKind::PageBreak
                    | ClusterKind::ColumnBreak
                    | ClusterKind::LineBreak
                    | ClusterKind::SoftHyphen
            ) {
            // Hidden text, a marker, and a break take no room and draw nothing.
            (Vec::new(), 0.0)
        } else if matches!(kind, ClusterKind::Object(_)) {
            // The object paints itself; the cluster only takes its advance.
            (Vec::new(), object_extent.0)
        } else if style.caps == CapsMode::None
            && matches!(kind, ClusterKind::Text | ClusterKind::Space)
        {
            // The contextual pass shapes the whole segment once, rather than parsing and
            // shaping the same font again for every grapheme.
            (Vec::new(), 0.0)
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
            glyphs,
            break_after: false,
            obj_h: object_extent.1,
            obj_d: 0.0,
            dot: matches!(first, Some('.')) || matches!(first, Some(',')),
            level: levels.get(start).copied().unwrap_or(0),
        };
        g0 += glyph_count;
        clusters.push(cluster);
    }
    contextual_shape(shaper, input, &mut clusters);
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
        // With kinsoku disabled, Wordcraft permits breaks between any two East Asian
        // characters, including punctuation.
        if !breakable && !input.kinsoku {
            if let Some(next) = clusters.get(index + 1) {
                let cur_char = input.text.get(cluster.start..cluster.end).unwrap_or("");
                let next_char = input.text.get(next.start..next.end).unwrap_or("");
                if cur_char.chars().next().is_some_and(is_east_asian)
                    && next_char.chars().next().is_some_and(is_east_asian)
                {
                    breakable = true;
                }
            }
        }
        // Word's wordWrap=0 additionally allows breaks inside Latin words.
        if !breakable && input.word_wrap_off {
            if let Some(next) = clusters.get(index + 1) {
                let latin = |s: &str| {
                    s.chars()
                        .next()
                        .is_some_and(|c| c.is_alphanumeric() && !is_east_asian(c))
                };
                if latin(input.text.get(cluster.start..cluster.end).unwrap_or(""))
                    && latin(input.text.get(next.start..next.end).unwrap_or(""))
                {
                    breakable = true;
                }
            }
        }
        // UAX #14 determines which spaces are break opportunities. In particular, it keeps
        // punctuation following a space with the preceding word rather than starting a line
        // with the punctuation.
        // Word keeps a slash joined to a following word, while preserving a
        // break before a slash that starts a word after ordinary whitespace.
        let cur = input.text.get(cluster.start..cluster.end).unwrap_or("");
        let next = clusters
            .get(index + 1)
            .and_then(|next| input.text.get(next.start..next.end))
            .and_then(|s| s.chars().next());
        if cur == "/" && next.is_some_and(char::is_alphanumeric) {
            breakable = false;
        }
        if cur == "-" && next.is_some_and(|c| c.is_ascii_digit()) {
            breakable = true;
        }
        if matches!(cluster.kind, ClusterKind::Space) && next == Some('/') {
            let after_slash = clusters
                .get(index + 2)
                .and_then(|next| input.text.get(next.start..next.end))
                .and_then(|s| s.chars().next());
            if after_slash.is_some_and(char::is_alphanumeric) {
                breakable = true;
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
    let (asc, desc, _) = shaper.line_metrics(style.face, style.line_size);
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
        let style = input
            .styles
            .get(input.mark_style as usize)
            .cloned()
            .unwrap_or(RunStyle {
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
                caps: CapsMode::None,
                symbol: None,
                synth_bold: false,
                synth_italic: false,
            });
        let (asc, desc) = cluster_metrics(shaper, &style);
        let (ascent, descent, gap) = shaper.line_metrics(style.face, style.line_size);
        let height = ascent + descent + gap;
        let _ = (asc, desc);
        lines.push(Line {
            top: y,
            height,
            baseline: y + ascent + gap.max(0.0),
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
        let (band_left, mut band_right) = bands.first().copied().unwrap_or((line_left, line_right));
        let mut xs: Vec<f32> = Vec::new();
        let mut pen = band_left;
        let mut used = 0.0f32;
        let mut ascent = 0.0f32;
        let mut descent = 0.0f32;
        let mut ascent_with_gap = 0.0f32;
        let mut leaders: Vec<(usize, char, u32)> = Vec::new();
        let mut last_fit = index;
        let mut end_kind = LineEnd::Wrap;
        let mut hyphen: Option<(u32, f32)> = None;
        let start_index = index;
        let mut cursor = band_left;
        let mut j = index;
        let mut space_width = 0.0f32;
        let mut word_from = cursor;
        let mut has_tab = false;
        while j < count {
            let Some(cluster) = clusters.get(j) else {
                break;
            };
            let style = input
                .styles
                .get(cluster.style as usize)
                .cloned()
                .unwrap_or_else(default_style);
            // Spaces and tabs do not enlarge a line. When they are its only content,
            // the paragraph mark supplies the line's font metrics instead.
            if !(matches!(cluster.kind, ClusterKind::Space | ClusterKind::Tab)
                || matches!(cluster.kind, ClusterKind::Object(_))
                    && cluster.adv == 0.0
                    && cluster.obj_h == 0.0)
            {
                let (asc, desc, gap) = shaper.line_metrics(style.face, style.line_size);
                ascent = ascent.max(asc + style.line_shift.max(0.0));
                descent = descent.max(desc + (-style.line_shift).max(0.0));
                ascent_with_gap =
                    ascent_with_gap.max(asc + gap.max(0.0) + style.line_shift.max(0.0));
            }
            match cluster.kind {
                ClusterKind::PageBreak => {
                    xs.push(cursor);
                    end_kind = LineEnd::PageBreak;
                    j += 1;
                    break;
                }
                ClusterKind::ColumnBreak => {
                    xs.push(cursor);
                    end_kind = LineEnd::ColumnBreak;
                    j += 1;
                    break;
                }
                ClusterKind::LineBreak => {
                    xs.push(cursor);
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
                    has_tab = true;
                    if style.positional_tab.is_some_and(|tab| tab.margin_relative) {
                        band_right = band_right
                            .max(input.tab_margins.map_or(input.width, |(_, right)| right));
                    } else if style.positional_tab.is_none() {
                        // A right or decimal tab can anchor text at a stop beyond the
                        // paragraph's right edge; Wordcraft grows the segment leftward.
                        if let Some(tab) = input
                            .tabs
                            .iter()
                            .filter(|tab| tab.pos > cursor + 0.01 && tab.align != TabAlign::Bar)
                            .min_by(|a, b| a.pos.total_cmp(&b.pos))
                        {
                            if matches!(tab.align, TabAlign::Right | TabAlign::Decimal) {
                                band_right = band_right.max(tab.pos);
                            }
                        }
                    }
                    let (target, leader) =
                        aligned_tab(input, &clusters, j, cursor, style.positional_tab);
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
                    let (_, oh) = input
                        .objects
                        .get(match cluster.kind {
                            ClusterKind::Object(i) => i,
                            _ => 0,
                        })
                        .copied()
                        .unwrap_or((0.0, 0.0));
                    ascent = ascent.max(oh);
                }
                _ => {}
            }
            let shrink_room =
                if input.shrink_justified_spaces && input.align == Align::Justify && !has_tab {
                    (space_width * 0.25).min((cursor + cluster.adv - word_from).max(0.0) * 0.355)
                } else {
                    0.0
                };
            if cursor + cluster.adv > band_right + shrink_room + 0.01 && j > start_index {
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
            if matches!(cluster.kind, ClusterKind::Space) {
                space_width += cluster.adv;
            }
            if cluster.break_after {
                word_from = if matches!(cluster.kind, ClusterKind::Space) {
                    cursor - cluster.adv
                } else {
                    cursor
                };
            }
            pen = cursor;
            used = cursor - band_left;
            j += 1;
            last_fit = j;
        }
        let shrink_line =
            input.shrink_justified_spaces && input.align == Align::Justify && !has_tab;
        if shrink_line && j < last_fit {
            xs.truncate(j - start_index);
            cursor = xs.last().copied().unwrap_or(band_left)
                + clusters.get(j.saturating_sub(1)).map_or(0.0, |c| c.adv);
            pen = cursor;
            used = cursor - band_left;
        }
        // If the line overran, fall back to the last break opportunity that fits.
        if !shrink_line && used > (band_right - band_left) + 0.001 {
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
        if shrink_line && free < -0.001 {
            let total: f32 = clusters[start_index..visible_end]
                .iter()
                .filter(|c| matches!(c.kind, ClusterKind::Space))
                .map(|c| c.adv)
                .sum();
            if total > 0.0 {
                let ratio = (-free / total).clamp(0.0, 0.25);
                let mut taken = 0.0;
                for (i, x) in xs.iter_mut().enumerate() {
                    *x -= taken;
                    if let Some(c) = clusters
                        .get(start_index + i)
                        .filter(|c| matches!(c.kind, ClusterKind::Space))
                    {
                        if start_index + i < visible_end {
                            taken += c.adv * ratio;
                        }
                    }
                }
                pen -= taken;
            }
        } else if is_justified {
            // Stretch spaces and tabs. See DP-F-28 and DP-F-31.
            let stretchable: Vec<usize> = clusters
                .get(start_index..visible_end)
                .unwrap_or(&[])
                .iter()
                .enumerate()
                .filter(|(_, c)| c.is_stretchable())
                .map(|(i, _)| i)
                .collect();
            let per = if stretchable.is_empty() {
                0.0
            } else {
                free / stretchable.len() as f32
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
            // A tab anchors everything through it. Wordcraft applies paragraph alignment
            // only to the segment after the last tab on this line.
            let align_from = clusters
                .get(start_index..j)
                .unwrap_or(&[])
                .iter()
                .rposition(|cluster| matches!(cluster.kind, ClusterKind::Tab))
                .map_or(0, |index| index + 1);
            for x in xs.iter_mut().skip(align_from) {
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
        // Each font's gap belongs to its own ascent. Combining the largest ascent from one
        // face with the largest gap from another makes mixed-font lines too tall.
        if ascent == 0.0 && descent == 0.0 {
            if let Some(style) = input.styles.get(input.mark_style as usize) {
                let (asc, desc, gap) = shaper.line_metrics(style.face, style.line_size);
                ascent = asc + style.line_shift.max(0.0);
                descent = desc + (-style.line_shift).max(0.0);
                ascent_with_gap = ascent + gap.max(0.0);
            }
        }
        let text_asc = ascent.max(1.0);
        let line_gap = (ascent_with_gap - text_asc).max(0.0);
        let text_desc = descent.max(0.0);
        let height = text_asc + text_desc + line_gap;
        // Wordcraft fonts::word::line_metrics places hhea lineGap above the ascent.
        let baseline = y + text_asc + line_gap.max(0.0);
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
    }
    // A final hard line break starts an empty line before the paragraph mark.
    // Page and column breaks are handled by pagination and do not do this.
    if let Some(last) = clusters
        .last()
        .filter(|c| matches!(c.kind, ClusterKind::LineBreak))
    {
        let style = input
            .styles
            .get(last.style as usize)
            .cloned()
            .unwrap_or_else(default_style);
        let (asc, desc, gap) = shaper.line_metrics(style.face, style.size);
        let height = asc + desc + gap;
        let left = input.indent_left;
        lines.push(Line {
            top: y,
            height,
            baseline: y + asc + gap.max(0.0),
            text_asc: asc,
            text_desc: desc,
            c0: count,
            c1: count,
            xs: vec![left],
            end: LineEnd::Para,
            start: input.text.len(),
            stop: input.text.len(),
            left,
            right: input.indent_left + content_width,
            hyphen: None,
            beside: false,
            leaders: Vec::new(),
        });
        y += height;
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
fn aligned_tab(
    input: &BreakInput<'_>,
    clusters: &[Cluster],
    index: usize,
    x: f32,
    positional: Option<PositionalTab>,
) -> (f32, Option<char>) {
    let (stop, align, leader) = if let Some(tab) = positional {
        let (left, right) = if tab.margin_relative {
            input.tab_margins.unwrap_or((0.0, input.width))
        } else {
            (0.0, input.width)
        };
        let stop = match tab.align {
            TabAlign::Center => (left + right) / 2.0,
            TabAlign::Right => right,
            _ => left,
        };
        (stop, tab.align, tab.leader)
    } else if let Some(tab) = input
        .tabs
        .iter()
        .filter(|tab| tab.pos > x + 0.01 && tab.align != TabAlign::Bar)
        .min_by(|a, b| a.pos.total_cmp(&b.pos))
    {
        (tab.pos, tab.align, tab.leader)
    } else {
        return next_tab(input, x);
    };
    let following: Vec<_> = clusters
        .iter()
        .skip(index + 1)
        .take_while(|c| {
            !matches!(
                c.kind,
                ClusterKind::Tab
                    | ClusterKind::LineBreak
                    | ClusterKind::PageBreak
                    | ClusterKind::ColumnBreak
            )
        })
        .collect();
    let count = following
        .iter()
        .rposition(|c| c.kind != ClusterKind::Space)
        .map_or(0, |i| i + 1);
    let length: f32 = following.iter().take(count).map(|c| c.adv).sum();
    let decimal: f32 = following.iter().take_while(|c| !c.dot).map(|c| c.adv).sum();
    let offset = match align {
        TabAlign::Right => length,
        TabAlign::Center => length / 2.0,
        TabAlign::Decimal => decimal.min(length),
        _ => 0.0,
    };
    ((stop - offset).max(x), leader)
}

pub fn next_tab(input: &BreakInput<'_>, x: f32) -> (f32, Option<char>) {
    let mut best: Option<(f32, Option<char>)> = None;
    for stop in input.tabs.iter().filter(|stop| stop.align != TabAlign::Bar) {
        if stop.pos > x + 0.01 {
            best = Some(match best {
                Some((b, leader)) if b <= stop.pos => (b, leader),
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
    // Default stops are on the column-margin grid, even when the current
    // paragraph is indented. For positions left of the margin Word truncates
    // toward zero and permits a tab to stay on an existing negative stop.
    let origin = input.tab_margins.map_or(0.0, |(left, _)| left);
    let relative = x - origin;
    let n = if relative < -0.01 {
        ((relative - 0.01) / step).ceil()
    } else {
        ((relative + 0.01) / step).floor() + 1.0
    };
    (n * step + origin, None)
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
            line_size: size,
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
            mark_style: 0,
            width: 300.0,
            tab_margins: None,
            indent_left: 0.0,
            indent_right: 0.0,
            indent_first: 0.0,
            align: Align::Left,
            shrink_justified_spaces: false,
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
    fn modern_justification_can_fit_a_word_by_shrinking_spaces() {
        let db = db();
        let shaper = DbShaper { db: &db };
        let styles = [style(11.0)];
        let text = "one two three four";
        let runs = [(0, text.len(), 0)];
        let mut paragraph = input(text, &styles, &runs);
        paragraph.align = Align::Justify;
        let clusters = build_clusters(&shaper, &paragraph);
        let through_three: f32 = clusters
            .iter()
            .take_while(|c| c.end <= "one two three".len())
            .map(|c| c.adv)
            .sum();
        let spaces: f32 = clusters
            .iter()
            .take_while(|c| c.end <= "one two three".len())
            .filter(|c| matches!(c.kind, ClusterKind::Space))
            .map(|c| c.adv)
            .sum();
        paragraph.width = through_three - spaces * 0.1;
        let legacy = layout_paragraph(&shaper, &paragraph);
        paragraph.shrink_justified_spaces = true;
        let modern = layout_paragraph(&shaper, &paragraph);
        assert!(legacy.lines[0].stop < "one two three".len());
        assert!(modern.lines[0].stop >= "one two three".len());
        let line = &modern.lines[0];
        assert!((line.xs.last().unwrap() - paragraph.width).abs() < 0.02);
    }

    #[test]
    fn modern_justification_does_not_shrink_a_tabbed_line() {
        let db = db();
        let shaper = DbShaper { db: &db };
        let styles = [style(11.0)];
        let text = "one\ttwo three four five";
        let runs = [(0, text.len(), 0)];
        let mut paragraph = input(text, &styles, &runs);
        paragraph.align = Align::Justify;
        paragraph.width = 65.0;
        let legacy = layout_paragraph(&shaper, &paragraph);
        paragraph.shrink_justified_spaces = true;
        let modern = layout_paragraph(&shaper, &paragraph);
        assert_eq!(legacy.lines[0], modern.lines[0]);
    }

    #[test]
    fn paragraph_wrap_options_change_break_opportunities() {
        let db = db();
        let shaper = DbShaper { db: &db };
        let styles = [style(11.0)];
        let runs = [(0, "abcde".len(), 0)];
        let mut latin = input("abcde", &styles, &runs);
        assert!(!build_clusters(&shaper, &latin)[0].break_after);
        latin.word_wrap_off = true;
        assert!(build_clusters(&shaper, &latin)[0].break_after);

        let text = "漢（字";
        let runs = [(0, text.len(), 0)];
        let mut asian = input(text, &styles, &runs);
        assert!(!build_clusters(&shaper, &asian)[1].break_after);
        asian.kinsoku = false;
        assert!(build_clusters(&shaper, &asian)[1].break_after);
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
    fn a_space_before_closing_punctuation_does_not_start_a_line() {
        let db = db();
        let shaper = DbShaper { db: &db };
        let styles = [style(11.0)];
        let text = "word, . next";
        let runs = [(0usize, text.len(), 0u32)];
        let clusters = build_clusters(&shaper, &input(text, &styles, &runs));
        assert!(!clusters[5].break_after);
        assert!(clusters[7].break_after);
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
    fn a_trailing_hard_line_break_keeps_an_empty_final_line() {
        let db = db();
        let shaper = DbShaper { db: &db };
        let styles = [style(11.0)];
        let text = "Hello\n";
        let runs = [(0, text.len(), 0)];
        let layout = layout_paragraph(&shaper, &input(text, &styles, &runs));
        assert_eq!(layout.lines.len(), 2);
        assert_eq!(layout.lines[1].start, text.len());
        assert_eq!(layout.lines[1].stop, text.len());
        assert!(layout.lines[1].height > 0.0);
    }

    #[test]
    fn hyphens_and_slashes_keep_wordcraft_break_opportunities() {
        let db = db();
        let shaper = DbShaper { db: &db };
        let styles = [style(11.0)];
        let text = "co-operation 2016-2022 and/or /word";
        let runs = [(0, text.len(), 0)];
        let clusters = build_clusters(&shaper, &input(text, &styles, &runs));
        let at = |index: usize| {
            clusters
                .iter()
                .find(|c| c.start == index)
                .map(|c| c.break_after)
        };
        assert_eq!(at(text.find('-').unwrap()), Some(true));
        assert_eq!(at(text.find("2016-").unwrap() + 4), Some(true));
        assert_eq!(at(text.find('/').unwrap()), Some(false));
        assert_eq!(at(text.rfind(" /word").unwrap()), Some(true));
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
    fn right_tab_just_beyond_margin_keeps_its_segment_on_one_line() {
        let db = db();
        let shaper = DbShaper { db: &db };
        let styles = [style(11.0)];
        let text = "\tPage 1 of 7 pages";
        let runs = [(0usize, text.len(), 0u32)];
        let stops = [TabStop {
            pos: 300.05,
            align: TabAlign::Right,
            leader: None,
        }];
        let mut inp = input(text, &styles, &runs);
        inp.width = 300.0;
        inp.tabs = &stops;
        let layout = layout_paragraph(&shaper, &inp);
        assert_eq!(layout.lines.len(), 1);
        assert_eq!(line_text(text, &layout.lines[0]), text);
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
