//! Geometry and units for the display library.
//!
//! Layout works in **points** (1/72 inch, `f32`). A package stores twips, EMU, and half-points.
//! The conversions live here, so every crate agrees on them. See `DP-A-17` and `DP-A-18`.

#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![deny(clippy::unimplemented, clippy::todo, clippy::unreachable)]

/// Twips (1/20 pt) per point.
pub const TWIPS_PER_PT: f32 = 20.0;
/// English Metric Units per point.
pub const EMU_PER_PT: f32 = 12_700.0;
/// Points per inch.
pub const PT_PER_IN: f32 = 72.0;

/// Twips to points.
pub fn twips(v: i32) -> f32 {
    v as f32 / TWIPS_PER_PT
}

/// Points to twips, rounded.
pub fn to_twips(pt: f32) -> i32 {
    if pt.is_finite() {
        (pt * TWIPS_PER_PT).round() as i32
    } else {
        0
    }
}

/// EMU to points.
pub fn emu(v: i64) -> f32 {
    v as f32 / EMU_PER_PT
}

/// Points to EMU, rounded.
pub fn to_emu(pt: f32) -> i64 {
    if pt.is_finite() {
        (f64::from(pt) * f64::from(EMU_PER_PT)).round() as i64
    } else {
        0
    }
}

/// Half-points (`w:sz`) to points.
pub fn half_points(v: i32) -> f32 {
    v as f32 / 2.0
}

/// Eighths of a point (`w:sz` of a border) to points.
pub fn eighth_points(v: i32) -> f32 {
    v as f32 / 8.0
}

/// A point on a page, in points, with the origin at the top left and `y` growing downward.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

/// A rectangle in points.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    /// A rectangle from two opposite corners, in any order.
    pub fn from_edges(left: f32, top: f32, right: f32, bottom: f32) -> Self {
        Self {
            x: left.min(right),
            y: top.min(bottom),
            w: (right - left).abs(),
            h: (bottom - top).abs(),
        }
    }

    pub fn right(&self) -> f32 {
        self.x + self.w
    }

    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }

    pub fn center(&self) -> Point {
        Point::new(self.x + self.w / 2.0, self.y + self.h / 2.0)
    }

    pub fn is_empty(&self) -> bool {
        !(self.w > 0.0 && self.h > 0.0)
    }

    pub fn contains(&self, p: Point) -> bool {
        p.x >= self.x && p.x <= self.right() && p.y >= self.y && p.y <= self.bottom()
    }

    pub fn intersects(&self, other: &Rect) -> bool {
        self.x < other.right()
            && other.x < self.right()
            && self.y < other.bottom()
            && other.y < self.bottom()
    }

    /// The overlap, or an empty rectangle at the origin.
    pub fn intersection(&self, other: &Rect) -> Rect {
        let left = self.x.max(other.x);
        let top = self.y.max(other.y);
        let right = self.right().min(other.right());
        let bottom = self.bottom().min(other.bottom());
        if right <= left || bottom <= top {
            return Rect::default();
        }
        Rect::new(left, top, right - left, bottom - top)
    }

    /// The smallest rectangle that holds both.
    pub fn union(&self, other: &Rect) -> Rect {
        if self.is_empty() {
            return *other;
        }
        if other.is_empty() {
            return *self;
        }
        Rect::from_edges(
            self.x.min(other.x),
            self.y.min(other.y),
            self.right().max(other.right()),
            self.bottom().max(other.bottom()),
        )
    }

    /// Inset on every side by `d`.
    pub fn inset(&self, d: f32) -> Rect {
        Rect::new(
            self.x + d,
            self.y + d,
            (self.w - 2.0 * d).max(0.0),
            (self.h - 2.0 * d).max(0.0),
        )
    }

    /// Move by an offset.
    pub fn translate(&self, dx: f32, dy: f32) -> Rect {
        Rect::new(self.x + dx, self.y + dy, self.w, self.h)
    }
}

/// An sRGB colour with an alpha byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba {
    pub const BLACK: Rgba = Rgba {
        r: 0,
        g: 0,
        b: 0,
        a: 255,
    };
    pub const WHITE: Rgba = Rgba {
        r: 255,
        g: 255,
        b: 255,
        a: 255,
    };

    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    pub const fn opaque(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    /// A colour from six hex digits, or `None` when the text is not a colour.
    pub fn parse_hex(text: &str) -> Option<Self> {
        let t = text.trim().trim_start_matches('#');
        if t.len() != 6 {
            return None;
        }
        let r = u8::from_str_radix(&t[0..2], 16).ok()?;
        let g = u8::from_str_radix(&t[2..4], 16).ok()?;
        let b = u8::from_str_radix(&t[4..6], 16).ok()?;
        Some(Self::opaque(r, g, b))
    }

    /// An `0xRRGGBBAA` word, which is the display-list form.
    pub fn to_word(self) -> u32 {
        (u32::from(self.r) << 24)
            | (u32::from(self.g) << 16)
            | (u32::from(self.b) << 8)
            | u32::from(self.a)
    }

    /// The CSS form `#rrggbb`, or `rgba(...)` when the alpha is not opaque.
    pub fn to_css(self) -> String {
        if self.a == 255 {
            format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
        } else {
            format!(
                "rgba({},{},{},{:.3})",
                self.r,
                self.g,
                self.b,
                f32::from(self.a) / 255.0
            )
        }
    }
}

/// A page size in points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageSize {
    pub w: f32,
    pub h: f32,
}

impl PageSize {
    pub fn new(w: f32, h: f32) -> Self {
        Self { w, h }
    }

    /// A4, 210 by 297 mm.
    pub fn a4() -> Self {
        Self::new(595.28, 841.89)
    }

    /// US Letter, 8.5 by 11 inch.
    pub fn letter() -> Self {
        Self::new(612.0, 792.0)
    }

    pub fn landscape(self) -> Self {
        Self::new(self.h, self.w)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn twips_and_points_round_trip() {
        assert!((twips(1440) - 72.0).abs() < 0.001);
        assert_eq!(to_twips(72.0), 1440);
    }

    #[test]
    fn emu_and_points_round_trip() {
        assert!((emu(914_400) - 72.0).abs() < 0.001);
        assert_eq!(to_emu(72.0), 914_400);
    }

    #[test]
    fn a_non_finite_length_becomes_zero() {
        assert_eq!(to_twips(f32::NAN), 0);
        assert_eq!(to_emu(f32::INFINITY), 0);
    }

    #[test]
    fn rect_intersection_is_empty_when_disjoint() {
        let a = Rect::new(0.0, 0.0, 10.0, 10.0);
        let b = Rect::new(20.0, 20.0, 5.0, 5.0);
        assert!(a.intersection(&b).is_empty());
        assert!(a.union(&b).contains(Point::new(24.0, 24.0)));
    }

    #[test]
    fn hex_colour_parses_and_rejects() {
        assert_eq!(
            Rgba::parse_hex("#1F4E79"),
            Some(Rgba::opaque(0x1F, 0x4E, 0x79))
        );
        assert_eq!(Rgba::parse_hex("auto"), None);
        assert_eq!(Rgba::parse_hex("#12345"), None);
    }
}
