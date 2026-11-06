//! The SVG painter: a page as one `<svg>` element.
//!
//! SVG output is for export, for print, and for a screenshot test that needs no rasterizer. Glyphs
//! can use the loaded font's outlines, so exports do not depend on fonts installed in the viewer.
//! The legacy face-name API writes text when no font database is supplied. See `DP-G-19`.

use edgedoc_geom::Rgba;

use crate::{Affine, FillRule, Op, Paint, PathSeg};

/// One face as a painter needs it: the family, the weight, and the slant.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FaceStyle {
    pub family: String,
    pub bold: bool,
    pub italic: bool,
}

impl FaceStyle {
    /// The CSS `font-weight` for the face.
    pub fn weight(&self) -> &'static str {
        if self.bold {
            "700"
        } else {
            "400"
        }
    }

    /// The CSS `font-style` for the face.
    pub fn style(&self) -> &'static str {
        if self.italic {
            "italic"
        } else {
            "normal"
        }
    }
}

/// A resource that the painter can embed: an image with its MIME type and bytes.
#[derive(Debug, Clone)]
pub struct ImageResource {
    pub id: u32,
    pub mime: String,
    pub bytes: Vec<u8>,
}

/// Sniff the MIME type of an image from its first bytes. See `DP-P-10`.
pub fn sniff_mime(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        return Some("image/png");
    }
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some("image/jpeg");
    }
    if bytes.starts_with(b"GIF8") {
        return Some("image/gif");
    }
    if bytes.len() > 2 && bytes.starts_with(b"BM") {
        return Some("image/bmp");
    }
    if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        return Some("image/webp");
    }
    None
}

/// Write one page as an SVG document, with a family name per face. See `DP-G-19`.
pub fn paint_page(page: &crate::DisplayPage, faces: &[String]) -> String {
    let styled: Vec<FaceStyle> = faces
        .iter()
        .map(|f| FaceStyle {
            family: f.clone(),
            bold: false,
            italic: false,
        })
        .collect();
    paint_page_styled(page, &styled, &[])
}

/// Write one page as an SVG document, with images embedded as data URLs.
pub fn paint_page_with(
    page: &crate::DisplayPage,
    faces: &[String],
    images: &[ImageResource],
) -> String {
    let styled: Vec<FaceStyle> = faces
        .iter()
        .map(|f| FaceStyle {
            family: f.clone(),
            bold: false,
            italic: false,
        })
        .collect();
    paint_page_styled(page, &styled, images)
}

/// Write one page with a face table that carries a weight and a slant per face.
pub fn paint_page_styled(
    page: &crate::DisplayPage,
    faces: &[FaceStyle],
    images: &[ImageResource],
) -> String {
    paint_page_impl(page, faces, images, None)
}

/// Export the actual shaped glyphs from the loaded fonts, retaining an invisible text layer.
pub fn paint_page_fonts(
    page: &crate::DisplayPage,
    fonts: &edgedoc_fonts::FontDb,
    images: &[ImageResource],
) -> String {
    paint_page_impl(page, &[], images, Some(fonts))
}

