//! The display list, its binary codec, and the painters.
//!
//! The flow crate produces page geometry. This crate lowers it to a flat list of paint
//! operations in page coordinates, encodes that list to bytes, and paints the bytes on a canvas,
//! an SVG tree, a PDF page, or a recorder. See `DP-G-01` to `DP-G-52`.

#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![deny(clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod codec;
pub mod lower;
pub mod painter;
pub mod svg;

pub use codec::{decode, encode};
pub use lower::{lower_page, LinkTarget, TextIndexEntry};
pub use painter::{Canvas2d, CanvasPainter, PdfPage, PdfPainter, RecordPainter, RecordedOp};

use edgedoc_geom::{Rect, Rgba};

/// The format version. A new operation changes it. See `DP-G-02` and `DP-G-16`.
pub const FORMAT_VERSION: u16 = 1;

/// The budget for one page display list, in bytes. See `DP-G-17`.
pub const MAX_PAGE_BYTES: usize = 2 << 20;

/// A point with a direction, for a path.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PathSeg {
    Move(f32, f32),
    Line(f32, f32),
    Quad(f32, f32, f32, f32),
    Cubic(f32, f32, f32, f32, f32, f32),
    Close,
}

/// A fill rule for a path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FillRule {
    #[default]
    NonZero,
    EvenOdd,
}

/// A line cap.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineCap {
    #[default]
    Butt,
    Round,
    Square,
}

/// A line join.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineJoin {
    #[default]
    Miter,
    Round,
    Bevel,
}

/// The style of a stroke.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct StrokeStyle {
    pub width: f32,
    pub cap: LineCap,
    pub join: LineJoin,
    /// Alternating dash and gap lengths, in points. An empty vector means a solid line.
    pub dash: [f32; 4],
    pub dash_len: u8,
}

/// A paint.
#[derive(Debug, Clone, PartialEq)]
pub enum Paint {
    Solid(Rgba),
    /// A linear gradient from one point to another.
    Linear {
        from: (f32, f32),
        to: (f32, f32),
        stops: Vec<(f32, Rgba)>,
    },
}

impl Paint {
    pub fn solid(c: Rgba) -> Self {
        Paint::Solid(c)
    }

    pub fn alpha(&self) -> u8 {
        match self {
            Paint::Solid(c) => c.a,
            Paint::Linear { stops, .. } => stops.iter().map(|(_, c)| c.a).max().unwrap_or(255),
        }
    }

    pub fn is_visible(&self) -> bool {
        self.alpha() > 0
    }
}

/// An affine transform, in the order `a b c d e f`: `x' = a x + c y + e`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Affine(pub [f32; 6]);

impl Default for Affine {
    fn default() -> Self {
        Affine([1.0, 0.0, 0.0, 1.0, 0.0, 0.0])
    }
}

impl Affine {
    pub fn identity() -> Self {
        Affine::default()
    }

    pub fn translate(x: f32, y: f32) -> Self {
        Affine([1.0, 0.0, 0.0, 1.0, x, y])
    }

    pub fn scale(s: f32) -> Self {
        Affine([s, 0.0, 0.0, s, 0.0, 0.0])
    }

    /// A rotation about a point, in degrees, with `y` down. A positive angle turns clockwise on
    /// the page.
    pub fn rotate_about(deg: f32, cx: f32, cy: f32) -> Self {
        let r = deg.to_radians();
        let (s, c) = (r.sin(), r.cos());
        Affine([c, s, -s, c, cx - c * cx + s * cy, cy - s * cx - c * cy])
    }

    /// A shear for synthetic italic, in degrees.
    pub fn shear(deg: f32, baseline: f32) -> Self {
        let t = deg.to_radians().tan();
        Affine([1.0, 0.0, -t, 1.0, t * baseline, 0.0])
    }

    pub fn apply(&self, x: f32, y: f32) -> (f32, f32) {
        let a = self.0;
        (a[0] * x + a[2] * y + a[4], a[1] * x + a[3] * y + a[5])
    }

