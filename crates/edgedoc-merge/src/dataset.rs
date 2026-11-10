//! A dataset for a merge, and the small JSON reader that reads one (specs/144 DOC2-03).
//!
//! The wire shape is closed:
//!
//! ```text
//! { "records": [ { "Name": "Ada", "Total__c": "120.00" } ],
//!   "repeats": { "Lines": [ [ { "Name": "Widget", "Quantity__c": "2" } ] ] } }
//! ```
//!
//! `records[i]` is parent `i`'s scalar map; `repeats.Lines[i]` is parent `i`'s child rows.
//!
//! The reader accepts exactly the JSON a host emits: objects, arrays, strings, numbers,
//! booleans, and null. It refuses anything else by name. It is not a general JSON parser and it is
//! not allowed to become one — a host that needs another shape is describing a different feature.
//! No dependency is added: the adapter must stay small enough for a browser bundle, and the grammar
//! here is smaller than the cost of a parser crate.

use std::collections::BTreeMap;

use edgedoc_limit::Budget;
use edgedoc_model::ModelDocument;

use crate::{
    finish, join_documents, Join, MergeError, MergeKind, MergeRequest, MissingField, Records, Repeats,
};

/// `code message`, the shape every adapter renders.
fn rendered(error: MergeError) -> String {
    format!("{} {}", error.code, error.message)
}

/// A merge of many parents, with each parent's child rows.
#[derive(Debug, Clone, Default)]
pub struct Dataset {
    /// Every parent, in the order the host gave them.
    pub parents: Vec<BTreeMap<String, String>>,
    /// `repeats[alias][parent]` — the child rows of one parent for one band.
    pub repeats: BTreeMap<String, Vec<Vec<BTreeMap<String, String>>>>,
}

/// How to merge a dataset.
#[derive(Debug, Clone)]
pub struct DatasetOptions {
    pub kind: MergeKind,
    pub join: Join,
    pub missing: MissingField,
    pub budget: Budget,
    /// The first parent to merge (0-based). `None` is 0.
    pub from: Option<usize>,
    /// One past the last parent. `None` is every parent.
    pub to: Option<usize>,
}

impl Default for DatasetOptions {
    fn default() -> Self {
        Self {
            kind: MergeKind::Letters,
            join: Join::Auto,
            missing: MissingField::Error,
            budget: Budget::default(),
            from: None,
            to: None,
        }
    }
}

/// Read a dataset from the JSON text a host sends.
pub fn dataset_from_json(text: &str) -> Result<Dataset, String> {
    let value = parse(text)?;
    let root = value.as_object("the dataset")?;
    let mut dataset = Dataset::default();
    if let Some(records) = root.get("records") {
        let rows = records.as_array("\"records\"")?;
        for (index, row) in rows.iter().enumerate() {
            dataset
                .parents
                .push(row.as_string_map(&format!("records[{index}]"))?);
        }
    }
    if let Some(repeats) = root.get("repeats") {
        let bands = repeats.as_object("\"repeats\"")?;
        for (alias, groups) in bands {
            let parents = groups.as_array(&format!("repeats.{alias}"))?;
            let mut by_parent = Vec::with_capacity(parents.len());
            for (index, children) in parents.iter().enumerate() {
                let rows = children.as_array(&format!("repeats.{alias}[{index}]"))?;
                let mut set = Vec::with_capacity(rows.len());
                for (row, child) in rows.iter().enumerate() {
                    set.push(child.as_string_map(&format!("repeats.{alias}[{index}][{row}]"))?);
                }
                by_parent.push(set);
            }
            dataset.repeats.insert(alias.clone(), by_parent);
        }
    }
    if dataset.parents.is_empty() {
        return Err("merge.no_records The merge has no records.".to_string());
    }
    Ok(dataset)
}

/// Read the options a host sends. An unknown kind or join is a named refusal.
pub fn options_from_json(text: &str) -> Result<DatasetOptions, String> {
    let value = parse(text)?;
    let root = value.as_object("the options")?;
    let mut options = DatasetOptions::default();
    if let Some(kind) = root.get("kind") {
        options.kind = merge_kind(kind.as_str("\"kind\"")?)?;
    }
    if let Some(join) = root.get("join") {
        options.join = join_mode(join.as_str("\"join\"")?)?;
    }
    if let Some(missing) = root.get("missing") {
        options.missing = missing_field(missing.as_str("\"missing\"")?)?;
    }
    if let Some(budget) = root.get("budget") {
        let numbers = budget.as_object("\"budget\"")?;
        options.budget = Budget::from_limits(
            numbers.get("maxPart").and_then(Json::as_u64),
            numbers.get("maxTotal").and_then(Json::as_u64),
            numbers.get("maxEntries").and_then(Json::as_u64).map(|value| value as usize),
        );
    }
    options.from = root.get("from").and_then(Json::as_u64).map(|value| value as usize);
    options.to = root.get("to").and_then(Json::as_u64).map(|value| value as usize);
    Ok(options)
}

