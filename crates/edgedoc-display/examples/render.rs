//! Render a document to SVG and text, so a person and a test can inspect the result.
//!
//!     cargo run -p edgedoc-display --example render -- report.docx out/
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let input = args
        .next()
        .unwrap_or_else(|| "target/examples/report.docx".into());
    let out = PathBuf::from(args.next().unwrap_or_else(|| "/tmp/edgedoc-display".into()));
    std::fs::create_dir_all(&out)?;
    let bytes = std::fs::read(&input)?;
    let mut doc =
        edgedoc_display::DisplayDocument::open(&bytes, &edgedoc_display::OpenOptions::default())?;
    let report = doc.layout()?;
    println!(
        "{}: {} pages, {} passes, {:.0} ms",
        input, report.pages, report.passes, report.elapsed_ms
    );
    for d in doc.diagnostics() {
        println!("  {} {}: {}", d.severity.as_str(), d.code, d.detail);
    }
    for i in 0..doc.pages().len() {
        if let Some(svg) = doc.to_svg(i) {
            std::fs::write(out.join(format!("page-{:03}.svg", i + 1)), svg)?;
        }
        let text = doc.page_text(i).unwrap_or_default();
        std::fs::write(out.join(format!("page-{:03}.txt", i + 1)), text)?;
    }
    std::fs::write(out.join("report.txt"), format!("{:#?}", doc.report()))?;
    println!("wrote {} pages to {}", doc.pages().len(), out.display());
    Ok(())
}
