//! The binary codec for a page display list.
//!
//! One page is one buffer. Coordinates are integers in 1/100 pt, so a round trip is exact within
//! 0.005 pt (`DP-G-11`). Strings are length-prefixed UTF-8. See `DP-G-10` to `DP-G-17`.

use edgedoc_geom::{Rect, Rgba};

use crate::{
    Affine, DisplayPage, FillRule, GlyphRunOp, LineCap, LineJoin, LinkOp, Op, Paint, PathSeg,
    StrokeStyle, FORMAT_VERSION,
};

/// The magic bytes at the start of a page buffer.
pub const MAGIC: [u8; 4] = *b"EDDL";

fn put_u16(out: &mut Vec<u8>, v: u16) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn put_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn put_f32(out: &mut Vec<u8>, v: f32) {
    let v = if v.is_finite() { v } else { 0.0 };
    out.extend_from_slice(&v.to_le_bytes());
}

/// A coordinate in 1/100 pt, as an `i32`.
fn put_coord(out: &mut Vec<u8>, v: f32) {
    let q = if v.is_finite() {
        (v * 100.0).round().clamp(-2_000_000_000.0, 2_000_000_000.0) as i32
    } else {
        0
    };
    out.extend_from_slice(&q.to_le_bytes());
}

fn put_str(out: &mut Vec<u8>, s: &str) {
    put_u32(out, s.len() as u32);
    out.extend_from_slice(s.as_bytes());
}

fn put_rect(out: &mut Vec<u8>, r: &Rect) {
    put_coord(out, r.x);
    put_coord(out, r.y);
    put_coord(out, r.w);
    put_coord(out, r.h);
}

fn put_rgba(out: &mut Vec<u8>, c: Rgba) {
    out.extend_from_slice(&[c.r, c.g, c.b, c.a]);
}

fn put_paint(out: &mut Vec<u8>, p: &Paint) {
    match p {
        Paint::Solid(c) => {
            out.push(0);
            put_rgba(out, *c);
        }
        Paint::Linear { from, to, stops } => {
            out.push(1);
            put_coord(out, from.0);
            put_coord(out, from.1);
            put_coord(out, to.0);
            put_coord(out, to.1);
            put_u16(out, stops.len().min(64) as u16);
            for (offset, c) in stops.iter().take(64) {
                put_f32(out, *offset);
                put_rgba(out, *c);
            }
        }
    }
}

fn put_style(out: &mut Vec<u8>, s: &StrokeStyle) {
    put_f32(out, s.width);
    out.push(s.cap as u8);
    out.push(s.join as u8);
    out.push(s.dash_len.min(4));
    for i in 0..s.dash_len.min(4) as usize {
        put_f32(out, s.dash.get(i).copied().unwrap_or(0.0));
    }
}

fn put_path(out: &mut Vec<u8>, segs: &[PathSeg]) {
    put_u32(out, segs.len() as u32);
    for seg in segs {
        match seg {
            PathSeg::Move(x, y) => {
                out.push(0);
                put_coord(out, *x);
                put_coord(out, *y);
            }
            PathSeg::Line(x, y) => {
                out.push(1);
                put_coord(out, *x);
                put_coord(out, *y);
            }
            PathSeg::Quad(x1, y1, x, y) => {
                out.push(2);
                put_coord(out, *x1);
                put_coord(out, *y1);
                put_coord(out, *x);
                put_coord(out, *y);
            }
            PathSeg::Cubic(x1, y1, x2, y2, x, y) => {
                out.push(3);
                put_coord(out, *x1);
                put_coord(out, *y1);
                put_coord(out, *x2);
                put_coord(out, *y2);
                put_coord(out, *x);
                put_coord(out, *y);
            }
            PathSeg::Close => out.push(4),
        }
    }
}

