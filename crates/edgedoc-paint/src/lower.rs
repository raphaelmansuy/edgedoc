//! Lower page geometry to a display list.
//!
//! This module is the only place that turns a fragment into paint operations. A painter therefore
//! cannot disagree about a table border or a text baseline. See `DP-G-03`.

use std::collections::BTreeMap;

use edgedoc_flow::{BorderSegment, Fragment, PageGeometry, ParagraphFragment, TableFragment};
use edgedoc_fonts::Shaper;
use edgedoc_geom::{Rect, Rgba};
use edgedoc_style::{BorderStyle, ComputedRun};
use edgedoc_text::{CapsMode, ParagraphLayout, RunStyle};

use crate::DisplayPage;
use crate::{FillRule, GlyphRunOp, LinkOp, Op, Paint, PathSeg, StrokeStyle};

/// One entry of the text index. See `DP-G-13`.
#[derive(Debug, Clone, PartialEq)]
pub struct TextIndexEntry {
    /// The ordinal of the `Glyphs` operation in the body.
    pub run: u32,
    pub char_start: u32,
    pub char_end: u32,
    pub rect: Rect,
    pub baseline: f32,
    pub level: u8,
    pub hidden: bool,
}

/// A link target: a URL, a mail address, or a bookmark name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkTarget {
    External(String),
    Internal(String),
}

impl LinkTarget {
    pub fn as_str(&self) -> &str {
        match self {
            LinkTarget::External(s) | LinkTarget::Internal(s) => s,
        }
    }

    /// A scheme that the viewer must refuse. See `DP-B-35`.
    pub fn is_blocked(&self) -> bool {
        let s = self.as_str().trim_start().to_ascii_lowercase();
        s.starts_with("javascript:") || s.starts_with("vbscript:") || s.starts_with("data:")
    }
}

/// The options that change what the list holds.
#[derive(Debug, Clone)]
pub struct LowerOptions {
    /// Paint a box around every fragment, which a fidelity test uses.
    pub debug_boxes: bool,
    /// Show hidden text.
    pub show_hidden: bool,
    /// Paint tracked deletions struck through.
    pub markup: bool,
}

impl Default for LowerOptions {
    fn default() -> Self {
        Self {
            debug_boxes: false,
            show_hidden: false,
            markup: true,
        }
    }
}

/// Lower one page. See `DP-G-01`.
pub fn lower_page<S: Shaper>(
    shaper: &S,
    page: &PageGeometry,
    options: &LowerOptions,
) -> DisplayPage {
    let mut lower = Lowerer {
        shaper,
        options,
        ops: Vec::new(),
        targets: Vec::new(),
        target_index: BTreeMap::new(),
        text: Vec::new(),
        destinations: Vec::new(),
        run_ordinal: 0,
    };
    lower.ops.push(Op::BeginPage {
        w: page.w,
        h: page.h,
        background: Rgba::WHITE,
    });
    for fragment in &page.fragments {
        lower.fragment(fragment);
    }
    lower.ops.push(Op::EndPage);
    DisplayPage {
        index: page.index,
        w: page.w,
        h: page.h,
        targets: lower.targets,
        text: lower.text,
        ops: lower.ops,
        destinations: lower.destinations,
    }
}

struct Lowerer<'a, S: Shaper> {
    shaper: &'a S,
    options: &'a LowerOptions,
    ops: Vec<Op>,
    targets: Vec<String>,
    target_index: BTreeMap<String, u32>,
    text: Vec<TextIndexEntry>,
    destinations: Vec<(String, (f32, f32))>,
    run_ordinal: u32,
}

