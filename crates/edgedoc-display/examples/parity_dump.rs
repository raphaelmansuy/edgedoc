//! Export actual painted glyph origins and SVG for an independent renderer comparison.
//! cargo run -p edgedoc-display --features serde --example parity_dump -- in.docx out --font file.ttf

use std::path::PathBuf;

use edgedoc_display::{DisplayDocument, Op, OpenOptions};
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1).peekable();
    let input = PathBuf::from(args.next().ok_or("Missing input DOCX")?);
    let out = PathBuf::from(args.next().ok_or("Missing output directory")?);
    let mut doc = DisplayDocument::open(&std::fs::read(&input)?, &OpenOptions::default())?;
    let mut loaded_fonts = Vec::new();
    while let Some(option) = args.next() {
        let (bold, italic) = match option.as_str() {
            "--font" => (false, false),
            "--font-bold" => (true, false),
            "--font-italic" => (false, true),
            "--font-bold-italic" => (true, true),
            _ => return Err(format!("Unknown option {option}").into()),
        };
        let path = args.next().ok_or("Missing font path")?;
        let index: u32 = if args.peek().is_some_and(|value| value == "--font-index") {
            args.next();
            args.next().ok_or("Missing collection index")?.parse()?
        } else {
            0
        };
        let id = doc
            .fonts_mut()
            .load_indexed(std::fs::read(&path)?, index, None, bold, italic, false)
            .ok_or_else(|| format!("Cannot load font {path}"))?;
        loaded_fonts.push(
            json!({ "path": path, "id": id, "index": index, "bold": bold, "italic": italic }),
        );
    }
    let report = doc.layout()?;
    let font_sources: std::collections::BTreeMap<_, _> = doc
        .fonts()
        .faces()
        .iter()
        .filter_map(|face| {
            loaded_fonts.iter().find_map(|loaded| {
                let id = loaded["id"].as_u64()? as u32;
                let supplied = doc.fonts().face(id)?;
                (face.data() == supplied.data() && face.index == supplied.index)
                    .then(|| (face.id, loaded["path"].clone()))
            })
        })
        .collect();
    std::fs::create_dir_all(&out)?;
    let mut pages = Vec::new();
    for (index, page) in doc.pages().iter().enumerate() {
        let display = doc.page(index).ok_or("Missing painted page")?;
        let mut chars = Vec::new();
        let mut unmapped_glyphs = 0;
        let mut ordinal = 0;
        for op in &display.ops {
            if let Op::Glyphs(run) = op {
                let source_start = display
                    .text
                    .iter()
                    .find(|entry| entry.run == ordinal)
                    .map(|entry| entry.char_start)
                    .or_else(|| run.clusters.iter().min().copied())
                    .unwrap_or(0);
                ordinal += 1;
                if run.hidden || run.text.is_empty() {
                    continue;
                }
                let face = doc.fonts().face(run.face).ok_or("Missing font face")?;
                let mut x = run.x;
                for (i, gid) in run.glyphs.iter().enumerate() {
                    let start = run
                        .clusters
                        .get(i)
                        .copied()
                        .unwrap_or(u32::MAX)
                        .saturating_sub(source_start) as usize;
                    let stop = run
                        .clusters
                        .iter()
                        .copied()
                        .filter(|byte| *byte > source_start.saturating_add(start as u32))
                        .min()
                        .map_or(run.text.len(), |byte| {
                            byte.saturating_sub(source_start) as usize
                        });
                    let shown = run.text.get(start..stop).map(|text| {
                        text.trim_end_matches(|ch| {
                            matches!(ch, '\t' | '\n' | '\r' | '\u{200B}' | '\u{FFFC}')
                        })
                    });
                    if shown.is_none() {
                        unmapped_glyphs += 1;
                    }
                    let (dx, dy) = run.offsets.get(i).copied().unwrap_or_default();
                    chars.push(
                        json!({ "text": shown.unwrap_or(""), "x": x + dx, "baseline": run.y + dy,
                        "size": run.size, "font": face.family, "bold": face.weight >= 700 || run.synth_bold,
                        "italic": face.intrinsic_italic || run.synth_italic, "advance": run.advances.get(i),
                        "gid": gid, "font_index": face.index, "font_path": font_sources.get(&face.id) }),
                    );
                    x += run.advances.get(i).copied().unwrap_or_default();
                }
            }
        }
        std::fs::write(
            out.join(format!("page-{:03}.svg", index + 1)),
            doc.to_svg(index).ok_or("Missing SVG")?,
        )?;
        pages.push(json!({ "width": page.w, "height": page.h, "chars": chars, "unmapped_glyphs": unmapped_glyphs }));
    }
    let diagnostics: Vec<_> = report
        .diagnostics
        .iter()
        .map(|d| json!({ "code": d.code, "detail": d.detail }))
        .collect();
    std::fs::write(
        out.join("layout.json"),
        serde_json::to_vec_pretty(&json!({
            "schema": 1, "input": input, "fonts": loaded_fonts, "pages": pages, "diagnostics": diagnostics
        }))?,
    )?;
    println!("{} painted pages in {}", report.pages, out.display());
    Ok(())
}