fn put_glyphs(out: &mut Vec<u8>, g: &GlyphRunOp) {
    put_u32(out, g.face);
    put_f32(out, g.size);
    put_coord(out, g.x);
    put_coord(out, g.y);
    put_u32(out, g.glyphs.len() as u32);
    for hid in &g.glyphs {
        put_u16(out, *hid);
    }
    for a in &g.advances {
        put_f32(out, *a);
    }
    for (dx, dy) in &g.offsets {
        put_f32(out, *dx);
        put_f32(out, *dy);
    }
    // Clusters are optional, which a run of plain left-to-right text does not need.
    if g.clusters.len() == g.glyphs.len() {
        out.push(1);
        for c in &g.clusters {
            put_u32(out, *c);
        }
    } else {
        out.push(0);
    }
    put_rgba(out, g.paint);
    out.push(g.level);
    out.push(u8::from(g.synth_bold) | (u8::from(g.synth_italic) << 1));
    put_f32(out, g.x_scale);
    put_str(out, &g.text);
    match g.link {
        Some(t) => {
            out.push(1);
            put_u32(out, t);
        }
        None => out.push(0),
    }
    out.push(u8::from(g.hidden));
    out.push(g.revision.unwrap_or(255));
}

/// Encode one page. See `DP-G-10`.
pub fn encode(page: &DisplayPage) -> Vec<u8> {
    let mut body: Vec<u8> = Vec::with_capacity(4096);
    // Body: the operations.
    for op in &page.ops {
        body.push(crate::op_tag(op));
        match op {
            Op::BeginPage { w, h, background } => {
                put_coord(&mut body, *w);
                put_coord(&mut body, *h);
                put_rgba(&mut body, *background);
            }
            Op::EndPage | Op::PopClip | Op::PopTransform => {}
            Op::PushClip(r) => put_rect(&mut body, r),
            Op::PushTransform(a) => {
                for v in a.0 {
                    put_f32(&mut body, v);
                }
            }
            Op::FillRect { rect, paint } => {
                put_rect(&mut body, rect);
                put_paint(&mut body, paint);
            }
            Op::StrokeRect { rect, paint, style } => {
                put_rect(&mut body, rect);
                put_rgba(&mut body, *paint);
                put_style(&mut body, style);
            }
            Op::FillPath { segs, paint, rule } => {
                put_path(&mut body, segs);
                put_paint(&mut body, paint);
                body.push(u8::from(matches!(rule, FillRule::EvenOdd)));
            }
            Op::StrokePath { segs, paint, style } => {
                put_path(&mut body, segs);
                put_rgba(&mut body, *paint);
                put_style(&mut body, style);
            }
            Op::Glyphs(g) => put_glyphs(&mut body, g),
            Op::Image {
                resource,
                rect,
                src,
                opacity,
                interpolate,
            } => {
                put_u32(&mut body, *resource);
                put_rect(&mut body, rect);
                for v in src {
                    put_f32(&mut body, *v);
                }
                put_f32(&mut body, *opacity);
                body.push(u8::from(*interpolate));
            }
            Op::Link(l) => {
                put_rect(&mut body, &l.rect);
                put_u32(&mut body, l.target);
            }
            Op::NamedDest { name, x, y } => {
                put_str(&mut body, name);
                put_coord(&mut body, *x);
                put_coord(&mut body, *y);
            }
            Op::Placeholder { rect, kind, label } => {
                put_rect(&mut body, rect);
                put_str(&mut body, kind);
                put_str(&mut body, label);
            }
            Op::DebugBox { rect, label } => {
                put_rect(&mut body, rect);
                put_str(&mut body, label);
            }
        }
    }
    // Tables: targets, text index, destinations.
    let mut tail: Vec<u8> = Vec::new();
    put_u16(&mut tail, page.targets.len().min(u16::MAX as usize) as u16);
    for t in &page.targets {
        put_str(&mut tail, t);
    }
    put_u32(&mut tail, page.text.len() as u32);
    for e in &page.text {
        put_u32(&mut tail, e.run);
        put_u32(&mut tail, e.char_start);
        put_u32(&mut tail, e.char_end);
        put_rect(&mut tail, &e.rect);
        put_f32(&mut tail, e.baseline);
        tail.push(e.level);
        tail.push(u8::from(e.hidden));
    }
    put_u16(
        &mut tail,
        page.destinations.len().min(u16::MAX as usize) as u16,
    );
    for (name, (x, y)) in &page.destinations {
        put_str(&mut tail, name);
        put_coord(&mut tail, *x);
        put_coord(&mut tail, *y);
    }
    // Header, then body, then tail.
    let mut out = Vec::with_capacity(40 + body.len() + tail.len());
    out.extend_from_slice(&MAGIC);
    put_u16(&mut out, FORMAT_VERSION);
    put_u16(&mut out, 0);
    put_u32(&mut out, page.index);
    put_u32(&mut out, body.len() as u32);
    put_coord(&mut out, page.w);
    put_coord(&mut out, page.h);
    put_u32(&mut out, tail.len() as u32);
    out.extend_from_slice(&body);
    out.extend_from_slice(&tail);
    // The CRC covers the body and the tail.
    let crc = crc32(&out[HEADER_LEN..]);
    put_u32(&mut out, crc);
    out
}

