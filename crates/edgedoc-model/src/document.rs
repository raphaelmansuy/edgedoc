use std::collections::BTreeMap;
use std::sync::Arc;

use crate::error::{ModelError, ModelResult};
use crate::fields::Instruction;
use crate::para::{InlineObject, Paragraph, Picture, RunBuilder, OBJ};
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

/// A caret after an edit, in source bytes of one paragraph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextCaret {
    pub path: String,
    pub offset: usize,
}

impl<'a> DocumentEdit<'a> {
    /// Insert characters at a caret. Object markers in `text` are dropped.
    pub fn insert_chars(
        &mut self,
        path: &str,
        offset: usize,
        text: &str,
        props: &crate::props::CharProps,
    ) -> ModelResult<TextCaret> {
        let mut caret = TextCaret {
            path: path.to_string(),
            offset,
        };
        self.with_leaf(path, |para| {
            let before = para.text.len();
            para.insert_text(offset, text, props)?;
            caret.offset = offset + para.text.len().saturating_sub(before);
            Ok(())
        })?;
        Ok(caret)
    }

    /// Insert a merge field. The caret moves to the far side of the field.
    pub fn insert_merge_field(
        &mut self,
        path: &str,
        offset: usize,
        name: &str,
        props: &crate::props::CharProps,
    ) -> ModelResult<TextCaret> {
        let instr = crate::fields::quote_name(name);
        let object = InlineObject::Field(crate::fields::Field::new(
            &instr,
            &crate::fields::placeholder(name),
        ));
        self.insert_inline(path, offset, object, props)
    }

    /// Insert a picture at the caret.
    pub fn insert_picture(
        &mut self,
        path: &str,
        offset: usize,
        picture: Picture,
        props: &crate::props::CharProps,
    ) -> ModelResult<TextCaret> {
        self.insert_inline(path, offset, InlineObject::Picture(picture), props)
    }

    fn insert_inline(
        &mut self,
        path: &str,
        offset: usize,
        object: InlineObject,
        props: &crate::props::CharProps,
    ) -> ModelResult<TextCaret> {
        self.with_leaf(path, |para| para.insert_object(offset, object, props))?;
        Ok(TextCaret {
            path: path.to_string(),
            offset: offset + OBJ.len_utf8(),
        })
    }

    /// Split the paragraph. The caret is the start of the new paragraph.
    pub fn break_paragraph(&mut self, path: &str, offset: usize) -> ModelResult<TextCaret> {
        let next = next_block_path(path);
        self.with_parent(path, |blocks, index| {
            let right = {
                let para = paragraph_mut(blocks, index)?;
                para.split_off(offset)?
            };
            blocks.insert(index.saturating_add(1), Arc::new(Block::Paragraph(right)));
            Ok(())
        })?;
        Ok(TextCaret {
            path: next,
            offset: 0,
        })
    }

    /// Pull this paragraph into the previous one. The caret sits at the join.
    pub fn join_previous(&mut self, path: &str) -> ModelResult<TextCaret> {
        let parsed = parse_path(path)?;
        let index = leaf_index(&parsed);
        if index == 0 {
            return Ok(TextCaret {
                path: path.to_string(),
                offset: 0,
            });
        }
        let prev = replace_leaf_index(path, index - 1);
        let mut joined = None;
        self.with_parent(path, |blocks, index| {
            if index == 0 || index >= blocks.len() {
                return Ok(());
            }
            let Block::Paragraph(_) = blocks[index - 1].as_ref() else {
                return Ok(());
            };
            let Block::Paragraph(_) = blocks[index].as_ref() else {
                return Ok(());
            };
            let right = blocks.remove(index);
            let Block::Paragraph(right) = right.as_ref() else {
                return Ok(());
            };
            let left = paragraph_mut(blocks, index - 1)?;
            let at = left.text.len();
            left.append_paragraph(right);
            joined = Some(at);
            Ok(())
        })?;
        match joined {
            Some(offset) => Ok(TextCaret { path: prev, offset }),
            None => self.paragraph_info(path, 0).map(|_| TextCaret {
                path: path.to_string(),
                offset: 0,
            }),
        }
    }

