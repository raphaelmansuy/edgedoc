use std::collections::BTreeMap;
use std::sync::Arc;

use crate::error::{ModelError, ModelResult};
use crate::fields::Instruction;
use crate::para::{InlineObject, Paragraph, RunBuilder};
use crate::props::{DropKind, EditOptions, EditReport};
use crate::section::{Part, PartKind, SectionProps, StoryRef};
use crate::style::StyleSheet;
use crate::table::Block;

/// `docProps/core.xml`. Every field maps to one Dublin Core or OPC element.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CoreProps {
    pub title: String,
    pub subject: String,
    pub creator: String,
    pub keywords: String,
    pub description: String,
    pub last_modified_by: String,
    pub revision: String,
    pub created: String,
    pub modified: String,
    pub category: String,
    pub content_status: String,
    pub identifier: String,
    pub language: String,
    pub version: String,
}

impl CoreProps {
    pub fn new() -> Self {
        Self {
            created: "2026-11-04T00:00:00Z".into(),
            ..Self::default()
        }
    }

    pub fn title(mut self, title: &str) -> Self {
        self.title = title.to_string();
        self
    }

    pub fn creator(mut self, creator: &str) -> Self {
        self.creator = creator.to_string();
        self
    }

    pub fn subject(mut self, subject: &str) -> Self {
        self.subject = subject.to_string();
        self
    }

    pub fn keywords(mut self, keywords: &str) -> Self {
        self.keywords = keywords.to_string();
        self
    }

    pub fn description(mut self, description: &str) -> Self {
        self.description = description.to_string();
        self
    }

    pub fn last_modified_by(mut self, name: &str) -> Self {
        self.last_modified_by = name.to_string();
        self
    }

    pub fn modified(mut self, time: &str) -> Self {
        self.modified = time.to_string();
        self
    }

    pub fn category(mut self, category: &str) -> Self {
        self.category = category.to_string();
        self
    }

    pub fn content_status(mut self, status: &str) -> Self {
        self.content_status = status.to_string();
        self
    }

