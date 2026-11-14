//! A closed block description that becomes a document (specs/144 DOC2-03).
//!
//! Why this exists: Markdown is a fine way to *write* a document and a poor way to describe a template.
//! `«Name»` arrives as text (so the package reports no `MERGEFIELD`), and `«Lines.Quantity__c»` loses
//! its underscores to emphasis. A host that installs a template must be able to say "this paragraph is
//! a merge field", and that is the whole vocabulary here:
//!
//! ```json
//! { "blocks": [
//!   { "paragraph": { "text": "For ", "field": "Customer", "after": "" } },
//!   { "table": [ { "header": "Article", "field": "Lines.Name" } ] }
//! ] }
//! ```
//!
//! Two block kinds, because they are the two a merge template needs. A paragraph is text, then one
//! field, then more text. A table is a header of plain words and **one** row of fields — which is what
//! a repeat band clones per child.
//!
//! The grammar is closed on purpose: an unknown key is refused by name, so a host cannot believe it
//! set something that was ignored.

use edgedoc_model::json::{parse_json, Json};
use edgedoc_model::{Block as ModelBlock, Cell, CoreProps, Paragraph, Row, Table};

use crate::{Document, Error};

/// One thing wrong with a block description.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockIssue {
    pub code: &'static str,
    pub message: String,
}

fn issue(code: &'static str, message: impl Into<String>) -> BlockIssue {
    BlockIssue {
        code,
        message: message.into(),
    }
}

/// A paragraph: `text`, one merge field, then `after`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParagraphBlock {
    pub text: String,
    pub field: Option<String>,
    pub after: String,
}

/// A table column: a plain header and the field its data row prints.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableColumn {
    pub header: String,
    pub field: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    Paragraph(ParagraphBlock),
    Table(Vec<TableColumn>),
}

/// The parsed description.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BlocksSpec {
    pub title: String,
    pub blocks: Vec<Block>,
}

impl BlocksSpec {
    /// Read the JSON a host sends. Every malformed value is a named refusal, never a silent default.
    pub fn parse(json: &str) -> Result<Self, BlockIssue> {
        let value = parse_json(json).map_err(|message| issue("blocks.json", message))?;
        let Some(root) = value.as_object() else {
            return Err(issue("blocks.json", "The description is one JSON object."));
        };
        let title = root
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let Some(entries) = root.get("blocks").and_then(|v| v.as_array()) else {
            return Err(issue(
                "blocks.json",
                "The description names a \"blocks\" array.",
            ));
        };
        let mut blocks = Vec::with_capacity(entries.len());
        for (index, entry) in entries.iter().enumerate() {
            let Some(object) = entry.as_object() else {
                return Err(issue(
                    "blocks.shape",
                    format!("blocks[{index}] must be an object with \"paragraph\" or \"table\"."),
                ));
            };
            if let Some(paragraph) = object.get("paragraph") {
                blocks.push(Block::Paragraph(parse_paragraph(index, paragraph)?));
                continue;
            }
            if let Some(table) = object.get("table") {
                blocks.push(Block::Table(parse_table(index, table)?));
                continue;
            }
            let keys: Vec<&str> = object.keys().map(String::as_str).collect();
            return Err(issue(
                "blocks.unknown",
                format!(
                    "blocks[{index}] has no recognised kind (found {}). Use \"paragraph\" or \"table\".",
                    keys.join(", ")
                ),
            ));
        }
        if blocks.is_empty() {
            return Err(issue(
                "blocks.empty",
                "A document needs at least one block.",
            ));
        }
        Ok(Self { title, blocks })
    }

    /// The document. A merge field is a real `MERGEFIELD`, so `merge_field_names` reports it.
    pub fn build(&self) -> Result<Document, Error> {
        let mut builder = Document::builder();
        if !self.title.is_empty() {
            builder = builder.core(CoreProps::new().title(&self.title));
        }
        for block in &self.blocks {
            match block {
                Block::Paragraph(paragraph) => {
                    let mut para = Paragraph::new();
                    if !paragraph.text.is_empty() {
                        para = para.text(&paragraph.text);
                    }
                    if let Some(field) = &paragraph.field {
                        para = para.merge_field(field);
                    }
                    if !paragraph.after.is_empty() {
                        para = para.text(&paragraph.after);
                    }
                    builder = builder.paragraph(para);
                }
                Block::Table(columns) => {
                    let header: Vec<&str> = columns
                        .iter()
                        .map(|column| column.header.as_str())
                        .collect();
                    let mut table = Table::new().header(header);
                    let cells: Vec<Cell> = columns
                        .iter()
                        .map(|column| {
                            let para = Paragraph::new().merge_field(&column.field);
                            // A cell's blocks are `Arc<Block>`; the paragraph replaces its start.
                            Cell::paragraph("")
                                .blocks(vec![std::sync::Arc::new(ModelBlock::Paragraph(para))])
                        })
                        .collect();
                    table.rows.push(Row::cells(cells));
                    builder = builder.table(table);
                }
            }
        }
        builder.build()
    }
}

fn parse_paragraph(index: usize, value: &Json) -> Result<ParagraphBlock, BlockIssue> {
    let Some(object) = value.as_object() else {
        return Err(issue(
            "blocks.shape",
            format!("blocks[{index}].paragraph must be an object."),
        ));
    };
    for key in object.keys() {
        if !matches!(key.as_str(), "text" | "field" | "after") {
            return Err(issue(
                "blocks.unknown",
                format!("blocks[{index}].paragraph has an unknown key \"{key}\". Use text, field, after."),
            ));
        }
    }
    let field = object
        .get("field")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string);
    if let Some(name) = &field {
        if name.chars().count() > 64 {
            return Err(issue(
                "blocks.field",
                format!("blocks[{index}].paragraph field \"{name}\" is longer than 64 characters."),
            ));
        }
    }
    Ok(ParagraphBlock {
        text: object
            .get("text")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        field,
        after: object
            .get("after")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
    })
}

fn parse_table(index: usize, value: &Json) -> Result<Vec<TableColumn>, BlockIssue> {
    let Some(columns) = value.as_array() else {
        return Err(issue(
            "blocks.shape",
            format!("blocks[{index}].table must be an array of columns."),
        ));
    };
    if columns.is_empty() {
        return Err(issue(
            "blocks.empty",
            format!("blocks[{index}].table has no column."),
        ));
    }
    let mut out = Vec::with_capacity(columns.len());
    for (column_index, column) in columns.iter().enumerate() {
        let Some(object) = column.as_object() else {
            return Err(issue(
                "blocks.shape",
                format!("blocks[{index}].table[{column_index}] must be an object."),
            ));
        };
        for key in object.keys() {
            if !matches!(key.as_str(), "header" | "field") {
                return Err(issue(
                    "blocks.unknown",
                    format!(
                        "blocks[{index}].table[{column_index}] has an unknown key \"{key}\". Use header, field."
                    ),
                ));
            }
        }
        let header = object
            .get("header")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let field = object
            .get("field")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .ok_or_else(|| {
                issue(
                    "blocks.field",
                    format!("blocks[{index}].table[{column_index}] needs a \"field\"."),
                )
            })?
            .to_string();
        out.push(TableColumn { header, field });
    }
    Ok(out)
}
