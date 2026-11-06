//! Markdown back into a Word document.
//!
//! [`Document::from_markdown`] reads the text. [`Document::open_markdown`] reads a file
//! and the images next to it. The result is an ordinary document: save it, or call
//! [`Document::to_markdown`] to see the same text again.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use edgedoc_model::{
    Block, CharProps, InlineObject, ListKind, ListSpec, Paragraph, PartKind, PartMeta, Picture,
    Style,
};

use crate::markdown_omml::omml_from_latex;
use crate::{Document, Error};

/// One image a Markdown reader can place when the file is not on disk.
///
/// `path` is the path written in the Markdown, such as `images/chart.png`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkdownImage {
    pub path: String,
    pub bytes: Vec<u8>,
    /// A content type such as `image/png`. An empty value is chosen from the path
    /// and the bytes.
    pub content_type: String,
}

impl MarkdownImage {
    pub fn new(path: impl Into<String>, bytes: Vec<u8>) -> Self {
        Self {
            path: path.into(),
            bytes,
            content_type: String::new(),
        }
    }

    /// Use this content type instead of guessing from the path.
    pub fn content_type(mut self, kind: impl Into<String>) -> Self {
        self.content_type = kind.into();
        self
    }
}

/// Where [`Document::from_markdown_with`] finds images.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MarkdownReadOptions {
    /// Images already in memory, matched by the path in the Markdown.
    pub images: Vec<MarkdownImage>,
    /// Directory that holds images named by a relative path.
    ///
    /// A path that climbs out of this directory with `..` is refused. The `fs`
    /// feature is required to read the files.
    pub image_dir: Option<PathBuf>,
}

impl MarkdownReadOptions {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add one image that the Markdown may reference.
    pub fn image(mut self, image: MarkdownImage) -> Self {
        self.images.push(image);
        self
    }

    /// Add every image.
    pub fn images(mut self, images: impl IntoIterator<Item = MarkdownImage>) -> Self {
        self.images.extend(images);
        self
    }

    /// Read relative image paths from `dir`.
    pub fn image_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.image_dir = Some(dir.into());
        self
    }
}

impl Document {
    /// Read a Markdown document from text.
    ///
    /// An image in the text needs its bytes on [`MarkdownReadOptions`], or a
    /// directory set there. [`Document::from_markdown`] has neither, so a text
    /// that names an image returns `markdown.image_missing`.
    pub fn from_markdown(markdown: &str) -> Result<Self, Error> {
        Self::from_markdown_with(markdown, &MarkdownReadOptions::new())
    }

    /// Read Markdown, using `options` for images.
    pub fn from_markdown_with(
        markdown: &str,
        options: &MarkdownReadOptions,
    ) -> Result<Self, Error> {
        if markdown.len() > 32 * 1024 * 1024 {
            return Err(Error::new(
                "markdown.limit",
                "The Markdown is larger than 32 MiB. Split it and read the parts.",
            ));
        }
        read_markdown(markdown, options)
    }

    /// Read a UTF-8 Markdown file. Images use paths relative to the file's folder.
    #[cfg(feature = "fs")]
    pub fn open_markdown(path: impl AsRef<Path>) -> Result<Self, Error> {
        Self::open_markdown_with(path, &MarkdownReadOptions::new())
    }

    /// Read a UTF-8 Markdown file, with any extra images in `options`.
    #[cfg(feature = "fs")]
    pub fn open_markdown_with(
        path: impl AsRef<Path>,
        options: &MarkdownReadOptions,
    ) -> Result<Self, Error> {
        let path = path.as_ref();
        let bytes = std::fs::read(path).map_err(|err| {
            Error::new(
                "markdown.read",
                format!("The Markdown file could not be read. {err}"),
            )
        })?;
        let text = String::from_utf8(bytes).map_err(|_| {
            Error::new(
                "markdown.utf8",
                "The Markdown file is not UTF-8. Save it as UTF-8 and read it again.",
            )
        })?;
        let mut options = options.clone();
        if options.image_dir.is_none() {
            options.image_dir = path.parent().map(Path::to_path_buf);
        }
        Self::from_markdown_with(&text, &options)
    }
}

