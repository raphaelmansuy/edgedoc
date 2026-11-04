//! ZIP package, content types, and relationships.
//! This crate does not know about paragraphs.

mod error;
mod package;
pub mod xml;

pub use error::{OpcError, OpcResult};
pub use package::{
    is_encrypted_package, normalize, read_rels, read_zip, rels_from_el, write_zip, Package, Part,
    Rel, CORE_PROPS, OFFICE_DOCUMENT,
};
pub use xml::escape;

#[cfg(test)]
mod tests {
    use super::*;
    use edgedoc_limit::Budget;

    #[test]
    fn memory_package_round_trips_part_bytes() -> OpcResult<()> {
        let bytes = write_zip(&[
            ("[Content_Types].xml".into(), b"<Types></Types>".to_vec()),
            ("_rels/.rels".into(), br#"<Relationships><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#.to_vec()),
            ("word/document.xml".into(), b"<document>Hi</document>".to_vec()),
        ])?;
        let package = read_zip(&bytes, &Budget::default())?;
        let part = package
            .part("word/document.xml")
            .ok_or_else(|| OpcError::new("package.missing_part", "missing"))?;
        if part.bytes != b"<document>Hi</document>" {
            return Err(OpcError::new("package.missing_part", "bytes changed"));
        }
        if package.rels.len() != 1 || package.rels[0].id != "rId1" {
            return Err(OpcError::new(
                "package.missing_part",
                "relationship changed",
            ));
        }
        Ok(())
    }

    #[test]
    fn part_names_are_case_insensitive() -> OpcResult<()> {
        let bytes = write_zip(&[
            ("[Content_Types].xml".into(), b"<Types></Types>".to_vec()),
            (
                "Word/Document.xml".into(),
                b"<document>A</document>".to_vec(),
            ),
            (
                "word/document.xml".into(),
                b"<document>B</document>".to_vec(),
            ),
        ])?;
        let package = read_zip(&bytes, &Budget::default())?;
        if package.parts.len() != 2 {
            return Err(OpcError::new(
                "package.limit",
                format!("parts {}", package.parts.len()),
            ));
        }
        if package
            .part("WORD/DOCUMENT.XML")
            .map(|part| part.bytes.as_slice())
            != Some(b"<document>A</document>".as_slice())
        {
            return Err(OpcError::new(
                "package.limit",
                "the first case variant was not kept",
            ));
        }
        Ok(())
    }

    #[test]
    fn zip_budget_rejects_a_huge_declared_size() -> OpcResult<()> {
        let bytes = write_zip(&[("[Content_Types].xml".into(), b"<Types></Types>".to_vec())])?;
        let budget = Budget {
            max_part: 4,
            max_total: 4,
            ..Budget::default()
        };
        match read_zip(&bytes, &budget) {
            Err(err) if err.code == "package.limit" => Ok(()),
            Ok(_) => Err(OpcError::new(
                "package.limit",
                "the budget accepted the part",
            )),
            Err(err) => Err(err),
        }
    }
}