/// The header length in bytes: magic, version, flags, index, body length, width, height,
/// tail length.
pub const HEADER_LEN: usize = 28;

/// A reader over a byte slice. Every read is bounds-checked, so a truncated buffer returns
/// `None` and never panics. See `DP-X-169`.
pub struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    pub fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let end = self.pos.checked_add(n)?;
        let slice = self.data.get(self.pos..end)?;
        self.pos = end;
        Some(slice)
    }

    fn u8(&mut self) -> Option<u8> {
        self.take(1).map(|b| b[0])
    }

    fn u16(&mut self) -> Option<u16> {
        let b = self.take(2)?;
        Some(u16::from_le_bytes([b[0], b[1]]))
    }

    fn u32(&mut self) -> Option<u32> {
        let b = self.take(4)?;
        Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn i32(&mut self) -> Option<i32> {
        let b = self.take(4)?;
        Some(i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn f32(&mut self) -> Option<f32> {
        let b = self.take(4)?;
        let v = f32::from_le_bytes([b[0], b[1], b[2], b[3]]);
        Some(if v.is_finite() { v } else { 0.0 })
    }

    /// A coordinate in 1/100 pt.
    fn coord(&mut self) -> Option<f32> {
        Some(self.i32()? as f32 / 100.0)
    }

    fn str(&mut self) -> Option<String> {
        let len = self.u32()? as usize;
        if len > self.remaining() {
            return None;
        }
        let b = self.take(len)?;
        String::from_utf8(b.to_vec()).ok()
    }

    fn rect(&mut self) -> Option<Rect> {
        Some(Rect::new(
            self.coord()?,
            self.coord()?,
            self.coord()?,
            self.coord()?,
        ))
    }

    fn rgba(&mut self) -> Option<Rgba> {
        let b = self.take(4)?;
        Some(Rgba::new(b[0], b[1], b[2], b[3]))
    }

    fn paint(&mut self) -> Option<Paint> {
        match self.u8()? {
            0 => Some(Paint::Solid(self.rgba()?)),
            1 => {
                let from = (self.coord()?, self.coord()?);
                let to = (self.coord()?, self.coord()?);
                let n = self.u16()? as usize;
                let mut stops = Vec::with_capacity(n.min(64));
                for _ in 0..n.min(64) {
                    let offset = self.f32()?;
                    let c = self.rgba()?;
                    stops.push((offset, c));
                }
                Some(Paint::Linear { from, to, stops })
            }
            _ => None,
        }
    }

    fn style(&mut self) -> Option<StrokeStyle> {
        let width = self.f32()?;
        let cap = match self.u8()? {
            1 => LineCap::Round,
            2 => LineCap::Square,
            _ => LineCap::Butt,
        };
        let join = match self.u8()? {
            1 => LineJoin::Round,
            2 => LineJoin::Bevel,
            _ => LineJoin::Miter,
        };
        let dash_len = self.u8()?.min(4);
        let mut dash = [0.0f32; 4];
        for slot in dash.iter_mut().take(dash_len as usize) {
            *slot = self.f32()?;
        }
        Some(StrokeStyle {
            width,
            cap,
            join,
            dash,
            dash_len,
        })
    }

    fn path(&mut self) -> Option<Vec<PathSeg>> {
        let n = self.u32()? as usize;
        if n > 4_000_000 {
            return None;
        }
        let mut out = Vec::with_capacity(n.min(4096));
        for _ in 0..n {
            match self.u8()? {
                0 => out.push(PathSeg::Move(self.coord()?, self.coord()?)),
                1 => out.push(PathSeg::Line(self.coord()?, self.coord()?)),
                2 => out.push(PathSeg::Quad(
                    self.coord()?,
                    self.coord()?,
                    self.coord()?,
                    self.coord()?,
                )),
                3 => out.push(PathSeg::Cubic(
                    self.coord()?,
                    self.coord()?,
                    self.coord()?,
                    self.coord()?,
                    self.coord()?,
                    self.coord()?,
                )),
                4 => out.push(PathSeg::Close),
                _ => return None,
            }
        }
        Some(out)
    }

    fn glyphs(&mut self) -> Option<GlyphRunOp> {
        let face = self.u32()?;
        let size = self.f32()?;
        let x = self.coord()?;
        let y = self.coord()?;
        let n = self.u32()? as usize;
        if n > 2_000_000 || n > self.remaining() / 2 {
            return None;
        }
        let mut glyphs = Vec::with_capacity(n);
        for _ in 0..n {
            glyphs.push(self.u16()?);
        }
        let mut advances = Vec::with_capacity(n);
        for _ in 0..n {
            advances.push(self.f32()?);
        }
        let mut offsets = Vec::with_capacity(n);
        for _ in 0..n {
            offsets.push((self.f32()?, self.f32()?));
        }
        let has_clusters = self.u8()? == 1;
        let mut clusters = Vec::new();
        if has_clusters {
            for _ in 0..n {
                clusters.push(self.u32()?);
            }
        }
        let paint = self.rgba()?;
        let level = self.u8()?;
        let flags = self.u8()?;
        let x_scale = self.f32()?;
        let text = self.str()?;
        let has_link = self.u8()? == 1;
        let link = if has_link { Some(self.u32()?) } else { None };
        let hidden = self.u8()? == 1;
        let rev = self.u8()?;
        Some(GlyphRunOp {
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
            synth_bold: flags & 1 != 0,
            synth_italic: flags & 2 != 0,
            x_scale,
            text,
            link,
            hidden,
            revision: if rev == 255 { None } else { Some(rev) },
        })
    }
}

/// Decode one page. A truncated or corrupt buffer returns `None`. See `DP-G-15`.
pub fn decode(bytes: &[u8]) -> Option<DisplayPage> {
    let header = bytes.get(0..HEADER_LEN)?;
    if header.get(0..4)? != MAGIC {
        return None;
    }
    let mut r = Reader::new(header);
    let _magic = r.take(4)?;
    let version = r.u16()?;
    if version > FORMAT_VERSION {
        return None;
    }
    let _flags = r.u16()?;
    let index = r.u32()?;
    let body_len = r.u32()? as usize;
    let w = r.coord()?;
    let h = r.coord()?;
    let tail_len = r.u32()? as usize;
    let body_start: usize = HEADER_LEN;
    let body_end = body_start.checked_add(body_len)?;
    let tail_end = body_end.checked_add(tail_len)?;
    let crc_stored = bytes.get(tail_end..tail_end.checked_add(4)?)?;
    let crc_stored = u32::from_le_bytes([
        crc_stored.first().copied()?,
        crc_stored.get(1).copied()?,
        crc_stored.get(2).copied()?,
        crc_stored.get(3).copied()?,
    ]);
    let covered = bytes.get(body_start..tail_end)?;
    if crc32(covered) != crc_stored {
        return None;
    }
    let body = bytes.get(body_start..body_end)?;
    let tail = bytes.get(body_end..tail_end)?;
    let mut br = Reader::new(body);
    let mut ops = Vec::new();
    while br.remaining() >= 1 {
        let tag = br.u8()?;
        let op = match tag {
            0x01 => Op::BeginPage {
                w: br.coord()?,
                h: br.coord()?,
                background: br.rgba()?,
            },
            0x02 => Op::EndPage,
            0x03 => Op::PushClip(br.rect()?),
            0x04 => Op::PopClip,
            0x05 => {
                let mut a = [0.0f32; 6];
                for slot in a.iter_mut() {
                    *slot = br.f32()?;
                }
                Op::PushTransform(Affine(a))
            }
            0x06 => Op::PopTransform,
            0x07 => Op::FillRect {
                rect: br.rect()?,
                paint: br.paint()?,
            },
            0x08 => Op::StrokeRect {
                rect: br.rect()?,
                paint: br.rgba()?,
                style: br.style()?,
            },
            0x09 => Op::FillPath {
                segs: br.path()?,
                paint: br.paint()?,
                rule: if br.u8()? == 1 {
                    FillRule::EvenOdd
                } else {
                    FillRule::NonZero
                },
            },
            0x0A => Op::StrokePath {
                segs: br.path()?,
                paint: br.rgba()?,
                style: br.style()?,
            },
            0x0B => Op::Glyphs(Box::new(br.glyphs()?)),
            0x0C => {
                let resource = br.u32()?;
                let rect = br.rect()?;
                let mut src = [0.0f32; 4];
                for slot in src.iter_mut() {
                    *slot = br.f32()?;
                }
                Op::Image {
                    resource,
                    rect,
                    src,
                    opacity: br.f32()?,
                    interpolate: br.u8()? == 1,
                }
            }
            0x0D => Op::Link(LinkOp {
                rect: br.rect()?,
                target: br.u32()?,
            }),
            0x0E => Op::NamedDest {
                name: br.str()?,
                x: br.coord()?,
                y: br.coord()?,
            },
            0x13 => Op::Placeholder {
                rect: br.rect()?,
                kind: br.str()?,
                label: br.str()?,
            },
            0x14 => Op::DebugBox {
                rect: br.rect()?,
                label: br.str()?,
            },
            _ => return None,
        };
        ops.push(op);
    }
    let mut tr = Reader::new(tail);
    let target_count = tr.u16()? as usize;
    let mut targets = Vec::with_capacity(target_count.min(4096));
    for _ in 0..target_count {
        targets.push(tr.str()?);
    }
    let text_count = tr.u32()? as usize;
    if text_count > 4_000_000 {
        return None;
    }
    let mut text = Vec::with_capacity(text_count.min(4096));
    for _ in 0..text_count {
        let run = tr.u32()?;
        let char_start = tr.u32()?;
        let char_end = tr.u32()?;
        let rect = tr.rect()?;
        let baseline = tr.f32()?;
        let level = tr.u8()?;
        let hidden = tr.u8()? == 1;
        text.push(crate::lower::TextIndexEntry {
            run,
            char_start,
            char_end,
            rect,
            baseline,
            level,
            hidden,
        });
    }
    let dest_count = tr.u16()? as usize;
    let mut destinations = Vec::with_capacity(dest_count.min(4096));
    for _ in 0..dest_count {
        let name = tr.str()?;
        let x = tr.coord()?;
        let y = tr.coord()?;
        destinations.push((name, (x, y)));
    }
    Some(DisplayPage {
        index,
        w,
        h,
        targets,
        text,
        ops,
        destinations,
    })
}

/// CRC-32 with the IEEE polynomial, which every platform computes the same way.
pub fn crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for byte in data {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> DisplayPage {
        DisplayPage {
            index: 3,
            w: 595.28,
            h: 841.89,
            targets: vec!["https://example.com/a".into(), "#bookmark1".into()],
            text: vec![crate::lower::TextIndexEntry {
                run: 0,
                char_start: 0,
                char_end: 5,
                rect: Rect::new(72.0, 100.0, 40.0, 12.0),
                baseline: 110.0,
                level: 0,
                hidden: false,
            }],
            ops: vec![
                Op::BeginPage {
                    w: 595.28,
                    h: 841.89,
                    background: Rgba::WHITE,
                },
                Op::FillRect {
                    rect: Rect::new(10.0, 20.0, 30.0, 40.0),
                    paint: Paint::solid(Rgba::opaque(1, 2, 3)),
                },
                Op::Glyphs(Box::new(GlyphRunOp {
                    face: 2,
                    size: 10.5,
                    x: 72.0,
                    y: 110.0,
                    glyphs: vec![36, 37, 38],
                    advances: vec![5.0, 6.0, 7.0],
                    offsets: vec![(0.0, 0.0), (0.5, 0.0), (0.0, -0.5)],
                    clusters: vec![0, 1, 2],
                    paint: Rgba::BLACK,
                    level: 0,
                    synth_bold: true,
                    synth_italic: false,
                    x_scale: 1.0,
                    text: "Hi!".into(),
                    link: Some(1),
                    hidden: false,
                    revision: Some(2),
                })),
                Op::Link(LinkOp {
                    rect: Rect::new(72.0, 100.0, 40.0, 12.0),
                    target: 1,
                }),
                Op::NamedDest {
                    name: "bookmark1".into(),
                    x: 72.0,
                    y: 100.0,
                },
                Op::Placeholder {
                    rect: Rect::new(100.0, 200.0, 150.0, 90.0),
                    kind: "chart".into(),
                    label: "Chart 1".into(),
                },
                Op::EndPage,
            ],
            destinations: vec![("bookmark1".into(), (72.0, 100.0))],
        }
    }

    #[test]
    fn a_page_round_trips() {
        let page = sample();
        let bytes = encode(&page);
        let back = decode(&bytes).expect("decode");
        assert_eq!(back.index, page.index);
        assert!((back.w - page.w).abs() < 0.01);
        assert!((back.h - page.h).abs() < 0.01);
        assert_eq!(back.targets, page.targets);
        assert_eq!(back.text.len(), page.text.len());
        assert_eq!(back.ops.len(), page.ops.len());
        assert_eq!(back.destinations, page.destinations);
    }

    #[test]
    fn a_coordinate_survives_within_one_two_hundredth_of_a_point() {
        let mut page = sample();
        page.ops = vec![Op::FillRect {
            rect: Rect::new(12.345, 67.891, 0.004, 0.006),
            paint: Paint::solid(Rgba::BLACK),
        }];
        let back = decode(&encode(&page)).expect("decode");
        match back.ops.first() {
            Some(Op::FillRect { rect, .. }) => {
                // A coordinate is a signed integer in 1/100 pt, so the error is at most 0.005 pt.
                assert!((rect.x - 12.345).abs() <= 0.005_1, "x {}", rect.x);
                assert!((rect.y - 67.891).abs() <= 0.005_1, "y {}", rect.y);
            }
            other => assert!(matches!(other, Some(Op::FillRect { .. })), "a fill rect"),
        }
    }

    #[test]
    fn the_header_carries_the_magic_and_version() {
        let bytes = encode(&sample());
        assert_eq!(&bytes[0..4], b"EDDL");
        assert_eq!(u16::from_le_bytes([bytes[4], bytes[5]]), FORMAT_VERSION);
    }

    #[test]
    fn a_truncated_buffer_is_refused() {
        let bytes = encode(&sample());
        for cut in [0, 4, 12, 35, 40, 60, bytes.len() - 1] {
            assert!(decode(&bytes[..cut]).is_none(), "cut at {cut}");
        }
    }

    #[test]
    fn a_corrupt_body_is_refused_by_the_crc() {
        let mut bytes = encode(&sample());
        // Flip a byte in the middle of the body.
        let mid = bytes.len() / 2;
        if let Some(b) = bytes.get_mut(mid) {
            *b ^= 0xFF;
        }
        assert!(decode(&bytes).is_none());
    }

    #[test]
    fn a_bad_magic_is_refused() {
        let mut bytes = encode(&sample());
        bytes[0] = b'X';
        assert!(decode(&bytes).is_none());
    }

    #[test]
    fn a_higher_version_is_refused() {
        let mut bytes = encode(&sample());
        bytes[4] = 99;
        assert!(decode(&bytes).is_none());
    }

    #[test]
    fn garbage_is_refused_without_a_panic() {
        for len in [1usize, 8, 64, 512] {
            let junk: Vec<u8> = (0..len).map(|i| (i * 37 % 251) as u8).collect();
            let _ = decode(&junk);
        }
    }

    #[test]
    fn crc32_matches_the_known_value() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }
}