fn read_markdown(markdown: &str, options: &MarkdownReadOptions) -> Result<Document, Error> {
    let mut text = markdown;
    if let Some(rest) = text.strip_prefix('\u{feff}') {
        text = rest;
    }
    let owned = text.replace("\r\n", "\n").replace('\r', "\n");
    let raw_lines: Vec<&str> = owned.lines().collect();
    let lines: Vec<String> = raw_lines.iter().copied().map(unquote).collect();
    let (skip, notes) = note_definitions(&lines, options)?;
    let blocks = parse_blocks(&lines, &skip, options)?;
    let mut styles = Vec::new();
    let mut seen = BTreeSet::new();
    collect_styles(&blocks, &mut styles, &mut seen);
    let mut builder = Document::builder();
    for style in styles {
        builder = builder.style(style);
    }
    for block in blocks {
        builder = builder.block(block);
    }
    let mut doc = builder.build()?;
    doc.attach_notes(notes);
    Ok(doc)
}

fn unquote(line: &str) -> String {
    let mut rest = line;
    while let Some(stripped) = rest.strip_prefix('>') {
        rest = stripped.strip_prefix(' ').unwrap_or(stripped);
    }
    rest.to_string()
}

type NotePart = (PartKind, PartMeta, Vec<Block>);

fn note_definitions(
    lines: &[String],
    options: &MarkdownReadOptions,
) -> Result<(Vec<bool>, Vec<NotePart>), Error> {
    let mut skip = vec![false; lines.len()];
    let mut notes = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let Some((id, first)) = note_heading(&lines[i]) else {
            i += 1;
            continue;
        };
        skip[i] = true;
        let mut body = vec![first];
        i += 1;
        while i < lines.len() && (lines[i].starts_with("    ") || lines[i].starts_with('\t')) {
            skip[i] = true;
            body.push(lines[i].trim_start().to_string());
            i += 1;
        }
        let (kind, number) = note_id(&id);
        let text = body.join("\n");
        let blocks = if text.trim().is_empty() {
            vec![Block::Paragraph(Paragraph::new())]
        } else {
            parse_blocks(&body, &vec![false; body.len()], options)?
        };
        notes.push((
            kind,
            PartMeta {
                id: Some(number),
                note_type: Some("normal".to_string()),
                ..PartMeta::default()
            },
            blocks,
        ));
    }
    Ok((skip, notes))
}

fn note_heading(line: &str) -> Option<(String, String)> {
    let rest = line.strip_prefix("[^")?;
    let (id, rest) = rest.split_once("]:")?;
    if note_id(id).1 < 0 && !id.starts_with("end-") {
        return None;
    }
    let (_, number) = note_id(id);
    if number < 0 {
        return None;
    }
    Some((id.to_string(), rest.trim_start().to_string()))
}

fn note_id(id: &str) -> (PartKind, i32) {
    if let Some(number) = id.strip_prefix("end-") {
        (PartKind::Endnote, number.parse().unwrap_or(-1))
    } else {
        (PartKind::Footnote, id.parse().unwrap_or(-1))
    }
}

fn parse_blocks(
    lines: &[String],
    skip: &[bool],
    options: &MarkdownReadOptions,
) -> Result<Vec<Block>, Error> {
    let mut blocks = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if skip.get(i).copied().unwrap_or(false) || lines[i].trim().is_empty() {
            i += 1;
            continue;
        }
        if fence_of(&lines[i]).is_some() {
            let (block, next) = code_fence(lines, i);
            blocks.push(block);
            i = next;
            continue;
        }
        if let Some(block) = display_math(&lines[i], lines, &mut i)? {
            blocks.push(block);
            continue;
        }
        if is_break(&lines[i]) {
            blocks.push(Block::Paragraph(Paragraph::new().page_break()));
            i += 1;
            continue;
        }
        if let Some((level, text)) = atx_heading(&lines[i]) {
            blocks.push(heading_block(level, text, options)?);
            i += 1;
            continue;
        }
        if is_table(lines, i) {
            let (table, next) = table_block(lines, i, options)?;
            blocks.push(table);
            i = next;
            continue;
        }
        if list_marker(&lines[i]).is_some() {
            let (items, next) = list_block(lines, i, skip, options)?;
            blocks.extend(items);
            i = next;
            continue;
        }
        let (para, next) = paragraph_block(lines, i, skip, options)?;
        blocks.push(para);
        i = next;
    }
    Ok(blocks)
}

fn fence_of(line: &str) -> Option<&str> {
    if line.starts_with("```") {
        Some("```")
    } else if line.starts_with("~~~") {
        Some("~~~")
    } else {
        None
    }
}