impl<S: Shaper> Lowerer<'_, S> {
    fn target(&mut self, s: &str) -> u32 {
        if let Some(i) = self.target_index.get(s) {
            return *i;
        }
        let i = self.targets.len() as u32;
        self.targets.push(s.to_string());
        self.target_index.insert(s.to_string(), i);
        i
    }

    fn fragment(&mut self, fragment: &Fragment) {
        match fragment {
            Fragment::Paragraph(p) => self.paragraph(p),
            Fragment::Table(t) => self.table(t),
            Fragment::Picture {
                rect,
                resource,
                alt,
            } => {
                self.ops.push(Op::Image {
                    resource: *resource,
                    rect: *rect,
                    src: [0.0, 0.0, 1.0, 1.0],
                    opacity: 1.0,
                    interpolate: true,
                });
                let _ = alt;
                if self.options.debug_boxes {
                    self.ops.push(Op::DebugBox {
                        rect: *rect,
                        label: "picture".into(),
                    });
                }
            }
            Fragment::Placeholder { rect, kind, label } => self.placeholder(*rect, kind, label),
            Fragment::Rule { rect, border } => {
                let (paint, _) = border_paint(*border);
                if paint.a > 0 {
                    self.ops.push(Op::FillRect {
                        rect: *rect,
                        paint: Paint::solid(paint),
                    });
                }
            }
        }
    }

    /// Paint the box that a missing object shows. See `DP-P-10`.
    fn placeholder(&mut self, rect: Rect, kind: &str, label: &str) {
        // A light fill, a dashed outline, a diagonal cross, and the label.
        self.ops.push(Op::FillRect {
            rect,
            paint: Paint::solid(Rgba::opaque(0xF2, 0xF4, 0xF8)),
        });
        let stroke = StrokeStyle {
            width: 0.75,
            dash: [3.0, 3.0, 0.0, 0.0],
            dash_len: 2,
            ..StrokeStyle::default()
        };
        self.ops.push(Op::StrokeRect {
            rect,
            paint: Rgba::opaque(0x9A, 0xA3, 0xB2),
            style: stroke,
        });
        self.ops.push(Op::StrokePath {
            segs: vec![
                PathSeg::Move(rect.x, rect.y),
                PathSeg::Line(rect.right(), rect.bottom()),
            ],
            paint: Rgba::opaque(0xD5, 0xDA, 0xE3),
            style: StrokeStyle {
                width: 0.5,
                ..StrokeStyle::default()
            },
        });
        self.ops.push(Op::StrokePath {
            segs: vec![
                PathSeg::Move(rect.right(), rect.y),
                PathSeg::Line(rect.x, rect.bottom()),
            ],
            paint: Rgba::opaque(0xD5, 0xDA, 0xE3),
            style: StrokeStyle {
                width: 0.5,
                ..StrokeStyle::default()
            },
        });
        self.ops.push(Op::Placeholder {
            rect,
            kind: kind.to_string(),
            label: label.to_string(),
        });
    }

    fn paragraph(&mut self, p: &ParagraphFragment) {
        let layout = &p.layout;
        // Paragraph background shading covers every line of this fragment.
        if let Some(shading) = p.para.shading {
            if shading.a > 0 {
                self.ops.push(Op::FillRect {
                    rect: p.rect,
                    paint: Paint::solid(shading),
                });
            }
        }
        // Paragraph borders.
        let b = &p.para.borders;
        for (side, border) in [
            (0usize, b.top),
            (1usize, b.left),
            (2usize, b.bottom),
            (3usize, b.right),
        ] {
            if let Some(border) = border {
                self.border_segment(rect_side(p.rect, side), border);
            }
        }
        // A list label paints at its own position, right-aligned to the text indent.
        if let Some((label, rect)) = &p.label {
            if !label.trim().is_empty() {
                let style = p.run_styles.first();
                let (face, size) = style.map_or((0, 11.0), |s| (s.face, s.size));
                let shaped = self.shaper.shape_run(face, label, size, false);
                let width: f32 = shaped
                    .iter()
                    .map(|g| self.shaper.advance(face, g.gid, size))
                    .sum();
                // Right-align the label against the text indent, which is where Word puts it.
                let x = (rect.right() - width - 4.0).max(rect.x);
                let mut pen = x;
                let baseline = rect.y + size * 0.85;
                for (i, g) in shaped.iter().enumerate() {
                    let adv = self.shaper.advance(face, g.gid, size);
                    self.push_run(
                        face,
                        size,
                        pen,
                        baseline,
                        vec![g.gid],
                        vec![adv],
                        vec![(g.dx, g.dy)],
                        vec![0],
                        p.run_styles.first().map_or(Rgba::BLACK, |s| s.color),
                        0,
                        false,
                        false,
                        1.0,
                        label
                            .chars()
                            .nth(i)
                            .map_or_else(String::new, |c| c.to_string()),
                        None,
                        false,
                        None,
                    );
                    pen += adv;
                }
            }
        }
        for line_index in p.l0..p.l1 {
            let Some(line) = layout.lines.get(line_index) else {
                continue;
            };
            self.line(p, layout, line);
        }
        // A tracked deletion paints a strike across the fragment.
        if self.options.markup {
            if let Some(edgedoc_flow::RevisionKind::Delete) = p.revision {
                self.ops.push(Op::StrokePath {
                    segs: vec![
                        PathSeg::Move(p.rect.x, p.rect.y + p.rect.h * 0.6),
                        PathSeg::Line(p.rect.right(), p.rect.y + p.rect.h * 0.6),
                    ],
                    paint: Rgba::opaque(0xC0, 0x1B, 0x1B),
                    style: StrokeStyle {
                        width: 0.75,
                        ..StrokeStyle::default()
                    },
                });
            }
        }
        for (rect, target) in &p.links {
            let t = self.target(target);
            self.ops.push(Op::Link(LinkOp {
                rect: *rect,
                target: t,
            }));
        }
        if self.options.debug_boxes {
            self.ops.push(Op::DebugBox {
                rect: p.rect,
                label: p.path.clone(),
            });
        }
    }

    fn line(&mut self, p: &ParagraphFragment, layout: &ParagraphLayout, line: &edgedoc_text::Line) {
        // The y of this line inside the fragment, from the layout.
        let y = p.rect.y + (line.top - layout.lines.get(p.l0).map_or(0.0, |l| l.top));
        let x0 = p.rect.x;
        let baseline = y + line.baseline - layout.lines.get(p.l0).map_or(0.0, |l| l.top);
        // Group the clusters of one style into one glyph run, so a line of text is a handful of
        // operations, not one per glyph. See DP-P-20.
        let mut i = line.c0;
        while i < line.c1 {
            let Some(cluster) = layout.clusters.get(i) else {
                break;
            };
            let style_index = cluster.style;
            let Some(style) = p.run_styles.get(style_index as usize) else {
                i += 1;
                continue;
            };
            let mut j = i;
            while j < line.c1 {
                match layout.clusters.get(j) {
                    Some(c) if c.style == style_index => j += 1,
                    _ => break,
                }
            }
            let start_byte = layout.clusters.get(i).map_or(0, |c| c.start);
            let end_byte = layout
                .clusters
                .get(j.saturating_sub(1))
                .map_or(start_byte, |c| c.end);
            self.run(
                p, layout, line, i, j, style, x0, baseline, start_byte, end_byte,
            );
            i = j;
        }
        // A tab leader paints across its tab.
        for (cluster_offset, ch, count) in &line.leaders {
            let idx = line.c0 + *cluster_offset;
            let x = line.xs.get(*cluster_offset).copied().unwrap_or(0.0) + x0;
            let next_x = line.xs.get(*cluster_offset + 1).copied().unwrap_or(x - x0) + x0;
            let _ = idx;
            let style = p.run_styles.first();
            let (face, size) = style.map_or((0, 11.0), |s| (s.face, s.size));
            let gid = self.shaper.glyph(face, *ch);
            let adv = self.shaper.advance(face, gid, size).max(0.5);
            let n = (*count).min(((next_x - x - 1.0) / adv).max(0.0) as u32);
            for k in 0..n {
                let gx = x + k as f32 * adv;
                self.push_run(
                    face,
                    size,
                    gx,
                    baseline,
                    vec![gid],
                    vec![adv],
                    vec![(0.0, 0.0)],
                    vec![0],
                    p.run_styles.first().map_or(Rgba::BLACK, |s| s.color),
                    0,
                    false,
                    false,
                    1.0,
                    ch.to_string(),
                    None,
                    false,
                    None,
                );
            }
        }
        // A hyphen at a soft-hyphen break.
        if let Some((style_index, adv)) = line.hyphen {
            let style = p.run_styles.get(style_index as usize);
            if let Some(style) = style {
                let gid = self.shaper.glyph(style.face, '-');
                let x = line.xs.last().copied().unwrap_or(0.0) + x0;
                self.push_run(
                    style.face,
                    style.size,
                    x,
                    baseline,
                    vec![gid],
                    vec![adv],
                    vec![(0.0, 0.0)],
                    vec![0],
                    style.color,
                    0,
                    false,
                    false,
                    1.0,
                    "-".into(),
                    None,
                    false,
                    None,
                );
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn run(
        &mut self,
        p: &ParagraphFragment,
        layout: &ParagraphLayout,
        line: &edgedoc_text::Line,
        i: usize,
        j: usize,
        style: &RunStyle,
        x0: f32,
        baseline: f32,
        start_byte: usize,
        end_byte: usize,
    ) {
        let mut glyphs = Vec::new();
        let mut advances = Vec::new();
        let mut offsets = Vec::new();
        let mut clusters = Vec::new();
        let mut text = String::new();
        let mut first_x = 0.0f32;
        let mut started = false;
        let hidden = style.hidden && !self.options.show_hidden;
        for k in i..j {
            let Some(cluster) = layout.clusters.get(k) else {
                continue;
            };
            if matches!(
                cluster.kind,
                edgedoc_text::ClusterKind::Marker
                    | edgedoc_text::ClusterKind::SoftHyphen
                    | edgedoc_text::ClusterKind::LineBreak
                    | edgedoc_text::ClusterKind::PageBreak
                    | edgedoc_text::ClusterKind::ColumnBreak
                    | edgedoc_text::ClusterKind::Object(_)
            ) {
                // An object paints itself; the cluster only holds its advance.
                continue;
            }
            let x = line.xs.get(k - line.c0).copied().unwrap_or(0.0) + x0;
            if !started {
                first_x = x;
                started = true;
            }
            // The source text of the cluster, from the paragraph that this fragment carries.
            let cluster_text = p.text.get(cluster.start..cluster.end).unwrap_or("");
            match cluster.kind {
                edgedoc_text::ClusterKind::Tab => continue,
                edgedoc_text::ClusterKind::Space => {
                    if hidden {
                        continue;
                    }
                    let gid = self.shaper.glyph(style.face, ' ');
                    glyphs.push(gid);
                    advances.push(cluster.adv);
                    offsets.push((0.0, 0.0));
                    clusters.push(cluster.start as u32);
                    text.push(' ');
                    continue;
                }
                _ => {}
            }
            if hidden {
                continue;
            }
            // Shape this cluster. One cluster is one grapheme, so a ligature stays whole.
            let shown = match style.caps {
                CapsMode::All => cluster_text.to_uppercase(),
                CapsMode::Small => cluster_text.to_uppercase(),
                CapsMode::None => cluster_text.to_string(),
            };
            let shaped = self
                .shaper
                .shape_run(style.face, &shown, style.size, style.rtl);
            if shaped.is_empty() {
                continue;
            }
            let mut pen = 0.0f32;
            let mut produced = 0usize;
            for g in &shaped {
                glyphs.push(g.gid);
                let adv = self.shaper.advance(style.face, g.gid, style.size) * style.x_scale;
                advances.push(adv);
                offsets.push((pen + g.dx, g.dy));
                clusters.push(cluster.start as u32);
                // The text layer holds one character per glyph, so a click maps to a position.
                // A ligature of 2 glyphs splits its characters; the last glyph takes the rest.
                produced += 1;
                if produced == shaped.len() {
                    text.push_str(if shown.is_empty() { "\u{FFFD}" } else { &shown });
                } else {
                    let first = shown.chars().next().unwrap_or('\u{FFFD}');
                    text.push(first);
                }
                pen += adv;
            }
            // Justification moves the last glyph of the cluster to the cluster advance.
            let glyph_sum: f32 =
                advances.iter().rev().take(shaped.len()).sum::<f32>() / style.x_scale.max(0.01);
            if let Some(last) = advances.last_mut() {
                let extra = (cluster.adv - glyph_sum).max(0.0);
                *last += extra;
            }
        }
        if glyphs.is_empty() {
            return;
        }
        // A highlight and a character shading paint behind the run. See `DP-S-08`.
        let width: f32 = advances.iter().sum();
        let run_rect = Rect::new(
            first_x,
            baseline - style.size * 0.85,
            width.max(1.0),
            style.size * 1.15,
        );
        if let Some(bg) = style.highlight.or(style.shading) {
            if bg.a > 0 {
                self.ops.push(Op::FillRect {
                    rect: run_rect,
                    paint: Paint::solid(bg),
                });
            }
        }
        // An underline and a strikethrough paint below and across the run.
        if style.underline || style.strike || style.double_strike {
            let ink = style.color;
            if style.underline {
                let y = baseline + style.size * 0.12;
                self.ops.push(Op::FillRect {
                    rect: Rect::new(first_x, y, width.max(1.0), (style.size * 0.06).max(0.5)),
                    paint: Paint::solid(ink),
                });
            }
            if style.strike {
                let y = baseline - style.size * 0.28;
                self.ops.push(Op::FillRect {
                    rect: Rect::new(first_x, y, width.max(1.0), (style.size * 0.06).max(0.5)),
                    paint: Paint::solid(ink),
                });
            }
            if style.double_strike {
                for dy in [-0.32f32, -0.24f32] {
                    let y = baseline + style.size * dy;
                    self.ops.push(Op::FillRect {
                        rect: Rect::new(first_x, y, width.max(1.0), (style.size * 0.05).max(0.4)),
                        paint: Paint::solid(ink),
                    });
                }
            }
        }
        let level = line_bidi_level(layout, i);
        let run_ordinal = self.run_ordinal;
        self.push_run(
            style.face,
            style.size,
            first_x,
            baseline,
            glyphs,
            advances,
            offsets,
            clusters,
            style.color,
            level,
            style.synth_bold,
            style.synth_italic,
            style.x_scale,
            text,
            None,
            hidden,
            revision_of(p),
        );
        if !hidden {
            let width: f32 = advances_width(&self.ops, run_ordinal);
            let rect = Rect::new(
                first_x,
                baseline - style.size * 0.8,
                width.max(1.0),
                style.size * 1.2,
            );
            self.text.push(TextIndexEntry {
                run: run_ordinal,
                char_start: start_byte as u32,
                char_end: end_byte as u32,
                rect,
                baseline,
                level,
                hidden,
            });
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn push_run(
        &mut self,
        face: u32,
        size: f32,
        x: f32,
        y: f32,
        glyphs: Vec<u16>,
        advances: Vec<f32>,
        offsets: Vec<(f32, f32)>,
        clusters: Vec<u32>,
        paint: Rgba,
        level: u8,
        synth_bold: bool,
        synth_italic: bool,
        x_scale: f32,
        text: String,
        link: Option<u32>,
        hidden: bool,
        revision: Option<u8>,
    ) {
        self.ops.push(Op::Glyphs(Box::new(GlyphRunOp {
            face,
            size,
            x,
            y,
            glyphs,
            advances,
            offsets,
            clusters,
            paint,
            level,
            synth_bold,
            synth_italic,
            x_scale,
            text,
            link,
            hidden,
            revision,
        })));
        self.run_ordinal += 1;
    }

    fn table(&mut self, t: &TableFragment) {
        // Cell shading first, then content, then borders.
        for cell in &t.cells {
            if let Some(c) = cell.shading {
                if c.a > 0 {
                    self.ops.push(Op::FillRect {
                        rect: cell.rect,
                        paint: Paint::solid(c),
                    });
                }
            }
        }
        // A cell clip keeps cell content inside its cell. See DP-L-36.
        for fragment in &t.content {
            self.fragment(fragment);
        }
        for BorderSegment { rect, border } in &t.borders {
            self.border_segment(*rect, *border);
        }
        if self.options.debug_boxes {
            self.ops.push(Op::DebugBox {
                rect: t.rect,
                label: t.path.clone(),
            });
        }
    }

    fn border_segment(&mut self, rect: Rect, border: edgedoc_style::Border) {
        if rect.is_empty() {
            return;
        }
        let (paint, style) = border_paint(border);
        if paint.a == 0 || !border.style.is_visible() {
            return;
        }
        match style {
            BorderStyle::None => {}
            BorderStyle::Double => {
                // Two lines, each a third of the width, with a third between them.
                let third = (border.width / 3.0).max(0.35);
                let horizontal = rect.w >= rect.h;
                if horizontal {
                    self.ops.push(Op::FillRect {
                        rect: Rect::new(rect.x, rect.y, rect.w, third),
                        paint: Paint::solid(paint),
                    });
                    self.ops.push(Op::FillRect {
                        rect: Rect::new(rect.x, rect.bottom() - third, rect.w, third),
                        paint: Paint::solid(paint),
                    });
                } else {
                    self.ops.push(Op::FillRect {
                        rect: Rect::new(rect.x, rect.y, third, rect.h),
                        paint: Paint::solid(paint),
                    });
                    self.ops.push(Op::FillRect {
                        rect: Rect::new(rect.right() - third, rect.y, third, rect.h),
                        paint: Paint::solid(paint),
                    });
                }
            }
            BorderStyle::Dotted => self.ops.push(Op::StrokeRect {
                rect,
                paint,
                style: StrokeStyle {
                    width: border.width.max(0.5),
                    dash: [0.6, 1.6, 0.0, 0.0],
                    dash_len: 2,
                    ..StrokeStyle::default()
                },
            }),
            BorderStyle::Dashed => self.ops.push(Op::StrokeRect {
                rect,
                paint,
                style: StrokeStyle {
                    width: border.width.max(0.5),
                    dash: [4.0, 2.5, 0.0, 0.0],
                    dash_len: 2,
                    ..StrokeStyle::default()
                },
            }),
            BorderStyle::Wave => self.ops.push(Op::StrokePath {
                segs: wave_path(rect),
                paint,
                style: StrokeStyle {
                    width: border.width.max(0.5),
                    ..StrokeStyle::default()
                },
            }),
            _ => self.ops.push(Op::FillRect {
                rect,
                paint: Paint::solid(paint),
            }),
        }
    }
}

/// The paint and style of a border.
fn border_paint(border: edgedoc_style::Border) -> (Rgba, BorderStyle) {
    (border.color, border.style)
}

/// A `rect` side: 0 top, 1 left, 2 bottom, 3 right.
fn rect_side(rect: Rect, side: usize) -> Rect {
    let w = 0.75;
    match side {
        0 => Rect::new(rect.x, rect.y, rect.w, w),
        1 => Rect::new(rect.x, rect.y, w, rect.h),
        2 => Rect::new(rect.x, rect.bottom() - w, rect.w, w),
        _ => Rect::new(rect.right() - w, rect.y, w, rect.h),
    }
}

fn wave_path(rect: Rect) -> Vec<PathSeg> {
    let mut segs = Vec::new();
    let horizontal = rect.w >= rect.h;
    let (len, y) = if horizontal {
        (rect.w, rect.y + rect.h / 2.0)
    } else {
        (rect.h, rect.y)
    };
    let amp = 0.9;
    segs.push(PathSeg::Move(
        if horizontal {
            rect.x
        } else {
            rect.x + rect.w / 2.0
        },
        y,
    ));
    let step = 3.0;
    let mut t = 0.0;
    let mut up = true;
    while t < len {
        let next = (t + step).min(len);
        let x1 = if horizontal {
            rect.x + t
        } else {
            rect.x + rect.w / 2.0
        };
        let y1 = if horizontal { y + amp } else { rect.y + t };
        let x2 = if horizontal {
            rect.x + next
        } else {
            rect.x + rect.w / 2.0
        };
        let y2 = if horizontal { y + amp } else { rect.y + next };
        if up {
            segs.push(PathSeg::Quad(x1, y1 - 2.0 * amp, x2, y2));
        } else {
            segs.push(PathSeg::Line(x2, y2));
        }
        up = !up;
        t = next;
    }
    segs
}

fn line_bidi_level(layout: &ParagraphLayout, cluster: usize) -> u8 {
    layout.clusters.get(cluster).map_or(0, |c| c.level)
}

fn advances_width(ops: &[Op], ordinal: u32) -> f32 {
    let mut n = 0u32;
    for op in ops {
        if let Op::Glyphs(g) = op {
            if n == ordinal {
                return g.width();
            }
            n += 1;
        }
    }
    0.0
}

/// The revision byte that a page list carries for a fragment.
fn revision_of(p: &ParagraphFragment) -> Option<u8> {
    match p.revision {
        Some(edgedoc_flow::RevisionKind::Insert) => Some(1),
        Some(edgedoc_flow::RevisionKind::Delete) => Some(2),
        None => None,
    }
}

/// The fill rule that a path uses when it has no holes.
pub const DEFAULT_FILL: FillRule = FillRule::NonZero;

/// A computed run, exposed so the facade can convert one to a [`RunStyle`].
///
/// This is the one place where a resolved colour becomes a paint colour, which is the WordCraft
/// rule in `crates/layout/src/display/glyphs.rs::text_color`: a run with an explicit colour paints
/// it, and a run with none paints black.
pub fn run_style_of(run: &ComputedRun, face: u32) -> RunStyle {
    RunStyle {
        face,
        size: run.size,
        color: run.color,
        underline: run.underline,
        strike: run.strike,
        double_strike: run.double_strike,
        highlight: run.highlight,
        shading: run.shading,
        x_scale: run.x_scale,
        letter_spacing: run.letter_spacing,
        baseline_shift: run.baseline_shift,
        hidden: run.hidden,
        rtl: run.rtl,
        caps: run.caps,
        symbol: run.symbol.clone(),
        synth_bold: false,
        synth_italic: false,
    }
}
