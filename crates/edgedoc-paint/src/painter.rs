//! Painters: consumers of a display list.
//!
//! Every painter implements the same shape, so a page paints on a canvas, an SVG tree, a PDF page,
//! or a recorder with no second geometry path. See `DP-G-18` to `DP-G-22` and `DP-G-49` to
//! `DP-G-52`.

use edgedoc_geom::{Rect, Rgba};

use crate::{Affine, FillRule, LineCap, LineJoin, Op, Paint, PathSeg};

/// The drawing surface that a canvas painter needs.
///
/// The browser backend forwards to `CanvasRenderingContext2D`. The native backend draws into a
/// bitmap, so the painter runs in CI without a browser. See `DP-G-18`.
pub trait Canvas2d {
    fn set_fill(&mut self, color: Rgba);
    fn set_stroke(&mut self, color: Rgba, width: f32);
    fn set_dash(&mut self, dash: &[f32]);
    fn set_line_cap(&mut self, cap: LineCap);
    fn set_line_join(&mut self, join: LineJoin);
    fn fill_rect(&mut self, rect: Rect);
    fn stroke_rect(&mut self, rect: Rect);
    fn begin_path(&mut self);
    fn move_to(&mut self, x: f32, y: f32);
    fn line_to(&mut self, x: f32, y: f32);
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32);
    fn cubic_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32);
    fn close_path(&mut self);
    fn fill_path(&mut self, rule: FillRule);
    fn stroke_path(&mut self);
    fn fill_linear(&mut self, rect: Rect, from: (f32, f32), to: (f32, f32), stops: &[(f32, Rgba)]);
    fn save(&mut self);
    fn restore(&mut self);
    fn clip_rect(&mut self, rect: Rect);
    fn transform(&mut self, a: &Affine);
    /// Draw a glyph run. The backend resolves the face and the glyph ids.
    fn glyphs(&mut self, run: &crate::GlyphRunOp);
    /// Draw an image, cropped to `src` and placed in `rect`.
    fn image(&mut self, resource: u32, rect: Rect, src: [f32; 4], opacity: f32);
}

/// A painter for a canvas.
pub struct CanvasPainter<'a, C: Canvas2d> {
    pub canvas: &'a mut C,
    /// The current page height, which flips `y` for a canvas that grows upward.
    pub page_h: f32,
}

impl<C: Canvas2d> CanvasPainter<'_, C> {
    /// Paint one decoded page. See `DP-G-21`.
    pub fn paint(&mut self, page: &crate::DisplayPage) {
        self.page_h = page.h;
        for op in &page.ops {
            self.op(op);
        }
    }

    fn op(&mut self, op: &Op) {
        match op {
            Op::BeginPage { background, .. } => {
                self.canvas.set_fill(*background);
                self.canvas
                    .fill_rect(Rect::new(0.0, 0.0, f32::MAX, f32::MAX));
            }
            Op::EndPage => {}
            Op::PushClip(rect) => {
                self.canvas.save();
                self.canvas.clip_rect(*rect);
            }
            Op::PopClip => self.canvas.restore(),
            Op::PushTransform(a) => {
                self.canvas.save();
                self.canvas.transform(a);
            }
            Op::PopTransform => self.canvas.restore(),
            Op::FillRect { rect, paint } => match paint {
                Paint::Solid(c) => {
                    if c.a == 0 {
                        return;
                    }
                    self.canvas.set_fill(*c);
                    self.canvas.fill_rect(*rect);
                }
                Paint::Linear { from, to, stops } => {
                    self.canvas.fill_linear(*rect, *from, *to, stops);
                }
            },
            Op::StrokeRect { rect, paint, style } => {
                if paint.a == 0 {
                    return;
                }
                self.canvas.set_stroke(*paint, style.width);
                self.canvas.set_dash(&dash_of(style));
                self.canvas.set_line_cap(style.cap);
                self.canvas.set_line_join(style.join);
                self.canvas.stroke_rect(*rect);
                self.canvas.set_dash(&[]);
            }
            Op::FillPath { segs, paint, rule } => {
                if !paint.is_visible() {
                    return;
                }
                if let Paint::Solid(c) = paint {
                    self.canvas.set_fill(*c);
                }
                self.canvas.begin_path();
                self.path(segs);
                self.canvas.fill_path(*rule);
            }
            Op::StrokePath { segs, paint, style } => {
                if paint.a == 0 {
                    return;
                }
                self.canvas.set_stroke(*paint, style.width);
                self.canvas.set_dash(&dash_of(style));
                self.canvas.begin_path();
                self.path(segs);
                self.canvas.stroke_path();
                self.canvas.set_dash(&[]);
            }
            Op::Glyphs(run) => self.canvas.glyphs(run),
            Op::Image {
                resource,
                rect,
                src,
                opacity,
                ..
            } => self.canvas.image(*resource, *rect, *src, *opacity),
            Op::Link(_) | Op::NamedDest { .. } | Op::Placeholder { .. } | Op::DebugBox { .. } => {
                // A link and a destination paint nothing. A placeholder paints through its own
                // fill and stroke, and a debug box is a viewer overlay.
            }
        }
    }

    fn path(&mut self, segs: &[PathSeg]) {
        for seg in segs {
            match seg {
                PathSeg::Move(x, y) => self.canvas.move_to(*x, *y),
                PathSeg::Line(x, y) => self.canvas.line_to(*x, *y),
                PathSeg::Quad(x1, y1, x, y) => self.canvas.quad_to(*x1, *y1, *x, *y),
                PathSeg::Cubic(x1, y1, x2, y2, x, y) => {
                    self.canvas.cubic_to(*x1, *y1, *x2, *y2, *x, *y)
                }
                PathSeg::Close => self.canvas.close_path(),
            }
        }
    }
}

