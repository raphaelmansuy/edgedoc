//! Map a package to the document model and back.
//! Unknown markup stays in the package parts the writer does not replace.

mod catalog;
mod error;
mod read;
mod write;

pub use catalog::{is_owned, OWNED};
pub use error::{WmlError, WmlResult};
pub use read::{document_from_package, style_default_properties};
pub use write::write_document;

use edgedoc_limit::Budget;
use edgedoc_model::ModelDocument;
use edgedoc_opc::{is_encrypted_package, read_zip, Package};

pub fn load(bytes: &[u8], budget: &Budget) -> WmlResult<(ModelDocument, Package)> {
    if is_encrypted_package(bytes) {
        return Err(WmlError::new(
            "package.password_required",
            "This document is protected with a password. Pass the password to open it.",
        ));
    }
    if looks_like_zip(bytes) {
        let package =
            read_zip(bytes, budget).map_err(|err| WmlError::new(err.code, err.message))?;
        let doc = document_from_package(&package, budget)?;
        return Ok((doc, package));
    }
    const CFB: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
    if bytes.len() >= 8 && bytes[..8] == CFB {
        return Err(WmlError::new(
            "unsupported",
            "Binary .doc files are not read by this version. Save the file as a .docx package.",
        ));
    }
    let text = String::from_utf8_lossy(bytes);
    let lower = text.to_ascii_lowercase();
    if lower.contains("worddocument") && !lower.contains("pkg:package") {
        return Err(WmlError::new(
            "unsupported",
            "Word 2003 XML is not written by this version. Save the file as a .docx package.",
        ));
    }
    if lower.contains("pkg:package") || lower.contains("<package") {
        let package = flat::read_flat(bytes, budget)?;
        let doc = document_from_package(&package, budget)?;
        return Ok((doc, package));
    }
    Err(WmlError::new(
        "package.not_zip",
        "The bytes are not a ZIP package and not a flat OPC package.",
    ))
}

fn looks_like_zip(bytes: &[u8]) -> bool {
    bytes.len() >= 4 && bytes[0] == b'P' && bytes[1] == b'K'
}

pub fn clamp_cols(span: usize, max_cols: usize) -> usize {
    span.clamp(1, max_cols)
}

mod flat {
    use super::*;
    use edgedoc_opc::xml;
    use edgedoc_opc::{normalize, Part};
    use std::collections::BTreeMap;

    pub fn read_flat(bytes: &[u8], budget: &Budget) -> WmlResult<Package> {
        let root = xml::parse(bytes, budget).map_err(|err| WmlError::new(err.code, err.message))?;
        let mut parts = BTreeMap::new();
        for child in root.child_els() {
            if child.name != "part" {
                continue;
            }
            let Some(name) = child.attr("name") else {
                continue;
            };
            let content_type = child.attr("contentType").map(str::to_string);
            let xml_data = child.child_els().find(|el| el.name == "xmlData");
            let payload = if let Some(data) = xml_data {
                let inner = data.child_els().next();
                match inner {
                    Some(el) => el.raw(bytes).as_bytes().to_vec(),
                    None => data.text().into_bytes(),
                }
            } else {
                Vec::new()
            };
            let key = normalize(name);
            parts.insert(
                key,
                Part {
                    name: name.trim_start_matches('/').to_string(),
                    bytes: payload,
                    content_type,
                },
            );
        }
        if !parts.contains_key("[content_types].xml") {
            return Err(WmlError::new(
                "package.missing_part",
                "The flat package has no [Content_Types].xml part.",
            ));
        }
        let rel_bytes = parts
            .get("_rels/.rels")
            .map(|part| part.bytes.clone())
            .unwrap_or_default();
        let rels = edgedoc_opc::read_rels(&rel_bytes, budget)
            .map_err(|err| WmlError::new(err.code, err.message))?;
        Ok(Package { parts, rels })
    }
}