/// Turn the dataset into the merge's record sets.
///
/// A parent with no entry for a band gets an empty child set, which removes the band row and keeps
/// the header (DOC2-01). A band that names a parent index beyond the parents is refused rather than
/// silently ignored: a host that miscounts has a defect worth seeing.
pub fn record_sets(dataset: &Dataset) -> Result<(Records, Repeats), String> {
    let headers = headers_of(&dataset.parents);
    let cells = dataset
        .parents
        .iter()
        .map(|parent| headers.iter().map(|header| parent.get(header).cloned().unwrap_or_default()).collect())
        .collect();
    let parents = Records::from_rows(headers, cells).map_err(rendered)?;

    let mut repeats = Repeats::new();
    for (alias, by_parent) in &dataset.repeats {
        if by_parent.len() > dataset.parents.len() {
            return Err(format!(
                "merge.repeats_length The repeating row \"{alias}\" has {} groups for {} records.",
                by_parent.len(),
                dataset.parents.len()
            ));
        }
        let mut sets = Vec::with_capacity(dataset.parents.len());
        for index in 0..dataset.parents.len() {
            let rows = by_parent.get(index).cloned().unwrap_or_default();
            let child_headers = headers_of(&rows);
            let child_cells = rows
                .iter()
                .map(|row| {
                    child_headers
                        .iter()
                        .map(|header| row.get(header).cloned().unwrap_or_default())
                        .collect()
                })
                .collect();
            // An empty child set needs no headers: `Records::from_rows` takes them as given.
            sets.push(
                Records::from_rows(child_headers, child_cells).map_err(rendered)?,
            );
        }
        repeats.insert(alias.clone(), sets);
    }
    Ok((parents, repeats))
}

/// Every header in first-seen order across the parents.
fn headers_of(rows: &[BTreeMap<String, String>]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for row in rows {
        for key in row.keys() {
            if !out.iter().any(|have| have == key) {
                out.push(key.clone());
            }
        }
    }
    out
}

/// Merge a dataset and return one package per document.
///
/// `join` decides the shape: `None` keeps one package per parent (the caller files them), and any
/// other join produces **one** package (`Auto`, `PageBreak`) or the catalogue shape (`Directory`,
/// `Labels`).
pub fn merge_dataset(
    template: &ModelDocument,
    dataset: &Dataset,
    options: &DatasetOptions,
) -> Result<Vec<ModelDocument>, String> {
    let (records, repeats) = record_sets(dataset)?;
    if records.is_empty() {
        return Err("merge.no_records The merge has no records.".to_string());
    }
    let mut request = MergeRequest {
        template,
        records,
        kind: options.kind,
        missing: options.missing,
        from: options.from.unwrap_or(0),
        to: options.to,
        maps: Vec::new(),
        budget: options.budget.clone(),
        today: template.core.created.clone(),
        repeats,
        band: None,
        band_parent: None,
        band_row: 0,
    };
    let batch = finish(&mut request).map_err(rendered)?;
    if matches!(options.join, Join::None) {
        return Ok(batch.documents);
    }
    let joined = join_documents(batch, &options.budget).map_err(rendered)?;
    Ok(vec![joined])
}

/// One row per band field: `{ name, story, table, row }`.
///
/// specs/144 DOC2-02: activate uses this to prove a band's fields share one table row. The `row` is
/// the index of the row that holds the field among the table's rows, `table` the index of the table
/// among the body's blocks. A field in a paragraph has neither.
pub fn field_index(template: &ModelDocument) -> Vec<FieldLocation> {
    let mut out = Vec::new();
    collect_index(&template.body, &mut out, None, None);
    for part in template.parts.values() {
        if matches!(part.kind, edgedoc_model::PartKind::Header | edgedoc_model::PartKind::Footer) {
            collect_index(&part.blocks, &mut out, None, None);
        }
    }
    out
}

