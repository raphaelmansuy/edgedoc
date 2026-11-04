//! Lay out every document in the corpus, and report what each one did.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stdout)]

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "tests/corpus/downloaded".into());
    let mut files: Vec<_> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "docx"))
        .collect();
    files.sort();
    println!(
        "{:<34} {:>6} {:>7} {:>9} {:>8}",
        "document", "pages", "glyphs", "ink px", "ms"
    );
    let mut ok = 0;
    let mut failed = 0;
    for path in &files {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let bytes = std::fs::read(path)?;
        match run(&bytes) {
            Ok((pages, glyphs, ink, ms)) => {
                ok += 1;
                println!("{name:<34} {pages:>6} {glyphs:>7} {ink:>9} {ms:>8.0}");
            }
            Err(e) => {
                failed += 1;
                println!("{name:<34}  FAILED: {e}");
            }
        }
    }
    println!("\n{ok} ok, {failed} failed, {} total", files.len());
    Ok(())
}

fn run(bytes: &[u8]) -> Result<(u32, usize, usize, f64), String> {
    let started = std::time::Instant::now();
    let mut doc =
        edgedoc_display::DisplayDocument::open(bytes, &edgedoc_display::OpenOptions::default())
            .map_err(|e| format!("{} {}", e.code, e.message))?;
    doc.layout()
        .map_err(|e| format!("{} {}", e.code, e.message))?;
    let mut glyphs = 0usize;
    for i in 0..doc.pages().len() {
        let svg = doc.to_svg(i).unwrap_or_default();
        glyphs += svg.matches("<text ").count();
    }
    let ink = doc.all_text().chars().count();
    Ok((
        doc.pages().len() as u32,
        glyphs,
        ink,
        started.elapsed().as_micros() as f64 / 1000.0,
    ))
}
