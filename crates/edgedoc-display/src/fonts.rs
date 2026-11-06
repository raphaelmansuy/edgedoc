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
    (
        "Arimo",
        include_bytes!("../../../assets/fonts/subset/Arimo-Regular.ttf"),
    ),
    (
        "Arimo-Bold",
        include_bytes!("../../../assets/fonts/subset/Arimo-Bold.ttf"),
    ),
    (
        "Arimo-Italic",
        include_bytes!("../../../assets/fonts/subset/Arimo-Italic.ttf"),
    ),
    (
        "Arimo-BoldItalic",
        include_bytes!("../../../assets/fonts/subset/Arimo-BoldItalic.ttf"),
    ),
    (
        "Tinos",
        include_bytes!("../../../assets/fonts/subset/Tinos-Regular.ttf"),
    ),
    (
        "Tinos-Bold",
        include_bytes!("../../../assets/fonts/subset/Tinos-Bold.ttf"),
    ),
    (
        "Tinos-Italic",
        include_bytes!("../../../assets/fonts/subset/Tinos-Italic.ttf"),
    ),
    (
        "Tinos-BoldItalic",
        include_bytes!("../../../assets/fonts/subset/Tinos-BoldItalic.ttf"),
    ),
    (
        "Source Sans 3",
        include_bytes!("../../../assets/fonts/SourceSans3-Regular.ttf"),
    ),
];

/// Load the built-in faces. Each face answers to the Word names that it substitutes, so a
/// document that asks for Calibri paints with Carlito and reports one diagnostic.
pub fn load_builtin_fonts(db: &mut FontDb) {
    for (label, bytes) in FONT_FAMILIES {
        let bold = label.contains("Bold");
        let italic = label.contains("Italic");
        let requests: &[&str] = if label.starts_with("Arimo") {
            &["Arial", "Helvetica", "Helvetica Neue", "Arimo"]
        } else if label.starts_with("Tinos") {
            &["Times New Roman", "Times", "Tinos"]
        } else if label.starts_with("Source Sans") {
            &["Source Sans 3"]
        } else {
            &[
                "Calibri",
                "Aptos",
                "Aptos Display",
                "Calibri Light",
                "Carlito",
            ]
        };
        if let Some(id) = db.load(
            bytes.to_vec(),
            Some(requests[0]),
            bold,
            italic,
            !label.starts_with("Source Sans"),
        ) {
            for name in requests {
                db.alias(name, bold, italic, id);
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
        assert_eq!(db.len(), 13, "faces {}", db.len());
        assert!(db.resolve("Source Sans 3", false, false).is_some());
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
    fn serif_and_sans_substitutes_keep_each_family_and_style() {
        let mut db = FontDb::new();
        load_builtin_fonts(&mut db);
        for (requested, actual) in [
            ("Arial", "Arimo"),
            ("Times New Roman", "Tinos"),
            ("Cambria", "Tinos"),
        ] {
            for (bold, italic) in [(false, false), (true, false), (false, true), (true, true)] {
                let face = db
                    .face(db.resolve(requested, bold, italic).expect("face ID"))
                    .expect("face");
                assert_eq!(face.family, actual);
                assert_eq!((face.bold, face.italic), (bold, italic));
            }
        }
    }

    #[test]
    fn the_embedded_names_include_every_style() {
        let names = embedded_face_names();
        assert_eq!(names.len(), 13);
        assert!(names.contains(&"Source Sans 3".to_string()));
    }

    #[test]
    fn supplied_metric_faces_take_precedence_over_builtin_substitutes() {
        let mut db = FontDb::new();
        load_builtin_fonts(&mut db);
        // Use open font bytes to exercise application-supplied font registrations.
        for (request, substitute, bytes) in [
            (
                "Arial",
                "Liberation Sans",
                include_bytes!("../../../assets/fonts/subset/Arimo-Regular.ttf").as_slice(),
            ),
            (
                "Times New Roman",
                "Liberation Serif",
                include_bytes!("../../../assets/fonts/subset/Tinos-Regular.ttf").as_slice(),
            ),
        ] {
            let supplied = db.load(bytes.to_vec(), Some(request), false, false, false);
            assert!(supplied.is_some());
            assert_eq!(db.resolve(substitute, false, false), supplied);
        }
    }

    #[test]
    fn the_substitute_table_names_a_metric_face() {
        assert_eq!(edgedoc_fonts::substitute_for("Calibri"), Some("Carlito"));
        assert_eq!(edgedoc_fonts::substitute_for("No Such Face"), None);
    }
}
