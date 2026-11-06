//! EdgeDoc reads, creates, edits, merges, saves, and exports Word documents.
//!
//! Adapters call this crate. They do not call the lower crates.

mod error;
mod markdown;
mod sheet;

pub use error::Error;
pub use markdown::{ExportedImage, MarkdownDocument, MarkdownOptions};
pub use sheet::{records_from_xlsx, xlsx_with_cached_formula as sheet_fixture};

pub use edgedoc_limit::Budget;
pub use edgedoc_merge::{
    CsvOptions, Join, MergeKind, MissingField, Op, Record, Records, Rule, SortKey, ENVELOPES,
};
pub use edgedoc_model::BlockPath;
pub use edgedoc_model::ContentItem;
pub use edgedoc_model::Field;
pub use edgedoc_model::FormField;
pub use edgedoc_model::FormKind;
pub use edgedoc_model::InlineObject;
pub use edgedoc_model::Row;
pub use edgedoc_model::SectionBreak;
pub use edgedoc_model::{
    Anchor, Block, Borders, Cell, CellMargins, CharProps, ContentControl, CoreProps, DocGrid,
    DropKind, EditOptions, EditReport, FootnotePr, FrameProps, HeaderRefs, LineBorder,
    LineNumbering, ListKind, ListSpec, ParaProps, Paragraph, PartKind, PartMeta, Picture,
    Retention, RunBuilder, SectionProps, SectionStart, StoryRef, Style, StyleKind, TabStop, Table,
    TablePosition,
};
pub use edgedoc_unit::{Cm, Emu, Inch, Mm, Pt, Twips};

use std::sync::Arc;

#[cfg(feature = "fs")]
use std::path::Path;

use edgedoc_merge::{finish, join_documents, preview as merge_preview, MapFn, MergeRequest};
use edgedoc_model::{DocumentEdit, ModelDocument};
use edgedoc_opc::{is_encrypted_package, write_zip};
use edgedoc_wml::{load, write_document};

/// Page size and margins for a new document.
#[derive(Debug, Clone, PartialEq)]
pub struct Page {
    props: SectionProps,
}

impl Page {
    pub fn a4() -> Self {
        Self {
            props: SectionProps::default(),
        }
    }

    pub fn letter() -> Self {
        let (w, h) = edgedoc_unit::letter();
        let props = SectionProps {
            page_w: w,
            page_h: h,
            ..SectionProps::default()
        };
        Self { props }
    }

    pub fn margins<T: edgedoc_unit::IntoTwips>(mut self, all: T) -> Self {
        self.props = self.props.clone().margins(all);
        self
    }

    pub fn landscape(mut self) -> Self {
        self.props = self.props.landscape();
        self
    }

    pub fn portrait(mut self) -> Self {
        if self.props.landscape {
            self.props = self.props.landscape();
        }
        self
    }

    pub fn columns(mut self, count: u32) -> Self {
        self.props = self.props.columns(count);
        self
    }

    pub fn column_space<T: edgedoc_unit::IntoTwips>(mut self, space: T) -> Self {
        self.props.column_space = Some(space.twips());
        self
    }

    pub fn title_page(mut self) -> Self {
        self.props = self.props.title_page();
        self
    }

    pub fn header<T: edgedoc_unit::IntoTwips>(mut self, distance: T) -> Self {
        self.props = self.props.header(distance);
        self
    }

    pub fn footer<T: edgedoc_unit::IntoTwips>(mut self, distance: T) -> Self {
        self.props = self.props.footer(distance);
        self
    }

    pub fn gutter<T: edgedoc_unit::IntoTwips>(mut self, size: T) -> Self {
        self.props = self.props.gutter(size);
        self
    }

    pub fn page_numbers(mut self, start: u32, format: &str) -> Self {
        self.props.page_start = Some(start);
        self.props.page_fmt = Some(format.to_string());
        self
    }

