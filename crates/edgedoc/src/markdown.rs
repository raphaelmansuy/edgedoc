//! DOCX body to Markdown.
//!
//! [`Document::to_markdown`] reads the document already in memory. Image bytes stay on
//! [`ExportedImage`] and the Markdown points at `images/<file>`. [`MarkdownOptions::image_dir`]
//! writes those files, and [`MarkdownOptions::link_prefix`] is the path the Markdown uses.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use edgedoc_model::{
    heading_level, Block, CharProps, InlineObject, ListKind, Paragraph, PartKind, Picture, Table,
};

use crate::{Document, Error};

/// Where a Markdown export puts images, and how the Markdown names them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MarkdownOptions {
    /// Directory that receives one file per image. It is created when missing.
    /// `None` writes nothing and leaves the bytes on [`ExportedImage::bytes`].
    pub image_dir: Option<PathBuf>,
    /// Path prefix in each image link, with `/` separators.
    ///
    /// `None` uses the last component of [`Self::image_dir`] when that directory is set,
    /// and `images` otherwise. An empty prefix makes the link the file name alone, which
    /// matches images stored beside the Markdown file.
    pub link_prefix: Option<String>,
}

impl MarkdownOptions {
    pub fn new() -> Self {
        Self::default()
    }

    /// Store exported images in `dir`.
    pub fn image_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.image_dir = Some(dir.into());
        self
    }

    /// Use `prefix` in image links, such as `media` or `https://cdn.example/media`.
    pub fn link_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.link_prefix = Some(prefix.into());
        self
    }
}

/// One image taken out of the document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportedImage {
    /// File name, with no directory components.
    pub file_name: String,
    /// The path written into the Markdown image link.
    pub markdown_path: String,
    /// Where the file was written. `None` when no image directory was requested.
    pub stored_at: Option<PathBuf>,
    pub content_type: String,
    /// The image bytes from the package.
    pub bytes: Vec<u8>,
    /// The picture's description, or an empty string.
    pub alt: String,
}

/// Markdown for one document, plus the images it references.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkdownDocument {
    pub markdown: String,
    pub images: Vec<ExportedImage>,
}

impl Document {
    /// Convert the body to Markdown.
    ///
    /// Images are not written. The Markdown references them under `images/`, and
    /// [`ExportedImage::bytes`] holds the file contents.
    pub fn to_markdown(&self) -> Result<MarkdownDocument, Error> {
        self.to_markdown_with(&MarkdownOptions::new())
    }

    /// Convert the body to Markdown, writing images when [`MarkdownOptions::image_dir`] is set.
    ///
    /// Headings come from a `Heading1`–`Heading9` style, a style based on one, or a
    /// Word outline level. Lists use the paragraph's list, or the numbering part after
    /// a DOCX read. A page or column break becomes a thematic break. An equation becomes
    /// LaTeX, with `$` for inline math and `$$` for a displayed equation. Scripts, radicals,
    /// matrices, delimiters, accents, and the other Office Math structures are included.
    /// Footnotes and
    /// endnotes become Markdown footnotes. The link in each image is
    /// [`ExportedImage::markdown_path`].
    pub fn to_markdown_with(&self, options: &MarkdownOptions) -> Result<MarkdownDocument, Error> {
        let mut export = Export::new(self, options);
        let markdown = export.render();
        if let Some(dir) = &options.image_dir {
            if !export.images.is_empty() {
                store_images(dir, &mut export.images)?;
            }
        }
        Ok(MarkdownDocument {
            markdown,
            images: export.images,
        })
    }
}

struct Export<'a> {
    doc: &'a Document,
    prefix: String,
    images: Vec<ExportedImage>,
    names: BTreeSet<String>,
    bullets: BTreeMap<(u32, u32), bool>,
}

impl<'a> Export<'a> {
    fn new(doc: &'a Document, options: &MarkdownOptions) -> Self {
        Self {
            doc,
            prefix: link_prefix(options),
            images: Vec::new(),
            names: BTreeSet::new(),
            bullets: numbering_bullets(doc),
        }
    }