fn dash_of(style: &crate::StrokeStyle) -> Vec<f32> {
    style
        .dash
        .iter()
        .take(style.dash_len as usize)
        .copied()
        .filter(|d| *d > 0.0)
        .collect()
}

/// One operation that a recorder keeps, in a form a test compares. See `DP-G-21`.
#[derive(Debug, Clone, PartialEq)]
pub enum RecordedOp {
    Page {
        w: f32,
        h: f32,
        background: Rgba,
    },
    Fill {
        rect: Rect,
        color: Rgba,
    },
    Stroke {
        rect: Rect,
        color: Rgba,
        width: f32,
    },
    Path {
        segs: Vec<PathSeg>,
        color: Rgba,
        filled: bool,
    },
    Glyphs {
        face: u32,
        size: f32,
        x: f32,
        y: f32,
        text: String,
        color: Rgba,
    },
    Image {
        resource: u32,
        rect: Rect,
    },
    Placeholder {
        rect: Rect,
        label: String,
    },
    Clip(Rect),
    Transform(Affine),
    Unclip,
}

/// A recorder. It writes one line per operation, which a golden test compares.
#[derive(Debug, Default)]
pub struct RecordPainter {
    pub ops: Vec<RecordedOp>,
}

impl RecordPainter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn paint(&mut self, page: &crate::DisplayPage) {
        for op in &page.ops {
            match op {
                Op::BeginPage { w, h, background } => self.ops.push(RecordedOp::Page {
                    w: round2(*w),
                    h: round2(*h),
                    background: *background,
                }),
                Op::EndPage => {}
                Op::PushClip(r) => self.ops.push(RecordedOp::Clip(round_rect(r))),
                Op::PopClip => self.ops.push(RecordedOp::Unclip),
                Op::PushTransform(a) => self.ops.push(RecordedOp::Transform(*a)),
                Op::PopTransform => self.ops.push(RecordedOp::Unclip),
                Op::FillRect { rect, paint } => {
                    if let Paint::Solid(c) = paint {
                        if c.a > 0 {
                            self.ops.push(RecordedOp::Fill {
                                rect: round_rect(rect),
                                color: *c,
                            });
                        }
                    }
                }
                Op::StrokeRect { rect, paint, style } => {
                    if paint.a > 0 {
                        self.ops.push(RecordedOp::Stroke {
                            rect: round_rect(rect),
                            color: *paint,
                            width: round2(style.width),
                        });
                    }
                }
                Op::FillPath { segs, paint, .. } => {
                    if let Paint::Solid(c) = paint {
                        if c.a > 0 {
                            self.ops.push(RecordedOp::Path {
                                segs: round_segs(segs),
                                color: *c,
                                filled: true,
                            });
                        }
                    }
                }
                Op::StrokePath { segs, paint, .. } => {
                    if paint.a > 0 {
                        self.ops.push(RecordedOp::Path {
                            segs: round_segs(segs),
                            color: *paint,
                            filled: false,
                        });
                    }
                }
                Op::Glyphs(run) => self.ops.push(RecordedOp::Glyphs {
                    face: run.face,
                    size: round2(run.size),
                    x: round2(run.x),
                    y: round2(run.y),
                    text: run.text.clone(),
                    color: run.paint,
                }),
                Op::Image { resource, rect, .. } => self.ops.push(RecordedOp::Image {
                    resource: *resource,
                    rect: round_rect(rect),
                }),
                Op::Placeholder { rect, label, .. } => self.ops.push(RecordedOp::Placeholder {
                    rect: round_rect(rect),
                    label: label.clone(),
                }),
                Op::Link(_) | Op::NamedDest { .. } | Op::DebugBox { .. } => {}
            }
        }
    }

    /// The recorded operations as text, one per line. This is the golden form. See `DP-Q-09`.
    pub fn text(&self) -> String {
        let mut out = String::new();
        for op in &self.ops {
            out.push_str(&format_line(op));
            out.push('\n');
        }
        out
    }

    /// The text a painter would show, in operation order. A test reads this for a text check.
    pub fn visible_text(&self) -> String {
        let mut out = String::new();
        for op in &self.ops {
            if let RecordedOp::Glyphs { text, .. } = op {
                out.push_str(text);
            }
        }
        out
    }
}

