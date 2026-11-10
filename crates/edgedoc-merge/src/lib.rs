//! Field merge. This crate does not read ZIP packages.
//!
//! The cursor rules follow WordCraft `finish` in `crates/engine/src/cmd/mailings/merge.rs`.
//! Preview returns a new document. It does not write into the template.

mod dataset;
mod records;

pub use dataset::{
    dataset_from_json, field_index, field_index_json, merge_dataset, options_from_json, record_sets,
    Dataset, DatasetOptions, FieldLocation,
};
pub use records::{CsvOptions, MissingField, Op, Record, Records, Rule, SortKey};

use std::sync::Arc;

use edgedoc_limit::Budget;
use edgedoc_model::{
    Block, Field, InlineObject, Instruction, ModelDocument, Paragraph, PartKind, SectionStart,
    StoryRef, PAGE_BREAK,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeKind {
    Letters,
    Emails,
    Envelopes,
    Labels,
    Directory,
    Normal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Join {
    Auto,
    PageBreak,
    None,
}

#[derive(Debug)]
pub struct MergeBatch {
    pub documents: Vec<ModelDocument>,
    pub kind: MergeKind,
    pub per_record_sections: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergeError {
    pub code: &'static str,
    pub message: String,
}

impl MergeError {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for MergeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for MergeError {}

pub type MapFn = Box<dyn Fn(&Record) -> String>;

/// The child rows of one parent, per band alias.
///
/// specs/144 DOC2-01: a repeat band is one table row per child of the **current parent**. The
/// parent cursor does not advance while its children are written — that is the difference between
/// an invoice and one invoice per line (probe `CS-05` in the EdgeBase spec).
pub type Repeats = std::collections::BTreeMap<String, Vec<Records>>;

pub struct MergeRequest<'a> {
    pub template: &'a ModelDocument,
    pub records: Records,
    pub kind: MergeKind,
    pub missing: MissingField,
    pub from: usize,
    pub to: Option<usize>,
    pub maps: Vec<(String, MapFn)>,
    pub budget: Budget,
    pub today: String,
    /// `repeats[alias][parent]` is the child rows of parent `parent` for that band.
    pub repeats: Repeats,
    /// While a band row is being written, the alias whose children are being walked.
    pub band: Option<String>,
    /// The parent whose children are being walked. `None` means "the cursor row".
    pub band_parent: Option<usize>,
    /// The child row inside the band (the band's own cursor).
    pub band_row: usize,
}

pub fn preview(
    template: &ModelDocument,
    records: &Records,
    index: usize,
    missing: MissingField,
    budget: &Budget,
) -> Result<ModelDocument, MergeError> {
    let mut request = MergeRequest {
        template,
        records: records.clone(),
        kind: MergeKind::Letters,
        missing,
        from: index,
        to: Some(index + 1),
        maps: Vec::new(),
        budget: budget.clone(),
        today: template.core.created.clone(),
        repeats: Repeats::new(),
        band: None,
        band_parent: None,
        band_row: 0,
    };
    let batch = finish(&mut request)?;
    batch
        .documents
        .into_iter()
        .next()
        .ok_or_else(|| MergeError::new("merge.no_records", "The record range is empty."))
}

pub fn finish(request: &mut MergeRequest<'_>) -> Result<MergeBatch, MergeError> {
    let end = request
        .to
        .unwrap_or(request.records.rows().len())
        .min(request.records.rows().len());
    if request.from >= end {
        return Err(MergeError::new(
            "merge.no_records",
            format!(
                "No records from {} to {end}. The list has {} records.",
                request.from + 1,
                request.records.rows().len()
            ),
        ));
    }
    let mut documents = Vec::new();
    let mut start = request.from;
    while start < end {
        let mut copy = request.template.clone();
        let mut cursor = start;
        let keep = merge_blocks(request, &mut copy.body, &mut cursor, end, 0)?;
        if keep {
            merge_headers(request, &mut copy, start, end)?;
            copy.ensure_nonempty();
            documents.push(copy);
        }
        start = cursor.saturating_add(1);
    }
    // specs/144 DOC2-17: `normal` is one continuing document — letters semantics without the
    // per-record section break that gives each letter its own page.
    let per_record_sections = !matches!(
        request.kind,
        MergeKind::Directory | MergeKind::Emails | MergeKind::Labels | MergeKind::Normal
    ) && header_has_merge(request.template);
    Ok(MergeBatch {
        documents,
        kind: request.kind,
        per_record_sections,
    })
}

fn merge_headers(
    request: &MergeRequest<'_>,
    doc: &mut ModelDocument,
    record: usize,
    end: usize,
) -> Result<(), MergeError> {
    let ids: Vec<u32> = doc
        .parts
        .iter()
        .filter(|(_, part)| matches!(part.kind, PartKind::Header | PartKind::Footer))
        .map(|(id, _)| *id)
        .collect();
    for id in ids {
        if let Some(part) = doc.parts.get_mut(&id) {
            let mut cursor = record;
            merge_blocks(request, &mut part.blocks, &mut cursor, end, 0)?;
        }
    }
    Ok(())
}

fn merge_blocks(
    request: &MergeRequest<'_>,
    blocks: &mut [Arc<Block>],
    cursor: &mut usize,
    end: usize,
    depth: usize,
) -> Result<bool, MergeError> {
    if depth > request.budget.max_table_depth {
        return Ok(true);
    }
    for block in blocks.iter_mut() {
        match Arc::make_mut(block) {
            Block::Paragraph(para) => {
                if !apply_paragraph(request, para, cursor, end)? {
                    return Ok(false);
                }
            }
            Block::Table(table) => {
                if !merge_table(request, table, cursor, end, depth)? {
                    return Ok(false);
                }
            }
            Block::Alternate { choice, .. } => {
                if !merge_blocks(request, choice, cursor, end, depth)? {
                    return Ok(false);
                }
            }
            Block::Control(control) => {
                if !merge_blocks(request, &mut control.blocks, cursor, end, depth)? {
                    return Ok(false);
                }
            }
            Block::Opaque(_) => {}
        }
    }
    Ok(true)
}

/// Merge the fields in a table.
///
/// Word repeats a table row that holds a merge field in a catalogue merge, and keeps
/// every row in place for a letters merge. The distinction is the merge kind, not the
/// row: a price list is a directory, and an invoice that lays out two blocks side by
/// side is a letter.
///
/// specs/144 DOC2-01: a row whose merge fields all share one `Alias.` prefix is a **band**
/// row. It is cloned once per child of the current parent, and the parent cursor does not
/// move. A row that mixes two prefixes is `merge.mixed_band` and produces no document.
fn merge_table(
    request: &MergeRequest<'_>,
    table: &mut edgedoc_model::Table,
    cursor: &mut usize,
    end: usize,
    depth: usize,
) -> Result<bool, MergeError> {
    let repeats_rows = matches!(request.kind, MergeKind::Directory | MergeKind::Labels);
    let mut rows = std::mem::take(&mut table.rows);
    let mut out: Vec<edgedoc_model::Row> = Vec::with_capacity(rows.len());
    let mut repeated = false;
    for row in rows.iter_mut() {
        // A band row is decided before the catalogue law: an invoice is a `letters` merge whose
        // line table repeats, which the catalogue rule would never do.
        if !row.header {
            match row_kind(request, row) {
                RowKind::Mixed => {
                    table.rows = out;
                    return Err(MergeError::new(
                        "merge.mixed_band",
                        "A table row holds fields from two repeating rows. Put each repeating row in its own table row.",
                    ));
                }
                RowKind::Band(alias) => {
                    let written = write_band_rows(request, row, &alias, *cursor, depth)?;
                    // Zero children removes the row and keeps the header (DOC2-01).
                    out.extend(written);
                    continue;
                }
                RowKind::Plain => {}
            }
        }
        let holds_field = row.cells.iter().any(|cell| blocks_have_field(&cell.blocks));
        // The first field-bearing data row of a catalogue repeats; every other row stays.
        let repeats_this_row = repeats_rows && holds_field && !repeated && !row.header;
        if !repeats_this_row {
            for cell in &mut row.cells {
                if !merge_blocks(request, &mut cell.blocks, cursor, end, depth + 1)? {
                    table.rows = out;
                    return Ok(false);
                }
            }
            out.push(row.clone());
            continue;
        }
        // A catalogue row takes one copy per record, from the cursor to the last record.
        repeated = true;
        let template = row.clone();
        let mut record = *cursor;
        while record < end {
            let mut copy = template.clone();
            let mut row_cursor = record;
            for cell in &mut copy.cells {
                if !merge_blocks(request, &mut cell.blocks, &mut row_cursor, end, depth + 1)? {
                    table.rows = out;
                    return Ok(false);
                }
            }
            out.push(copy);
            record += 1;
        }
        // The document now holds every record, so the outer loop stops after it.
        *cursor = end;
    }
    table.rows = out;
    Ok(true)
}

/// What a table row is, for the merge.
enum RowKind {
    /// An ordinary row: its fields read the parent.
    Plain,
    /// A band row for one alias: it is cloned once per child of the current parent.
    Band(String),
    /// A row whose fields name two different bands. Refused, never guessed.
    Mixed,
}

fn row_kind(request: &MergeRequest<'_>, row: &edgedoc_model::Row) -> RowKind {
    let mut found: Option<String> = None;
    for instr in row_fields(row) {
        let Some(name) = field_name(&instr) else {
            continue;
        };
        let Some((alias, _)) = name.split_once('.') else {
            continue;
        };
        if !request.repeats.contains_key(alias) {
            continue;
        }
        match &found {
            None => found = Some(alias.to_string()),
            Some(first) if first == alias => {}
            Some(_) => return RowKind::Mixed,
        }
    }
    match found {
        Some(alias) => RowKind::Band(alias),
        None => RowKind::Plain,
    }
}

/// Write one copy of a band row per child of the current parent.
///
/// The parent cursor is frozen: the band walks its own row index and nothing else moves. This is
/// what keeps a parent scalar (`Total`) on every copy (probe `CS-06`).
fn write_band_rows(
    request: &MergeRequest<'_>,
    row: &edgedoc_model::Row,
    alias: &str,
    parent: usize,
    depth: usize,
) -> Result<Vec<edgedoc_model::Row>, MergeError> {
    let Some(set) = request.repeats.get(alias).and_then(|by_parent| by_parent.get(parent)) else {
        return Ok(Vec::new());
    };
    let mut out = Vec::with_capacity(set.len());
    for child in 0..set.len() {
        let scoped = MergeRequest {
            template: request.template,
            records: request.records.clone(),
            kind: request.kind,
            missing: request.missing,
            from: request.from,
            to: request.to,
            maps: Vec::new(),
            budget: request.budget.clone(),
            today: request.today.clone(),
            repeats: request.repeats.clone(),
            band: Some(alias.to_string()),
            band_parent: Some(parent),
            band_row: child,
        };
        let mut copy = row.clone();
        // The band never advances the parent, so the outer cursor is passed but cannot move past it.
        let mut frozen = parent;
        let mut keep = true;
        for cell in &mut copy.cells {
            if !merge_blocks(&scoped, &mut cell.blocks, &mut frozen, parent + 1, depth + 1)? {
                keep = false;
                break;
            }
        }
        // A `SKIPIF` inside the band drops this child row only; the other children still print.
        if keep {
            out.push(copy);
        }
    }
    Ok(out)
}

/// Every merge field name in a row, in document order.
fn row_fields(row: &edgedoc_model::Row) -> Vec<String> {
    let mut out = Vec::new();
    collect_fields(&row.cells.iter().flat_map(|cell| cell.blocks.iter().cloned()).collect::<Vec<_>>(), &mut out);
    out
}

fn collect_fields(blocks: &[Arc<Block>], out: &mut Vec<String>) {
    for block in blocks {
        match block.as_ref() {
            Block::Paragraph(para) => {
                for object in &para.objects {
                    if let InlineObject::Field(Field { instr, .. }) = object {
                        out.push(instr.clone());
                    }
                }
            }
            Block::Table(table) => {
                for row in &table.rows {
                    for cell in &row.cells {
                        collect_fields(&cell.blocks, out);
                    }
                }
            }
            Block::Alternate { choice, .. } => collect_fields(choice, out),
            Block::Control(control) => collect_fields(&control.blocks, out),
            Block::Opaque(_) => {}
        }
    }
}

fn field_name(instr: &str) -> Option<String> {
    let parsed = Instruction::parse(instr);
    if parsed.keyword != "MERGEFIELD" {
        return None;
    }
    parsed.merge_name()
}

fn apply_paragraph(
    request: &MergeRequest<'_>,
    para: &mut Paragraph,
    cursor: &mut usize,
    end: usize,
) -> Result<bool, MergeError> {
    let mut replacements: Vec<(usize, String)> = Vec::new();
    for (index, object) in para.objects.iter().enumerate() {
        let InlineObject::Field(field) = object else {
            continue;
        };
        match eval(request, &field.instr, *cursor, end)? {
            Action::Leave => {}
            Action::Text(text) => replacements.push((index, text)),
            Action::Next => {
                *cursor = cursor.saturating_add(1).min(end);
                replacements.push((index, String::new()));
            }
            Action::Skip => return Ok(false),
        }
    }
    // Apply from the last field to the first, so the earlier indexes stay valid.
    for (index, text) in replacements.into_iter().rev() {
        para.replace_object(index, &text);
    }
    para.raw_xml = None;
    Ok(true)
}

enum Action {
    Text(String),
    Next,
    Skip,
    Leave,
}

fn eval(
    request: &MergeRequest<'_>,
    instr: &str,
    row: usize,
    _end: usize,
) -> Result<Action, MergeError> {
    let parsed = Instruction::parse(instr);
    match parsed.keyword.as_str() {
        "MERGEFIELD" => {
            let Some(name) = parsed.merge_name() else {
                return Ok(Action::Leave);
            };
            Ok(Action::Text(value(request, row, &name)?))
        }
        "ADDRESSBLOCK" => Ok(Action::Text(address_block(request, row)?)),
        "GREETINGLINE" => Ok(Action::Text(greeting(request, row)?)),
        "MERGEREC" => Ok(Action::Text((row + 1).to_string())),
        "NEXT" => Ok(Action::Next),
        "NEXTIF" => Ok(if condition(request, instr, row)?.0 {
            Action::Next
        } else {
            Action::Text(String::new())
        }),
        "SKIPIF" => Ok(if condition(request, instr, row)?.0 {
            Action::Skip
        } else {
            Action::Text(String::new())
        }),
        "IF" => {
            let (holds, texts) = condition(request, instr, row)?;
            let index = if holds { 0 } else { 1 };
            Ok(Action::Text(texts.get(index).cloned().unwrap_or_default()))
        }
        "DATE" | "TIME" | "CREATED" => Ok(Action::Text(request.today.clone())),
        "AUTHOR" => Ok(Action::Text(request.template.core.creator.clone())),
        "TITLE" => Ok(Action::Text(request.template.core.title.clone())),
        "QUOTE" => Ok(Action::Text(
            quoted(instr).into_iter().next().unwrap_or_default(),
        )),
        _ => Ok(Action::Leave),
    }
}

/// The value of one merge field, honouring the band scope.
///
/// `Alias.Field` reads the child rows of the **current parent** while a band row is being written;
/// a bare name always reads the parent. This is what makes a line-item table possible without
/// advancing the parent cursor (DOC2-01).
fn value(request: &MergeRequest<'_>, row: usize, field: &str) -> Result<String, MergeError> {
    if let Some((alias, leaf)) = field.split_once('.') {
        return band_value(request, row, alias, leaf);
    }
    if let Some((_, map)) = request
        .maps
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(field))
    {
        let record = request.records.record(row);
        return Ok(map(&record));
    }
    if let Some(text) = request.records.cell(row, field) {
        return Ok(text);
    }
    missing_value(request.missing, field)
}

/// A child value. `alias` must be a declared band; the row is the parent the band belongs to.
fn band_value(
    request: &MergeRequest<'_>,
    row: usize,
    alias: &str,
    leaf: &str,
) -> Result<String, MergeError> {
    let Some(children) = request.repeats.get(alias) else {
        return missing_value(request.missing, &format!("{alias}.{leaf}"));
    };
    // A band row knows its own parent and child; anywhere else the band reads its first child, so a
    // `«Lines.Product»` in a paragraph still means something rather than silently blanking.
    let parent = request.band_parent.unwrap_or(row);
    let child_row = if request.band_parent.is_some() {
        request.band_row
    } else {
        0
    };
    let Some(set) = children.get(parent) else {
        return missing_value(request.missing, &format!("{alias}.{leaf}"));
    };
    match set.cell(child_row, leaf) {
        Some(text) => Ok(text),
        None => missing_value(request.missing, &format!("{alias}.{leaf}")),
    }
}

fn missing_value(missing: MissingField, field: &str) -> Result<String, MergeError> {
    match missing {
        MissingField::Empty => Ok(String::new()),
        MissingField::Placeholder => Ok(format!("«{field}»")),
        MissingField::Error => Err(MergeError::new(
            "merge.unknown_field",
            format!(
                "The field \"{field}\" has no header. Add the header or set MissingField::Empty."
            ),
        )),
    }
}

fn condition(
    request: &MergeRequest<'_>,
    instr: &str,
    row: usize,
) -> Result<(bool, Vec<String>), MergeError> {
    let field = instr
        .split('{')
        .nth(1)
        .and_then(|rest| rest.split('}').next())
        .unwrap_or("");
    let name = Instruction::parse(field).merge_name().unwrap_or_default();
    let after = instr.split('}').nth(1).unwrap_or("");
    let op: String = after
        .trim_start()
        .chars()
        .take_while(|c| matches!(c, '<' | '=' | '>'))
        .collect();
    let texts = quoted(after);
    let cmp = texts.first().map(String::as_str).unwrap_or("");
    let left = value(request, row, &name).unwrap_or_default();
    let order = match (left.trim().parse::<f64>(), cmp.trim().parse::<f64>()) {
        (Ok(a), Ok(b)) => a.partial_cmp(&b),
        _ => Some(left.as_str().cmp(cmp)),
    };
    let holds = match op.as_str() {
        "<>" => order != Some(std::cmp::Ordering::Equal),
        "<" => order == Some(std::cmp::Ordering::Less),
        ">" => order == Some(std::cmp::Ordering::Greater),
        "<=" => matches!(
            order,
            Some(std::cmp::Ordering::Less | std::cmp::Ordering::Equal)
        ),
        ">=" => matches!(
            order,
            Some(std::cmp::Ordering::Greater | std::cmp::Ordering::Equal)
        ),
        _ => order == Some(std::cmp::Ordering::Equal),
    };
    let rest = if texts.is_empty() {
        Vec::new()
    } else {
        texts.into_iter().skip(1).collect()
    };
    Ok((holds, rest))
}

fn quoted(text: &str) -> Vec<String> {
    text.split('"')
        .skip(1)
        .step_by(2)
        .map(str::to_string)
        .collect()
}

fn address_block(request: &MergeRequest<'_>, row: usize) -> Result<String, MergeError> {
    let map = address_map(request.records.headers());
    let get = |key: &str| -> Result<String, MergeError> {
        match map.get(key) {
            Some(field) => value(request, row, field),
            None => Ok(String::new()),
        }
    };
    let name = join([get("title")?, get("first")?, get("last")?]);
    let city = join([get("city")?, join([get("state")?, get("postal")?])]);
    Ok([
        name,
        get("company")?,
        get("address")?,
        city,
        get("country")?,
    ]
    .into_iter()
    .filter(|line| !line.is_empty())
    .collect::<Vec<_>>()
    .join("\n"))
}

fn greeting(request: &MergeRequest<'_>, row: usize) -> Result<String, MergeError> {
    let map = address_map(request.records.headers());
    let get = |key: &str| -> Result<String, MergeError> {
        match map.get(key) {
            Some(field) => value(request, row, field),
            None => Ok(String::new()),
        }
    };
    let name = join([get("first")?, get("last")?]);
    if name.is_empty() {
        Ok("Dear Sir or Madam,".into())
    } else {
        Ok(format!("Dear {name},"))
    }
}

fn join(parts: impl IntoIterator<Item = String>) -> String {
    parts
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn address_map(headers: &[String]) -> std::collections::BTreeMap<&'static str, String> {
    let find = |names: &[&str]| {
        headers
            .iter()
            .find(|header| {
                names
                    .iter()
                    .any(|name| header.replace([' ', '_'], "").eq_ignore_ascii_case(name))
            })
            .cloned()
    };
    let mut map = std::collections::BTreeMap::new();
    for (key, names) in [
        ("first", &["FirstName", "First", "GivenName"][..]),
        ("last", &["LastName", "Last", "Surname", "FamilyName"][..]),
        ("company", &["Company", "Organization", "CompanyName"][..]),
        (
            "address",
            &["Address", "AddressLine1", "Address1", "Street"][..],
        ),
        ("city", &["City", "Town"][..]),
        ("state", &["State", "Province", "Region"][..]),
        ("postal", &["PostalCode", "ZIP", "ZipCode", "Postcode"][..]),
        ("country", &["Country"][..]),
        ("title", &["Title", "CourtesyTitle"][..]),
    ] {
        if let Some(header) = find(names) {
            map.insert(key, header);
        }
    }
    map
}

pub fn join_documents(batch: MergeBatch, budget: &Budget) -> Result<ModelDocument, MergeError> {
    if batch.documents.is_empty() {
        return Err(MergeError::new(
            "merge.no_records",
            "The merge produced no documents.",
        ));
    }
    let separate = matches!(batch.kind, MergeKind::Emails);
    if separate {
        return Ok(batch.documents.into_iter().next().unwrap_or_default());
    }
    let directory = matches!(batch.kind, MergeKind::Directory);
    let labels = matches!(batch.kind, MergeKind::Labels);
    if labels {
        return Ok(labels_from(batch));
    }
    let mut joined = batch.documents[0].clone();
    joined.body.clear();
    let per_record = batch.per_record_sections;
    for (index, doc) in batch.documents.iter().enumerate() {
        budget
            .merged_blocks(joined.body.len() + doc.body.len())
            .map_err(|err| MergeError::new(err.code, err.message))?;
        if per_record {
            if index > 0 {
                let mut marker = Paragraph::new();
                let mut props = doc.last_section.clone();
                props.start = SectionStart::NextPage;
                marker.section = Some(edgedoc_model::SectionBreak { props });
                joined.body.push(Arc::new(Block::Paragraph(marker)));
            }
        } else if index > 0 && !directory {
            if let Some(Block::Paragraph(para)) = joined.body.last_mut().map(Arc::make_mut) {
                para.push_text(
                    &PAGE_BREAK.to_string(),
                    &edgedoc_model::CharProps::default(),
                );
            }
        }
        for block in &doc.body {
            joined.body.push(block.clone());
        }
    }
    joined.ensure_nonempty();
    let _ = StoryRef::Body;
    Ok(joined)
}

fn header_has_merge(doc: &ModelDocument) -> bool {
    doc.parts
        .iter()
        .filter(|(_, part)| matches!(part.kind, PartKind::Header | PartKind::Footer))
        .any(|(_, part)| blocks_have_field(&part.blocks))
}

fn blocks_have_field(blocks: &[Arc<Block>]) -> bool {
    blocks.iter().any(|block| match block.as_ref() {
        Block::Paragraph(para) => para.objects.iter().any(|object| matches!(object, InlineObject::Field(Field { instr, .. }) if Instruction::parse(instr).merge_name().is_some() || Instruction::parse(instr).keyword == "MERGEFIELD")),
        Block::Table(table) => table.rows.iter().flat_map(|row| &row.cells).any(|cell| blocks_have_field(&cell.blocks)),
        _ => false,
    })
}

fn labels_from(batch: MergeBatch) -> ModelDocument {
    let mut doc = batch.documents.first().cloned().unwrap_or_default();
    let mut table = edgedoc_model::Table::new();
    let mut row = edgedoc_model::Row::cells(Vec::new());
    for doc in &batch.documents {
        row.cells
            .push(edgedoc_model::Cell::paragraph("").blocks(doc.body.clone()));
        if row.cells.len() == 3 {
            table.rows.push(row);
            row = edgedoc_model::Row::cells(Vec::new());
        }
    }
    if !row.cells.is_empty() {
        table.rows.push(row);
    }
    doc.body = vec![Arc::new(Block::Table(table))];
    doc
}

pub const ENVELOPES: &[(&str, i32, i32)] = &[
    ("Envelope #10", 13680, 5940),
    ("Envelope #9", 12780, 5580),
    ("Envelope Monarch", 10800, 5580),
    ("Envelope DL", 12472, 6236),
    ("Envelope C4", 18368, 12982),
    ("Envelope C5", 12982, 9184),
    ("Envelope C6", 9184, 6462),
    ("Envelope B5", 14174, 9978),
];