    /// The product `self * other`, which applies `other` first.
    pub fn then(&self, other: &Affine) -> Affine {
        let a = self.0;
        let b = other.0;
        Affine([
            b[0] * a[0] + b[2] * a[1],
            b[1] * a[0] + b[3] * a[1],
            b[0] * a[2] + b[2] * a[3],
            b[1] * a[2] + b[3] * a[3],
            b[0] * a[4] + b[2] * a[5] + b[4],
            b[1] * a[4] + b[3] * a[5] + b[5],
        ])
    }
}

/// One text run, ready to paint.
#[derive(Debug, Clone, PartialEq)]
pub struct GlyphRunOp {
    pub face: u32,
    pub size: f32,
    /// The pen position of the first glyph, in page coordinates.
    pub x: f32,
    pub y: f32,
    pub glyphs: Vec<u16>,
    /// The advance of each glyph, in points.
    pub advances: Vec<f32>,
    /// The x and y offset of each glyph from its pen, in points.
    pub offsets: Vec<(f32, f32)>,
    /// The source character range of each glyph, as byte offsets.
    pub clusters: Vec<u32>,
    pub paint: Rgba,
    /// The bidi level. An odd level means the run reads right to left.
    pub level: u8,
    pub synth_bold: bool,
    pub synth_italic: bool,
    /// The horizontal scale from `w:w`, which stretches each glyph across from its pen.
    pub x_scale: f32,
    /// The text the run draws, which the text layer and a PDF need.
    pub text: String,
    /// The hyperlink target, when the run is a link.
    pub link: Option<u32>,
    /// True when the run is hidden text, which the layer omits.
    pub hidden: bool,
    /// The revision mark, for the layer state.
    pub revision: Option<u8>,
}

impl GlyphRunOp {
    pub fn width(&self) -> f32 {
        self.advances.iter().sum()
    }
}

/// A rectangle a link covers.
#[derive(Debug, Clone, PartialEq)]
pub struct LinkOp {
    pub rect: Rect,
    /// The index into the target table.
    pub target: u32,
}

/// One operation in a display list.
#[derive(Debug, Clone, PartialEq)]
pub enum Op {
    BeginPage {
        w: f32,
        h: f32,
        background: Rgba,
    },
    EndPage,
    PushClip(Rect),
    PopClip,
    PushTransform(Affine),
    PopTransform,
    FillRect {
        rect: Rect,
        paint: Paint,
    },
    StrokeRect {
        rect: Rect,
        paint: Rgba,
        style: StrokeStyle,
    },
    FillPath {
        segs: Vec<PathSeg>,
        paint: Paint,
        rule: FillRule,
    },
    StrokePath {
        segs: Vec<PathSeg>,
        paint: Rgba,
        style: StrokeStyle,
    },
    Glyphs(Box<GlyphRunOp>),
    Image {
        resource: u32,
        rect: Rect,
        /// The source rectangle in normalized units, `(x, y, w, h)`.
        src: [f32; 4],
        opacity: f32,
        interpolate: bool,
    },
    Link(LinkOp),
    NamedDest {
        name: String,
        x: f32,
        y: f32,
    },
    Placeholder {
        rect: Rect,
        kind: String,
        label: String,
    },
    DebugBox {
        rect: Rect,
        label: String,
    },
}

/// One encoded page: the operations plus the tables that a viewer needs.
#[derive(Debug, Clone, PartialEq)]
pub struct DisplayPage {
    pub index: u32,
    pub w: f32,
    pub h: f32,
    /// The link targets, by index. An `Op::Link` names one.
    pub targets: Vec<String>,
    /// The text index, which selection and search read. See `DP-G-13`.
    pub text: Vec<lower::TextIndexEntry>,
    pub ops: Vec<Op>,
    /// The named destinations, by name.
    pub destinations: Vec<(String, (f32, f32))>,
}

impl DisplayPage {
    /// The byte length of the encoded page.
    pub fn encoded_len(&self) -> usize {
        codec::encode(self).len()
    }