    /// Delete a span. Both ends must sit in the same block list.
    pub fn delete_span(
        &mut self,
        start_path: &str,
        start_off: usize,
        end_path: &str,
        end_off: usize,
    ) -> ModelResult<TextCaret> {
        let start = parse_path(start_path)?;
        let end = parse_path(end_path)?;
        if !same_container(&start, &end) {
            return Err(ModelError::new(
                "model.bad_path",
                "The selection crosses a boundary the editor cannot delete in one step.",
            ));
        }
        let mut from = leaf_index(&start);
        let mut to = leaf_index(&end);
        let mut from_off = start_off;
        let mut to_off = end_off;
        let mut from_path = start_path.to_string();
        if to < from || (to == from && to_off < from_off) {
            std::mem::swap(&mut from, &mut to);
            std::mem::swap(&mut from_off, &mut to_off);
            from_path = end_path.to_string();
        }
        self.with_parent(&from_path, |blocks, _| {
            if from >= blocks.len() || to >= blocks.len() {
                return Err(ModelError::new(
                    "model.bad_path",
                    "The selection does not land on a paragraph.",
                ));
            }
            if to == from {
                let para = paragraph_mut(blocks, from)?;
                let end = to_off.min(para.text.len());
                let start = from_off.min(end);
                para.delete_range(start, end)?;
                return Ok(());
            }
            {
                let para = paragraph_mut(blocks, from)?;
                let end = para.text.len();
                let start = from_off.min(end);
                para.delete_range(start, end)?;
            }
            if to_off > 0 {
                let para = paragraph_mut(blocks, to)?;
                let end = to_off.min(para.text.len());
                para.delete_range(0, end)?;
            }
            if to > from + 1 {
                blocks.drain(from + 1..to);
            }
            let join = to_off > 0 && from + 1 < blocks.len();
            if join {
                let right = blocks.remove(from + 1);
                let right_para = matches!(right.as_ref(), Block::Paragraph(_));
                let left_para =
                    matches!(blocks.get(from).map(Arc::as_ref), Some(Block::Paragraph(_)));
                if right_para && left_para {
                    if let Block::Paragraph(para) = right.as_ref() {
                        let left = paragraph_mut(blocks, from)?;
                        left.append_paragraph(para);
                    }
                } else {
                    blocks.insert(from + 1, right);
                }
            }
            Ok(())
        })?;
        let offset = from_off;
        Ok(TextCaret {
            path: from_path,
            offset,
        })
    }

    /// Set or clear bold, italic, or underline on a range.
    pub fn format_range(
        &mut self,
        path: &str,
        start: usize,
        end: usize,
        kind: FormatKind,
        enable: bool,
    ) -> ModelResult<()> {
        self.with_leaf(path, |para| {
            para.update_range(start.min(end), start.max(end), |props| match kind {
                FormatKind::Bold => props.bold = enable,
                FormatKind::Italic => props.italic = enable,
                FormatKind::Underline => props.underline = enable,
                FormatKind::Strike => props.strike = enable,
                FormatKind::FontSize(size) => props.size_pt = Some(size),
                FormatKind::Font(ref name) => props.font = Some(name.clone()),
                // An empty colour or highlight means "inherit", the convention `VertAlign` already uses:
                // it is how Word's "Automatic" and "No highlight" arrive through one door.
                FormatKind::Color(ref hex) => {
                    props.color = if hex.is_empty() { None } else { Some(hex.clone()) };
                }
                FormatKind::Highlight(ref name) => {
                    props.highlight = if name.is_empty() || name == "none" { None } else { Some(name.clone()) };
                }
                FormatKind::VertAlign(ref name) => {
                    props.vert_align = if enable && !name.is_empty() {
                        Some(name.clone())
                    } else {
                        None
                    };
                }
            })
        })
    }

    pub fn set_align(&mut self, path: &str, align: &str) -> ModelResult<()> {
        self.with_leaf(path, |para| {
            para.props.align = Some(align.to_string());
            para.raw_xml = None;
            Ok(())
        })
    }