fn code_fence(lines: &[String], start: usize) -> (Block, usize) {
    let fence = fence_of(&lines[start]).unwrap_or("```");
    let mut i = start + 1;
    let mut body = Vec::new();
    while i < lines.len() && !lines[i].starts_with(fence) {
        body.push(lines[i].as_str());
        i += 1;
    }
    if i < lines.len() {
        i += 1;
    }
    let mut para = Paragraph::new();
    para.push_text(&body.join("\n"), &CharProps::default());
    (Block::Paragraph(para), i)
}

fn display_math(line: &str, lines: &[String], i: &mut usize) -> Result<Option<Block>, Error> {
    if line == "$$" {
        *i += 1;
        let mut body = Vec::new();
        while *i < lines.len() && lines[*i] != "$$" {
            body.push(lines[*i].as_str());
            *i += 1;
        }
        if *i < lines.len() {
            *i += 1;
        }
        return Ok(Some(math_block(&body.join("\n"), true)));
    }
    if let Some(inner) = line
        .strip_prefix("$$")
        .and_then(|rest| rest.strip_suffix("$$"))
    {
        if !inner.is_empty() || line == "$$$$" {
            *i += 1;
            return Ok(Some(math_block(inner, true)));
        }
    }
    Ok(None)
}

fn math_block(latex: &str, display: bool) -> Block {
    let mut para = Paragraph::new();
    push_math(&mut para, latex, display, &CharProps::default());
    Block::Paragraph(para)
}

fn push_math(para: &mut Paragraph, latex: &str, display: bool, props: &CharProps) {
    let latex = latex.trim().to_string();
    if latex.is_empty() {
        return;
    }
    para.push_object(
        InlineObject::Math {
            raw: omml_from_latex(&latex, display),
            latex,
            display,
        },
        props,
    );
}

fn is_break(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.len() < 3 {
        return false;
    }
    let kind = trimmed.chars().find(|ch| *ch != ' ');
    matches!(kind, Some('-' | '*' | '_'))
        && trimmed.chars().all(|ch| ch == ' ' || kind == Some(ch))
        && trimmed.chars().filter(|ch| kind == Some(*ch)).count() >= 3
}

fn atx_heading(line: &str) -> Option<(u32, &str)> {
    let marks = line.chars().take_while(|ch| *ch == '#').count();
    if !(1..=9).contains(&marks) {
        return None;
    }
    let rest = &line[marks..];
    if !rest.starts_with(' ') {
        return None;
    }
    Some((marks as u32, rest.trim().trim_end_matches('#').trim()))
}

fn heading_block(level: u32, text: &str, options: &MarkdownReadOptions) -> Result<Block, Error> {
    let spans = parse_inline(text, options)?;
    Ok(Block::Paragraph(spans_to_para(
        spans,
        Some(format!("Heading{level}")),
        None,
    )))
}

fn is_table(lines: &[String], index: usize) -> bool {
    index + 1 < lines.len() && looks_like_row(&lines[index]) && is_separator(&lines[index + 1])
}

fn looks_like_row(line: &str) -> bool {
    line.contains('|')
}

fn is_separator(line: &str) -> bool {
    let cells = split_cells(line);
    !cells.is_empty()
        && cells.iter().all(|cell| {
            let core = cell.trim().trim_matches(':');
            !core.is_empty() && core.chars().all(|ch| ch == '-')
        })
}

fn table_block(
    lines: &[String],
    start: usize,
    options: &MarkdownReadOptions,
) -> Result<(Block, usize), Error> {
    let mut rows = Vec::new();
    let mut i = start;
    let mut header = true;
    while i < lines.len() && looks_like_row(&lines[i]) {
        if i == start + 1 && is_separator(&lines[i]) {
            i += 1;
            continue;
        }
        if i > start && !lines[i].contains('|') {
            break;
        }
        let mut cells = Vec::new();
        for cell in split_cells(&lines[i]) {
            cells.push(cell_from(&cell, options)?);
        }
        let mut row = edgedoc_model::Row::cells(cells);
        if header {
            row = row.header();
            header = false;
        }
        rows.push(row);
        i += 1;
        if i < lines.len() && is_separator(&lines[i]) {
            i += 1;
        }
    }
    let mut table = edgedoc_model::Table::new();
    table.rows = rows;
    Ok((Block::Table(table), i))
}