fn paint_page_impl(
    page: &crate::DisplayPage,
    faces: &[FaceStyle],
    images: &[ImageResource],
    fonts: Option<&edgedoc_fonts::FontDb>,
) -> String {
    let mut out = String::with_capacity(8192);
    out.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{:.2}\" height=\"{:.2}\" viewBox=\"0 0 {:.2} {:.2}\" role=\"img\">\n",
        page.w, page.h, page.w, page.h
    ));
    out.push_str("<rect width=\"100%\" height=\"100%\" fill=\"#ffffff\"/>\n");
    let mut clip_depth = 0usize;
    let mut clip_serial = 0usize;
    let mut transform_depth = 0usize;
    for op in &page.ops {
        match op {
            Op::BeginPage { .. } | Op::EndPage => {}
            Op::PushClip(r) => {
                let clip_id = clip_serial;
                clip_serial += 1;
                out.push_str(&format!(
                    "<g clip-path=\"url(#clip{clip_id})\"><clipPath id=\"clip{clip_id}\"><rect x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\"/></clipPath>\n",
                    r.x, r.y, r.w, r.h
                ));
                clip_depth += 1;
            }
            Op::PopClip => {
                if clip_depth > 0 {
                    out.push_str("</g>\n");
                    clip_depth -= 1;
                }
            }
            Op::PushTransform(a) => {
                out.push_str(&format!("<g transform=\"{}\">\n", transform_attr(a)));
                transform_depth += 1;
            }
            Op::PopTransform => {
                if transform_depth > 0 {
                    out.push_str("</g>\n");
                    transform_depth -= 1;
                }
            }
            Op::FillRect { rect, paint } => {
                if let Paint::Solid(c) = paint {
                    if c.a > 0 {
                        out.push_str(&format!(
                            "<rect x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" fill=\"{}\"/>\n",
                            rect.x,
                            rect.y,
                            rect.w,
                            rect.h,
                            c.to_css()
                        ));
                    }
                }
            }
            Op::StrokeRect { rect, paint, style } => {
                if paint.a > 0 {
                    out.push_str(&format!(
                        "<rect x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{:.2}\"{}/>\n",
                        rect.x,
                        rect.y,
                        rect.w,
                        rect.h,
                        paint.to_css(),
                        style.width,
                        dash_attr(style)
                    ));
                }
            }
            Op::FillPath { segs, paint, rule } => {
                if let Paint::Solid(c) = paint {
                    out.push_str(&format!(
                        "<path d=\"{}\" fill=\"{}\" fill-rule=\"{}\"/>\n",
                        path_data(segs),
                        c.to_css(),
                        if matches!(rule, FillRule::EvenOdd) {
                            "evenodd"
                        } else {
                            "nonzero"
                        }
                    ));
                }
            }
            Op::StrokePath { segs, paint, style } => {
                if paint.a > 0 {
                    out.push_str(&format!(
                        "<path d=\"{}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{:.2}\"{}/>\n",
                        path_data(segs),
                        paint.to_css(),
                        style.width,
                        dash_attr(style)
                    ));
                }
            }
            Op::Glyphs(run) => {
                if let Some(fonts) = fonts {
                    if !run.hidden {
                        outline_run(&mut out, run, fonts);
                    }
                    continue;
                }
                // One `<text>` per glyph, placed at its pen, so the SVG matches the display list
                // and a screenshot test compares the same positions.
                if run.hidden {
                    continue;
                }
                let face = faces.get(run.face as usize).cloned().unwrap_or_default();
                let family = if face.family.is_empty() {
                    "sans-serif".to_string()
                } else {
                    face.family.clone()
                };
                // The face carries its own weight and slant, so a bold run drawn from the bold
                // face is bold without a synthetic stroke. See `DP-F-02`.
                let weight = face.weight();
                let slant = face.style();
                let mut x = run.x;
                for (i, _gid) in run.glyphs.iter().enumerate() {
                    let adv = run.advances.get(i).copied().unwrap_or(0.0);
                    let (dx, dy) = run.offsets.get(i).copied().unwrap_or((0.0, 0.0));
                    let ch = run.text.chars().nth(i).unwrap_or(' ');
                    if !ch.is_whitespace() {
                        // A synthesized face adds a stroke (bold) or a skew (italic), which is
                        // what Word draws when the family has no such style. See `DP-F-13`.
                        let mut extra = String::new();
                        if run.synth_bold {
                            extra.push_str(&format!(
                                " stroke=\"{}\" stroke-width=\"{:.2}\"",
                                run.paint.to_css(),
                                (run.size * 0.025).max(0.25)
                            ));
                        }
                        let skew = if run.synth_italic {
                            format!(
                                " transform=\"skewX(-12) translate(0 {:.2})\"",
                                -run.y * 0.21
                            )
                        } else {
                            String::new()
                        };
                        out.push_str(&format!(
                            "<text x=\"{:.2}\" y=\"{:.2}\" font-family=\"{}\" font-size=\"{:.2}\" font-weight=\"{}\" font-style=\"{}\" fill=\"{}\"{}{}>{}</text>\n",
                            x + dx,
                            run.y + dy,
                            escape_attr(&family),
                            run.size,
                            weight,
                            slant,
                            run.paint.to_css(),
                            extra,
                            skew,
                            escape_text(&ch.to_string())
                        ));
                    }
                    x += adv;
                }
            }
            Op::Image {
                resource,
                rect,
                src,
                opacity,
                ..
            } => {
                match images.iter().find(|i| i.id == *resource) {
                    Some(image) => {
                        if *src == [0.0, 0.0, 1.0, 1.0] {
                            out.push_str(&format!(
                                "<image x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" preserveAspectRatio=\"none\" href=\"data:{};base64,{}\"/>\n",
                                rect.x, rect.y, rect.w, rect.h, image.mime, base64(&image.bytes)
                            ));
                        } else if src[2] > 0.0 && src[3] > 0.0 {
                            out.push_str(&format!(
                                "<svg x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" viewBox=\"{:.6} {:.6} {:.6} {:.6}\" preserveAspectRatio=\"none\" overflow=\"hidden\"><image x=\"0\" y=\"0\" width=\"1\" height=\"1\" opacity=\"{:.4}\" preserveAspectRatio=\"none\" href=\"data:{};base64,{}\"/></svg>\n",
                                rect.x, rect.y, rect.w, rect.h, src[0], src[1], src[2], src[3], opacity, image.mime, base64(&image.bytes)
                            ));
                        }
                    }
                    None => {
                        // A missing resource paints a marked box, so the page shows where it goes.
                        out.push_str(&format!(
                            "<g><rect x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" fill=\"#f2f4f8\" stroke=\"#9aa3b2\" stroke-dasharray=\"3 3\"/><line x1=\"{:.2}\" y1=\"{:.2}\" x2=\"{:.2}\" y2=\"{:.2}\" stroke=\"#d5dae3\"/><line x1=\"{:.2}\" y1=\"{:.2}\" x2=\"{:.2}\" y2=\"{:.2}\" stroke=\"#d5dae3\"/></g>\n",
                            rect.x, rect.y, rect.w, rect.h,
                            rect.x, rect.y, rect.right(), rect.bottom(),
                            rect.right(), rect.y, rect.x, rect.bottom()
                        ));
                    }
                }
            }
            Op::Link(_) | Op::NamedDest { .. } | Op::DebugBox { .. } => {}
            Op::Placeholder { rect, label, .. } => {
                out.push_str(&format!(
                    "<g><rect x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" fill=\"#f2f4f8\" stroke=\"#9aa3b2\" stroke-dasharray=\"3 3\"/><text x=\"{:.2}\" y=\"{:.2}\" font-family=\"sans-serif\" font-size=\"8\" fill=\"#5b6472\">{}</text></g>\n",
                    rect.x,
                    rect.y,
                    rect.w,
                    rect.h,
                    rect.x + 4.0,
                    rect.y + rect.h / 2.0,
                    escape_text(label)
                ));
            }
        }
    }
    while clip_depth > 0 {
        out.push_str("</g>\n");
        clip_depth -= 1;
    }
    while transform_depth > 0 {
        out.push_str("</g>\n");
        transform_depth -= 1;
    }
    out.push_str("</svg>\n");
    out
}