    pub fn language(mut self, language: &str) -> Self {
        self.language = language.to_string();
        self
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModelDocument {
    pub body: Vec<Arc<Block>>,
    pub last_section: SectionProps,
    pub parts: BTreeMap<u32, Part>,
    pub styles: StyleSheet,
    pub core: CoreProps,
    /// The document settings element, kept so that protection and vendor settings survive.
    pub settings_xml: Option<String>,
    /// The numbering part, kept so that a list definition survives a save.
    pub numbering_xml: Option<String>,
    next_part: u32,
}

impl Default for ModelDocument {
    fn default() -> Self {
        Self::new()
    }
}

impl ModelDocument {
    pub fn new() -> Self {
        Self {
            body: Vec::new(),
            last_section: SectionProps::default(),
            parts: BTreeMap::new(),
            styles: StyleSheet::default(),
            core: CoreProps::new(),
            settings_xml: None,
            numbering_xml: None,
            next_part: 1,
        }
    }

    pub fn ensure_nonempty(&mut self) {
        if self.body.is_empty() {
            self.body.push(Arc::new(Block::Paragraph(Paragraph::new())));
        }
        for part in self.parts.values_mut() {
            if part.blocks.is_empty() {
                part.blocks
                    .push(Arc::new(Block::Paragraph(Paragraph::new())));
            }
        }
    }

    pub fn add_part(&mut self, kind: PartKind, blocks: Vec<Arc<Block>>) -> u32 {
        self.add_part_with(kind, blocks, crate::section::PartMeta::default())
    }

    pub fn add_part_with(
        &mut self,
        kind: PartKind,
        blocks: Vec<Arc<Block>>,
        meta: crate::section::PartMeta,
    ) -> u32 {
        let id = self.next_part;
        self.next_part = self.next_part.saturating_add(1);
        self.parts.insert(id, Part { kind, blocks, meta });
        id
    }

    pub fn parts_of(&self, kind: PartKind) -> Vec<u32> {
        self.parts
            .iter()
            .filter(|(_, part)| part.kind == kind)
            .map(|(id, _)| *id)
            .collect()
    }

    pub fn plain_text(&self, story: StoryRef) -> String {
        let blocks = match story {
            StoryRef::Body => &self.body,
            StoryRef::Part(id) => match self.parts.get(&id) {
                Some(part) => &part.blocks,
                None => return String::new(),
            },
        };
        plain_blocks(blocks)
    }

    pub fn merge_field_names(&self) -> Vec<String> {
        let mut names = Vec::new();
        collect_fields(&self.body, &mut names);
        let mut ids: Vec<u32> = self
            .parts
            .iter()
            .filter(|(_, part)| matches!(part.kind, PartKind::Header | PartKind::Footer))
            .map(|(id, _)| *id)
            .collect();
        ids.sort_unstable();
        for id in ids {
            if let Some(part) = self.parts.get(&id) {
                collect_fields(&part.blocks, &mut names);
            }
        }
        names
    }

    pub fn balance_fields(&mut self) -> usize {
        let mut removed = 0;
        removed += balance_blocks(&mut self.body);
        let ids: Vec<u32> = self.parts.keys().copied().collect();
        for id in ids {
            if let Some(part) = self.parts.get_mut(&id) {
                removed += balance_blocks(&mut part.blocks);
            }
        }
        removed
    }

    pub fn edit<F>(&mut self, opts: EditOptions, f: F) -> ModelResult<EditReport>
    where
        F: FnOnce(&mut DocumentEdit<'_>) -> ModelResult<()>,
    {
        let mut working = self.clone();
        let mut edit = DocumentEdit {
            doc: &mut working,
            opts,
            report: EditReport::default(),
        };
        match f(&mut edit) {
            Ok(()) => {
                let report = edit.report;
                *self = working;
                Ok(report)
            }
            Err(err) => Err(err),
        }
    }

    /// Every text block in the body, in document order.
    pub fn blocks(&self) -> impl Iterator<Item = &Block> {
        self.body.iter().map(|block| block.as_ref())
    }

    pub fn header_count(&self) -> usize {
        self.parts
            .values()
            .filter(|part| part.kind == PartKind::Header)
            .count()
    }
}

fn plain_blocks(blocks: &[Arc<Block>]) -> String {
    let mut lines = Vec::new();
    for block in blocks {
        match block.as_ref() {
            Block::Paragraph(p) => lines.push(p.plain_text()),
            Block::Table(table) => {
                for row in &table.rows {
                    let cells: Vec<String> = row
                        .cells
                        .iter()
                        .map(|cell| {
                            plain_blocks(&cell.blocks)
                                .trim_end_matches('\n')
                                .to_string()
                        })
                        .collect();
                    lines.push(cells.join("\t"));
                }
            }
            Block::Alternate { choice, .. } => lines.push(plain_blocks(choice)),
            Block::Control(control) => lines.push(plain_blocks(&control.blocks)),
            Block::Opaque(_) => {}
        }
    }
    lines.join("\n")
}

fn collect_fields(blocks: &[Arc<Block>], names: &mut Vec<String>) {
    for block in blocks {
        match block.as_ref() {
            Block::Paragraph(p) => {
                for object in &p.objects {
                    if let InlineObject::Field(field) = object {
                        if let Some(name) = Instruction::parse(&field.instr).merge_name() {
                            if !names.iter().any(|have| have.eq_ignore_ascii_case(&name)) {
                                names.push(name);
                            }
                        }
                    }
                }
            }
            Block::Table(table) => {
                for row in &table.rows {
                    for cell in &row.cells {
                        collect_fields(&cell.blocks, names);
                    }
                }
            }
            Block::Alternate { choice, .. } => collect_fields(choice, names),
            Block::Control(control) => collect_fields(&control.blocks, names),
            Block::Opaque(_) => {}
        }
    }
}

fn balance_blocks(blocks: &mut [Arc<Block>]) -> usize {
    let mut removed = 0;
    for block in blocks.iter_mut() {
        let Block::Paragraph(para) = Arc::make_mut(block) else {
            continue;
        };
        let before = para.objects.len();
        para.objects
            .retain(|object| !matches!(object, InlineObject::Orphan));
        let drop_n = before - para.objects.len();
        if drop_n > 0 {
            let mut text = String::new();
            let mut seen = 0;
            for ch in para.text.chars() {
                if ch == crate::para::OBJ {
                    if seen < para.objects.len() {
                        text.push(ch);
                    }
                    seen += 1;
                } else {
                    text.push(ch);
                }
            }
            para.text = text;
            removed += drop_n;
        }
    }
    removed
}

pub struct DocumentEdit<'a> {
    pub doc: &'a mut ModelDocument,
    pub opts: EditOptions,
    pub report: EditReport,
}

/// Where a paragraph sits, so that a caller can reach it in one call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockPath {
    Body(usize),
    Cell {
        table: usize,
        row: usize,
        cell: usize,
        block: usize,
    },
    Control {
        index: usize,
        block: usize,
    },
    /// A paragraph in a story part: `part` is the id from [`ModelDocument::parts_of`].
    Part {
        part: u32,
        block: usize,
    },
}

impl<'a> DocumentEdit<'a> {
    pub fn body(&mut self) -> BodyCursor<'_, 'a> {
        BodyCursor { edit: self }
    }