    fn render(&mut self) -> String {
        let mut pieces = Vec::new();
        self.blocks(self.doc.body_blocks(), &mut pieces);
        let mut markdown = join_pieces(&pieces);
        let notes = self.note_definitions();
        if !notes.is_empty() {
            if !markdown.is_empty() && !markdown.ends_with("\n\n") {
                if markdown.ends_with('\n') {
                    markdown.push('\n');
                } else {
                    markdown.push_str("\n\n");
                }
            }
            markdown.push_str(&notes);
            if !markdown.ends_with('\n') {
                markdown.push('\n');
            }
        }
        markdown
    }

    fn blocks(&mut self, blocks: &[std::sync::Arc<Block>], out: &mut Vec<Piece>) {
        for block in blocks {
            match block.as_ref() {
                Block::Paragraph(para) => self.paragraph(para, out),
                Block::Table(table) => {
                    let text = self.table(table);
                    if !text.is_empty() {
                        out.push(Piece::Block(text));
                    }
                }
                Block::Control(control) => self.blocks(&control.blocks, out),
                Block::Alternate { choice, .. } => self.blocks(choice, out),
                Block::Opaque(_) => {}
            }
        }
    }

    fn paragraph(&mut self, para: &Paragraph, out: &mut Vec<Piece>) {
        if break_only(para) {
            out.push(Piece::Rule);
            return;
        }
        let flat = heading_of(self.doc, para).is_none() && list_of(para, &self.bullets).is_some();
        let inline = self.inline(para, false, flat);
        let text = inline.trim();
        if text.is_empty() {
            return;
        }
        if let Some(level) = heading_of(self.doc, para) {
            let hashes = "#".repeat(level as usize);
            out.push(Piece::Block(format!("{hashes} {text}")));
            return;
        }
        if let Some((bullet, level)) = list_of(para, &self.bullets) {
            let indent = "  ".repeat(level as usize);
            let marker = if bullet { "-" } else { "1." };
            out.push(Piece::List {
                bullet,
                level,
                line: format!("{indent}{marker} {text}"),
            });
            return;
        }
        out.push(Piece::Block(text.to_string()));
    }

    fn table(&mut self, table: &Table) -> String {
        let mut rows = Vec::new();
        for row in &table.rows {
            if row.hidden {
                continue;
            }
            let cells: Vec<String> = row
                .cells
                .iter()
                .map(|cell| self.cell(&cell.blocks))
                .collect();
            rows.push(cells);
        }
        if rows.is_empty() {
            return String::new();
        }
        let width = rows.iter().map(Vec::len).max().unwrap_or(0);
        if width == 0 {
            return String::new();
        }
        for row in &mut rows {
            row.resize(width, String::new());
        }
        let mut out = String::new();
        push_row(&mut out, &rows[0]);
        out.push('|');
        for _ in 0..width {
            out.push_str(" --- |");
        }
        out.push('\n');
        for row in rows.iter().skip(1) {
            push_row(&mut out, row);
        }
        while out.ends_with('\n') {
            out.pop();
        }
        out
    }

    fn cell(&mut self, blocks: &[std::sync::Arc<Block>]) -> String {
        let mut parts = Vec::new();
        for block in blocks {
            match block.as_ref() {
                Block::Paragraph(para) => {
                    let text = self.inline(para, true, false);
                    let text = text.replace('\n', " ").trim().to_string();
                    if text.is_empty() {
                        continue;
                    }
                    if let Some((bullet, _)) = list_of(para, &self.bullets) {
                        let marker = if bullet { "-" } else { "1." };
                        parts.push(format!("{marker} {text}"));
                    } else {
                        parts.push(text);
                    }
                }
                Block::Table(table) => {
                    let nested =
                        escape_pipes_outside_math(&self.table(table)).replace('\n', "<br>");
                    if !nested.is_empty() {
                        parts.push(nested);
                    }
                }
                Block::Control(control) => {
                    let text = self.cell(&control.blocks);
                    if !text.is_empty() {
                        parts.push(text);
                    }
                }
                Block::Alternate { choice, .. } => {
                    let text = self.cell(choice);
                    if !text.is_empty() {
                        parts.push(text);
                    }
                }
                Block::Opaque(_) => {}
            }
        }
        parts.join("<br>")
    }