fn cell_from(text: &str, options: &MarkdownReadOptions) -> Result<edgedoc_model::Cell, Error> {
    let mut blocks = Vec::new();
    for part in split_breaks(text) {
        let spans = parse_inline(part.trim(), options)?;
        let para = spans_to_para(spans, None, None);
        if para.text.is_empty() && para.objects.is_empty() {
            continue;
        }
        blocks.push(Arc::new(Block::Paragraph(para)));
    }
    if blocks.is_empty() {
        blocks.push(Arc::new(Block::Paragraph(Paragraph::new())));
    }
    Ok(edgedoc_model::Cell::paragraph("").blocks(blocks))
}

fn split_breaks(text: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut cur = String::new();
    let mut rest = text;
    while !rest.is_empty() {
        let lower = rest.to_ascii_lowercase();
        if lower.starts_with("<br>") || lower.starts_with("<br/>") || lower.starts_with("<br />") {
            parts.push(std::mem::take(&mut cur));
            let skip = if lower.starts_with("<br />") {
                6
            } else if lower.starts_with("<br/>") {
                5
            } else {
                4
            };
            rest = &rest[skip..];
            continue;
        }
        let ch = rest.chars().next().unwrap_or(' ');
        cur.push(ch);
        rest = &rest[ch.len_utf8()..];
    }
    parts.push(cur);
    parts
}

fn split_cells(line: &str) -> Vec<String> {
    let mut line = line.trim();
    if let Some(rest) = line.strip_prefix('|') {
        line = rest;
    }
    if let Some(rest) = line.strip_suffix('|') {
        line = rest;
    }
    let mut cells = Vec::new();
    let mut cur = String::new();
    let mut chars = line.chars().peekable();
    let mut math = false;
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            if let Some(next) = chars.next() {
                if next == '|' && !math {
                    cur.push('|');
                } else {
                    cur.push('\\');
                    cur.push(next);
                }
            }
            continue;
        }
        if ch == '$' {
            cur.push('$');
            if chars.peek() == Some(&'$') {
                cur.push(chars.next().unwrap_or('$'));
            }
            math = !math;
            continue;
        }
        if ch == '|' && !math {
            cells.push(cur.trim().to_string());
            cur.clear();
            continue;
        }
        cur.push(ch);
    }
    cells.push(cur.trim().to_string());
    cells
}

fn list_block(
    lines: &[String],
    start: usize,
    skip: &[bool],
    options: &MarkdownReadOptions,
) -> Result<(Vec<Block>, usize), Error> {
    let mut items = Vec::new();
    let mut i = start;
    while i < lines.len() {
        if skip.get(i).copied().unwrap_or(false) {
            break;
        }
        if lines[i].trim().is_empty() {
            let mut look = i + 1;
            while look < lines.len() && lines[look].trim().is_empty() {
                look += 1;
            }
            if look < lines.len() && list_marker(&lines[look]).is_some() {
                i = look;
                continue;
            }
            break;
        }
        let Some((level, bullet, text)) = list_marker(&lines[i]) else {
            break;
        };
        let mut text = text.to_string();
        i += 1;
        while i < lines.len()
            && list_marker(&lines[i]).is_none()
            && !lines[i].trim().is_empty()
            && !is_block_boundary(&lines[i])
        {
            if lines[i].starts_with("  ") {
                if text.ends_with("  ") {
                    text = text.trim_end().to_string();
                    text.push('\n');
                } else if !text.is_empty() {
                    text.push(' ');
                }
                text.push_str(lines[i].trim());
                i += 1;
            } else {
                break;
            }
        }
        let spans = parse_inline(&text, options)?;
        let kind = if bullet {
            ListKind::Bullet
        } else {
            ListKind::Decimal
        };
        items.push(Block::Paragraph(spans_to_para(
            spans,
            None,
            Some((kind, level)),
        )));
    }
    Ok((items, i))
}

fn list_marker(line: &str) -> Option<(u32, bool, &str)> {
    let indent = line.chars().take_while(|ch| *ch == ' ').count();
    let rest = &line[indent..];
    let (bullet, marker) =
        if rest.starts_with("- ") || rest.starts_with("* ") || rest.starts_with("+ ") {
            (true, 2)
        } else {
            let digits = rest.chars().take_while(|ch| ch.is_ascii_digit()).count();
            if digits == 0 || digits > 9 {
                return None;
            }
            let after = &rest[digits..];
            if after.starts_with(". ") || after.starts_with(") ") {
                (false, digits + 2)
            } else {
                return None;
            }
        };
    Some(((indent / 2).min(8) as u32, bullet, &rest[marker..]))
}

