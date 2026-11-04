//! One field-instruction parser. The reader, the builder, and the merge engine call it.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Instruction {
    pub keyword: String,
    pub raw: String,
}

/// A legacy form field (`w:ffData`), which lives inside a `w:fldChar` begin marker.
///
/// Word still writes these when a document has form fields rather than content
/// controls. The model keeps the settings so a save does not turn the field into a
/// plain text run.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FormField {
    /// `w:name/@w:val`: the bookmark name that the field value is stored under.
    pub name: String,
    pub enabled: bool,
    /// `w:calcOnExit`: recalculate the field when the reader leaves it.
    pub calc_on_exit: bool,
    pub help: Option<String>,
    pub status: Option<String>,
    pub kind: FormKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormKind {
    /// `w:textInput`.
    Text {
        /// `w:type`: `regular`, `number`, `date`, `currentTime`, or `currentDate`.
        kind: Option<String>,
        default: Option<String>,
        max_length: Option<u32>,
        format: Option<String>,
    },
    /// `w:checkBox`.
    CheckBox {
        size: Option<u32>,
        size_auto: bool,
        default: Option<bool>,
        checked: Option<bool>,
    },
    /// `w:ddList`.
    DropDown {
        result: Option<u32>,
        entries: Vec<String>,
    },
}

impl Default for FormKind {
    fn default() -> Self {
        FormKind::Text {
            kind: None,
            default: None,
            max_length: None,
            format: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    pub instr: String,
    pub result: String,
    /// `w:fldChar/@w:dirty`. Word recalculates the result the next time it opens the file.
    pub dirty: bool,
    /// `w:fldChar/@w:fldLock`. Word keeps the cached result and does not recalculate.
    pub locked: bool,
    /// The legacy form-field settings, when the begin marker holds `w:ffData`.
    pub form: Option<FormField>,
}

impl Field {
    pub fn new(instr: &str, result: &str) -> Self {
        Self {
            instr: instr.to_string(),
            result: result.to_string(),
            dirty: false,
            locked: false,
            form: None,
        }
    }

    /// Mark the field for recalculation, as Word's F9 does on the next open.
    pub fn dirty(mut self) -> Self {
        self.dirty = true;
        self
    }

    /// Keep the cached result. Word does not recalculate a locked field.
    pub fn locked(mut self) -> Self {
        self.locked = true;
        self
    }

    /// Attach the legacy form-field settings.
    pub fn form(mut self, form: FormField) -> Self {
        self.form = Some(form);
        self
    }

    /// True when the field is a legacy form field rather than a field code.
    pub fn is_form_field(&self) -> bool {
        self.form.is_some()
    }
}

impl Instruction {
    pub fn parse(raw: &str) -> Self {
        let trimmed = raw.trim();
        let split_at = trimmed
            .find(|c: char| c.is_whitespace() || c == '"')
            .unwrap_or(trimmed.len());
        let keyword = trimmed[..split_at].to_ascii_uppercase();
        Self {
            keyword,
            raw: raw.to_string(),
        }
    }

    /// `MERGEFIELD` only. `MERGEFIELDX` is a different keyword.
    pub fn merge_name(&self) -> Option<String> {
        if self.keyword != "MERGEFIELD" {
            return None;
        }
        let rest = self.raw.trim();
        let rest = rest.get(self.keyword.len()..).unwrap_or("").trim();
        let name = if let Some(quoted) = rest.strip_prefix('"') {
            quoted.split('"').next().unwrap_or("").to_string()
        } else {
            rest.split_whitespace().next().unwrap_or("").to_string()
        };
        if name.is_empty() {
            None
        } else {
            Some(name)
        }
    }
}

pub fn quote_name(name: &str) -> String {
    if name.contains(' ') {
        format!("MERGEFIELD \"{name}\"")
    } else {
        format!("MERGEFIELD {name}")
    }
}

pub fn placeholder(name: &str) -> String {
    format!("«{name}»")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mergefieldx_is_not_a_merge_field() -> Result<(), String> {
        let instr = Instruction::parse("MERGEFIELDX Name");
        if instr.merge_name().is_some() {
            return Err("MERGEFIELDX was treated as MERGEFIELD".into());
        }
        Ok(())
    }

    #[test]
    fn quoted_name() -> Result<(), String> {
        let instr = Instruction::parse("MERGEFIELD \"Invoice Number\" \\* MERGEFORMAT");
        if instr.merge_name().as_deref() != Some("Invoice Number") {
            return Err(format!("{:?}", instr.merge_name()));
        }
        Ok(())
    }
}
