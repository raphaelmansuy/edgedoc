//! Independent adapter: call the unmodified sibling Wordcraft reader, layout, and display list.
use std::collections::BTreeMap;
use std::path::PathBuf;

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use wordcraft_layout::display::{DisplayOptions, Draw, page_display};

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn draw(
    items: &[Draw],
    chars: &mut Vec<Value>,
    svg: &mut String,
    fonts: &mut BTreeMap<String, Value>,
    unsupported: &mut Vec<String>,
    media: &BTreeMap<String, std::sync::Arc<Vec<u8>>>,
) {
    for item in items {
        match item {
            Draw::Glyphs {
                face,
                size,
                glyphs,
                text,
                ranges,
                color,
                alpha,
                synth_bold,
                synth_italic,
                x_scale,
                ..
            } => {
                let face = face.get();
                if *synth_bold || *synth_italic {
                    unsupported
                        .push("Reference adapter requires an actual bold/italic font face".into());
                }
                let hash = hex(&Sha256::digest(face.data()));
                fonts.entry(hash.clone()).or_insert_with(
                    || json!({"sha256": hash, "family": face.family, "style": face.style}),
                );
                // Numbering labels and hyphens deliberately omit ranges in Wordcraft's display
                // list. Infer only a verified one-to-one character/glyph mapping. Unknown text
                // remains a blocker, while empty-text decorative glyphs still paint.
                let direct: Vec<_> = text.char_indices().collect();
                let direct_valid = ranges.is_empty()
                    && direct.len() == glyphs.len()
                    && direct
                        .iter()
                        .zip(glyphs)
                        .all(|((_, ch), (gid, _, _))| face.glyph_for(*ch) == *gid);
                for (i, (gid, x, y)) in glyphs.iter().enumerate() {
                    let shown = ranges.get(i).and_then(|r| text.get(r.clone())).or_else(|| {
                        if !direct_valid {
                            return None;
                        }
                        let start = direct.get(i)?.0;
                        let end = direct.get(i + 1).map_or(text.len(), |(byte, _)| *byte);
                        text.get(start..end)
                    });
                    if let Some(shown) = shown {
                        chars.push(json!({"text": shown, "x": x, "baseline": y, "size": size,
                            "font": face.family, "bold": face.weight >= 700.0 || *synth_bold,
                            "italic": face.italic || *synth_italic, "gid": gid, "font_sha256": hash}));
                    } else if !text.is_empty() {
                        unsupported
                            .push("Reference glyph has no verified source text mapping".into());
                    }
                    let path = face.glyph_outline(*gid).to_svg();
                    let scale = f64::from(*size) / face.upem;
                    svg.push_str(&format!("<path d=\"{path}\" fill=\"rgb({},{},{})\" opacity=\"{alpha}\" transform=\"translate({x} {y}) scale({} {scale})\"/>\n", color.0, color.1, color.2, scale * f64::from(*x_scale)));
                }
            }
            Draw::Fill {
                rect, color, alpha, ..
            } => {
                svg.push_str(&format!("<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"rgb({},{},{})\" opacity=\"{alpha}\"/>\n", rect.x, rect.y, rect.w, rect.h, color.0, color.1, color.2));
            }
            Draw::Line {
                x0,
                y0,
                x1,
                y1,
                width,
                color,
                alpha,
                stroke,
                ..
            } => {
                use wordcraft_layout::display::Stroke;
                let (dx, dy) = (x1 - x0, y1 - y0);
                let len = (dx * dx + dy * dy).sqrt().max(0.001);
                let normal = (-dy / len, dx / len);
                let mut line = |offset: f32, line_width: f32, dash: Option<&[f32]>| {
                    let (ox, oy) = (normal.0 * offset, normal.1 * offset);
                    let dashes = dash
                        .map(|d| {
                            format!(
                                " stroke-dasharray=\"{}\"",
                                d.iter()
                                    .map(|v| (v * line_width).to_string())
                                    .collect::<Vec<_>>()
                                    .join(" ")
                            )
                        })
                        .unwrap_or_default();
                    svg.push_str(&format!("<path d=\"M{} {} L{} {}\" fill=\"none\" stroke=\"rgb({},{},{})\" stroke-width=\"{}\" opacity=\"{}\"{}/>\n", x0 + ox, y0 + oy, x1 + ox, y1 + oy, color.0, color.1, color.2, line_width, alpha, dashes));
                };
                match stroke {
                    Stroke::Double => {
                        line(width * 0.9, width * 0.6, None);
                        line(-width * 0.9, width * 0.6, None);
                    }
                    Stroke::Triple => {
                        for offset in [-1.8, 0.0, 1.8] {
                            line(width * offset, width * 0.6, None);
                        }
                    }
                    Stroke::Wave => {
                        unsupported.push("Reference adapter does not render wave strokes".into());
                        line(0.0, *width, None);
                    }
                    _ => line(0.0, *width, stroke.dashes()),
                }
            }
            Draw::Image {
                rect,
                media: name,
                crop,
                alpha,
                fx,
                ..
            } => {
                if fx.changes_pixels() {
                    unsupported.push(
                        "Reference image brightness/contrast is unsupported by the adapter".into(),
                    );
                }
                if let Some(bytes) = media.get(name) {
                    let mime = if bytes.starts_with(b"\x89PNG") {
                        Some("image/png")
                    } else if bytes.starts_with(&[255, 216, 255]) {
                        Some("image/jpeg")
                    } else if bytes.starts_with(b"GIF8") {
                        Some("image/gif")
                    } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
                        Some("image/webp")
                    } else {
                        None
                    };
                    if let Some(mime) = mime {
                        let [left, top, right, bottom] = *crop;
                        let w = 1.0 - left - right;
                        let h = 1.0 - top - bottom;
                        if w > 0.0 && h > 0.0 {
                            svg.push_str(&format!("<svg x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" viewBox=\"0 0 1 1\" preserveAspectRatio=\"none\" overflow=\"hidden\"><image x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" opacity=\"{alpha}\" preserveAspectRatio=\"none\" href=\"data:{mime};base64,{}\"/></svg>", rect.x, rect.y, rect.w, rect.h, -left/w, -top/h, 1.0/w, 1.0/h, base64(bytes)));
                        } else {
                            unsupported.push("Reference image has invalid crop".into());
                        }
                    } else {
                        unsupported.push(format!("Unsupported reference image format: {name}"));
                    }
                } else {
                    unsupported.push(format!("Missing reference image data: {name}"));
                }
            }
            Draw::Figure { draws, .. } | Draw::Clip { items: draws, .. } => {
                if let Draw::Clip { rect, .. } = item {
                    let id = format!("clip{}", svg.len());
                    svg.push_str(&format!("<defs><clipPath id=\"{id}\"><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"/></clipPath></defs><g clip-path=\"url(#{id})\">", rect.x, rect.y, rect.w, rect.h));
                    draw(draws, chars, svg, fonts, unsupported, media);
                    svg.push_str("</g>");
                } else {
                    draw(draws, chars, svg, fonts, unsupported, media);
                }
            }
            other => unsupported.push(format!("Reference adapter does not paint {other:?}")),
        }
    }
}

fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let word = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        for (i, shift) in [18, 12, 6, 0].iter().enumerate() {
            out.push(if i > chunk.len() {
                '='
            } else {
                ALPHABET[((word >> shift) & 63) as usize] as char
            });
        }
    }
    out
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let input = PathBuf::from(args.next().ok_or("Missing input DOCX")?);
    let out = PathBuf::from(args.next().ok_or("Missing output directory")?);
    let mut fonts = Vec::new();
    for path in args {
        let bytes = std::fs::read(&path)?;
        let face = ttf_parser::Face::parse(&bytes, 0)?;
        let family = face
            .names()
            .into_iter()
            .filter(|n| n.name_id == ttf_parser::name_id::FAMILY)
            .find_map(|n| n.to_string())
            .ok_or("Comparison font has no family name")?;
        fonts.push((bytes, family));
    }
    wordcraft_fonts::FontDb::global().add_document_fonts(fonts);
    let bytes = std::fs::read(&input)?;
    let doc = wordcraft_docx::read(&bytes)?;
    let layout = wordcraft_layout::layout(
        &doc,
        &mut wordcraft_layout::LayoutCache::new(),
        &wordcraft_layout::LayoutOptions::default(),
    );
    let options = DisplayOptions {
        dim_header: false,
        ..DisplayOptions::default()
    };
    std::fs::create_dir_all(&out)?;
    let mut pages = Vec::new();
    let mut font_evidence = BTreeMap::new();
    let mut unsupported = Vec::new();
    for (i, page) in layout.pages.iter().enumerate() {
        let items = page_display(&doc, page, &options);
        let mut chars = Vec::new();
        let mut svg = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\"><rect width=\"100%\" height=\"100%\" fill=\"white\"/>",
            page.w, page.h, page.w, page.h
        );
        draw(
            &items,
            &mut chars,
            &mut svg,
            &mut font_evidence,
            &mut unsupported,
            &doc.media,
        );
        svg.push_str("</svg>");
        std::fs::write(out.join(format!("page-{:03}.svg", i + 1)), svg)?;
        pages
            .push(json!({"width": page.w, "height": page.h, "chars": chars, "unmapped_glyphs": 0}));
    }
    std::fs::write(
        out.join("layout.json"),
        serde_json::to_vec_pretty(&json!({"schema": 1,
        "source_sha256": hex(&Sha256::digest(&bytes)), "engine": "Wordcraft",
        "fonts": font_evidence.into_values().collect::<Vec<_>>(), "pages": pages, "unsupported_draws": unsupported}))?,
    )?;
    println!(
        "{} Wordcraft pages in {}",
        layout.pages.len(),
        out.display()
    );
    Ok(())
}
