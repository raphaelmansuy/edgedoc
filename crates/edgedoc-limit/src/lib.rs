//! One budget type for every EdgeDoc parser.
//!
//! The numbers match WordCraft `MAX_PART`, `MAX_TOTAL`, `MAX_ENTRIES`, and the XML caps in
//! `crates/docx/src/xml.rs`.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LimitError {
    pub code: &'static str,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Budget {
    pub max_part: u64,
    pub max_total: u64,
    pub max_entries: usize,
    pub max_depth: usize,
    pub max_elements: usize,
    pub max_attrs: usize,
    pub max_parts: usize,
    pub max_cols: usize,
    pub max_table_depth: usize,
    pub max_merged_blocks: usize,
    pub max_password_chars: usize,
    pub max_sheet_part: u64,
    pub max_sheet_rows: usize,
    pub max_sheet_cols: usize,
    pub max_sheet_cells: usize,
    pub max_cell_chars: usize,
    pub max_sheet_text: u64,
    pub max_label_chars: usize,
}

impl Default for Budget {
    fn default() -> Self {
        Self {
            max_part: 256 * 1024 * 1024,
            max_total: 512 * 1024 * 1024,
            max_entries: 20_000,
            max_depth: 256,
            max_elements: 8_000_000,
            max_attrs: 1_024,
            max_parts: 50_000,
            max_cols: 63,
            max_table_depth: 24,
            max_merged_blocks: 200_000,
            max_password_chars: 255,
            max_sheet_part: 64 * 1024 * 1024,
            max_sheet_rows: 100_000,
            max_sheet_cols: 1_000,
            max_sheet_cells: 2_000_000,
            max_cell_chars: 32_767,
            max_sheet_text: 64 * 1024 * 1024,
            max_label_chars: 2_000,
        }
    }
}

impl Budget {
    pub fn part(&self, observed: u64, what: &str) -> Result<(), LimitError> {
        if observed > self.max_part {
            return Err(self.over("package.limit", what, observed, self.max_part));
        }
        Ok(())
    }

    pub fn total(&self, observed: u64) -> Result<(), LimitError> {
        if observed > self.max_total {
            return Err(self.over(
                "package.limit",
                "decompressed package",
                observed,
                self.max_total,
            ));
        }
        Ok(())
    }

    pub fn entries(&self, observed: usize) -> Result<(), LimitError> {
        if observed > self.max_entries {
            return Err(self.over(
                "package.limit",
                "zip entries",
                observed as u64,
                self.max_entries as u64,
            ));
        }
        Ok(())
    }

    pub fn depth(&self, observed: usize) -> Result<(), LimitError> {
        if observed > self.max_depth {
            return Err(self.over(
                "xml.limit",
                "xml depth",
                observed as u64,
                self.max_depth as u64,
            ));
        }
        Ok(())
    }

    pub fn elements(&self, observed: usize) -> Result<(), LimitError> {
        if observed > self.max_elements {
            return Err(self.over(
                "xml.limit",
                "xml elements",
                observed as u64,
                self.max_elements as u64,
            ));
        }
        Ok(())
    }

    pub fn attrs(&self, observed: usize) -> Result<(), LimitError> {
        if observed > self.max_attrs {
            return Err(self.over(
                "xml.limit",
                "attributes",
                observed as u64,
                self.max_attrs as u64,
            ));
        }
        Ok(())
    }

    pub fn merged_blocks(&self, observed: usize) -> Result<(), LimitError> {
        if observed > self.max_merged_blocks {
            return Err(self.over(
                "merge.limit",
                "merged blocks",
                observed as u64,
                self.max_merged_blocks as u64,
            ));
        }
        Ok(())
    }

    pub fn font_size(&self, points: f32) -> Result<(), LimitError> {
        if !(1.0..=1638.0).contains(&points) || !points.is_finite() {
            return Err(LimitError {
                code: "model.limit",
                message: format!("The font size {points} is outside 1 to 1638 points. Choose a size in that range."),
            });
        }
        Ok(())
    }

    fn over(&self, code: &'static str, what: &str, observed: u64, max: u64) -> LimitError {
        LimitError {
            code,
            message: format!(
                "The {what} size is {observed}. The limit is {max}. Reduce the input."
            ),
        }
    }
}