/// Where one merge field sits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldLocation {
    pub name: String,
    /// `1` for the body, `0` otherwise — the display module's story numbering.
    pub story: u16,
    /// The table that holds it, when it is in a table.
    pub table: Option<usize>,
    /// The row inside that table.
    pub row: Option<usize>,
}

fn collect_index(
    blocks: &[std::sync::Arc<edgedoc_model::Block>],
    out: &mut Vec<FieldLocation>,
    table: Option<usize>,
    row: Option<usize>,
) {
    use edgedoc_model::{Block, InlineObject, Instruction};
    for block in blocks {
        match block.as_ref() {
            Block::Paragraph(para) => {
                for object in &para.objects {
                    if let InlineObject::Field(field) = object {
                        let parsed = Instruction::parse(&field.instr);
                        if parsed.keyword == "MERGEFIELD" {
                            if let Some(name) = parsed.merge_name() {
                                out.push(FieldLocation {
                                    name,
                                    story: 1,
                                    table,
                                    row,
                                });
                            }
                        }
                    }
                }
            }
            Block::Table(table_block) => {
                let index = next_table_index(out);
                for (row_index, table_row) in table_block.rows.iter().enumerate() {
                    for cell in &table_row.cells {
                        collect_index(&cell.blocks, out, Some(index), Some(row_index));
                    }
                }
            }
            Block::Alternate { choice, .. } => collect_index(choice, out, table, row),
            Block::Control(control) => collect_index(&control.blocks, out, table, row),
            Block::Opaque(_) => {}
        }
    }
}

/// The table index for a table being walked: how many distinct tables already produced fields.
fn next_table_index(out: &[FieldLocation]) -> usize {
    let mut highest = None;
    for location in out {
        if let Some(table) = location.table {
            highest = Some(highest.map_or(table, |have: usize| have.max(table)));
        }
    }
    highest.map_or(0, |have| have + 1)
}

/// The JSON the field index is sent as.
pub fn field_index_json(template: &ModelDocument) -> String {
    let fields = field_index(template);
    let mut out = String::from("[");
    for (index, field) in fields.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str("{\"name\":");
        push_json_string(&mut out, &field.name);
        out.push_str(&format!(",\"story\":{}", field.story));
        match (field.table, field.row) {
            (Some(table), Some(row)) => {
                out.push_str(&format!(",\"table\":{table},\"row\":{row}"));
            }
            _ => out.push_str(",\"table\":null,\"row\":null"),
        }
        out.push('}');
    }
    out.push(']');
    out
}

// ── the closed grammar ────────────────────────────────────────────────────────

fn merge_kind(raw: &str) -> Result<MergeKind, String> {
    match raw.to_ascii_lowercase().as_str() {
        "letters" => Ok(MergeKind::Letters),
        // One continuing document: letters semantics without the per-record section break
        // (specs/144 RB-17). The binder accepts the word, so the engine must read it.
        "normal" => Ok(MergeKind::Normal),
        "directory" => Ok(MergeKind::Directory),
        "labels" => Ok(MergeKind::Labels),
        "emails" => Ok(MergeKind::Emails),
        "envelopes" => Ok(MergeKind::Envelopes),
        other => Err(format!(
            "unsupported Unknown merge kind \"{other}\". Use letters, normal, directory, labels, emails, or envelopes."
        )),
    }
}

fn join_mode(raw: &str) -> Result<Join, String> {
    match raw.to_ascii_lowercase().as_str() {
        "auto" => Ok(Join::Auto),
        "page-break" | "page_break" => Ok(Join::PageBreak),
        "none" => Ok(Join::None),
        other => Err(format!(
            "unsupported Unknown join \"{other}\". Use auto, page-break, or none."
        )),
    }
}