    /// Edit a story that is not the body: a header, footer, footnote, endnote, or comment.
    pub fn has_part(&self, id: u32) -> bool {
        self.doc.parts.contains_key(&id)
    }
}

pub struct BodyCursor<'a, 'b> {
    edit: &'a mut DocumentEdit<'b>,
}

impl BodyCursor<'_, '_> {
    pub fn push_paragraph(&mut self, para: Paragraph) -> ModelResult<()> {
        self.edit.doc.body.push(Arc::new(Block::Paragraph(para)));
        Ok(())
    }

    pub fn push_table(&mut self, table: crate::table::Table) -> ModelResult<()> {
        let cols = table
            .rows
            .iter()
            .map(|row| row.cells.len())
            .max()
            .unwrap_or(0);
        if cols > edgedoc_limit::Budget::default().max_cols {
            return Err(ModelError::new(
                "model.limit",
                format!("The table has {cols} columns. The limit is 63. Remove the extra columns."),
            ));
        }
        self.edit.doc.body.push(Arc::new(Block::Table(table)));
        Ok(())
    }

    pub fn push_block(&mut self, block: Block) -> ModelResult<()> {
        self.edit.doc.body.push(Arc::new(block));
        Ok(())
    }

    pub fn paragraph_after_last<F>(&mut self, f: F) -> ModelResult<()>
    where
        F: FnOnce(&mut ParaCursor<'_>) -> ModelResult<()>,
    {
        self.edit
            .doc
            .body
            .push(Arc::new(Block::Paragraph(Paragraph::new())));
        let index = self.edit.doc.body.len() - 1;
        self.paragraph_at(index, f)
    }

    pub fn paragraph_at<F>(&mut self, index: usize, f: F) -> ModelResult<()>
    where
        F: FnOnce(&mut ParaCursor<'_>) -> ModelResult<()>,
    {
        let block = self.edit.doc.body.get_mut(index).ok_or_else(|| {
            ModelError::new(
                "model.bad_path",
                format!("No paragraph at index {index}. Choose a paragraph that exists."),
            )
        })?;
        let Block::Paragraph(para) = Arc::make_mut(block) else {
            return Err(ModelError::new(
                "model.bad_path",
                format!("Block {index} is not a paragraph."),
            ));
        };
        if para.locked && !self.edit.opts.allow_objects {
            return Err(ModelError::new(
                "model.locked",
                "The content control is locked. Unlock the control before you edit it.",
            ));
        }
        consent(para, &self.edit.opts, &mut self.edit.report)?;
        let mut cursor = ParaCursor { para };
        f(&mut cursor)
    }

    /// Edit the paragraph at a path that passes through a table cell or a content control.
    pub fn paragraph_at_path<F>(&mut self, path: &BlockPath, f: F) -> ModelResult<()>
    where
        F: FnOnce(&mut ParaCursor<'_>) -> ModelResult<()>,
    {
        let block = match path {
            BlockPath::Body(index) => self.edit.doc.body.get_mut(*index),
            BlockPath::Cell {
                table,
                row,
                cell,
                block,
            } => self
                .edit
                .doc
                .body
                .get_mut(*table)
                .and_then(|block| match Arc::make_mut(block) {
                    Block::Table(table) => table.rows.get_mut(*row),
                    _ => None,
                })
                .and_then(|row| row.cells.get_mut(*cell))
                .and_then(|cell| cell.blocks.get_mut(*block)),
            BlockPath::Control { index, block } => {
                let mut seen = 0;
                let mut found = None;
                for candidate in &mut self.edit.doc.body {
                    if let Block::Control(control) = Arc::make_mut(candidate) {
                        if seen == *index {
                            found = control.blocks.get_mut(*block);
                            break;
                        }
                        seen += 1;
                    }
                }
                found
            }
            BlockPath::Part { part, block } => self
                .edit
                .doc
                .parts
                .get_mut(part)
                .and_then(|part| part.blocks.get_mut(*block)),
        };
        let block = block
            .ok_or_else(|| ModelError::new("model.bad_path", "The path does not reach a block."))?;
        let Block::Paragraph(para) = Arc::make_mut(block) else {
            return Err(ModelError::new(
                "model.bad_path",
                "The path does not reach a paragraph.",
            ));
        };
        if para.locked && !self.edit.opts.allow_objects {
            return Err(ModelError::new(
                "model.locked",
                "The content control is locked.",
            ));
        }
        consent(para, &self.edit.opts, &mut self.edit.report)?;
        f(&mut ParaCursor { para })
    }

    pub fn control_at<F>(&mut self, index: usize, f: F) -> ModelResult<()>
    where
        F: FnOnce(&mut crate::table::ContentControl) -> ModelResult<()>,
    {
        let mut seen = 0;
        for block in &mut self.edit.doc.body {
            if let Block::Control(control) = Arc::make_mut(block) {
                if seen == index {
                    if control.lock_content && !self.edit.opts.allow_objects {
                        return Err(ModelError::new(
                            "model.locked",
                            "The content control is locked. Unlock the control before you edit it.",
                        ));
                    }
                    return f(control);
                }
                seen += 1;
            }
        }
        Err(ModelError::new(
            "model.bad_path",
            format!("No content control at index {index}."),
        ))
    }

    /// Change the table at a body index. Use it to add a row, or to set the widths.
    pub fn table_at<F>(&mut self, index: usize, f: F) -> ModelResult<()>
    where
        F: FnOnce(&mut crate::table::Table) -> ModelResult<()>,
    {
        let block = self.edit.doc.body.get_mut(index).ok_or_else(|| {
            ModelError::new(
                "model.bad_path",
                format!("No block at index {index}. Choose a block that exists."),
            )
        })?;
        let Block::Table(table) = Arc::make_mut(block) else {
            return Err(ModelError::new(
                "model.bad_path",
                format!("Block {index} is not a table."),
            ));
        };
        f(table)
    }

    /// Append a paragraph to a story part.
    pub fn part_paragraph(&mut self, id: u32, para: Paragraph) -> ModelResult<()> {
        let part = self.edit.doc.parts.get_mut(&id).ok_or_else(|| {
            ModelError::new("model.bad_path", format!("No story part with id {id}."))
        })?;
        part.blocks.push(Arc::new(Block::Paragraph(para)));
        Ok(())
    }

    /// The body blocks, for a caller that reads before it writes.
    pub fn blocks(&self) -> &[Arc<Block>] {
        &self.edit.doc.body
    }
}

fn consent(para: &mut Paragraph, opts: &EditOptions, report: &mut EditReport) -> ModelResult<()> {
    let checks = [
        (para.retention.revisions, DropKind::Revisions),
        (para.retention.comments, DropKind::Comments),
        (para.retention.equations, DropKind::Equations),
    ];
    for (present, kind) in checks {
        if present && !opts.permits(kind) {
            return Err(ModelError::new(
                "model.needs_drop_consent",
                "The edit would drop retained markup. Pass EditOptions that allow that drop.",
            ));
        }
        if present && opts.permits(kind) && !report.dropped.contains(&kind) {
            report.dropped.push(kind);
            para.raw_xml = None;
            match kind {
                DropKind::Revisions => para.retention.revisions = false,
                DropKind::Comments => para.retention.comments = false,
                DropKind::Equations => para.retention.equations = false,
                DropKind::Objects => {}
            }
        }
    }
    Ok(())
}

pub struct ParaCursor<'a> {
    para: &'a mut Paragraph,
}

impl ParaCursor<'_> {
    pub fn text(&mut self, text: &str) -> ModelResult<()> {
        self.para
            .push_text(text, &crate::props::CharProps::default());
        Ok(())
    }

    pub fn merge_field(&mut self, name: &str) -> ModelResult<()> {
        let instr = crate::fields::quote_name(name);
        let result = crate::fields::placeholder(name);
        self.para.push_object(
            InlineObject::Field(crate::fields::Field::new(&instr, &result)),
            &crate::props::CharProps::default(),
        );
        Ok(())
    }

    pub fn insert_text(&mut self, offset: usize, text: &str) -> ModelResult<()> {
        self.para
            .insert_text(offset, text, &crate::props::CharProps::default())
    }

    pub fn run(&mut self, builder: RunBuilder) -> ModelResult<()> {
        if let Some(err) = builder.error {
            return Err(err);
        }
        self.para.push_text(&builder.text, &builder.props);
        Ok(())
    }

    pub fn paragraph(&self) -> &Paragraph {
        self.para
    }

    pub fn paragraph_mut(&mut self) -> &mut Paragraph {
        self.para
    }
}