    pub fn set_list(
        &mut self,
        path: &str,
        kind: Option<crate::props::ListKind>,
    ) -> ModelResult<()> {
        self.with_leaf(path, |para| {
            para.props.list = kind.map(|kind| crate::props::ListSpec { kind, level: 0 });
            if kind.is_none() {
                para.props.num_id = None;
            }
            para.raw_xml = None;
            Ok(())
        })
    }

    /// Insert a grid after the paragraph and put the caret in its first cell.
    pub fn insert_table(&mut self, path: &str, rows: usize, cols: usize) -> ModelResult<TextCaret> {
        if !(1..=32).contains(&rows) || !(1..=32).contains(&cols) {
            return Err(ModelError::new(
                "model.limit",
                "A table needs between 1 and 32 rows and columns.",
            ));
        }
        let table_path = next_block_path(path);
        self.with_parent(path, |blocks, index| {
            blocks.insert(
                index.saturating_add(1),
                Arc::new(Block::Table(blank_table(rows, cols))),
            );
            Ok(())
        })?;
        Ok(TextCaret {
            path: format!("{table_path}/row[0]/cell[0]/block[0]"),
            offset: 0,
        })
    }

    /// Insert an empty row into the table the caret is in, below the caret's own row
    /// (specs/144 DOC2-41).
    ///
    /// Word's *Insert Below*, and the one table operation a quotation needs: a merge template's row
    /// count is a shape the author sets while designing, and adding a row is how that shape changes.
    /// The new row copies the anchor row's cell count — a table whose rows disagree about their width
    /// is not a table anybody designed.
    ///
    /// Returns the caret's new path (first cell of the anchor row), so a person keeps typing inside
    /// the table. A path that is not inside a table is refused: "insert a row" has no meaning for a
    /// paragraph, and a silent no-op would look like a broken button.
    pub fn insert_table_row(&mut self, cell_path: &str) -> ModelResult<TextCaret> {
        let (table_path, row_index) = table_and_row(cell_path)?;
        let index = table_block_index(&table_path)?;
        self.with_parent(&table_path, |blocks, _| {
            let block = blocks.get_mut(index).ok_or_else(|| {
                ModelError::new("model.bad_path", "That table is not in the document anymore.")
            })?;
            let Block::Table(table) = Arc::make_mut(block) else {
                return Err(ModelError::new("model.bad_path", "That path does not name a table."));
            };
            let cols = table
                .rows
                .get(row_index)
                .map(|row| row.cells.len())
                .ok_or_else(|| {
                    ModelError::new("model.bad_path", "That row is not in the table anymore.")
                })?;
            let cells = (0..cols).map(|_| crate::table::Cell::paragraph(""));
            table
                .rows
                .insert(row_index.saturating_add(1), crate::table::Row::cells(cells));
            Ok(())
        })?;
        Ok(TextCaret {
            path: format!("{table_path}/row[{row_index}]/cell[0]/block[0]"),
            offset: 0,
        })
    }

    /// Delete the row the caret is in (specs/144 DOC2-41). A table keeps at least one row.
    pub fn delete_table_row(&mut self, cell_path: &str) -> ModelResult<TextCaret> {
        let (table_path, row_index) = table_and_row(cell_path)?;
        let index = table_block_index(&table_path)?;
        let mut landed = row_index;
        self.with_parent(&table_path, |blocks, _| {
            let block = blocks.get_mut(index).ok_or_else(|| {
                ModelError::new("model.bad_path", "That table is not in the document anymore.")
            })?;
            let Block::Table(table) = Arc::make_mut(block) else {
                return Err(ModelError::new("model.bad_path", "That path does not name a table."));
            };
            if table.rows.len() <= 1 {
                return Err(ModelError::new(
                    "model.limit",
                    "A table needs at least one row. Delete the whole table instead.",
                ));
            }
            if row_index >= table.rows.len() {
                return Err(ModelError::new(
                    "model.bad_path",
                    "That row is not in the table anymore.",
                ));
            }
            table.rows.remove(row_index);
            // The caret lands on the row that took its place, or the new last row.
            landed = row_index.min(table.rows.len().saturating_sub(1));
            Ok(())
        })?;
        Ok(TextCaret {
            path: format!("{table_path}/row[{landed}]/cell[0]/block[0]"),
            offset: 0,
        })
    }