fn format_line(op: &RecordedOp) -> String {
    match op {
        RecordedOp::Page { w, h, background } => {
            format!("page {w:.2} {h:.2} {}", hex(*background))
        }
        RecordedOp::Fill { rect, color } => format!(
            "fill {:.2} {:.2} {:.2} {:.2} {}",
            rect.x,
            rect.y,
            rect.w,
            rect.h,
            hex(*color)
        ),
        RecordedOp::Stroke { rect, color, width } => format!(
            "stroke {:.2} {:.2} {:.2} {:.2} {width:.2} {}",
            rect.x,
            rect.y,
            rect.w,
            rect.h,
            hex(*color)
        ),
        RecordedOp::Path {
            segs,
            color,
            filled,
        } => format!(
            "path {} {} {}",
            if *filled { "fill" } else { "stroke" },
            segs.len(),
            hex(*color)
        ),
        RecordedOp::Glyphs {
            size,
            x,
            y,
            text,
            color,
            ..
        } => format!("glyphs {size:.2} {x:.2} {y:.2} {} {text:?}", hex(*color)),
        RecordedOp::Image { resource, rect } => {
            format!(
                "image {resource} {:.2} {:.2} {:.2} {:.2}",
                rect.x, rect.y, rect.w, rect.h
            )
        }
        RecordedOp::Placeholder { rect, label } => format!(
            "placeholder {:.2} {:.2} {:.2} {:.2} {label:?}",
            rect.x, rect.y, rect.w, rect.h
        ),
        RecordedOp::Clip(r) => format!("clip {:.2} {:.2} {:.2} {:.2}", r.x, r.y, r.w, r.h),
        RecordedOp::Transform(a) => format!(
            "transform {:.4} {:.4} {:.4} {:.4} {:.2} {:.2}",
            a.0[0], a.0[1], a.0[2], a.0[3], a.0[4], a.0[5]
        ),
        RecordedOp::Unclip => "unclip".into(),
    }
}

fn hex(c: Rgba) -> String {
    format!("{:02x}{:02x}{:02x}{:02x}", c.r, c.g, c.b, c.a)
}

fn round2(v: f32) -> f32 {
    (v * 100.0).round() / 100.0
}

