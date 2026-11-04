//! Write every demo document into a directory.
//!
//! ```text
//! cargo run -p edgedoc-demos --bin write-demos -- target/examples
//! ```

use std::path::Path;

fn main() -> Result<(), String> {
    let root = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "target/examples".to_string());
    let root = Path::new(&root);

    let mut written = 0usize;
    for builder in [
        edgedoc_demos::invoice::render,
        edgedoc_demos::report::render,
        edgedoc_demos::letter::render,
    ] {
        for file in builder()? {
            let path = root.join(&file.name);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
            }
            std::fs::write(&path, &file.bytes).map_err(|err| err.to_string())?;
            println!("  {:<42} {:>7} bytes", file.name, file.bytes.len());
            written += 1;
        }
    }
    println!();
    println!("  {written} documents in {}", root.display());
    Ok(())
}