    pub fn paragraph_info(&self, path: &str, offset: usize) -> ModelResult<ParagraphInfo> {        let para = self.paragraph_ref(path)?;
        let align = para
            .props
            .align
            .clone()
            .unwrap_or_else(|| "left".to_string());
        let field = para.object_at(offset).and_then(|object| match object {
            InlineObject::Field(field) => {
                crate::fields::Instruction::parse(&field.instr).merge_name()
            }
            _ => None,
        });
        Ok(ParagraphInfo {
            len: para.text.len(),
            text: para.text.clone(),
            align,
            props: para.props_at(offset),
            field,
            list: para.props.list.map(|spec| spec.kind),
            indent_left: para.props.indent_left.map(|twips| twips.0).unwrap_or(0),
            line: para.props.line.map(|twips| twips.0),
            shading: para.props.shading.clone(),
            border_bottom: para.props.borders.bottom.is_some(),
        })
    }

    /// Bump the left indent by `delta` twips. Half an inch is 720.
    pub fn bump_indent(&mut self, path: &str, delta: i32) -> ModelResult<()> {
        self.with_leaf(path, |para| {
            let current = para.props.indent_left.map(|twips| twips.0).unwrap_or(0);
            let next = (current + delta).clamp(0, 10_080);
            para.props.indent_left = if next == 0 {
                None
            } else {
                Some(crate::Twips(next))
            };
            para.raw_xml = None;
            Ok(())
        })
    }

    /// Set automatic line spacing. Common values: 240 single, 360 1.5, 480 double.
    pub fn set_line_spacing(&mut self, path: &str, line: i32) -> ModelResult<()> {
        self.with_leaf(path, |para| {
            if line <= 0 {
                para.props.line = None;
                para.props.line_rule = None;
            } else {
                para.props.line = Some(crate::Twips(line));
                para.props.line_rule = Some("auto".into());
            }
            para.raw_xml = None;
            Ok(())
        })
    }

    /// Set space after the paragraph in twips.
    pub fn set_space_after(&mut self, path: &str, twips: i32) -> ModelResult<()> {
        self.with_leaf(path, |para| {
            para.props.spacing_after = if twips <= 0 {
                Some(crate::Twips(0))
            } else {
                Some(crate::Twips(twips))
            };
            para.raw_xml = None;
            Ok(())
        })
    }

    /// Set space before the paragraph in twips.
    pub fn set_space_before(&mut self, path: &str, twips: i32) -> ModelResult<()> {
        self.with_leaf(path, |para| {
            para.props.spacing_before = if twips <= 0 {
                Some(crate::Twips(0))
            } else {
                Some(crate::Twips(twips))
            };
            para.raw_xml = None;
            Ok(())
        })
    }

    /// Paint or clear the paragraph background.
    pub fn set_para_shading(&mut self, path: &str, fill: Option<&str>) -> ModelResult<()> {
        self.with_leaf(path, |para| {
            para.props.shading = fill.map(|color| color.trim_start_matches('#').to_uppercase());
            para.raw_xml = None;
            Ok(())
        })
    }

    /// Apply a paragraph border preset at the caret paragraph.
    pub fn set_para_border(&mut self, path: &str, kind: &str) -> ModelResult<()> {
        self.with_leaf(path, |para| {
            let stroke = crate::table::LineBorder {
                val: "single".into(),
                size: Some(6),
                color: Some("auto".into()),
                space: Some(1),
            };
            match kind {
                "bottom" => {
                    para.props.borders = crate::table::Borders {
                        bottom: Some(stroke),
                        ..crate::table::Borders::default()
                    };
                    para.props.borders_present = true;
                    para.raw_xml = None;
                }
                "top" => {
                    para.props.borders = crate::table::Borders {
                        top: Some(stroke),
                        ..crate::table::Borders::default()
                    };
                    para.props.borders_present = true;
                    para.raw_xml = None;
                }
                "all" => {
                    para.props.borders = crate::table::Borders {
                        top: Some(stroke.clone()),
                        left: Some(stroke.clone()),
                        bottom: Some(stroke.clone()),
                        right: Some(stroke.clone()),
                        ..crate::table::Borders::default()
                    };
                    para.props.borders_present = true;
                    para.raw_xml = None;
                }
                "none" => {
                    para.props.borders = crate::table::Borders::default();
                    para.props.borders_present = true;
                    para.raw_xml = None;
                }
                _ => {}
            }
            Ok(())
        })
    }