    pub fn start(self, start: SectionStart) -> Self {
        let mut props = self.props;
        props.start = start;
        Self { props }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flavor {
    Docx,
    Docm,
    Dotx,
    Dotm,
}

impl Flavor {
    fn as_str(self) -> &'static str {
        match self {
            Flavor::Docx => "docx",
            Flavor::Docm => "docm",
            Flavor::Dotx => "dotx",
            Flavor::Dotm => "dotm",
        }
    }

    fn from_content_type(kind: &str) -> Self {
        if kind.contains("macroEnabled.main") {
            Flavor::Docm
        } else if kind.contains("macroEnabledTemplate") {
            Flavor::Dotm
        } else if kind.contains("template.main") {
            Flavor::Dotx
        } else {
            Flavor::Docx
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnknownPart {
    Retain,
    Drop,
}

#[derive(Debug, Clone)]
pub struct ReadOptions<'a> {
    pub password: Option<&'a str>,
    pub budget: Budget,
    pub unknown: UnknownPart,
}

impl Default for ReadOptions<'static> {
    fn default() -> Self {
        Self {
            password: None,
            budget: Budget::default(),
            unknown: UnknownPart::Retain,
        }
    }
}

impl<'a> ReadOptions<'a> {
    pub fn new() -> Self {
        Self {
            password: None,
            budget: Budget::default(),
            unknown: UnknownPart::Retain,
        }
    }
}

#[derive(Debug, Clone)]
pub struct WriteOptions {
    pub flavor: Flavor,
    pub budget: Budget,
    pub pretty: bool,
}

impl Default for WriteOptions {
    fn default() -> Self {
        Self {
            flavor: Flavor::Docx,
            budget: Budget::default(),
            pretty: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordRange {
    pub from: usize,
    pub to: Option<usize>,
}

impl RecordRange {
    pub fn all() -> Self {
        Self { from: 0, to: None }
    }

    pub fn from_to(from: usize, to: usize) -> Self {
        Self { from, to: Some(to) }
    }
}

/// A Word document. Stories are private. Change them through [`Document::edit`].
#[derive(Debug, Clone)]
pub struct Document {
    inner: ModelDocument,
    original: Option<Vec<u8>>,
    kept: Vec<(String, Vec<u8>, Option<String>)>,
    dirty: bool,
    flavor: Flavor,
    budget: Budget,
    styles_part_present: bool,
}

impl Document {
    pub fn builder() -> DocumentBuilder {
        DocumentBuilder::default()
    }

    pub fn read_bytes(bytes: &[u8]) -> Result<Self, Error> {
        Self::read_bytes_with(bytes, &ReadOptions::new())
    }

    pub fn read_bytes_with(bytes: &[u8], options: &ReadOptions<'_>) -> Result<Self, Error> {
        if is_encrypted_package(bytes) {
            return encrypted(options);
        }
        let (inner, package) = load(bytes, &options.budget)?;
        let kept = if options.unknown == UnknownPart::Drop {
            Vec::new()
        } else {
            package
                .parts
                .values()
                .map(|part| {
                    (
                        part.name.clone(),
                        part.bytes.clone(),
                        part.content_type.clone(),
                    )
                })
                .collect()
        };
        let flavor = package
            .part("word/document.xml")
            .and_then(|part| part.content_type.as_deref())
            .map(Flavor::from_content_type)
            .unwrap_or(Flavor::Docx);
        let styles_part_present = package.parts.values().any(|p| {
            p.content_type
                .as_deref()
                .is_some_and(|t| t.ends_with("wordprocessingml.styles+xml"))
        });
        Ok(Self {
            inner,
            styles_part_present,
            original: Some(bytes.to_vec()),
            kept,
            dirty: false,
            flavor,
            budget: options.budget.clone(),
        })
    }

    #[cfg(feature = "fs")]
    pub fn open(path: impl AsRef<Path>) -> Result<Self, Error> {
        let bytes = std::fs::read(path.as_ref())?;
        Self::read_bytes(&bytes)
    }

    pub fn write_bytes(&self) -> Result<Vec<u8>, Error> {
        self.write_bytes_with(&WriteOptions {
            flavor: self.flavor,
            budget: self.budget.clone(),
            pretty: false,
        })
    }

    pub fn write_bytes_with(&self, options: &WriteOptions) -> Result<Vec<u8>, Error> {
        // An unedited package returns its bytes, unless the caller asks for another
        // flavour. A flavour change rewrites the package.
        if !self.dirty && options.flavor == self.flavor {
            if let Some(bytes) = &self.original {
                return Ok(bytes.clone());
            }
        }
        let kept = if self.kept.is_empty() {
            Vec::new()
        } else {
            self.kept.clone()
        };
        // A rewritten package gains a styles part. Make the no-styles import defaults explicit
        // so the next reader does not switch from that fallback to an empty-styles fallback.
        let mut normalized;
        let model = if !self.styles_part_present && self.inner.styles.doc_defaults_xml.is_none() {
            normalized = self.inner.clone();
            normalized.styles.doc_defaults_xml = Some(concat!(
                "<w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii=\"Calibri\"/>",
                "<w:sz w:val=\"24\"/><w:kern w:val=\"2\"/></w:rPr></w:rPrDefault>",
                "<w:pPrDefault><w:pPr><w:spacing w:after=\"160\" w:line=\"276\" w:lineRule=\"auto\"/>",
                "</w:pPr></w:pPrDefault></w:docDefaults>"
            ).into());
            &normalized
        } else {
            &self.inner
        };
        let written = write_document(model, options.flavor.as_str(), &kept)?;
        Ok(write_zip(&written.files)?)
    }

    #[cfg(feature = "fs")]
    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), Error> {
        self.save_with(
            path,
            &WriteOptions {
                flavor: self.flavor,
                budget: self.budget.clone(),
                pretty: false,
            },
        )
    }

    #[cfg(feature = "fs")]
    pub fn save_with(&self, path: impl AsRef<Path>, options: &WriteOptions) -> Result<(), Error> {
        let path = path.as_ref();
        let meta = std::fs::symlink_metadata(path);
        if let Ok(meta) = &meta {
            if meta.file_type().is_symlink() {
                return Err(Error::new(
                    "io.destination",
                    "The destination is a symbolic link. Choose a regular file.",
                ));
            }
        }
        let bytes = self.write_bytes_with(options)?;
        let tmp = path.with_extension("edgedoc-tmp");
        {
            let mut file = std::fs::File::create(&tmp)
                .map_err(|err| Error::new("io.destination", err.to_string()))?;
            std::io::Write::write_all(&mut file, &bytes)
                .map_err(|err| Error::new("io.destination", err.to_string()))?;
            file.sync_all()
                .map_err(|err| Error::new("io.destination", err.to_string()))?;
        }
        std::fs::rename(&tmp, path).map_err(|err| {
            let _ = std::fs::remove_file(&tmp);
            Error::new("io.destination", err.to_string())
        })?;
        Ok(())
    }

    pub fn edit<F>(&mut self, opts: EditOptions, f: F) -> Result<EditReport, Error>
    where
        F: FnOnce(&mut DocumentEdit<'_>) -> edgedoc_model::ModelResult<()>,
    {
        let report = self.inner.edit(opts, f)?;
        self.dirty = true;
        Ok(report)
    }

    pub fn plain_text(&self, story: StoryRef) -> String {
        self.inner.plain_text(story)
    }

    pub(crate) fn body_blocks(&self) -> &[Arc<Block>] {
        &self.inner.body
    }

    pub(crate) fn limits(&self) -> &Budget {
        &self.budget
    }

    pub fn merge_field_names(&self) -> Vec<String> {
        self.inner.merge_field_names()
    }

    pub fn stories(&self) -> Vec<StoryRef> {
        let mut stories = vec![StoryRef::Body];
        for id in self.inner.parts.keys() {
            stories.push(StoryRef::Part(*id));
        }
        stories
    }

    pub fn part_names(&self) -> Vec<String> {
        if self.kept.is_empty() {
            return vec![
                "[Content_Types].xml".into(),
                "_rels/.rels".into(),
                "word/document.xml".into(),
                "word/styles.xml".into(),
                "word/settings.xml".into(),
                "docProps/core.xml".into(),
            ];
        }
        self.kept.iter().map(|(name, _, _)| name.clone()).collect()
    }

    pub fn flavor(&self) -> Flavor {
        self.flavor
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn balance_fields(&mut self) -> usize {
        let removed = self.inner.balance_fields();
        if removed > 0 {
            self.dirty = true;
        }
        removed
    }

    pub fn style_outline(&self, name: &str) -> Option<i32> {
        self.inner.styles.outline_of(name)
    }

    pub fn core_title(&self) -> &str {
        &self.inner.core.title
    }

    pub fn body_len(&self) -> usize {
        self.inner.body.len()
    }

    pub fn set_budget(&mut self, budget: Budget) {
        self.budget = budget;
    }

    pub fn cell_span(&self, block: usize, row: usize, cell: usize) -> Option<usize> {
        let Block::Table(table) = self.inner.body.get(block)?.as_ref() else {
            return None;
        };
        Some(table.rows.get(row)?.cells.get(cell)?.span)
    }

    pub fn run_is_rtl(&self, block: usize) -> bool {
        let Some(block) = self.inner.body.get(block) else {
            return false;
        };
        let Block::Paragraph(para) = block.as_ref() else {
            return false;
        };
        para.runs.iter().any(|run| run.props.rtl)
    }

    pub fn run_lang(&self, block: usize) -> Option<String> {
        let Block::Paragraph(para) = self.inner.body.get(block)?.as_ref() else {
            return None;
        };
        para.runs.iter().find_map(|run| run.props.lang.clone())
    }

    pub fn section_break_count(&self) -> usize {
        self.inner
            .body
            .iter()
            .filter(
                |block| matches!(block.as_ref(), Block::Paragraph(para) if para.section.is_some()),
            )
            .count()
    }

    /// The core properties, such as the title and the author.
    pub fn core_props(&self) -> &CoreProps {
        &self.inner.core
    }

    /// Replace the core properties. The next save writes them.
    pub fn set_core_props(&mut self, core: CoreProps) {
        self.inner.core = core;
        self.dirty = true;
    }

    /// The style sheet, read from `styles.xml` and built by the builder.
    pub fn styles(&self) -> &edgedoc_model::StyleSheet {
        &self.inner.styles
    }

    /// Whether the imported package included a styles part.
    pub fn has_styles_part(&self) -> bool {
        self.styles_part_present
    }

    /// Document defaults decoded for layout, with Wordcraft's fallback for omitted styles.
    /// Reading these values does not rewrite the preserved defaults markup.
    pub fn default_formatting(&self) -> Result<(CharProps, ParaProps), Error> {
        let (mut run, mut para) = match self.inner.styles.doc_defaults_xml.as_deref() {
            Some(raw) => edgedoc_wml::style_default_properties(raw, &self.budget)?,
            None => (CharProps::default(), ParaProps::default()),
        };
        if run.font.is_none() {
            run.font = Some("Calibri".into());
        }
        if run.size_pt.is_none() {
            run.size_pt = Some(if self.styles_part_present { 10.0 } else { 12.0 });
        }
        if !self.styles_part_present {
            run.kern = Some(2);
            para.spacing_after = Some(Twips(160));
            para.line = Some(Twips(276));
            para.line_rule = Some("auto".into());
        }
        Ok((run, para))
    }

    /// Add a style. A style with the same id is replaced.
    pub fn add_style(&mut self, style: Style) {
        self.inner.styles.replace(style);
        self.dirty = true;
    }

    /// Part ids for a story kind, in document order.
    pub fn parts(&self, kind: PartKind) -> Vec<u32> {
        self.inner.parts_of(kind)
    }

    /// The blocks of one story part, in document order. A header part is one example.
    pub fn part_blocks(&self, id: u32) -> Vec<Block> {
        self.inner
            .parts
            .get(&id)
            .map(|part| part.blocks.iter().map(|b| b.as_ref().clone()).collect())
            .unwrap_or_default()
    }

    /// The identity of a story part: note id, author, and date.
    pub fn part_meta(&self, id: u32) -> Option<&PartMeta> {
        self.inner.parts.get(&id).map(|part| &part.meta)
    }

    pub fn part_kind(&self, id: u32) -> Option<PartKind> {
        self.inner.parts.get(&id).map(|part| part.kind)
    }

    /// Every picture in the body, with its size and its anchor.
    pub fn pictures(&self) -> Vec<&Picture> {
        fn walk<'a>(blocks: &'a [Arc<Block>], out: &mut Vec<&'a Picture>) {
            for block in blocks {
                match block.as_ref() {
                    Block::Paragraph(para) => out.extend(para.pictures()),
                    Block::Table(table) => {
                        for row in &table.rows {
                            for cell in &row.cells {
                                walk(&cell.blocks, out);
                            }
                        }
                    }
                    Block::Control(control) => walk(&control.blocks, out),
                    Block::Alternate { choice, .. } => walk(choice, out),
                    Block::Opaque(_) => {}
                }
            }
        }
        let mut out = Vec::new();
        walk(&self.inner.body, &mut out);
        out
    }

    /// The last section properties, which set the page size and the margins.
    pub fn section(&self) -> &SectionProps {
        &self.inner.last_section
    }

    /// The validated document settings markup, when the package defines it.
    pub fn settings_xml(&self) -> Option<&str> {
        self.inner.settings_xml.as_deref()
    }

    /// The preserved DrawingML theme, used to resolve major/minor text font slots.
    pub fn theme_xml(&self) -> Option<&str> {
        self.kept
            .iter()
            .find(|(_, _, content_type)| {
                content_type
                    .as_deref()
                    .is_some_and(|t| t.ends_with("officedocument.theme+xml"))
            })
            .and_then(|(_, bytes, _)| std::str::from_utf8(bytes).ok())
    }

    /// The paragraph properties at a body index, if the block is a paragraph.
    pub fn paragraph_props(&self, index: usize) -> Option<&ParaProps> {
        match self.inner.body.get(index)?.as_ref() {
            Block::Paragraph(para) => Some(&para.props),
            _ => None,
        }
    }

    /// The run properties at a body index and a run index.
    pub fn run_props(&self, block: usize, run: usize) -> Option<&CharProps> {
        match self.inner.body.get(block)?.as_ref() {
            Block::Paragraph(para) => para.runs.get(run).map(|run| &run.props),
            _ => None,
        }
    }

    /// The list kind of a paragraph, when the paragraph carries a generated list.
    pub fn list_kind(&self, index: usize) -> Option<ListKind> {
        self.paragraph_props(index)?.list.map(|spec| spec.kind)
    }

    /// The numbering xml as the writer will emit it.
    pub fn numbering_xml(&self) -> Option<&str> {
        self.inner.numbering_xml.as_deref()
    }

    /// The table at a body index.
    pub fn table(&self, index: usize) -> Option<&Table> {
        match self.inner.body.get(index)?.as_ref() {
            Block::Table(table) => Some(table),
            _ => None,
        }
    }

    /// The content controls in the body, in document order.
    pub fn controls(&self) -> Vec<&ContentControl> {
        self.inner
            .body
            .iter()
            .filter_map(|block| match block.as_ref() {
                Block::Control(control) => Some(control),
                _ => None,
            })
            .collect()
    }

    /// The block at a body index.
    pub fn block_at(&self, index: usize) -> Option<&Block> {
        self.inner.body.get(index).map(|block| block.as_ref())
    }

    /// Append a picture to a new paragraph at the end of the body.
    pub fn append_picture(&mut self, picture: Picture) -> Result<(), Error> {
        let para = Paragraph::new().picture(picture);
        self.inner.body.push(Arc::new(Block::Paragraph(para)));
        self.dirty = true;
        Ok(())
    }

    /// Append a table at the end of the body.
    pub fn append_table(&mut self, table: Table) -> Result<(), Error> {
        let cols = table
            .rows
            .iter()
            .map(|row| row.cells.len())
            .max()
            .unwrap_or(0);
        if cols > self.budget.max_cols {
            return Err(Error::new(
                "model.limit",
                format!("The table has {cols} columns. The limit is 63."),
            ));
        }
        self.inner.body.push(Arc::new(Block::Table(table)));
        self.dirty = true;
        Ok(())
    }

    /// True when the bytes hold a package that a password protects.
    ///
    /// The check reads the container, not the password, so it answers without a key.
    pub fn is_password_protected(bytes: &[u8]) -> bool {
        is_encrypted_package(bytes)
    }

    pub fn has_opaque_drawing(&self) -> bool {
        fn walk(blocks: &[std::sync::Arc<Block>]) -> bool {
            blocks.iter().any(|block| match block.as_ref() {
                Block::Paragraph(para) => para
                    .objects
                    .iter()
                    .any(|object| matches!(object, edgedoc_model::InlineObject::Opaque(_))),
                Block::Table(table) => table
                    .rows
                    .iter()
                    .flat_map(|row| &row.cells)
                    .any(|cell| walk(&cell.blocks)),
                Block::Alternate { choice, .. } => walk(choice),
                Block::Control(control) => walk(&control.blocks),
                Block::Opaque(_) => true,
            })
        }
        walk(&self.inner.body)
    }

    /// One record with its fields replaced. The template does not change.
    ///
    /// A field with no column returns an error. Use [`Document::preview_with`] when the
    /// template holds a field that the caller fills with `map_value`.
    pub fn preview(&self, records: &Records, index: usize) -> Result<Document, Error> {
        self.preview_with(records, index, MissingField::Error)
    }

    /// One record with its fields replaced, with the missing-field mode that the
    /// caller chooses. A template with computed fields needs `MissingField::Empty`.
    pub fn preview_with(
        &self,
        records: &Records,
        index: usize,
        missing: MissingField,
    ) -> Result<Document, Error> {
        let model = merge_preview(&self.inner, records, index, missing, &self.budget)?;
        Ok(self.wrap(model))
    }

    pub fn merge(&self) -> MergeBuilder<'_> {
        MergeBuilder {
            doc: self,
            records: None,
            kind: MergeKind::Letters,
            missing: MissingField::Error,
            range: RecordRange::all(),
            maps: Vec::new(),
        }
    }

    /// One merge path for Rust, Python, and WebAssembly.
    pub fn merge_letters_bytes(template: &[u8], csv: &[u8]) -> Result<Vec<u8>, Error> {
        let doc = Document::read_bytes(template)?;
        let records = Records::from_csv(csv, &CsvOptions::default())?;
        let batch = doc
            .merge()
            .records(records)
            .kind(MergeKind::Letters)
            .missing(MissingField::Empty)
            .finish()?;
        batch.join(Join::Auto)?.write_bytes()
    }

    fn wrap(&self, inner: ModelDocument) -> Document {
        Document {
            inner,
            styles_part_present: self.styles_part_present,
            original: None,
            kept: Vec::new(),
            dirty: true,
            flavor: self.flavor,
            budget: self.budget.clone(),
        }
    }
}

pub struct MergeBuilder<'a> {
    doc: &'a Document,
    records: Option<Records>,
    kind: MergeKind,
    missing: MissingField,
    range: RecordRange,
    maps: Vec<(String, MapFn)>,
}

impl<'a> MergeBuilder<'a> {
    pub fn records(mut self, records: Records) -> Self {
        self.records = Some(records);
        self
    }

    pub fn kind(mut self, kind: MergeKind) -> Self {
        self.kind = kind;
        self
    }

    pub fn missing(mut self, missing: MissingField) -> Self {
        self.missing = missing;
        self
    }

    pub fn range(mut self, range: RecordRange) -> Self {
        self.range = range;
        self
    }

    pub fn map_value<F>(mut self, field: &str, map: F) -> Self
    where
        F: Fn(&Record) -> String + 'static,
    {
        self.maps.push((field.to_string(), Box::new(map)));
        self
    }

    pub fn finish(mut self) -> Result<Batch, Error> {
        let records = self
            .records
            .take()
            .ok_or_else(|| Error::new("merge.no_records", "The merge has no records."))?;
        let mut request = MergeRequest {
            template: &self.doc.inner,
            records,
            kind: self.kind,
            missing: self.missing,
            from: self.range.from,
            to: self.range.to,
            maps: std::mem::take(&mut self.maps),
            budget: self.doc.budget.clone(),
            today: self.doc.inner.core.created.clone(),
        };
        let inner = finish(&mut request)?;
        Ok(Batch {
            inner,
            flavor: self.doc.flavor,
            budget: self.doc.budget.clone(),
            styles_part_present: self.doc.styles_part_present,
        })
    }
}

pub struct Batch {
    inner: edgedoc_merge::MergeBatch,
    flavor: Flavor,
    budget: Budget,
    styles_part_present: bool,
}

impl Batch {
    pub fn len(&self) -> usize {
        self.inner.documents.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.documents.is_empty()
    }

    pub fn document(&self, index: usize) -> Option<Document> {
        self.inner.documents.get(index).map(|model| Document {
            inner: model.clone(),
            original: None,
            kept: Vec::new(),
            dirty: true,
            flavor: self.flavor,
            budget: self.budget.clone(),
            styles_part_present: self.styles_part_present,
        })
    }

    pub fn join(&self, join: Join) -> Result<Document, Error> {
        if matches!(self.inner.kind, MergeKind::Emails)
            && matches!(join, Join::Auto | Join::PageBreak)
        {
            return Err(Error::new(
                "unsupported",
                "Email messages stay separate. Read each document from the batch.",
            ));
        }
        let mut batch = edgedoc_merge::MergeBatch {
            documents: self.inner.documents.clone(),
            kind: self.inner.kind,
            per_record_sections: self.inner.per_record_sections,
        };
        if matches!(join, Join::None) {
            batch.kind = MergeKind::Emails;
        }
        if matches!(join, Join::PageBreak) {
            batch.per_record_sections = false;
            if matches!(batch.kind, MergeKind::Directory) {
                batch.kind = MergeKind::Letters;
            }
        }
        let model = join_documents(batch, &self.budget)?;
        Ok(Document {
            inner: model,
            original: None,
            kept: Vec::new(),
            dirty: true,
            flavor: self.flavor,
            budget: self.budget.clone(),
            styles_part_present: self.styles_part_present,
        })
    }
}

#[derive(Debug, Default)]
pub struct DocumentBuilder {
    core: CoreProps,
    page: Option<SectionProps>,
    styles: Vec<Style>,
    blocks: Vec<Block>,
    header: Vec<Block>,
    footer: Vec<Block>,
    header_first: Vec<Block>,
    footer_first: Vec<Block>,
    error: Option<Error>,
}

impl DocumentBuilder {
    pub fn core(mut self, core: CoreProps) -> Self {
        self.core = core;
        self
    }

    pub fn page(mut self, page: Page) -> Self {
        self.page = Some(page.props);
        self
    }

    /// The blocks of the default header.
    pub fn header(mut self, blocks: impl IntoIterator<Item = Block>) -> Self {
        self.header = blocks.into_iter().collect();
        self
    }

    /// The blocks of the default footer.
    pub fn footer(mut self, blocks: impl IntoIterator<Item = Block>) -> Self {
        self.footer = blocks.into_iter().collect();
        self
    }

    /// The blocks of the first-page header. It shows only with a title page.
    pub fn header_first(mut self, blocks: impl IntoIterator<Item = Block>) -> Self {
        self.header_first = blocks.into_iter().collect();
        self
    }

    /// The blocks of the first-page footer. It shows only with a title page.
    pub fn footer_first(mut self, blocks: impl IntoIterator<Item = Block>) -> Self {
        self.footer_first = blocks.into_iter().collect();
        self
    }

    pub fn style(mut self, style: Style) -> Self {
        self.styles.push(style);
        self
    }

    pub fn paragraph(mut self, para: Paragraph) -> Self {
        self.blocks.push(Block::Paragraph(para));
        self
    }

    /// A content control, or any other block that has no short form.
    pub fn block(mut self, block: Block) -> Self {
        self.blocks.push(block);
        self
    }

    pub fn table(mut self, table: Table) -> Self {
        let cols = table
            .rows
            .iter()
            .map(|row| row.cells.len())
            .max()
            .unwrap_or(0);
        if cols > Budget::default().max_cols {
            self.error = Some(Error::new(
                "model.limit",
                format!("The table has {cols} columns. The limit is 63. Remove the extra columns."),
            ));
            return self;
        }
        self.blocks.push(Block::Table(table));
        self
    }

    pub fn build(self) -> Result<Document, Error> {
        if let Some(err) = self.error {
            return Err(err);
        }
        let mut inner = ModelDocument::new();
        inner.core = self.core;
        if let Some(page) = self.page {
            inner.last_section = page;
        }
        inner.styles.styles = self.styles;
        inner.body = self.blocks.into_iter().map(Arc::new).collect();
        inner.ensure_nonempty();
        if !self.header.is_empty() {
            let id = inner.add_part(
                PartKind::Header,
                self.header.into_iter().map(Arc::new).collect(),
            );
            inner.last_section.headers.default = Some(id);
        }
        if !self.footer.is_empty() {
            let id = inner.add_part(
                PartKind::Footer,
                self.footer.into_iter().map(Arc::new).collect(),
            );
            inner.last_section.footers.default = Some(id);
        }
        if !self.header_first.is_empty() {
            let id = inner.add_part(
                PartKind::Header,
                self.header_first.into_iter().map(Arc::new).collect(),
            );
            inner.last_section.headers.first = Some(id);
            inner.last_section.title_page = true;
        }
        if !self.footer_first.is_empty() {
            let id = inner.add_part(
                PartKind::Footer,
                self.footer_first.into_iter().map(Arc::new).collect(),
            );
            inner.last_section.footers.first = Some(id);
            inner.last_section.title_page = true;
        }
        Ok(Document {
            inner,
            styles_part_present: true,
            original: None,
            kept: Vec::new(),
            dirty: true,
            flavor: Flavor::Docx,
            budget: Budget::default(),
        })
    }
}

fn encrypted(options: &ReadOptions<'_>) -> Result<Document, Error> {
    match options.password {
        None => Err(Error::new(
            "package.password_required",
            "This document is protected with a password. Pass the password to open it.",
        )),
        Some(password) => {
            if password.chars().count() > options.budget.max_password_chars {
                return Err(Error::new(
                    "package.limit",
                    "The password is longer than 255 characters. Use a shorter password.",
                ));
            }
            Err(Error::new(
                "package.encrypted_unsupported",
                "This version detects the encrypted package and does not decrypt it. No document body was read.",
            ))
        }
    }
}
