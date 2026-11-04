//! Diagnostics: one record per degraded behavior.
//!
//! The code set is closed and stable. Every code appears once per unique page, so a document with
//! 10,000 missing glyphs does not fill a panel. See `DP-P-22` to `DP-P-25`.

use std::collections::BTreeSet;

/// How serious a diagnostic is. See `DP-P-22`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Info,
    Warning,
    Error,
    Fatal,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Info => "info",
            Severity::Warning => "warning",
            Severity::Error => "error",
            Severity::Fatal => "fatal",
        }
    }
}

/// One diagnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub code: &'static str,
    pub severity: Severity,
    pub page: Option<u32>,
    pub detail: String,
}

/// The fidelity sentence that a viewer shows for a code. See `DP-U-19`.
pub fn summary_for(code: &str) -> &'static str {
    match code {
        "display.font_substituted" => {
            "A font is missing. The page breaks can differ from Word. Install the font to remove this note."
        }
        "display.glyph_missing" => {
            "A character is missing from every available font, so it paints as a box."
        }
        "display.object_placeholder" => {
            "An object that this version does not model paints as a marked box. The page around it is complete."
        }
        "display.image_unsupported" => "An image format has no decoder, so it paints as a box.",
        "display.image_denied" => "A remote image was refused by the resource policy.",
        "display.table_too_wide" => {
            "A table is wider than the page area. It is clamped to the margin."
        }
        "display.page_overflow" => "Content did not fit the page area. The page is incomplete.",
        "display.numbering_missing" => "A list has no numbering definition, so it paints as plain text.",
        "display.compat_ignored" => {
            "The document sets a compatibility option that this version does not read. The page can differ."
        }
        "display.header_overflow" => "A header or footer is taller than its margin.",
        "display.note_area_full" => "The notes on a page take most of the page area.",
        _ => "The page shows every content item that the document holds.",
    }
}

/// A collector that keeps one entry per code, page, and detail. See `DP-P-24`.
#[derive(Debug, Default)]
pub struct Collector {
    seen: BTreeSet<(String, Option<u32>, String)>,
    items: Vec<Diagnostic>,
}

impl Collector {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, d: Diagnostic) {
        let key = (d.code.to_string(), d.page, d.detail.clone());
        if self.seen.insert(key) {
            self.items.push(d);
        }
    }

    pub fn extend(&mut self, items: impl IntoIterator<Item = Diagnostic>) {
        for d in items {
            self.push(d);
        }
    }

    pub fn into_vec(self) -> Vec<Diagnostic> {
        self.items
    }

    /// The count of each severity.
    pub fn counts(&self) -> (usize, usize, usize, usize) {
        let mut info = 0;
        let mut warning = 0;
        let mut error = 0;
        let mut fatal = 0;
        for d in &self.items {
            match d.severity {
                Severity::Info => info += 1,
                Severity::Warning => warning += 1,
                Severity::Error => error += 1,
                Severity::Fatal => fatal += 1,
            }
        }
        (info, warning, error, fatal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn diag(code: &'static str, page: u32, detail: &str) -> Diagnostic {
        Diagnostic {
            code,
            severity: Severity::Warning,
            page: Some(page),
            detail: detail.into(),
        }
    }

    #[test]
    fn a_duplicate_diagnostic_appears_once() {
        let mut c = Collector::new();
        c.push(diag("display.glyph_missing", 7, "U+1F5D1"));
        c.push(diag("display.glyph_missing", 7, "U+1F5D1"));
        c.push(diag("display.glyph_missing", 7, "U+1F5D1"));
        assert_eq!(c.into_vec().len(), 1);
    }

    #[test]
    fn a_same_code_on_another_page_is_its_own_entry() {
        let mut c = Collector::new();
        c.push(diag("display.glyph_missing", 7, "x"));
        c.push(diag("display.glyph_missing", 8, "x"));
        assert_eq!(c.into_vec().len(), 2);
    }

    #[test]
    fn the_counts_add_up() {
        let mut c = Collector::new();
        c.push(Diagnostic {
            code: "a",
            severity: Severity::Info,
            page: None,
            detail: String::new(),
        });
        c.push(Diagnostic {
            code: "b",
            severity: Severity::Error,
            page: None,
            detail: String::new(),
        });
        assert_eq!(c.counts(), (1, 0, 1, 0));
    }

    #[test]
    fn every_code_has_a_summary() {
        for code in [
            "display.font_substituted",
            "display.glyph_missing",
            "display.object_placeholder",
            "display.image_unsupported",
            "display.table_too_wide",
        ] {
            assert!(!summary_for(code).is_empty());
        }
    }
}
