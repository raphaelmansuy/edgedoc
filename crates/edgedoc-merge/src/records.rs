use edgedoc_limit::Budget;

use crate::MergeError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MissingField {
    Empty,
    Placeholder,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Equal,
    NotEqual,
    Less,
    Greater,
    LessOrEqual,
    GreaterOrEqual,
    Blank,
    NotBlank,
    Contains,
    NotContains,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    pub field: String,
    pub op: Op,
    pub value: String,
    pub and: bool,
}

impl Rule {
    pub fn new(field: &str, op: Op, value: &str) -> Self {
        Self {
            field: field.into(),
            op,
            value: value.into(),
            and: false,
        }
    }

    pub fn and(mut self) -> Self {
        self.and = true;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SortKey {
    pub field: String,
    pub descending: bool,
}

impl SortKey {
    pub fn new(field: &str) -> Self {
        Self {
            field: field.into(),
            descending: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    headers: Vec<String>,
    cells: Vec<String>,
}

impl Record {
    pub fn get(&self, field: &str) -> Option<&str> {
        let index = self
            .headers
            .iter()
            .position(|header| header.eq_ignore_ascii_case(field))?;
        self.cells.get(index).map(String::as_str)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Records {
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
}

#[derive(Debug, Clone, Default)]
pub struct CsvOptions {
    pub encoding_label: Option<String>,
}

impl Records {
    pub fn from_rows(headers: Vec<String>, rows: Vec<Vec<String>>) -> Result<Self, MergeError> {
        let mut seen = Vec::new();
        for (index, header) in headers.iter().enumerate() {
            if let Some(first) = seen
                .iter()
                .position(|have: &String| have.eq_ignore_ascii_case(header))
            {
                return Err(MergeError::new(
                    "merge.duplicate_header",
                    format!(
                        "The header \"{header}\" is repeated at columns {} and {}.",
                        first + 1,
                        index + 1
                    ),
                ));
            }
            seen.push(header.clone());
        }
        Ok(Self { headers, rows })
    }

    pub fn from_csv(bytes: &[u8], _options: &CsvOptions) -> Result<Self, MergeError> {
        let text = decode_csv(bytes);
        let table = parse_csv(&text)?;
        let mut rows = table.into_iter();
        let headers = rows.next().unwrap_or_default();
        let mut records = Vec::new();
        for row in rows {
            if row.iter().all(|cell| cell.is_empty()) {
                continue;
            }
            if row.len() > headers.len() {
                return Err(MergeError::new(
                    "merge.extra_column",
                    "A CSV record has more cells than headers. Remove the extra cells.",
                ));
            }
            let mut cells = row;
            while cells.len() < headers.len() {
                cells.push(String::new());
            }
            records.push(cells);
        }
        Self::from_rows(headers, records)
    }

    pub fn filter(mut self, rules: &[Rule]) -> Result<Self, MergeError> {
        self.rows.retain(|row| keep_row(&self.headers, row, rules));
        Ok(self)
    }

    pub fn sort(mut self, keys: &[SortKey]) -> Result<Self, MergeError> {
        self.rows.sort_by(|left, right| {
            for key in keys {
                let index = self
                    .headers
                    .iter()
                    .position(|header| header.eq_ignore_ascii_case(&key.field));
                let a = index
                    .and_then(|index| left.get(index))
                    .map(String::as_str)
                    .unwrap_or("");
                let b = index
                    .and_then(|index| right.get(index))
                    .map(String::as_str)
                    .unwrap_or("");
                let order = a.cmp(b);
                let order = if key.descending {
                    order.reverse()
                } else {
                    order
                };
                if order != std::cmp::Ordering::Equal {
                    return order;
                }
            }
            std::cmp::Ordering::Equal
        });
        Ok(self)
    }

    pub fn headers(&self) -> &[String] {
        &self.headers
    }

    pub fn rows(&self) -> &[Vec<String>] {
        &self.rows
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    pub fn cell(&self, row: usize, field: &str) -> Option<String> {
        let index = self
            .headers
            .iter()
            .position(|header| header.eq_ignore_ascii_case(field))?;
        if row >= self.rows.len() {
            return Some(String::new());
        }
        Some(
            self.rows
                .get(row)
                .and_then(|cells| cells.get(index))
                .cloned()
                .unwrap_or_default(),
        )
    }

    pub fn record(&self, row: usize) -> Record {
        Record {
            headers: self.headers.clone(),
            cells: self.rows.get(row).cloned().unwrap_or_default(),
        }
    }

    /// One record built from a header list and a row of cells.
    ///
    /// specs/144 DOC2-03: a dataset arrives as JSON, so a caller assembles a record set without
    /// encoding CSV. The duplicate-header law is the same one `from_rows` enforces.
    pub fn from_single(headers: Vec<String>, cells: Vec<String>) -> Result<Self, MergeError> {
        Self::from_rows(headers, vec![cells])
    }
}

fn keep_row(headers: &[String], row: &[String], rules: &[Rule]) -> bool {
    if rules.is_empty() {
        return true;
    }
    let mut acc = true;
    let mut have = false;
    for rule in rules {
        let index = headers
            .iter()
            .position(|header| header.eq_ignore_ascii_case(&rule.field));
        let cell = index
            .and_then(|index| row.get(index))
            .map(String::as_str)
            .unwrap_or("");
        let this = match rule.op {
            Op::Equal => cell == rule.value,
            Op::NotEqual => cell != rule.value,
            Op::Less => cell < rule.value.as_str(),
            Op::Greater => cell > rule.value.as_str(),
            Op::LessOrEqual => cell <= rule.value.as_str(),
            Op::GreaterOrEqual => cell >= rule.value.as_str(),
            Op::Blank => cell.is_empty(),
            Op::NotBlank => !cell.is_empty(),
            Op::Contains => cell.contains(&rule.value),
            Op::NotContains => !cell.contains(&rule.value),
        };
        if !have {
            acc = this;
            have = true;
        } else if rule.and {
            acc = acc && this;
        } else {
            acc = acc || this;
        }
    }
    acc
}

fn decode_csv(bytes: &[u8]) -> String {
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        String::from_utf8_lossy(&bytes[3..]).into_owned()
    } else {
        String::from_utf8_lossy(bytes).into_owned()
    }
}

fn parse_csv(text: &str) -> Result<Vec<Vec<String>>, MergeError> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut cell = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if quoted {
            if ch == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    cell.push('"');
                } else {
                    quoted = false;
                }
            } else {
                cell.push(ch);
            }
            continue;
        }
        match ch {
            '"' => quoted = true,
            ',' => {
                row.push(std::mem::take(&mut cell));
            }
            '\n' => {
                row.push(std::mem::take(&mut cell));
                rows.push(std::mem::take(&mut row));
            }
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                row.push(std::mem::take(&mut cell));
                rows.push(std::mem::take(&mut row));
            }
            _ => cell.push(ch),
        }
    }
    if !cell.is_empty() || !row.is_empty() {
        row.push(cell);
        rows.push(row);
    }
    let _ = Budget::default();
    Ok(rows)
}