fn is_block_boundary(line: &str) -> bool {
    atx_heading(line).is_some()
        || is_break(line)
        || line == "$$"
        || line.starts_with("$$")
        || fence_of(line).is_some()
}

fn paragraph_block(
    lines: &[String],
    start: usize,
    skip: &[bool],
    options: &MarkdownReadOptions,
) -> Result<(Block, usize), Error> {
    let mut taken: Vec<&str> = Vec::new();
    let mut i = start;
    while i < lines.len() {
        if skip.get(i).copied().unwrap_or(false) || lines[i].trim().is_empty() {
            break;
        }
        if i == start + 1 && setext_level(&lines[i]).is_some() && taken.len() == 1 {
            let level = setext_level(&lines[i]).unwrap_or(1);
            let text = taken[0].trim_end();
            return Ok((heading_block(level, text, options)?, i + 1));
        }
        if i > start && is_block_boundary(&lines[i]) {
            break;
        }
        if i > start && list_marker(&lines[i]).is_some() {
            break;
        }
        taken.push(lines[i].as_str());
        i += 1;
    }
    let text = join_lines(&taken);
    let spans = parse_inline(&text, options)?;
    Ok((Block::Paragraph(spans_to_para(spans, None, None)), i))
}

fn setext_level(line: &str) -> Option<u32> {
    let trimmed = line.trim();
    if trimmed.len() < 3 {
        return None;
    }
    if trimmed.chars().all(|ch| ch == '=') {
        Some(1)
    } else if trimmed.chars().all(|ch| ch == '-') {
        Some(2)
    } else {
        None
    }
}

fn join_lines(lines: &[&str]) -> String {
    let mut out = String::new();
    for (index, line) in lines.iter().enumerate() {
        if index > 0 {
            let previous = lines[index - 1].ends_with("  ");
            if previous {
                out.push('\n');
            } else {
                out.push(' ');
            }
        }
        out.push_str(line.trim_end());
    }
    out
}

struct Span {
    props: CharProps,
    kind: SpanKind,
}

enum SpanKind {
    Text(String),
    Object(Box<InlineObject>),
}

impl Span {
    fn text(text: impl Into<String>) -> Self {
        Self {
            props: CharProps::default(),
            kind: SpanKind::Text(text.into()),
        }
    }

    fn object(object: InlineObject) -> Self {
        Self {
            props: CharProps::default(),
            kind: SpanKind::Object(Box::new(object)),
        }
    }
}

fn spans_to_para(
    spans: Vec<Span>,
    style: Option<String>,
    list: Option<(ListKind, u32)>,
) -> Paragraph {
    let mut para = Paragraph::new();
    if let Some(style) = style {
        para.props.style = Some(style);
    }
    if let Some((kind, level)) = list {
        para.props.list = Some(ListSpec { kind, level });
    }
    for span in spans {
        match span.kind {
            SpanKind::Text(text) => {
                if !text.is_empty() {
                    para.push_text(&text, &span.props);
                }
            }
            SpanKind::Object(object) => para.push_object(*object, &span.props),
        }
    }
    para
}

fn parse_inline(src: &str, options: &MarkdownReadOptions) -> Result<Vec<Span>, Error> {
    let mut parser = Inline { src, i: 0, options };
    parser.parse_until(None)
}

struct Inline<'a> {
    src: &'a str,
    i: usize,
    options: &'a MarkdownReadOptions,
}