    /// True when every character in the range satisfies `pred`.
    pub fn range_is<F>(&self, path: &str, start: usize, end: usize, pred: F) -> ModelResult<bool>
    where
        F: FnMut(&crate::props::CharProps) -> bool,
    {
        let para = self.paragraph_ref(path)?;
        let (from, to) = if start <= end {
            (start, end)
        } else {
            (end, start)
        };
        Ok(para.range_all(from, to, pred))
    }

    fn paragraph_ref(&self, path: &str) -> ModelResult<&Paragraph> {
        paragraph_ref_in(self.doc, &parse_path(path)?)
    }

    fn with_leaf<F>(&mut self, path: &str, f: F) -> ModelResult<()>
    where
        F: FnOnce(&mut Paragraph) -> ModelResult<()>,
    {
        self.with_parent(path, |blocks, index| {
            let para = paragraph_mut(blocks, index)?;
            f(para)
        })
    }

    fn with_parent<F>(&mut self, path: &str, f: F) -> ModelResult<()>
    where
        F: FnOnce(&mut Vec<Arc<Block>>, usize) -> ModelResult<()>,
    {
        let parsed = parse_path(path)?;
        let doc = &mut *self.doc;
        match parsed.head {
            Head::Body(index) => descend(&mut doc.body, index, &parsed.nests, f),
            Head::Part { id, block } => {
                let part = doc.parts.get_mut(&id).ok_or_else(|| {
                    ModelError::new("model.bad_path", format!("No story part with id {id}."))
                })?;
                descend(&mut part.blocks, block, &parsed.nests, f)
            }
        }
    }
}

/// What a format command changes on a character range.
#[derive(Debug, Clone, PartialEq)]
pub enum FormatKind {
    Bold,
    Italic,
    Underline,
    Strike,
    FontSize(f32),
    Font(String),
    Color(String),
    Highlight(String),
    /// `superscript`, `subscript`, or empty to clear.
    VertAlign(String),
}

/// The paragraph facts the ribbon reads at the caret.
#[derive(Debug, Clone, PartialEq)]
pub struct ParagraphInfo {
    pub len: usize,
    pub text: String,
    pub align: String,
    pub props: crate::props::CharProps,
    pub field: Option<String>,
    pub list: Option<crate::props::ListKind>,
    /// Left indent in twips.
    pub indent_left: i32,
    /// Line spacing in twips when the rule is `auto`.
    pub line: Option<i32>,
    pub shading: Option<String>,
    pub border_bottom: bool,
}

fn paragraph_mut(blocks: &mut [Arc<Block>], index: usize) -> ModelResult<&mut Paragraph> {
    let block = blocks
        .get_mut(index)
        .ok_or_else(|| ModelError::new("model.bad_path", format!("No block at index {index}.")))?;
    match Arc::make_mut(block) {
        Block::Paragraph(para) => {
            if para.locked {
                return Err(ModelError::new(
                    "model.locked",
                    "This content is locked. Unlock it before editing.",
                ));
            }
            Ok(para)
        }
        _ => Err(ModelError::new(
            "model.bad_path",
            format!("Block {index} is not a paragraph."),
        )),
    }
}

fn descend<F>(
    blocks: &mut Vec<Arc<Block>>,
    index: usize,
    nests: &[(usize, usize, usize)],
    f: F,
) -> ModelResult<()>
where
    F: FnOnce(&mut Vec<Arc<Block>>, usize) -> ModelResult<()>,
{
    if nests.is_empty() {
        return f(blocks, index);
    }
    let (row, cell, block_index) = nests[0];
    let current = blocks
        .get_mut(index)
        .ok_or_else(|| ModelError::new("model.bad_path", format!("No block at index {index}.")))?;
    let Block::Table(table) = Arc::make_mut(current) else {
        return Err(ModelError::new(
            "model.bad_path",
            "The path goes through a block that is not a table.",
        ));
    };
    let cell_blocks = table
        .rows
        .get_mut(row)
        .and_then(|row| row.cells.get_mut(cell))
        .ok_or_else(|| {
            ModelError::new(
                "model.bad_path",
                format!("No cell at row {row}, column {cell}."),
            )
        })?;
    descend(&mut cell_blocks.blocks, block_index, &nests[1..], f)
}

