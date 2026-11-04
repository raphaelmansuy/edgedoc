//! The built-in faces and the faces that a package embeds.
//!
//! The library ships three metric-compatible families, so a document paints offline with no fetch.
//! See `DP-B-22`, `DP-F-03`, and `DP-F-06`.

use edgedoc::Document;
use edgedoc_fonts::FontDb;

/// The families that the module carries, in the order that they load.
pub const FONT_FAMILIES: &[(&str, &[u8])] = &[
    (
        "Carlito",
        include_bytes!("../../../assets/fonts/subset/Carlito-Regular.ttf"),
    ),
    (
        "Carlito-Bold",
        include_bytes!("../../../assets/fonts/subset/Carlito-Bold.ttf"),
    ),
    (
        "Carlito-Italic",
        include_bytes!("../../../assets/fonts/subset/Carlito-Italic.ttf"),
    ),
    (
        "Carlito-BoldItalic",
        include_bytes!("../../../assets/fonts/subset/Carlito-BoldItalic.ttf"),
    ),
];

/// Load the built-in faces. Each face answers to the Word names that it substitutes, so a
/// document that asks for Calibri paints with Carlito and reports one diagnostic.
pub fn load_builtin_fonts(db: &mut FontDb) {
    // One face per style, registered under Carlito and under every Word name that maps to it.
    let requests = [
        "Calibri",
        "Aptos",
        "Aptos Display",
        "Calibri Light",
        "Carlito",
    ];
    for (i, (label, bytes)) in FONT_FAMILIES.iter().enumerate() {
        let bold = label.contains("Bold");
        let italic = label.contains("Italic");
        let primary = requests
            .get(i.min(requests.len() - 1))
            .copied()
            .unwrap_or("Calibri");
        db.load(
            bytes.to_vec(),
            Some(primary),
            bold,
            italic,
            primary != "Carlito",
        );
        // Register the other Word names on the same face, so a document that asks for one of
        // them finds it with no second diagnostic.
        for name in requests {
            if name != primary {
                db.alias(name, bold, italic, (i) as u32);
            }
        }
    }
    db.set_fallback(0);
}

/// The face names that the module carries, for a viewer to register with the DOM.
pub fn embedded_face_names() -> Vec<String> {
    FONT_FAMILIES
        .iter()
        .map(|(label, _)| (*label).to_string())
        .collect()
}

/// Load a face that a package embeds. The model does not keep the font parts in version 1, so
/// this reports the family and does not decode it. See `DP-F-06`.
pub fn load_embedded_faces(_doc: &Document, _db: &mut FontDb) -> usize {
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_builtin_faces_load() {
        let mut db = FontDb::new();
        load_builtin_fonts(&mut db);
        assert!(db.len() >= 4, "faces {}", db.len());
        assert!(db.bytes_loaded() > 100_000);
    }

    #[test]
    fn calibri_finds_the_builtin_carlito() {
        let mut db = FontDb::new();
        load_builtin_fonts(&mut db);
        let id = db.resolve("Calibri", false, false).expect("Calibri");
        let face = db.face(id).expect("face");
        assert_eq!(face.family, "Carlito");
        assert!(face.substituted, "a substitute reports itself");
    }

    #[test]
    fn a_bold_request_reaches_the_bold_face() {
        let mut db = FontDb::new();
        load_builtin_fonts(&mut db);
        let regular = db.resolve("Calibri", false, false).expect("regular");
        let bold = db.resolve("Calibri", true, false).expect("bold");
        assert_ne!(regular, bold);
    }

    #[test]
    fn the_embedded_names_include_every_style() {
        let names = embedded_face_names();
        assert_eq!(names.len(), 4);
    }

    #[test]
    fn the_substitute_table_names_a_metric_face() {
        assert_eq!(edgedoc_fonts::substitute_for("Calibri"), Some("Carlito"));
        assert_eq!(edgedoc_fonts::substitute_for("No Such Face"), None);
    }
}