impl<'a> Inline<'a> {
    fn rest(&self) -> &'a str {
        &self.src[self.i..]
    }

    fn parse_until(&mut self, closer: Option<&str>) -> Result<Vec<Span>, Error> {
        let mut out = Vec::new();
        while self.i < self.src.len() {
            if closer.is_some_and(|marker| self.rest().starts_with(marker)) {
                break;
            }
            if let Some(spans) = self.special()? {
                out.extend(spans);
            } else {
                out.push(self.plain(closer));
            }
        }
        Ok(out)
    }

    fn special(&mut self) -> Result<Option<Vec<Span>>, Error> {
        let rest = self.rest();
        if rest.starts_with('\\') {
            return Ok(Some(vec![self.escaped()]));
        }
        if rest.starts_with("$$") {
            return Ok(self.math(true));
        }
        if rest.starts_with('$') {
            return Ok(self.math(false));
        }
        if rest.starts_with("![") {
            return self.image();
        }
        if rest.starts_with("[^") {
            if let Some(span) = self.note_ref() {
                return Ok(Some(vec![span]));
            }
        }
        if rest.starts_with('[') {
            return self.link();
        }
        if let Some(tag) = html_tag(rest) {
            return Ok(self.html(tag));
        }
        if let Some(spans) = self.marker("~~", false, false, true)? {
            return Ok(Some(spans));
        }
        if let Some(spans) = self.marker("***", true, true, false)? {
            return Ok(Some(spans));
        }
        if let Some(spans) = self.marker("**", true, false, false)? {
            return Ok(Some(spans));
        }
        if let Some(spans) = self.marker("*", false, true, false)? {
            return Ok(Some(spans));
        }
        if let Some(spans) = self.marker("__", true, false, false)? {
            return Ok(Some(spans));
        }
        if let Some(spans) = self.marker("_", false, true, false)? {
            return Ok(Some(spans));
        }
        Ok(None)
    }

    fn escaped(&mut self) -> Span {
        self.i += 1;
        let ch = self.rest().chars().next();
        if let Some(ch) = ch {
            self.i += ch.len_utf8();
            Span::text(ch.to_string())
        } else {
            Span::text("\\")
        }
    }

    fn math(&mut self, display: bool) -> Option<Vec<Span>> {
        let marker = if display { "$$" } else { "$" };
        let after = &self.src[self.i + marker.len()..];
        let end = after.find(marker)?;
        if !display && after[..end].contains('\n') {
            return None;
        }
        let latex = after[..end].to_string();
        self.i += marker.len() + end + marker.len();
        let mut span = Span::object(InlineObject::Math {
            raw: omml_from_latex(latex.trim(), display),
            latex: latex.trim().to_string(),
            display,
        });
        if latex.trim().is_empty() {
            span = Span::text("");
        }
        Some(vec![span])
    }

    fn image(&mut self) -> Result<Option<Vec<Span>>, Error> {
        let Some((alt, path, total)) = bracket_target(self.rest(), true) else {
            return Ok(None);
        };
        self.i += total;
        let picture = load_image(&path, &alt, self.options)?;
        Ok(Some(vec![Span::object(InlineObject::Picture(picture))]))
    }

    fn link(&mut self) -> Result<Option<Vec<Span>>, Error> {
        let Some((label, url, total)) = bracket_target(self.rest(), false) else {
            return Ok(None);
        };
        let start = self.i;
        self.i += 1;
        let mut inner = self.parse_until(Some("]("))?;
        if !self.rest().starts_with("](") {
            self.i = start;
            return Ok(None);
        }
        self.i = start + total;
        let url = percent_decode(&url);
        for span in &mut inner {
            span.props.link = Some(url.clone());
        }
        if inner.is_empty() {
            inner.push(Span::text(""));
        }
        let _ = label;
        Ok(Some(inner))
    }

    fn note_ref(&mut self) -> Option<Span> {
        let rest = self.rest();
        let end = rest.find(']')?;
        let id = &rest[2..end];
        let (kind, number) = note_id(id);
        if number < 0 {
            return None;
        }
        if !(id.chars().all(|ch| ch.is_ascii_digit())
            || id
                .strip_prefix("end-")
                .is_some_and(|n| n.chars().all(|ch| ch.is_ascii_digit())))
        {
            return None;
        }
        self.i += end + 1;
        Some(Span::object(InlineObject::Note {
            kind: match kind {
                PartKind::Endnote => "endnote".to_string(),
                _ => "footnote".to_string(),
            },
            id: number,
        }))
    }

    fn html(&mut self, tag: HtmlTag) -> Option<Vec<Span>> {
        let rest = self.rest();
        let lower = rest.to_ascii_lowercase();
        match tag {
            HtmlTag::Break => {
                let skip = if lower.starts_with("<br />") {
                    6
                } else if lower.starts_with("<br/>") {
                    5
                } else {
                    4
                };
                self.i += skip;
                Some(vec![Span::text("\n")])
            }
            HtmlTag::Sup | HtmlTag::Sub => {
                let open = if tag == HtmlTag::Sup {
                    "<sup>"
                } else {
                    "<sub>"
                };
                let close = if tag == HtmlTag::Sup {
                    "</sup>"
                } else {
                    "</sub>"
                };
                if !lower.starts_with(open) {
                    return None;
                }
                let after = &rest[open.len()..];
                let close_at = after.to_ascii_lowercase().find(close)?;
                let inner = &after[..close_at];
                self.i += open.len() + close_at + close.len();
                let mut spans = parse_inline(inner, self.options).ok()?;
                let align = if tag == HtmlTag::Sup {
                    "superscript"
                } else {
                    "subscript"
                };
                for span in &mut spans {
                    span.props.vert_align = Some(align.to_string());
                }
                Some(spans)
            }
        }
    }

    fn marker(
        &mut self,
        marker: &str,
        bold: bool,
        italic: bool,
        strike: bool,
    ) -> Result<Option<Vec<Span>>, Error> {
        if !self.rest().starts_with(marker) {
            return Ok(None);
        }
        let after = &self.src[self.i + marker.len()..];
        if !after.contains(marker) {
            return Ok(None);
        }
        let start = self.i;
        self.i += marker.len();
        let inner = self.parse_until(Some(marker))?;
        if !self.rest().starts_with(marker) {
            self.i = start;
            return Ok(None);
        }
        self.i += marker.len();
        let mut inner = inner;
        for span in &mut inner {
            if bold {
                span.props.bold = true;
            }
            if italic {
                span.props.italic = true;
            }
            if strike {
                span.props.strike = true;
            }
        }
        Ok(Some(inner))
    }

    fn plain(&mut self, closer: Option<&str>) -> Span {
        let start = self.i;
        let rest = self.rest();
        let mut len = 0;
        for ch in rest.chars() {
            if matches!(ch, '\\' | '$' | '!' | '[' | '<' | '*' | '_' | '~') {
                break;
            }
            if closer.is_some_and(|marker| rest[len..].starts_with(marker)) {
                break;
            }
            len += ch.len_utf8();
        }
        if len == 0 {
            let ch = rest.chars().next().unwrap_or(' ');
            self.i += ch.len_utf8();
            return Span::text(ch.to_string());
        }
        self.i += len;
        Span::text(&self.src[start..self.i])
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum HtmlTag {
    Break,
    Sup,
    Sub,
}

fn html_tag(rest: &str) -> Option<HtmlTag> {
    let lower = rest.to_ascii_lowercase();
    if lower.starts_with("<br>") || lower.starts_with("<br/>") || lower.starts_with("<br />") {
        Some(HtmlTag::Break)
    } else if lower.starts_with("<sup>") {
        Some(HtmlTag::Sup)
    } else if lower.starts_with("<sub>") {
        Some(HtmlTag::Sub)
    } else {
        None
    }
}

fn bracket_target(src: &str, image: bool) -> Option<(String, String, usize)> {
    let body = if image { src.strip_prefix('!')? } else { src };
    if !body.starts_with('[') {
        return None;
    }
    let mut i = 1;
    let bytes = body.as_bytes();
    while i < bytes.len() && bytes[i] != b']' {
        i += 1;
    }
    if i + 1 >= bytes.len() || bytes.get(i + 1) != Some(&b'(') {
        return None;
    }
    let alt = body[1..i].to_string();
    let mut j = i + 2;
    while j < bytes.len() && bytes[j] != b')' {
        j += 1;
    }
    if j >= bytes.len() {
        return None;
    }
    let path = body[i + 2..j].to_string();
    let total = if image { j + 2 } else { j + 1 };
    Some((unescape_plain(&alt), path, total))
}

fn unescape_plain(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            if let Some(next) = chars.next() {
                out.push(next);
            }
        } else {
            out.push(ch);
        }
    }
    out
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Some(value) = std::str::from_utf8(&bytes[i + 1..i + 3])
                .ok()
                .and_then(|hex| u8::from_str_radix(hex, 16).ok())
            {
                out.push(value);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| text.to_string())
}