    fn inline(&mut self, para: &Paragraph, cell: bool, flat: bool) -> String {
        let mut objects = para.objects.iter();
        let mut cursor = 0usize;
        let mut chunks: Vec<(Option<String>, String)> = Vec::new();
        for run in &para.runs {
            let end = cursor.saturating_add(run.len).min(para.text.len());
            let slice = para.text.get(cursor..end).unwrap_or("");
            cursor = end;
            if run.props.vanish || run.props.web_hidden {
                for ch in slice.chars() {
                    if ch == edgedoc_model::OBJ {
                        let _ = objects.next();
                    }
                }
                continue;
            }
            let body = self.slice(slice, &run.props, &mut objects, cell, flat);
            if body.is_empty() {
                continue;
            }
            let link = run.props.link.clone();
            if let Some((current, acc)) = chunks.last_mut() {
                if *current == link {
                    separate_math(acc, &body);
                    acc.push_str(&body);
                    continue;
                }
            }
            chunks.push((link, body));
        }
        let mut out = String::new();
        for (link, body) in chunks {
            if let Some(url) = link {
                out.push('[');
                out.push_str(&body);
                out.push_str("](");
                out.push_str(&escape_url(&url));
                out.push(')');
            } else {
                separate_math(&mut out, &body);
                out.push_str(&body);
            }
        }
        out
    }

    fn slice(
        &mut self,
        slice: &str,
        props: &CharProps,
        objects: &mut std::slice::Iter<'_, InlineObject>,
        cell: bool,
        flat: bool,
    ) -> String {
        let mut out = String::new();
        let mut text = String::new();
        let flush = |text: &mut String, out: &mut String| {
            if !text.is_empty() {
                out.push_str(&emphasize(text, props));
                text.clear();
            }
        };
        for ch in slice.chars() {
            if ch == edgedoc_model::OBJ {
                flush(&mut text, &mut out);
                let piece = self.object(objects.next(), cell || flat);
                if out.ends_with('$') && piece.starts_with('$') {
                    out.push(' ');
                }
                out.push_str(&piece);
            } else if ch == '\n' {
                flush(&mut text, &mut out);
                out.push_str(if cell { "<br>" } else { "  \n" });
            } else if ch == edgedoc_model::PAGE_BREAK || ch == edgedoc_model::COLUMN_BREAK {
                flush(&mut text, &mut out);
                if !cell {
                    out.push_str("\n\n---\n\n");
                }
            } else {
                text.push(ch);
            }
        }
        flush(&mut text, &mut out);
        out
    }

    fn object(&mut self, object: Option<&InlineObject>, cell: bool) -> String {
        match object {
            Some(InlineObject::Picture(picture)) => self.image(picture),
            Some(InlineObject::Field(field)) => field_text(field),
            Some(InlineObject::Note { kind, id }) => note_marker(kind, *id),
            Some(InlineObject::Math { latex, display, .. }) => math_markdown(latex, *display, cell),
            Some(InlineObject::PositionalTab { .. }) => {
                if cell {
                    " ".into()
                } else {
                    "\t".into()
                }
            }
            _ => String::new(),
        }
    }

    fn image(&mut self, picture: &Picture) -> String {
        let file_name = image_file_name(picture, self.images.len() + 1, &mut self.names);
        let markdown_path = join_prefix(&self.prefix, &file_name);
        let alt = picture.alt.clone().unwrap_or_default();
        let markdown = format!("![{}]({markdown_path})", escape_alt(&alt));
        self.images.push(ExportedImage {
            file_name,
            markdown_path,
            stored_at: None,
            content_type: picture.content_type.clone(),
            bytes: picture.bytes.clone(),
            alt,
        });
        markdown
    }