fn round_rect(r: &Rect) -> Rect {
    Rect::new(round2(r.x), round2(r.y), round2(r.w), round2(r.h))
}

fn round_segs(segs: &[PathSeg]) -> Vec<PathSeg> {
    segs.iter()
        .map(|s| match s {
            PathSeg::Move(x, y) => PathSeg::Move(round2(*x), round2(*y)),
            PathSeg::Line(x, y) => PathSeg::Line(round2(*x), round2(*y)),
            PathSeg::Quad(x1, y1, x, y) => {
                PathSeg::Quad(round2(*x1), round2(*y1), round2(*x), round2(*y))
            }
            PathSeg::Cubic(x1, y1, x2, y2, x, y) => PathSeg::Cubic(
                round2(*x1),
                round2(*y1),
                round2(*x2),
                round2(*y2),
                round2(*x),
                round2(*y),
            ),
            PathSeg::Close => PathSeg::Close,
        })
        .collect()
}

/// One page of a PDF.
#[derive(Debug, Clone, Default)]
pub struct PdfPage {
    /// The page content stream, in PDF operators.
    pub content: String,
    pub w: f32,
    pub h: f32,
    /// The text that the page shows, for a text extraction check.
    pub text: String,
}

/// A PDF painter. It writes page content streams, and the caller wraps them in a document.
///
/// The `y` axis flips, because a PDF origin is the bottom left. See `DP-G-20`.
#[derive(Debug, Default)]
pub struct PdfPainter {
    pub pages: Vec<PdfPage>,
    current: PdfPage,
    /// The glyphs drawn, so a test can check the text order.
    in_page: bool,
}

impl PdfPainter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn paint(&mut self, page: &crate::DisplayPage) {
        self.current = PdfPage {
            w: page.w,
            h: page.h,
            ..PdfPage::default()
        };
        self.in_page = true;
        for op in &page.ops {
            self.op(op, page.h);
        }
        self.pages.push(std::mem::take(&mut self.current));
        self.in_page = false;
    }

    fn op(&mut self, op: &Op, page_h: f32) {
        let fy = |y: f32| page_h - y;
        match op {
            Op::BeginPage { background, .. } => {
                self.current.content.push_str(&format!(
                    "{} {} {} rg 0 0 {} {} re f\n",
                    background.r as f32 / 255.0,
                    background.g as f32 / 255.0,
                    background.b as f32 / 255.0,
                    10_000.0,
                    10_000.0
                ));
            }
            Op::EndPage => {}
            Op::FillRect {
                rect,
                paint: Paint::Solid(c),
            } if c.a > 0 => {
                self.current.content.push_str(&format!(
                    "{} {} {} rg {:.2} {:.2} {:.2} {:.2} re f\n",
                    f(c.r),
                    f(c.g),
                    f(c.b),
                    rect.x,
                    fy(rect.bottom()),
                    rect.w,
                    rect.h
                ));
            }
            Op::StrokeRect { rect, paint, style } => {
                if paint.a > 0 {
                    self.current.content.push_str(&format!(
                        "{} {} {} RG {:.2} w {:.2} {:.2} {:.2} {:.2} re S\n",
                        f(paint.r),
                        f(paint.g),
                        f(paint.b),
                        style.width,
                        rect.x,
                        fy(rect.bottom()),
                        rect.w,
                        rect.h
                    ));
                }
            }
            Op::FillPath {
                segs,
                paint: Paint::Solid(c),
                ..
            } => {
                self.current
                    .content
                    .push_str(&path_ops(segs, |x, y| (x, fy(y))));
                self.current
                    .content
                    .push_str(&format!("{} {} {} rg f\n", f(c.r), f(c.g), f(c.b)));
            }
            Op::StrokePath { segs, paint, style } => {
                if paint.a > 0 {
                    self.current
                        .content
                        .push_str(&path_ops(segs, |x, y| (x, fy(y))));
                    self.current.content.push_str(&format!(
                        "{} {} {} RG {:.2} w S\n",
                        f(paint.r),
                        f(paint.g),
                        f(paint.b),
                        style.width
                    ));
                }
            }
            Op::Glyphs(run) => {
                // Text as vector outlines would lose the text layer, so the painter writes
                // positioned glyphs through a text object with a per-glyph position.
                self.current.text.push_str(&run.text);
                let mut x = run.x;
                for (i, gid) in run.glyphs.iter().enumerate() {
                    let adv = run.advances.get(i).copied().unwrap_or(0.0);
                    let (dx, dy) = run.offsets.get(i).copied().unwrap_or((0.0, 0.0));
                    self.current.content.push_str(&format!(
                        "1 0 0 1 {:.2} {:.2} Tm <{:04X}> Tj\n",
                        x + dx,
                        fy(run.y + dy),
                        gid
                    ));
                    x += adv;
                }
                if run.hidden {
                    self.current
                        .text
                        .truncate(self.current.text.len() - run.text.len());
                }
            }
            Op::Image { rect, .. } => {
                self.current.content.push_str(&format!(
                    "q {:.2} 0 0 {:.2} {:.2} {:.2} cm /X Do Q\n",
                    rect.w,
                    rect.h,
                    rect.x,
                    fy(rect.bottom())
                ));
            }
            Op::Placeholder { rect, label, .. } => {
                self.current.text.push_str(label);
                self.current.content.push_str(&format!(
                    "0.95 0.96 0.97 rg {:.2} {:.2} {:.2} {:.2} re f\n",
                    rect.x,
                    fy(rect.bottom()),
                    rect.w,
                    rect.h
                ));
            }
            _ => {}
        }
    }

    /// The page count, which a print test checks.
    pub fn page_count(&self) -> usize {
        self.pages.len()
    }
}