fn load_image(path: &str, alt: &str, options: &MarkdownReadOptions) -> Result<Picture, Error> {
    if let Some(image) = find_image(path, &options.images) {
        return Ok(picture_from(
            path,
            image.bytes.clone(),
            alt,
            image.content_type.as_str(),
        ));
    }
    if let Some(dir) = &options.image_dir {
        let bytes = read_image_file(dir, path)?;
        return Ok(picture_from(path, bytes, alt, ""));
    }
    Err(Error::new(
        "markdown.image_missing",
        format!("The image {path} has no bytes. Pass it in MarkdownReadOptions or set an image directory."),
    ))
}

fn find_image<'a>(path: &str, images: &'a [MarkdownImage]) -> Option<&'a MarkdownImage> {
    let wanted = path.replace('\\', "/");
    if let Some(found) = images
        .iter()
        .find(|image| image.path.replace('\\', "/") == wanted)
    {
        return Some(found);
    }
    let file = wanted.rsplit('/').next().unwrap_or(wanted.as_str());
    let mut matches = images.iter().filter(|image| {
        image
            .path
            .rsplit(['/', '\\'])
            .next()
            .is_some_and(|name| name == file)
    });
    let first = matches.next()?;
    if matches.next().is_some() {
        None
    } else {
        Some(first)
    }
}