fn paragraph_ref_in<'a>(doc: &'a ModelDocument, parsed: &Parsed) -> ModelResult<&'a Paragraph> {
    let (blocks, index) = match parsed.head {
        Head::Body(index) => (&doc.body, index),
        Head::Part { id, block } => {
            let part = doc.parts.get(&id).ok_or_else(|| {
                ModelError::new("model.bad_path", format!("No story part with id {id}."))
            })?;
            (&part.blocks, block)
        }
    };
    let mut blocks = blocks;
    let mut index = index;
    for (row, cell, block_index) in &parsed.nests {
        let Block::Table(table) = blocks
            .get(index)
            .map(Arc::as_ref)
            .ok_or_else(|| ModelError::new("model.bad_path", "The path does not reach a table."))?
        else {
            return Err(ModelError::new(
                "model.bad_path",
                "The path goes through a block that is not a table.",
            ));
        };
        blocks = &table
            .rows
            .get(*row)
            .and_then(|row| row.cells.get(*cell))
            .ok_or_else(|| ModelError::new("model.bad_path", "The path does not reach a cell."))?
            .blocks;
        index = *block_index;
    }
    match blocks.get(index).map(Arc::as_ref) {
        Some(Block::Paragraph(para)) => Ok(para),
        _ => Err(ModelError::new(
            "model.bad_path",
            "The path does not reach a paragraph.",
        )),
    }
}

enum Head {
    Body(usize),
    Part { id: u32, block: usize },
}

struct Parsed {
    head: Head,
    nests: Vec<(usize, usize, usize)>,
}

fn parse_path(path: &str) -> ModelResult<Parsed> {
    let tokens: Vec<&str> = path.split('/').filter(|token| !token.is_empty()).collect();
    if tokens.is_empty() {
        return Err(ModelError::new(
            "model.bad_path",
            "The block path is empty.",
        ));
    }
    let (head, mut at) = if tokens[0].starts_with("body[") {
        (Head::Body(number_in(tokens[0], "body")?), 1)
    } else if tokens[0].starts_with("part[") {
        let id = number_in(tokens[0], "part")? as u32;
        let block = tokens
            .get(1)
            .ok_or_else(|| ModelError::new("model.bad_path", "A part path needs a block index."))?;
        (
            Head::Part {
                id,
                block: number_in(block, "block")?,
            },
            2,
        )
    } else {
        return Err(ModelError::new(
            "model.bad_path",
            format!("The path \"{path}\" does not start at the body or a story part."),
        ));
    };
    let mut nests = Vec::new();
    while at < tokens.len() {
        let row = number_in(token_at(&tokens, at)?, "row")?;
        let cell = number_in(token_at(&tokens, at + 1)?, "cell")?;
        let block = number_in(token_at(&tokens, at + 2)?, "block")?;
        nests.push((row, cell, block));
        at += 3;
    }
    Ok(Parsed { head, nests })
}

fn token_at<'a>(tokens: &[&'a str], index: usize) -> ModelResult<&'a str> {
    tokens.get(index).copied().ok_or_else(|| {
        ModelError::new(
            "model.bad_path",
            "The path is missing a row, cell, or block index.",
        )
    })
}

fn number_in(token: &str, name: &str) -> ModelResult<usize> {
    let Some(rest) = token.strip_prefix(name) else {
        return Err(ModelError::new(
            "model.bad_path",
            format!("Expected {name}[…] and found \"{token}\"."),
        ));
    };
    let Some(rest) = rest.strip_prefix('[') else {
        return Err(ModelError::new(
            "model.bad_path",
            format!("Expected {name}[…] and found \"{token}\"."),
        ));
    };
    let Some(rest) = rest.strip_suffix(']') else {
        return Err(ModelError::new(
            "model.bad_path",
            format!("Expected {name}[…] and found \"{token}\"."),
        ));
    };
    rest.parse::<usize>().map_err(|_| {
        ModelError::new(
            "model.bad_path",
            format!("The index in \"{token}\" is not a number."),
        )
    })
}

