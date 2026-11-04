use std::collections::BTreeMap;
use std::io::{Cursor, Read, Write};

use edgedoc_limit::Budget;
use zip::write::FileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::error::{OpcError, OpcResult};
use crate::xml::{self, El};

pub const OFFICE_DOCUMENT: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument";
pub const CORE_PROPS: &str =
    "http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties";

#[derive(Debug, Clone)]
pub struct Part {
    pub name: String,
    pub bytes: Vec<u8>,
    pub content_type: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Rel {
    pub id: String,
    pub rel_type: String,
    pub target: String,
    pub external: bool,
}

#[derive(Debug, Clone)]
pub struct Package {
    pub parts: BTreeMap<String, Part>,
    pub rels: Vec<Rel>,
}

impl Package {
    pub fn part(&self, name: &str) -> Option<&Part> {
        self.parts.get(&normalize(name))
    }

    pub fn office_document(&self) -> OpcResult<&Part> {
        let rel = self
            .rels
            .iter()
            .find(|rel| rel.rel_type == OFFICE_DOCUMENT)
            .ok_or_else(|| {
                OpcError::new(
                    "package.not_word",
                    "The package has no office document relationship.",
                )
            })?;
        let name = normalize(rel.target.trim_start_matches('/'));
        self.parts.get(&name).ok_or_else(|| {
            OpcError::new(
                "package.missing_part",
                format!("The part {name} is missing."),
            )
        })
    }
}

pub fn normalize(name: &str) -> String {
    name.trim_start_matches('/')
        .replace('\\', "/")
        .to_ascii_lowercase()
}

pub fn read_zip(bytes: &[u8], budget: &Budget) -> OpcResult<Package> {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).map_err(|err| {
        OpcError::new(
            "package.not_zip",
            format!("The bytes are not a ZIP package. {err}"),
        )
    })?;
    budget.entries(archive.len()).map_err(map_limit)?;
    let mut parts = BTreeMap::new();
    let mut total = 0u64;
    for index in 0..archive.len() {
        let mut file = archive
            .by_index(index)
            .map_err(|err| OpcError::new("package.not_zip", err.to_string()))?;
        let name = file.name().to_string();
        if name.ends_with('/') {
            continue;
        }
        let declared = file.size();
        budget.part(declared, &name).map_err(map_limit)?;
        total = total.saturating_add(declared);
        budget.total(total).map_err(map_limit)?;
        let mut buf = Vec::new();
        file.by_ref()
            .take(budget.max_part)
            .read_to_end(&mut buf)
            .map_err(|err| OpcError::new("package.not_zip", err.to_string()))?;
        if file.size() > budget.max_part && buf.len() as u64 >= budget.max_part {
            return Err(OpcError::new(
                "package.limit",
                format!("The part {name} exceeds the size limit."),
            ));
        }
        let key = normalize(&name);
        if parts.contains_key(&key) {
            continue;
        }
        parts.insert(
            key,
            Part {
                name,
                bytes: buf,
                content_type: None,
            },
        );
    }
    if !parts.contains_key("[content_types].xml") {
        return Err(OpcError::new(
            "package.missing_part",
            "The part [Content_Types].xml is missing.",
        ));
    }
    apply_content_types(&mut parts, budget)?;
    let rels = read_rels(
        parts
            .get("_rels/.rels")
            .map(|part| part.bytes.as_slice())
            .unwrap_or(&[]),
        budget,
    )?;
    Ok(Package { parts, rels })
}

pub fn write_zip(parts: &[(String, Vec<u8>)]) -> OpcResult<Vec<u8>> {
    let mut cursor = Cursor::new(Vec::new());
    {
        let mut writer = ZipWriter::new(&mut cursor);
        let options = FileOptions::default().compression_method(CompressionMethod::Deflated);
        for (name, bytes) in parts {
            writer
                .start_file(name, options)
                .map_err(|err| OpcError::new("package.not_zip", err.to_string()))?;
            writer
                .write_all(bytes)
                .map_err(|err| OpcError::new("package.not_zip", err.to_string()))?;
        }
        writer
            .finish()
            .map_err(|err| OpcError::new("package.not_zip", err.to_string()))?;
    }
    Ok(cursor.into_inner())
}

fn apply_content_types(parts: &mut BTreeMap<String, Part>, budget: &Budget) -> OpcResult<()> {
    let bytes = parts
        .get("[content_types].xml")
        .map(|part| part.bytes.clone())
        .unwrap_or_default();
    let root = xml::parse(&bytes, budget)?;
    let mut defaults = Vec::new();
    let mut overrides = Vec::new();
    for child in root.child_els() {
        if child.name == "Default" {
            if let (Some(ext), Some(kind)) = (child.attr("Extension"), child.attr("ContentType")) {
                defaults.push((ext.to_ascii_lowercase(), kind.to_string()));
            }
        } else if child.name == "Override" {
            if let (Some(name), Some(kind)) = (child.attr("PartName"), child.attr("ContentType")) {
                overrides.push((normalize(name), kind.to_string()));
            }
        }
    }
    for part in parts.values_mut() {
        let key = normalize(&part.name);
        if let Some((_, kind)) = overrides.iter().find(|(name, _)| name == &key) {
            part.content_type = Some(kind.clone());
            continue;
        }
        if let Some(ext) = part.name.rsplit('.').next() {
            if let Some((_, kind)) = defaults
                .iter()
                .find(|(name, _)| name == &ext.to_ascii_lowercase())
            {
                part.content_type = Some(kind.clone());
            }
        }
    }
    Ok(())
}

pub fn read_rels(bytes: &[u8], budget: &Budget) -> OpcResult<Vec<Rel>> {
    if bytes.is_empty() {
        return Ok(Vec::new());
    }
    let root = xml::parse(bytes, budget)?;
    Ok(rels_from_el(&root))
}

pub fn rels_from_el(root: &El) -> Vec<Rel> {
    root.child_els()
        .filter(|el| el.name == "Relationship")
        .filter_map(|el| {
            Some(Rel {
                id: el.attr("Id")?.to_string(),
                rel_type: el.attr("Type")?.to_string(),
                target: el.attr("Target")?.to_string(),
                external: el
                    .attr("TargetMode")
                    .map(|mode| mode.eq_ignore_ascii_case("External"))
                    .unwrap_or(false),
            })
        })
        .collect()
}

fn map_limit(err: edgedoc_limit::LimitError) -> OpcError {
    OpcError::new(err.code, err.message)
}

pub fn is_encrypted_package(bytes: &[u8]) -> bool {
    const CFB: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
    if bytes.len() < 8 || bytes[..8] != CFB {
        return false;
    }
    let utf16 = encode_utf16_bytes("EncryptionInfo");
    bytes
        .windows(utf16.len())
        .any(|window| window == utf16.as_slice())
}

fn encode_utf16_bytes(text: &str) -> Vec<u8> {
    text.encode_utf16()
        .flat_map(|unit| unit.to_le_bytes())
        .collect()
}