    fn note_definitions(&mut self) -> String {
        let mut lines = Vec::new();
        for kind in [PartKind::Footnote, PartKind::Endnote] {
            for id in self.doc.parts(kind) {
                let meta = self.doc.part_meta(id);
                if meta
                    .and_then(|meta| meta.note_type.as_deref())
                    .is_some_and(|kind| kind != "normal")
                {
                    continue;
                }
                let Some(note_id) = meta.and_then(|meta| meta.id) else {
                    continue;
                };
                if note_id < 0 {
                    continue;
                }
                let label = if kind == PartKind::Footnote {
                    format!("[^{note_id}]")
                } else {
                    format!("[^end-{note_id}]")
                };
                let blocks = arcs_of(self.doc.part_blocks(id));
                let mut pieces = Vec::new();
                self.blocks(&blocks, &mut pieces);
                let body = join_pieces(&pieces).trim().replace('\n', "\n    ");
                if body.is_empty() {
                    continue;
                }
                lines.push(format!("{label}: {body}"));
            }
        }
        let mut out = lines.join("\n\n");
        if !out.is_empty() {
            out.push('\n');
        }
        out
    }
}

fn arcs_of(blocks: Vec<Block>) -> Vec<std::sync::Arc<Block>> {
    blocks.into_iter().map(std::sync::Arc::new).collect()
}

enum Piece {
    Block(String),
    List {
        bullet: bool,
        level: u32,
        line: String,
    },
    Rule,
}