    /// The total advance of a glyph run in the list, by its ordinal. See `DP-G-13`.
    pub fn run_width(&self, ordinal: u32) -> f32 {
        let mut n = 0u32;
        for op in &self.ops {
            if let Op::Glyphs(g) = op {
                if n == ordinal {
                    return g.width();
                }
                n += 1;
            }
        }
        0.0
    }
}

/// The operation tag. The codec and a test both read this table.
pub fn op_tag(op: &Op) -> u8 {
    match op {
        Op::BeginPage { .. } => 0x01,
        Op::EndPage => 0x02,
        Op::PushClip(_) => 0x03,
        Op::PopClip => 0x04,
        Op::PushTransform(_) => 0x05,
        Op::PopTransform => 0x06,
        Op::FillRect { .. } => 0x07,
        Op::StrokeRect { .. } => 0x08,
        Op::FillPath { .. } => 0x09,
        Op::StrokePath { .. } => 0x0A,
        Op::Glyphs(_) => 0x0B,
        Op::Image { .. } => 0x0C,
        Op::Link(_) => 0x0D,
        Op::NamedDest { .. } => 0x0E,
        Op::Placeholder { .. } => 0x13,
        Op::DebugBox { .. } => 0x14,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_affine_translation_moves_a_point() {
        let a = Affine::translate(10.0, 20.0);
        assert_eq!(a.apply(1.0, 1.0), (11.0, 21.0));
    }

    #[test]
    fn a_rotation_about_a_point_holds_that_point() {
        let a = Affine::rotate_about(90.0, 50.0, 50.0);
        let (x, y) = a.apply(50.0, 50.0);
        assert!((x - 50.0).abs() < 0.01 && (y - 50.0).abs() < 0.01);
    }

    #[test]
    fn a_transparent_paint_is_not_visible() {
        assert!(!Paint::solid(Rgba::new(0, 0, 0, 0)).is_visible());
        assert!(Paint::solid(Rgba::BLACK).is_visible());
    }

    #[test]
    fn op_tags_are_unique() {
        let ops = [
            Op::BeginPage {
                w: 1.0,
                h: 1.0,
                background: Rgba::WHITE,
            },
            Op::EndPage,
            Op::PushClip(Rect::default()),
            Op::PopClip,
            Op::PushTransform(Affine::default()),
            Op::PopTransform,
            Op::FillRect {
                rect: Rect::default(),
                paint: Paint::solid(Rgba::BLACK),
            },
            Op::StrokeRect {
                rect: Rect::default(),
                paint: Rgba::BLACK,
                style: StrokeStyle::default(),
            },
            Op::FillPath {
                segs: Vec::new(),
                paint: Paint::solid(Rgba::BLACK),
                rule: FillRule::NonZero,
            },
            Op::StrokePath {
                segs: Vec::new(),
                paint: Rgba::BLACK,
                style: StrokeStyle::default(),
            },
            Op::Glyphs(Box::new(GlyphRunOp {
                face: 0,
                size: 10.0,
                x: 0.0,
                y: 0.0,
                glyphs: Vec::new(),
                advances: Vec::new(),
                offsets: Vec::new(),
                clusters: Vec::new(),
                paint: Rgba::BLACK,
                level: 0,
                synth_bold: false,
                synth_italic: false,
                x_scale: 1.0,
                text: String::new(),
                link: None,
                hidden: false,
                revision: None,
            })),
            Op::Image {
                resource: 0,
                rect: Rect::default(),
                src: [0.0; 4],
                opacity: 1.0,
                interpolate: true,
            },
            Op::Link(LinkOp {
                rect: Rect::default(),
                target: 0,
            }),
            Op::NamedDest {
                name: String::new(),
                x: 0.0,
                y: 0.0,
            },
            Op::Placeholder {
                rect: Rect::default(),
                kind: String::new(),
                label: String::new(),
            },
            Op::DebugBox {
                rect: Rect::default(),
                label: String::new(),
            },
        ];
        let mut tags: Vec<u8> = ops.iter().map(op_tag).collect();
        let before = tags.len();
        tags.sort_unstable();
        tags.dedup();
        assert_eq!(tags.len(), before, "every operation has its own tag");
    }
}