fn picture_from(path: &str, bytes: Vec<u8>, alt: &str, content_type: &str) -> Picture {
    let kind = if content_type.is_empty() {
        sniff_type(path, &bytes)
    } else {
        content_type.to_string()
    };
    let name = path
        .rsplit(['/', '\\'])
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or("image.bin");
    Picture::new(name, bytes, &kind).alt(alt)
}

fn sniff_type(path: &str, bytes: &[u8]) -> String {
    if bytes.starts_with(b"\x89PNG") {
        return "image/png".to_string();
    }
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return "image/jpeg".to_string();
    }
    if bytes.starts_with(b"GIF8") {
        return "image/gif".to_string();
    }
    match path
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "tif" | "tiff" => "image/tiff",
        "svg" => "image/svg+xml",
        "emf" => "image/x-emf",
        "wmf" => "image/x-wmf",
        _ => "application/octet-stream",
    }
    .to_string()
}

fn read_image_file(dir: &Path, path: &str) -> Result<Vec<u8>, Error> {
    if path.starts_with("http://") || path.starts_with("https://") {
        return Err(Error::new(
            "markdown.image_missing",
            format!("The image {path} is a URL. Pass its bytes in MarkdownReadOptions."),
        ));
    }
    let relative = safe_relative(path)?;
    #[cfg(not(feature = "fs"))]
    {
        let _ = (dir, relative);
        Err(Error::new(
            "markdown.image_dir",
            "Reading image files needs the fs feature. Pass the image bytes in MarkdownReadOptions.",
        ))
    }
    #[cfg(feature = "fs")]
    {
        let full = dir.join(relative);
        if let Ok(meta) = std::fs::symlink_metadata(&full) {
            if meta.file_type().is_symlink() {
                return Err(Error::new(
                    "markdown.image_dir",
                    format!("The image {path} is a symbolic link. Choose a regular file."),
                ));
            }
        }
        std::fs::read(&full).map_err(|_| {
            Error::new(
                "markdown.image_missing",
                format!("The image {path} is not in the image directory. Add the file or pass its bytes."),
            )
        })
    }
}

fn safe_relative(path: &str) -> Result<PathBuf, Error> {
    let path = path.replace('\\', "/");
    if path.starts_with('/') {
        return Err(Error::new(
            "markdown.image_dir",
            format!("The image {path} is an absolute path. Use a path inside the image directory."),
        ));
    }
    let mut out = PathBuf::new();
    for part in path.split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." {
            return Err(Error::new(
                "markdown.image_dir",
                format!("The image {path} leaves the image directory. Use a path inside it."),
            ));
        }
        out.push(part);
    }
    if out.as_os_str().is_empty() {
        return Err(Error::new(
            "markdown.image_missing",
            "The image path is empty. Name the file in the Markdown link.",
        ));
    }
    Ok(out)
}

fn collect_styles(blocks: &[Block], styles: &mut Vec<Style>, seen: &mut BTreeSet<String>) {
    for block in blocks {
        if let Block::Paragraph(para) = block {
            if let Some(id) = para.props.style.clone() {
                if seen.insert(id.clone()) {
                    let name = id
                        .strip_prefix("Heading")
                        .map(|level| format!("heading {level}"))
                        .unwrap_or_else(|| id.clone());
                    styles.push(Style::paragraph(&id).named(&name));
                }
            }
        }
        if let Block::Table(table) = block {
            for row in &table.rows {
                for cell in &row.cells {
                    let nested: Vec<Block> = cell
                        .blocks
                        .iter()
                        .map(|block| block.as_ref().clone())
                        .collect();
                    collect_styles(&nested, styles, seen);
                }
            }
        }
    }
}