/// A glyph's SVG path in font units, with positive y pointing up.
pub fn glyph_path(face: &edgedoc_fonts::FontFace, gid: u16) -> String {
    use edgedoc_fonts::PathSeg as FontPath;
    let segs: Vec<_> = face
        .outline(gid)
        .into_iter()
        .map(|seg| match seg {
            FontPath::Move(x, y) => PathSeg::Move(x, y),
            FontPath::Line(x, y) => PathSeg::Line(x, y),
            FontPath::Quad(x1, y1, x, y) => PathSeg::Quad(x1, y1, x, y),
            FontPath::Cubic(x1, y1, x2, y2, x, y) => PathSeg::Cubic(x1, y1, x2, y2, x, y),
            FontPath::Close => PathSeg::Close,
        })
        .collect();
    path_data(&segs)
}

fn outline_run(out: &mut String, run: &crate::GlyphRunOp, fonts: &edgedoc_fonts::FontDb) {
    let Some(face) = fonts.face(run.face) else {
        return;
    };
    out.push_str(&format!("<g aria-label=\"{}\">\n", escape_attr(&run.text)));
    let scale = run.size / face.units_per_em;
    let mut x = run.x;
    for (i, gid) in run.glyphs.iter().enumerate() {
        let (dx, dy) = run.offsets.get(i).copied().unwrap_or_default();
        let path = glyph_path(face, *gid);
        if !path.is_empty() {
            let bold = if run.synth_bold {
                format!(
                    " stroke=\"{}\" stroke-width=\"{}\" vector-effect=\"non-scaling-stroke\"",
                    run.paint.to_css(),
                    (run.size * 0.025).max(0.25)
                )
            } else {
                String::new()
            };
            let skew = if run.synth_italic { "skewX(-12) " } else { "" };
            out.push_str(&format!("<path d=\"{path}\" fill=\"{}\"{bold} transform=\"translate({} {}) {skew}scale({} {})\"/>\n", run.paint.to_css(), x + dx, run.y + dy, scale * run.x_scale, -scale));
        }
        x += run.advances.get(i).copied().unwrap_or_default();
    }
    out.push_str(&format!("<text x=\"{}\" y=\"{}\" font-family=\"{}\" font-size=\"{}\" fill-opacity=\"0\">{}</text></g>\n", run.x, run.y, escape_attr(&face.family), run.size, escape_text(&run.text)));
}