fn join_pieces(pieces: &[Piece]) -> String {
    let mut out = String::new();
    let mut in_list = false;
    let mut last_bullet = false;
    let mut last_level = 0u32;
    for piece in pieces {
        match piece {
            Piece::List {
                bullet,
                level,
                line,
            } => {
                let nested = *level > last_level;
                separate(&mut out, in_list && (*bullet == last_bullet || nested));
                out.push_str(line);
                in_list = true;
                last_bullet = *bullet;
                last_level = *level;
            }
            Piece::Rule => {
                separate(&mut out, false);
                out.push_str("---");
                in_list = false;
            }
            Piece::Block(text) => {
                if text.is_empty() {
                    continue;
                }
                separate(&mut out, false);
                out.push_str(text);
                in_list = false;
            }
        }
    }
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

/// Insert one newline when `tight` is set, and a blank line otherwise.
fn separate(out: &mut String, tight: bool) {
    if out.is_empty() {
        return;
    }
    let want = if tight { 1 } else { 2 };
    let have = if out.ends_with("\n\n") {
        2
    } else if out.ends_with('\n') {
        1
    } else {
        0
    };
    for _ in have..want {
        out.push('\n');
    }
}

fn push_row(out: &mut String, cells: &[String]) {
    out.push('|');
    for cell in cells {
        out.push(' ');
        if cell.is_empty() {
            out.push(' ');
        } else {
            out.push_str(cell);
        }
        out.push_str(" |");
    }
    out.push('\n');
}

fn break_only(para: &Paragraph) -> bool {
    !para.text.is_empty()
        && para.objects.is_empty()
        && para
            .text
            .chars()
            .all(|ch| ch == edgedoc_model::PAGE_BREAK || ch == edgedoc_model::COLUMN_BREAK)
}

fn heading_of(doc: &Document, para: &Paragraph) -> Option<u32> {
    if let Some(id) = para.props.style.as_deref() {
        if let Some(level) = style_heading(doc, id, 0) {
            return Some(level);
        }
    }
    para.props.outline.and_then(word_outline)
}

fn style_heading(doc: &Document, id: &str, depth: usize) -> Option<u32> {
    if depth > 32 {
        return None;
    }
    if let Some(level) = heading_level(id).and_then(named_heading) {
        return Some(level);
    }
    let style = doc.styles().by_id(id)?;
    if let Some(level) = heading_level(&style.name).and_then(named_heading) {
        return Some(level);
    }
    if let Some(level) = style.para_props.outline.and_then(word_outline) {
        return Some(level);
    }
    style
        .based_on
        .as_deref()
        .and_then(|next| style_heading(doc, next, depth + 1))
}

/// Word stores heading 1 as outline level 0.
fn word_outline(level: i32) -> Option<u32> {
    u32::try_from(level)
        .ok()
        .filter(|level| *level < 9)
        .map(|level| level + 1)
}

fn named_heading(level: i32) -> Option<u32> {
    u32::try_from(level)
        .ok()
        .filter(|level| (1..=9).contains(level))
}

fn list_of(para: &Paragraph, bullets: &BTreeMap<(u32, u32), bool>) -> Option<(bool, u32)> {
    if let Some(spec) = para.props.list {
        let level = spec.level.min(8);
        return Some((spec.kind == ListKind::Bullet, level));
    }
    let num_id = para.props.num_id?;
    if num_id == 0 {
        return None;
    }
    let level = para.props.ilvl.unwrap_or(0).min(8);
    let bullet = bullets.get(&(num_id, level)).copied().unwrap_or(false);
    Some((bullet, level))
}

fn numbering_bullets(doc: &Document) -> BTreeMap<(u32, u32), bool> {
    let Some(xml) = doc.numbering_xml() else {
        return BTreeMap::new();
    };
    let Ok(root) = edgedoc_opc::xml::parse(xml.as_bytes(), doc.limits()) else {
        return BTreeMap::new();
    };
    let mut abstracts: BTreeMap<u32, BTreeMap<u32, bool>> = BTreeMap::new();
    for element in root.child_els() {
        if local_name(&element.name) != "abstractNum" {
            continue;
        }
        let Some(id) = element
            .attr("abstractNumId")
            .and_then(|value| value.parse().ok())
        else {
            continue;
        };
        abstracts.insert(id, level_formats(element));
    }
    let mut bullets = BTreeMap::new();
    for element in root.child_els() {
        if local_name(&element.name) != "num" {
            continue;
        }
        let Some(num_id) = element.attr("numId").and_then(|value| value.parse().ok()) else {
            continue;
        };
        let abstract_id = element
            .child_els()
            .find(|child| local_name(&child.name) == "abstractNumId")
            .and_then(|child| child.attr("val"))
            .and_then(|value| value.parse().ok());
        if let Some(levels) = abstract_id.and_then(|id| abstracts.get(&id)) {
            for (level, bullet) in levels {
                bullets.insert((num_id, *level), *bullet);
            }
        }
        for over in element
            .child_els()
            .filter(|child| local_name(&child.name) == "lvlOverride")
        {
            let Some(level) = over.attr("ilvl").and_then(|value| value.parse().ok()) else {
                continue;
            };
            if let Some(lvl) = over
                .child_els()
                .find(|child| local_name(&child.name) == "lvl")
            {
                bullets.insert((num_id, level), is_bullet(lvl));
            }
        }
    }
    bullets
}

fn level_formats(abstract_num: &edgedoc_opc::xml::El) -> BTreeMap<u32, bool> {
    let mut levels = BTreeMap::new();
    for level in abstract_num.child_els() {
        if local_name(&level.name) != "lvl" {
            continue;
        }
        let Some(ilvl) = level.attr("ilvl").and_then(|value| value.parse().ok()) else {
            continue;
        };
        levels.insert(ilvl, is_bullet(level));
    }
    levels
}

fn is_bullet(level: &edgedoc_opc::xml::El) -> bool {
    level.child_els().any(|child| {
        local_name(&child.name) == "numFmt" && child.attr("val").unwrap_or("") == "bullet"
    })
}

fn local_name(name: &str) -> &str {
    name.rsplit(':').next().unwrap_or(name)
}

fn field_text(field: &edgedoc_model::Field) -> String {
    let keyword = field
        .instr
        .split_whitespace()
        .next()
        .unwrap_or("")
        .trim_start_matches('\\')
        .to_ascii_uppercase();
    if keyword == "HYPERLINK" {
        if let Some(url) = hyperlink_target(&field.instr) {
            return format!(
                "[{}]({})",
                emphasize(&field.result, &CharProps::default()),
                escape_url(&url)
            );
        }
    }
    emphasize(&field.result, &CharProps::default())
}

fn hyperlink_target(instr: &str) -> Option<String> {
    let bytes = instr.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() {
        if bytes[index] == b'"' {
            let start = index + 1;
            let end = bytes[start..]
                .iter()
                .position(|byte| *byte == b'"')
                .map(|offset| start + offset)?;
            return Some(instr.get(start..end)?.to_string());
        }
        index += 1;
    }
    instr.split_whitespace().nth(1).map(str::to_string)
}

fn math_markdown(latex: &str, display: bool, cell: bool) -> String {
    if latex.is_empty() {
        return String::new();
    }
    if display && !cell {
        format!("\n\n$$\n{latex}\n$$\n\n")
    } else if display {
        format!("$${latex}$$")
    } else {
        format!("${latex}$")
    }
}

fn separate_math(out: &mut String, next: &str) {
    if out.ends_with('$') && next.starts_with('$') {
        out.push(' ');
    }
}

/// A nested table is one cell. Its column pipes are escaped, and pipes inside `$...$` stay.
fn escape_pipes_outside_math(text: &str) -> String {
    let mut out = String::new();
    let mut math = false;
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            out.push('\\');
            if let Some(next) = chars.next() {
                out.push(next);
            }
            continue;
        }
        if ch == '$' {
            out.push('$');
            if chars.peek() == Some(&'$') {
                out.push(chars.next().unwrap_or('$'));
            }
            math = !math;
            continue;
        }
        if ch == '|' && !math {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

fn note_marker(kind: &str, id: i32) -> String {
    if id < 0 {
        return String::new();
    }
    match kind {
        "footnote" => format!("[^{id}]"),
        "endnote" => format!("[^end-{id}]"),
        _ => String::new(),
    }
}

fn emphasize(text: &str, props: &CharProps) -> String {
    if text.is_empty() {
        return String::new();
    }
    // A marker that starts or ends on a space is not emphasis in CommonMark.
    let core_start = text.len() - text.trim_start_matches([' ', '\t']).len();
    let core_end = text.trim_end_matches([' ', '\t']).len();
    if core_start >= core_end {
        return escape_text(text);
    }
    let lead = &text[..core_start];
    let trail = &text[core_end..];
    let core = &text[core_start..core_end];
    let mut out = escape_text(core);
    if props.strike || props.double_strike {
        out = format!("~~{out}~~");
    }
    out = match (props.bold, props.italic) {
        (true, true) => format!("***{out}***"),
        (true, false) => format!("**{out}**"),
        (false, true) => format!("*{out}*"),
        (false, false) => out,
    };
    if props.vert_align.as_deref() == Some("superscript") {
        out = format!("<sup>{out}</sup>");
    } else if props.vert_align.as_deref() == Some("subscript") {
        out = format!("<sub>{out}</sub>");
    }
    format!("{lead}{out}{trail}")
}

fn escape_text(text: &str) -> String {
    let mut out = String::new();
    for ch in text.chars() {
        if matches!(ch, '\\' | '`' | '*' | '_' | '[' | ']' | '|') {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

fn escape_alt(text: &str) -> String {
    escape_text(&text.replace(['\n', '\r'], " "))
}

fn escape_url(url: &str) -> String {
    let mut out = String::new();
    for ch in url.chars() {
        match ch {
            ' ' => out.push_str("%20"),
            '(' => out.push_str("%28"),
            ')' => out.push_str("%29"),
            '\\' => out.push_str("%5C"),
            _ => out.push(ch),
        }
    }
    out
}

fn link_prefix(options: &MarkdownOptions) -> String {
    if let Some(prefix) = &options.link_prefix {
        return prefix.trim_matches(['/', '\\']).replace('\\', "/");
    }
    if let Some(dir) = &options.image_dir {
        if let Some(name) = dir.file_name().and_then(|name| name.to_str()) {
            let clean = sanitize_stem(name);
            if !clean.is_empty() {
                return clean;
            }
        }
    }
    "images".to_string()
}

fn join_prefix(prefix: &str, file_name: &str) -> String {
    if prefix.is_empty() {
        file_name.to_string()
    } else {
        format!("{prefix}/{file_name}")
    }
}

fn image_file_name(picture: &Picture, index: usize, used: &mut BTreeSet<String>) -> String {
    let ext = image_extension(picture);
    let stem = sanitize_stem(&picture.name);
    let stem = if stem.is_empty() {
        format!("image-{index}")
    } else {
        stem
    };
    let mut name = format!("{stem}.{ext}");
    let mut copy = 2u32;
    while !used.insert(name.clone()) {
        name = format!("{stem}-{copy}.{ext}");
        copy = copy.saturating_add(1);
    }
    name
}

fn image_extension(picture: &Picture) -> String {
    let kind = picture.content_type.to_ascii_lowercase();
    let from_type = if kind.contains("png") {
        Some("png")
    } else if kind.contains("jpeg") || kind.contains("jpg") {
        Some("jpg")
    } else if kind.contains("gif") {
        Some("gif")
    } else if kind.contains("webp") {
        Some("webp")
    } else if kind.contains("bmp") {
        Some("bmp")
    } else if kind.contains("tiff") || kind.contains("tif") {
        Some("tif")
    } else if kind.contains("svg") {
        Some("svg")
    } else if kind.contains("emf") {
        Some("emf")
    } else if kind.contains("wmf") {
        Some("wmf")
    } else {
        None
    };
    if let Some(ext) = from_type {
        return ext.to_string();
    }
    Path::new(&picture.name)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(sanitize_stem)
        .filter(|ext| !ext.is_empty())
        .unwrap_or_else(|| "bin".to_string())
}

fn sanitize_stem(name: &str) -> String {
    let file = Path::new(name)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or(name);
    let mut out = String::new();
    for ch in file.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            out.push(ch.to_ascii_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
        if out.len() >= 80 {
            break;
        }
    }
    out.trim_matches('-').to_string()
}

fn store_images(dir: &Path, images: &mut [ExportedImage]) -> Result<(), Error> {
    #[cfg(not(feature = "fs"))]
    {
        let _ = (dir, images);
        return Err(Error::new(
            "markdown.image_dir",
            "Image files need the fs feature. Build EdgeDoc with the fs feature, or leave the image directory unset and write ExportedImage::bytes yourself.",
        ));
    }
    #[cfg(feature = "fs")]
    {
        if let Ok(meta) = std::fs::symlink_metadata(dir) {
            if meta.file_type().is_symlink() {
                return Err(Error::new(
                    "markdown.image_dir",
                    "The image directory is a symbolic link. Choose a regular directory.",
                ));
            }
            if !meta.is_dir() {
                return Err(Error::new(
                    "markdown.image_dir",
                    "The image path is a file. Choose a directory.",
                ));
            }
        }
        std::fs::create_dir_all(dir).map_err(|err| {
            Error::new(
                "markdown.image_dir",
                format!("The image directory could not be created. {err}"),
            )
        })?;
        for image in images.iter_mut() {
            let path = dir.join(&image.file_name);
            if let Ok(meta) = std::fs::symlink_metadata(&path) {
                if meta.file_type().is_symlink() {
                    return Err(Error::new(
                        "markdown.image_dir",
                        format!(
                            "The image {} is a symbolic link. Choose another directory.",
                            image.file_name
                        ),
                    ));
                }
            }
            std::fs::write(&path, &image.bytes).map_err(|err| {
                Error::new(
                    "markdown.image_write",
                    format!("The image {} could not be written. {err}", image.file_name),
                )
            })?;
            image.stored_at = Some(path);
        }
        Ok(())
    }
}