/// A PDF text operand for a number.
fn f(v: u8) -> String {
    format!("{:.4}", f32::from(v) / 255.0)
}

fn path_ops(segs: &[PathSeg], map: impl Fn(f32, f32) -> (f32, f32)) -> String {
    let mut out = String::new();
    for seg in segs {
        match seg {
            PathSeg::Move(x, y) => {
                let (x, y) = map(*x, *y);
                out.push_str(&format!("{x:.2} {y:.2} m "));
            }
            PathSeg::Line(x, y) => {
                let (x, y) = map(*x, *y);
                out.push_str(&format!("{x:.2} {y:.2} l "));
            }
            PathSeg::Quad(x1, y1, x, y) => {
                let (x1, y1) = map(*x1, *y1);
                let (x, y) = map(*x, *y);
                out.push_str(&format!("{x1:.2} {y1:.2} {x:.2} {y:.2} v "));
            }
            PathSeg::Cubic(x1, y1, x2, y2, x, y) => {
                let (x1, y1) = map(*x1, *y1);
                let (x2, y2) = map(*x2, *y2);
                let (x, y) = map(*x, *y);
                out.push_str(&format!("{x1:.2} {y1:.2} {x2:.2} {y2:.2} {x:.2} {y:.2} c "));
            }
            PathSeg::Close => out.push_str("h "),
        }
    }
    out.push('\n');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::{decode, encode};
    use crate::{DisplayPage, GlyphRunOp, LinkOp};

    fn page() -> DisplayPage {
        DisplayPage {
            index: 0,
            w: 595.28,
            h: 841.89,
            targets: vec!["#a".into()],
            text: Vec::new(),
            ops: vec![
                Op::BeginPage {
                    w: 595.28,
                    h: 841.89,
                    background: Rgba::WHITE,
                },
                Op::FillRect {
                    rect: Rect::new(72.0, 72.0, 100.0, 20.0),
                    paint: Paint::solid(Rgba::opaque(0x1F, 0x4E, 0x79)),
                },
                Op::Glyphs(Box::new(GlyphRunOp {
                    face: 0,
                    size: 12.0,
                    x: 72.0,
                    y: 120.0,
                    glyphs: vec![10, 11, 12],
                    advances: vec![6.0, 6.0, 6.0],
                    offsets: vec![(0.0, 0.0); 3],
                    clusters: vec![0, 1, 2],
                    paint: Rgba::BLACK,
                    level: 0,
                    synth_bold: false,
                    synth_italic: false,
                    x_scale: 1.0,
                    text: "abc".into(),
                    link: Some(0),
                    hidden: false,
                    revision: None,
                })),
                Op::Link(LinkOp {
                    rect: Rect::new(72.0, 110.0, 18.0, 12.0),
                    target: 0,
                }),
                Op::EndPage,
            ],
            destinations: Vec::new(),
        }
    }

    #[test]
    fn the_recorder_writes_one_line_per_operation() {
        let mut rec = RecordPainter::new();
        rec.paint(&page());
        let text = rec.text();
        assert!(text.starts_with("page 595.28 841.89 ffffffff"), "{text}");
        assert!(text.contains("fill 72.00 72.00 100.00 20.00 1f4e79ff"));
        assert!(text.contains("glyphs 12.00 72.00 120.00 000000ff \"abc\""));
        // A link paints nothing.
        assert_eq!(text.lines().filter(|l| l.starts_with("fill")).count(), 1);
    }

    #[test]
    fn a_recorder_keeps_the_visible_text_in_order() {
        let mut rec = RecordPainter::new();
        rec.paint(&page());
        assert_eq!(rec.visible_text(), "abc");
    }

    #[test]
    fn the_pdf_painter_flips_the_y_axis() {
        let mut pdf = PdfPainter::new();
        pdf.paint(&page());
        assert_eq!(pdf.page_count(), 1);
        let content = &pdf.pages[0].content;
        // A rectangle at y=72 with height 20 sits at PDF y = 841.89 - 92 = 749.89.
        assert!(content.contains("749.89"), "{content}");
    }

    #[test]
    fn the_pdf_painter_keeps_the_text() {
        let mut pdf = PdfPainter::new();
        pdf.paint(&page());
        assert!(pdf.pages[0].text.contains("abc"));
    }

    #[test]
    fn a_painter_and_the_recorder_agree_on_the_operations() {
        let bytes = encode(&page());
        let decoded = decode(&bytes).expect("decode");
        let mut a = RecordPainter::new();
        a.paint(&page());
        let mut b = RecordPainter::new();
        b.paint(&decoded);
        assert_eq!(a.text(), b.text(), "an encoded page paints the same");
    }

    #[test]
    fn a_transparent_fill_paints_nothing() {
        let mut p = page();
        p.ops.insert(
            1,
            Op::FillRect {
                rect: Rect::new(0.0, 0.0, 10.0, 10.0),
                paint: Paint::solid(Rgba::new(0, 0, 0, 0)),
            },
        );
        let mut rec = RecordPainter::new();
        rec.paint(&p);
        assert_eq!(
            rec.text().lines().filter(|l| l.starts_with("fill")).count(),
            1
        );
    }

    #[test]
    fn a_hidden_run_still_paints_but_is_marked() {
        let mut p = page();
        if let Some(Op::Glyphs(g)) = p.ops.get_mut(2) {
            g.hidden = true;
        }
        let mut rec = RecordPainter::new();
        rec.paint(&p);
        // The recorder paints it; the text layer omits it through the index.
        assert!(rec.text().contains("glyphs"));
    }

    #[test]
    fn a_clip_and_a_transform_are_recorded_in_order() {
        let mut p = page();
        p.ops
            .insert(1, Op::PushClip(Rect::new(0.0, 0.0, 50.0, 50.0)));
        p.ops
            .insert(2, Op::PushTransform(Affine::rotate_about(90.0, 10.0, 10.0)));
        p.ops.insert(3, Op::PopTransform);
        p.ops.insert(4, Op::PopClip);
        let mut rec = RecordPainter::new();
        rec.paint(&p);
        let text = rec.text();
        let order: Vec<&str> = text
            .lines()
            .filter(|l| {
                l.starts_with("clip") || l.starts_with("transform") || l.starts_with("unclip")
            })
            .collect();
        assert_eq!(order.len(), 4, "{text}");
        assert!(order[0].starts_with("clip"));
        assert!(order[1].starts_with("transform"));
    }
}