fn leaf_index(parsed: &Parsed) -> usize {
    parsed
        .nests
        .last()
        .map(|(_, _, block)| *block)
        .unwrap_or(match parsed.head {
            Head::Body(index) => index,
            Head::Part { block, .. } => block,
        })
}

fn same_container(a: &Parsed, b: &Parsed) -> bool {
    let head = match (&a.head, &b.head) {
        (Head::Body(_), Head::Body(_)) => a.nests.is_empty() && b.nests.is_empty(),
        (Head::Part { id: left, .. }, Head::Part { id: right, .. }) => {
            left == right && a.nests.is_empty() && b.nests.is_empty()
        }
        _ => false,
    };
    if head {
        return true;
    }
    if a.nests.len() != b.nests.len() || a.nests.is_empty() {
        return false;
    }
    let heads = match (&a.head, &b.head) {
        (Head::Body(left), Head::Body(right)) => left == right,
        (
            Head::Part {
                id: left,
                block: left_block,
            },
            Head::Part {
                id: right,
                block: right_block,
            },
        ) => left == right && left_block == right_block,
        _ => false,
    };
    if !heads {
        return false;
    }
    a.nests[..a.nests.len() - 1] == b.nests[..b.nests.len() - 1]
        && a.nests.last().map(|item| (item.0, item.1))
            == b.nests.last().map(|item| (item.0, item.1))
}

fn replace_leaf_index(path: &str, index: usize) -> String {
    match path.rfind('[') {
        Some(start) => match path[start + 1..].find(']') {
            Some(end) => format!(
                "{}[{index}]{}",
                &path[..start],
                &path[start + 1 + end + 1..]
            ),
            None => path.to_string(),
        },
        None => path.to_string(),
    }
}

/// Where a table sits and which of its rows a cell path names.
///
/// A cell path is `body[n]/row[r]/cell[c]/block[b]`. The table lives at the path up to `/row[`, and
/// the row index is the first nest — the one `parse_path` already read.
fn table_and_row(cell_path: &str) -> ModelResult<(String, usize)> {
    let parsed = parse_path(cell_path)?;
    let Some((row, _, _)) = parsed.nests.first() else {
        return Err(ModelError::new(
            "model.bad_path",
            "That is not a table cell, so there is no row to add or remove.",
        ));
    };
    let table_path = cell_path
        .split("/row[")
        .next()
        .filter(|head| !head.is_empty())
        .ok_or_else(|| ModelError::new("model.bad_path", "That path does not name a table."))?
        .to_string();
    Ok((table_path, *row))
}

/// The index of a table among the blocks of the list that holds it.
fn table_block_index(table_path: &str) -> ModelResult<usize> {
    let parsed = parse_path(table_path)?;
    if !parsed.nests.is_empty() {
        return Err(ModelError::new(
            "model.bad_path",
            "That path does not name a table at the top level of its story.",
        ));
    }
    match parsed.head {
        Head::Body(index) => Ok(index),
        Head::Part { block, .. } => Ok(block),
    }
}

fn next_block_path(path: &str) -> String {    let parsed = parse_path(path);
    let Ok(parsed) = parsed else {
        return path.to_string();
    };
    replace_leaf_index(path, leaf_index(&parsed).saturating_add(1))
}

fn blank_table(rows: usize, cols: usize) -> crate::table::Table {
    use crate::table::{Borders, LineBorder, Table};
    let line = LineBorder {
        val: "single".into(),
        size: Some(4),
        color: Some("BFBFBF".into()),
        space: Some(0),
    };
    let mut table = Table::new();
    table.borders = Borders {
        top: Some(line.clone()),
        left: Some(line.clone()),
        bottom: Some(line.clone()),
        right: Some(line.clone()),
        inside_h: Some(line.clone()),
        inside_v: Some(line),
        ..Borders::default()
    };
    for _ in 0..rows {
        let cells = (0..cols).map(|_| crate::table::Cell::paragraph(""));
        table.rows.push(crate::table::Row::cells(cells));
    }
    table
}
