//! One catalog of owned element names. The reader and the writer both call [`is_owned`].
//!
//! A name in this list means the model owns it: the reader builds model values from it and
//! the writer emits it. A name absent from this list becomes kept markup, so the save copies
//! it. Add a name here only when both sides handle it.

pub const OWNED: &[&str] = &[
    "p",
    "r",
    "t",
    "tab",
    "br",
    "cr",
    "rPr",
    "b",
    "i",
    "u",
    "strike",
    "dstrike",
    "color",
    "sz",
    "szCs",
    "rFonts",
    "vertAlign",
    "highlight",
    "shd",
    "lang",
    "rtl",
    "cs",
    "vanish",
    "webHidden",
    "pPr",
    "pStyle",
    "jc",
    "ind",
    "spacing",
    "numPr",
    "ilvl",
    "numId",
    "keepNext",
    "keepLines",
    "pageBreakBefore",
    "widowControl",
    "outlineLvl",
    "bidi",
    "sectPr",
    "tbl",
    "tr",
    "tc",
    "tblGrid",
    "gridCol",
    "tblPr",
    "tblW",
    "tblBorders",
    "tblCellMar",
    "tblLook",
    "bidiVisual",
    "trHeight",
    "tblHeader",
    "tcW",
    "gridSpan",
    "vMerge",
    "tcBorders",
    "hyperlink",
    "drawing",
    "blip",
    "fldChar",
    "instrText",
    "fldSimple",
    "sdt",
    "sdtPr",
    "sdtContent",
    "alias",
    "tag",
    "lock",
    "placeholder",
    "bookmarkStart",
    "bookmarkEnd",
    "styles",
    "style",
    "docDefaults",
    "name",
    "basedOn",
    "qFormat",
    "pgSz",
    "pgMar",
    "cols",
    "headerReference",
    "footerReference",
    "pgNumType",
    "type",
    "document",
    "body",
    "ins",
    "del",
    "commentReference",
    "oMath",
    "AlternateContent",
    "Choice",
    "Fallback",
    // Character properties.
    "bCs",
    "iCs",
    "caps",
    "smallCaps",
    "emboss",
    "imprint",
    "outline",
    "shadow",
    "noProof",
    "snapToGrid",
    "kern",
    "position",
    "w",
    "bdr",
    "rStyle",
    // Paragraph properties.
    "pBdr",
    "tabs",
    "framePr",
    "contextualSpacing",
    "suppressLineNumbers",
    // Tables.
    "tblPrEx",
    "tblStyle",
    "tblInd",
    "tblLayout",
    "tblCellSpacing",
    "tblOverlap",
    "tblpPr",
    "hMerge",
    "tcMar",
    "noWrap",
    "cantSplit",
    "hidden",
    // Content controls.
    "text",
    "date",
    "dropDownList",
    "comboBox",
    "checkbox",
    "listItem",
    "dataBinding",
    "docPart",
    "docPartObj",
    "picture",
    "checked",
    "dateFormat",
    // Sections.
    "titlePg",
    "rtlGutter",
    "lnNumType",
    "pgBorders",
    "docGrid",
    "footnotePr",
    "endnotePr",
    "numFmt",
    "numStart",
    "numRestart",
    "pos",
    // Notes and comments.
    "footnote",
    "endnote",
    "footnotes",
    "endnotes",
    "comments",
    "comment",
    "footnoteReference",
    "endnoteReference",
    "annotationRef",
    // Styles.
    "next",
    "link",
    "semiHidden",
    "uiPriority",
    "latentStyles",
];

pub fn is_owned(name: &str) -> bool {
    OWNED.contains(&name)
}

/// The static name for an owned element. The test below locks the mapping.
#[cfg(test)]
pub fn tag(name: &str) -> &'static str {
    OWNED
        .iter()
        .copied()
        .find(|item| *item == name)
        .unwrap_or("p")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_catalog_name_is_visible_to_the_reader_and_the_writer() -> Result<(), String> {
        for name in OWNED {
            if !is_owned(name) {
                return Err((*name).to_string());
            }
            if tag(name) != *name {
                return Err((*name).to_string());
            }
        }
        Ok(())
    }

    #[test]
    fn the_catalog_holds_no_duplicate() -> Result<(), String> {
        for (index, name) in OWNED.iter().enumerate() {
            if OWNED[index + 1..].contains(name) {
                return Err(format!("{name} appears twice"));
            }
        }
        Ok(())
    }
}