fn missing_field(raw: &str) -> Result<MissingField, String> {
    match raw.to_ascii_lowercase().as_str() {
        "error" => Ok(MissingField::Error),
        "empty" => Ok(MissingField::Empty),
        "placeholder" => Ok(MissingField::Placeholder),
        other => Err(format!(
            "unsupported Unknown missing-field mode \"{other}\". Use error, empty, or placeholder."
        )),
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Json {
    Null,
    Bool(bool),
    Number(f64),
    Text(String),
    Array(Vec<Json>),
    Object(BTreeMap<String, Json>),
}

impl Json {
    fn as_object(&self, what: &str) -> Result<&BTreeMap<String, Json>, String> {
        match self {
            Json::Object(map) => Ok(map),
            _ => Err(format!("merge.dataset {what} must be an object.")),
        }
    }
    fn as_array(&self, what: &str) -> Result<&[Json], String> {
        match self {
            Json::Array(items) => Ok(items),
            _ => Err(format!("merge.dataset {what} must be an array.")),
        }
    }
    fn as_str(&self, what: &str) -> Result<&str, String> {
        match self {
            Json::Text(text) => Ok(text),
            _ => Err(format!("merge.dataset {what} must be text.")),
        }
    }
    fn as_u64(&self) -> Option<u64> {
        match self {
            Json::Number(value) if *value >= 0.0 && value.fract() == 0.0 => Some(*value as u64),
            _ => None,
        }
    }
    /// One record: every value must be text, or a number/bool/null that a host widened for us.
    fn as_string_map(&self, what: &str) -> Result<BTreeMap<String, String>, String> {
        let map = self.as_object(what)?;
        let mut out = BTreeMap::new();
        for (key, value) in map {
            let text = match value {
                Json::Text(text) => text.clone(),
                Json::Number(number) => {
                    // A host must send a decimal string for money; a float here is a host defect
                    // that would round a price, so it keeps every digit JavaScript printed.
                    if number.fract() == 0.0 && number.abs() < 9.0e15 {
                        format!("{}", *number as i64)
                    } else {
                        format!("{number}")
                    }
                }
                Json::Bool(flag) => flag.to_string(),
                Json::Null => String::new(),
                _ => {
                    return Err(format!(
                        "merge.dataset {what}.{key} must be text, a number, a boolean, or null."
                    ))
                }
            };
            out.insert(key.clone(), text);
        }
        Ok(out)
    }
}

fn push_json_string(out: &mut String, text: &str) {
    out.push('"');
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

/// Read one value. The grammar is closed and the depth is bounded, so a hostile payload cannot
/// recurse without limit.
fn parse(text: &str) -> Result<Json, String> {
    let mut parser = Parser {
        bytes: text.as_bytes(),
        at: 0,
    };
    parser.skip_space();
    let value = parser.value(0)?;
    parser.skip_space();
    if parser.at != parser.bytes.len() {
        return Err(format!(
            "merge.dataset Unexpected text at offset {}. The dataset is one JSON value.",
            parser.at
        ));
    }
    Ok(value)
}

struct Parser<'a> {
    bytes: &'a [u8],
    at: usize,
}

const MAX_DEPTH: usize = 32;

impl Parser<'_> {
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.at).copied()
    }

    fn bump(&mut self) -> Option<u8> {
        let byte = self.peek()?;
        self.at += 1;
        Some(byte)
    }

    fn skip_space(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.at += 1;
        }
    }

    fn expect(&mut self, byte: u8, what: &str) -> Result<(), String> {
        match self.bump() {
            Some(found) if found == byte => Ok(()),
            _ => Err(format!(
                "merge.dataset Expected {what} at offset {}. The dataset is not valid JSON.",
                self.at
            )),
        }
    }

    fn value(&mut self, depth: usize) -> Result<Json, String> {
        if depth > MAX_DEPTH {
            return Err("merge.dataset The dataset is nested too deeply.".to_string());
        }
        match self.peek() {
            Some(b'{') => self.object(depth),
            Some(b'[') => self.array(depth),
            Some(b'"') => Ok(Json::Text(self.string()?)),
            Some(b't') => self.literal("true", Json::Bool(true)),
            Some(b'f') => self.literal("false", Json::Bool(false)),
            Some(b'n') => self.literal("null", Json::Null),
            Some(byte) if byte == b'-' || byte.is_ascii_digit() => self.number(),
            _ => Err(format!(
                "merge.dataset Expected a value at offset {}. The dataset is not valid JSON.",
                self.at
            )),
        }
    }

    fn literal(&mut self, word: &str, value: Json) -> Result<Json, String> {
        if self.bytes[self.at..].starts_with(word.as_bytes()) {
            self.at += word.len();
            Ok(value)
        } else {
            Err(format!(
                "merge.dataset Expected {word} at offset {}.",
                self.at
            ))
        }
    }

    fn object(&mut self, depth: usize) -> Result<Json, String> {
        self.expect(b'{', "\"{\"")?;
        let mut map = BTreeMap::new();
        self.skip_space();
        if self.peek() == Some(b'}') {
            self.at += 1;
            return Ok(Json::Object(map));
        }
        loop {
            self.skip_space();
            let key = self.string()?;
            self.skip_space();
            self.expect(b':', "\":\"")?;
            self.skip_space();
            let value = self.value(depth + 1)?;
            map.insert(key, value);
            self.skip_space();
            match self.bump() {
                Some(b',') => continue,
                Some(b'}') => return Ok(Json::Object(map)),
                _ => {
                    return Err(format!(
                        "merge.dataset Expected \",\" or \"}}\" at offset {}.",
                        self.at
                    ))
                }
            }
        }
    }

    fn array(&mut self, depth: usize) -> Result<Json, String> {
        self.expect(b'[', "\"[\"")?;
        let mut items = Vec::new();
        self.skip_space();
        if self.peek() == Some(b']') {
            self.at += 1;
            return Ok(Json::Array(items));
        }
        loop {
            self.skip_space();
            items.push(self.value(depth + 1)?);
            self.skip_space();
            match self.bump() {
                Some(b',') => continue,
                Some(b']') => return Ok(Json::Array(items)),
                _ => {
                    return Err(format!(
                        "merge.dataset Expected \",\" or \"]\" at offset {}.",
                        self.at
                    ))
                }
            }
        }
    }

    fn string(&mut self) -> Result<String, String> {
        self.expect(b'"', "\"\\\"\"")?;
        let mut out = String::new();
        loop {
            match self.bump() {
                None => return Err("merge.dataset A text value is not closed.".to_string()),
                Some(b'"') => return Ok(out),
                Some(b'\\') => match self.bump() {
                    Some(b'"') => out.push('"'),
                    Some(b'\\') => out.push('\\'),
                    Some(b'/') => out.push('/'),
                    Some(b'b') => out.push('\u{8}'),
                    Some(b'f') => out.push('\u{c}'),
                    Some(b'n') => out.push('\n'),
                    Some(b'r') => out.push('\r'),
                    Some(b't') => out.push('\t'),
                    Some(b'u') => out.push(self.unicode_escape()?),
                    _ => {
                        return Err(format!(
                            "merge.dataset Unknown escape at offset {}.",
                            self.at
                        ))
                    }
                },
                Some(byte) => {
                    // Multi-byte UTF-8 arrives one byte at a time; collect the continuation bytes.
                    let start = self.at - 1;
                    let width = utf8_width(byte);
                    if width > 1 {
                        self.at = start + width;
                        if self.at > self.bytes.len() {
                            return Err("merge.dataset A text value ends inside a character.".to_string());
                        }
                    }
                    let slice = &self.bytes[start..self.at];
                    match std::str::from_utf8(slice) {
                        Ok(text) => out.push_str(text),
                        Err(_) => {
                            return Err(format!(
                                "merge.dataset Invalid UTF-8 at offset {start}."
                            ))
                        }
                    }
                }
            }
        }
    }

    fn unicode_escape(&mut self) -> Result<char, String> {
        let mut code = 0u32;
        for _ in 0..4 {
            let byte = self
                .bump()
                .ok_or_else(|| "merge.dataset A \\u escape is short.".to_string())?;
            let digit = (byte as char)
                .to_digit(16)
                .ok_or_else(|| format!("merge.dataset A \\u escape has a bad digit at offset {}.", self.at))?;
            code = code * 16 + digit;
        }
        char::from_u32(code)
            .ok_or_else(|| "merge.dataset A \\u escape is not a character.".to_string())
    }

    fn number(&mut self) -> Result<Json, String> {
        let start = self.at;
        if self.peek() == Some(b'-') {
            self.at += 1;
        }
        while matches!(self.peek(), Some(byte) if byte.is_ascii_digit()) {
            self.at += 1;
        }
        if self.peek() == Some(b'.') {
            self.at += 1;
            while matches!(self.peek(), Some(byte) if byte.is_ascii_digit()) {
                self.at += 1;
            }
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.at += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.at += 1;
            }
            while matches!(self.peek(), Some(byte) if byte.is_ascii_digit()) {
                self.at += 1;
            }
        }
        let text = std::str::from_utf8(&self.bytes[start..self.at]).unwrap_or("");
        text.parse::<f64>()
            .map(Json::Number)
            .map_err(|_| format!("merge.dataset Invalid number at offset {start}."))
    }
}

fn utf8_width(byte: u8) -> usize {
    if byte < 0x80 {
        1
    } else if byte >> 5 == 0b110 {
        2
    } else if byte >> 4 == 0b1110 {
        3
    } else if byte >> 3 == 0b11110 {
        4
    } else {
        1
    }
}