fn transform_attr(a: &Affine) -> String {
    let m = a.0;
    format!(
        "matrix({:.4} {:.4} {:.4} {:.4} {:.2} {:.2})",
        m[0], m[1], m[2], m[3], m[4], m[5]
    )
}

fn dash_attr(style: &crate::StrokeStyle) -> String {
    let dashes: Vec<String> = style
        .dash
        .iter()
        .take(style.dash_len as usize)
        .filter(|d| **d > 0.0)
        .map(|d| format!("{d:.2}"))
        .collect();
    if dashes.is_empty() {
        String::new()
    } else {
        format!(" stroke-dasharray=\"{}\"", dashes.join(" "))
    }
}

fn path_data(segs: &[PathSeg]) -> String {
    let mut out = String::new();
    for seg in segs {
        match seg {
            PathSeg::Move(x, y) => out.push_str(&format!("M{x:.2} {y:.2} ")),
            PathSeg::Line(x, y) => out.push_str(&format!("L{x:.2} {y:.2} ")),
            PathSeg::Quad(x1, y1, x, y) => {
                out.push_str(&format!("Q{x1:.2} {y1:.2} {x:.2} {y:.2} "))
            }
            PathSeg::Cubic(x1, y1, x2, y2, x, y) => {
                out.push_str(&format!("C{x1:.2} {y1:.2} {x2:.2} {y2:.2} {x:.2} {y:.2} "))
            }
            PathSeg::Close => out.push_str("Z "),
        }
    }
    out.trim_end().to_string()
}

fn escape_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            c if (c < '\u{20}' && !matches!(c, '\t' | '\n' | '\r'))
                || matches!(c, '\u{FFFE}' | '\u{FFFF}') => {}
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }
    out
}

fn escape_attr(s: &str) -> String {
    escape_text(s).replace('"', "&quot;")
}

/// Standard base64, which an SVG data URL needs.
pub fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = u32::from(chunk[0]);
        let b1 = chunk.get(1).copied().map_or(0, u32::from);
        let b2 = chunk.get(2).copied().map_or(0, u32::from);
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[(n >> 18) as usize & 63] as char);
        out.push(TABLE[(n >> 12) as usize & 63] as char);
        if chunk.len() > 1 {
            out.push(TABLE[(n >> 6) as usize & 63] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(TABLE[n as usize & 63] as char);
        } else {
            out.push('=');
        }
    }
    out
}

/// The CSS colour of a transparent fill, which an SVG needs as `none`.
pub fn css_fill(c: Rgba) -> &'static str {
    if c.a == 0 {
        "none"
    } else {
        "currentColor"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DisplayPage;
    use crate::{GlyphRunOp, LinkOp};

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
                    rect: edgedoc_geom::Rect::new(72.0, 72.0, 100.0, 20.0),
                    paint: Paint::solid(Rgba::opaque(0x1F, 0x4E, 0x79)),
                },
                Op::Glyphs(Box::new(GlyphRunOp {
                    face: 0,
                    size: 12.0,
                    x: 72.0,
                    y: 120.0,
                    glyphs: vec![10, 11],
                    advances: vec![6.0, 6.0],
                    offsets: vec![(0.0, 0.0), (0.0, 0.0)],
                    clusters: vec![0, 1],
                    paint: Rgba::BLACK,
                    level: 0,
                    synth_bold: false,
                    synth_italic: false,
                    x_scale: 1.0,
                    text: "Hi".into(),
                    link: Some(0),
                    hidden: false,
                    revision: None,
                })),
                Op::Link(LinkOp {
                    rect: edgedoc_geom::Rect::new(72.0, 110.0, 18.0, 12.0),
                    target: 0,
                }),
                Op::EndPage,
            ],
            destinations: Vec::new(),
        }
    }

    #[test]
    fn the_svg_has_the_page_size() {
        let svg = paint_page(&page(), &["Carlito".into()]);
        assert!(svg.contains("width=\"595.28\""));
        assert!(svg.contains("height=\"841.89\""));
        assert!(svg.ends_with("</svg>\n"));
    }

    #[test]
    fn the_svg_holds_each_glyph_as_text() {
        let svg = paint_page(&page(), &["Carlito".into()]);
        assert!(svg.contains("<text"), "{svg}");
        assert!(svg.contains(">H</text>"));
        assert!(svg.contains(">i</text>"));
    }

    #[test]
    fn the_svg_closes_every_group_it_opens() {
        let svg = paint_page(&page(), &["Carlito".into()]);
        let opens = svg.matches("<g ").count() + svg.matches("<g>").count();
        let closes = svg.matches("</g>").count();
        assert_eq!(opens, closes);
    }
}
