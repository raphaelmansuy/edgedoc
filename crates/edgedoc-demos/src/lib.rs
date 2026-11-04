//! Realistic Word documents built with the EdgeDoc fluent API.
//!
//! Each module builds a full document and returns the bytes, so an example binary
//! and a test can both call it. The documents show the parts of the API that a
//! production job needs: styles, headers and footers, tables, pictures, fields,
//! and mail merge.
//!
//! ```no_run
//! let files = edgedoc_demos::invoice::render()?;
//! for file in files {
//!     std::fs::write(format!("out/{}", file.name), &file.bytes)?;
//! }
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

pub mod common;
pub mod invoice;
pub mod letter;
pub mod report;

/// One document ready to write.
#[derive(Debug, Clone)]
pub struct Rendered {
    /// A relative path, with forward slashes.
    pub name: String,
    pub bytes: Vec<u8>,
}
