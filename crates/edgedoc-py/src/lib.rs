//! Python calls the same Rust functions as the other adapters.
//!
//! Every function holds bytes, not a file handle, so the caller decides where the
//! document lives. The `python` feature adds the native module that maturin builds.

use edgedoc::{
    Budget, CsvOptions, Document, Join, MergeKind, MissingField, PartKind, ReadOptions, Records,
    SortKey, StoryRef, WriteOptions,
};

/// An error with the stable code that the Rust facade returns.
#[derive(Debug)]
pub struct AdapterError {
    pub code: String,
    pub message: String,
}

impl AdapterError {
    fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }

    /// `code: message`, which is what the adapters return as text.
    pub fn render(&self) -> String {
        format!("{} {}", self.code, self.message)
    }
}

impl std::fmt::Display for AdapterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.render())
    }
}

fn error(err: edgedoc::Error) -> AdapterError {
    AdapterError::new(err.code, err.message)
}

/// Merge letters. The bytes match [`edgedoc::Document::merge_letters_bytes`].
pub fn merge_csv_bytes(template: &[u8], csv: &[u8]) -> Result<Vec<u8>, String> {
    Document::merge_letters_bytes(template, csv)
        .map_err(|err| err.code.to_string() + " " + &err.message)
}

/// Read Markdown and return a `.docx` package.
///
/// Each image is `(path, bytes)`. The path is the reference in the Markdown, such as
/// `images/chart.png`.
pub fn markdown_to_docx(markdown: &str, images: &[(&str, &[u8])]) -> Result<Vec<u8>, AdapterError> {
    let options = edgedoc::MarkdownReadOptions::new().images(
        images
            .iter()
            .map(|(path, bytes)| edgedoc::MarkdownImage::new(*path, (*bytes).to_vec())),
    );
    let doc = Document::from_markdown_with(markdown, &options).map_err(error)?;
    doc.write_bytes().map_err(error)
}

/// Read a Markdown file and return a `.docx` package. Images resolve beside the file.
pub fn open_markdown_file(path: &std::path::Path) -> Result<Vec<u8>, AdapterError> {
    let doc = Document::open_markdown(path).map_err(error)?;
    doc.write_bytes().map_err(error)
}

pub fn read_plain(bytes: &[u8]) -> Result<String, String> {
    let doc = Document::read_bytes(bytes).map_err(|err| format!("{} {}", err.code, err.message))?;
    Ok(doc.plain_text(StoryRef::Body))
}

// ---------------------------------------------------------------- the full surface

/// Read a document and return its title. An empty title returns an empty string.
pub fn title(bytes: &[u8]) -> Result<String, AdapterError> {
    let doc = Document::read_bytes(bytes).map_err(error)?;
    Ok(doc.core_props().title.clone())
}

/// The merge field names in body, headers, and footers, in order.
pub fn merge_fields(bytes: &[u8]) -> Result<Vec<String>, AdapterError> {
    let doc = Document::read_bytes(bytes).map_err(error)?;
    Ok(doc.merge_field_names())
}

/// The package part names, so a caller can see what stays in the file.
pub fn parts(bytes: &[u8]) -> Result<Vec<String>, AdapterError> {
    let doc = Document::read_bytes(bytes).map_err(error)?;
    Ok(doc.part_names())
}

/// The plain text of the body, or of one story.
///
/// `story` is `"body"`, `"header"`, `"footer"`, `"footnote"`, `"endnote"`, or
/// `"comment"`. An unknown name returns an error. A story of that kind with no part
/// returns an empty string.
pub fn text_of(bytes: &[u8], story: &str) -> Result<String, AdapterError> {
    let doc = Document::read_bytes(bytes).map_err(error)?;
    let kind = match story.to_ascii_lowercase().as_str() {
        "body" => return Ok(doc.plain_text(StoryRef::Body)),
        "header" => PartKind::Header,
        "footer" => PartKind::Footer,
        "footnote" => PartKind::Footnote,
        "endnote" => PartKind::Endnote,
        "comment" => PartKind::Comment,
        other => {
            return Err(AdapterError::new(
                "unsupported",
                format!("Unknown story \"{other}\". Use body, header, footer, footnote, endnote, or comment."),
            ))
        }
    };
    let mut out = String::new();
    for id in doc.parts(kind) {
        out.push_str(&doc.plain_text(StoryRef::Part(id)));
    }
    Ok(out)
}

/// The number of stories of each kind: body, header, footer, footnote, endnote, comment.
pub fn story_counts(bytes: &[u8]) -> Result<Vec<(&'static str, usize)>, AdapterError> {
    let doc = Document::read_bytes(bytes).map_err(error)?;
    Ok(vec![
        ("body", 1),
        ("header", doc.parts(PartKind::Header).len()),
        ("footer", doc.parts(PartKind::Footer).len()),
        ("footnote", doc.parts(PartKind::Footnote).len()),
        ("endnote", doc.parts(PartKind::Endnote).len()),
        ("comment", doc.parts(PartKind::Comment).len()),
    ])
}

/// A document that was read and not edited returns the bytes it came from.
pub fn is_dirty(bytes: &[u8]) -> Result<bool, AdapterError> {
    let doc = Document::read_bytes(bytes).map_err(error)?;
    Ok(doc.is_dirty())
}

/// Replace every merge field in one record and return the bytes.
///
/// `record` is the zero-based index into the CSV rows. `missing` is `"error"`,
/// `"empty"`, or `"placeholder"`.
pub fn merge_one(
    template: &[u8],
    csv: &[u8],
    record: usize,
    missing: &str,
) -> Result<Vec<u8>, AdapterError> {
    let doc = Document::read_bytes(template).map_err(error)?;
    let records = Records::from_csv(csv, &CsvOptions::default())
        .map_err(|err| AdapterError::new(err.code, err.message))?;
    let preview = doc
        .preview_with(&records, record, missing_field(missing)?)
        .map_err(error)?;
    preview.write_bytes().map_err(error)
}

/// Merge every record and join the results.
///
/// `kind` is `"letters"`, `"directory"`, `"labels"`, or `"emails"`. `join` is
/// `"auto"`, `"page-break"`, or `"none"`. `missing` is `"error"`, `"empty"`, or
/// `"placeholder"`.
pub fn merge_all(
    template: &[u8],
    csv: &[u8],
    kind: &str,
    join: &str,
    missing: &str,
) -> Result<Vec<u8>, AdapterError> {
    let doc = Document::read_bytes(template).map_err(error)?;
    let records = Records::from_csv(csv, &CsvOptions::default())
        .map_err(|err| AdapterError::new(err.code, err.message))?;
    let header = first_header(csv);
    let records = if header.is_empty() {
        records
    } else {
        // A sort cannot fail for a header that the CSV holds, so fall back to the input.
        match records.clone().sort(&[SortKey::new(&header)]) {
            Ok(sorted) => sorted,
            Err(_) => records,
        }
    };
    let batch = doc
        .merge()
        .records(records)
        .kind(merge_kind(kind)?)
        .missing(missing_field(missing)?)
        .finish()
        .map_err(error)?;
    let joined = batch.join(join_mode(join)?).map_err(error)?;
    joined.write_bytes().map_err(error)
}

/// The first header of a CSV, which is the column that a directory usually sorts on.
fn first_header(csv: &[u8]) -> String {
    let text = String::from_utf8_lossy(csv);
    text.lines()
        .next()
        .and_then(|line| line.split(',').next())
        .map(|cell| cell.trim().trim_matches('"').to_string())
        .unwrap_or_default()
}

fn merge_kind(kind: &str) -> Result<MergeKind, AdapterError> {
    match kind.to_ascii_lowercase().as_str() {
        "letters" => Ok(MergeKind::Letters),
        "directory" => Ok(MergeKind::Directory),
        "labels" => Ok(MergeKind::Labels),
        "emails" => Ok(MergeKind::Emails),
        other => Err(AdapterError::new(
            "unsupported",
            format!("Unknown merge kind \"{other}\". Use letters, directory, labels, or emails."),
        )),
    }
}

fn join_mode(join: &str) -> Result<Join, AdapterError> {
    match join.to_ascii_lowercase().as_str() {
        "auto" => Ok(Join::Auto),
        "page-break" | "page_break" => Ok(Join::PageBreak),
        "none" => Ok(Join::None),
        other => Err(AdapterError::new(
            "unsupported",
            format!("Unknown join \"{other}\". Use auto, page-break, or none."),
        )),
    }
}

fn missing_field(missing: &str) -> Result<MissingField, AdapterError> {
    match missing.to_ascii_lowercase().as_str() {
        "error" => Ok(MissingField::Error),
        "empty" => Ok(MissingField::Empty),
        "placeholder" => Ok(MissingField::Placeholder),
        other => Err(AdapterError::new(
            "unsupported",
            format!("Unknown missing-field mode \"{other}\". Use error, empty, or placeholder."),
        )),
    }
}

/// Write the package in another flavour: `"docx"`, `"docm"`, `"dotx"`, or `"dotm"`.
pub fn write_as(bytes: &[u8], flavor: &str) -> Result<Vec<u8>, AdapterError> {
    let doc = Document::read_bytes(bytes).map_err(error)?;
    let flavor = match flavor.to_ascii_lowercase().as_str() {
        "docx" => edgedoc::Flavor::Docx,
        "docm" => edgedoc::Flavor::Docm,
        "dotx" => edgedoc::Flavor::Dotx,
        "dotm" => edgedoc::Flavor::Dotm,
        other => {
            return Err(AdapterError::new(
                "unsupported",
                format!("Unknown flavor \"{other}\". Use docx, docm, dotx, or dotm."),
            ))
        }
    };
    doc.write_bytes_with(&WriteOptions {
        flavor,
        budget: Budget::default(),
        pretty: false,
    })
    .map_err(error)
}

/// True when the bytes hold an encrypted package. The library does not decrypt it.
pub fn is_encrypted(bytes: &[u8]) -> bool {
    Document::is_password_protected(bytes)
}

/// The error code of a failed call, or an empty string. For callers that want to
/// branch on the code instead of reading the message.
pub fn error_code(message: &str) -> String {
    message.split_whitespace().next().unwrap_or("").to_string()
}

/// Check that the bytes are a document this library can read, and return the flavor.
pub fn inspect(bytes: &[u8]) -> Result<Vec<(&'static str, String)>, AdapterError> {
    let doc = Document::read_bytes_with(
        bytes,
        &ReadOptions {
            password: None,
            budget: Budget::default(),
            unknown: edgedoc::UnknownPart::Retain,
        },
    )
    .map_err(error)?;
    Ok(vec![
        ("title", doc.core_props().title.clone()),
        ("creator", doc.core_props().creator.clone()),
        ("subject", doc.core_props().subject.clone()),
        (
            "last_modified_by",
            doc.core_props().last_modified_by.clone(),
        ),
        ("flavor", format!("{:?}", doc.flavor()).to_ascii_lowercase()),
        ("parts", doc.part_names().len().to_string()),
        ("merge_fields", doc.merge_field_names().len().to_string()),
        ("body_blocks", doc.body_len().to_string()),
        ("section_breaks", doc.section_break_count().to_string()),
        ("pictures", doc.pictures().len().to_string()),
    ])
}

#[cfg(feature = "python")]
mod py {
    use pyo3::exceptions::PyRuntimeError;
    use pyo3::prelude::*;
    use pyo3::types::PyBytes;

    fn runtime(message: String) -> PyErr {
        PyRuntimeError::new_err(message)
    }

    #[pyfunction]
    fn merge(py: Python<'_>, template: &[u8], csv: &[u8]) -> PyResult<Py<PyBytes>> {
        let out = super::merge_csv_bytes(template, csv).map_err(runtime)?;
        Ok(PyBytes::new_bound(py, &out).unbind())
    }

    #[pyfunction]
    fn plain_text(bytes: &[u8]) -> PyResult<String> {
        super::read_plain(bytes).map_err(runtime)
    }

    #[pyfunction]
    fn doc_title(bytes: &[u8]) -> PyResult<String> {
        super::title(bytes).map_err(|err| runtime(err.render()))
    }

    #[pyfunction]
    fn merge_field_names(bytes: &[u8]) -> PyResult<Vec<String>> {
        super::merge_fields(bytes).map_err(|err| runtime(err.render()))
    }

    #[pyfunction]
    fn part_names(bytes: &[u8]) -> PyResult<Vec<String>> {
        super::parts(bytes).map_err(|err| runtime(err.render()))
    }

    #[pyfunction]
    fn text_of_story(bytes: &[u8], story: &str) -> PyResult<String> {
        super::text_of(bytes, story).map_err(|err| runtime(err.render()))
    }

    #[pyfunction]
    fn story_summary(bytes: &[u8]) -> PyResult<Vec<(String, usize)>> {
        let counts = super::story_counts(bytes).map_err(|err| runtime(err.render()))?;
        Ok(counts
            .into_iter()
            .map(|(name, count)| (name.to_string(), count))
            .collect())
    }

    #[pyfunction]
    fn merge_record(
        py: Python<'_>,
        template: &[u8],
        csv: &[u8],
        record: usize,
        missing: &str,
    ) -> PyResult<Py<PyBytes>> {
        let out = super::merge_one(template, csv, record, missing)
            .map_err(|err| runtime(err.render()))?;
        Ok(PyBytes::new_bound(py, &out).unbind())
    }

    #[pyfunction]
    #[pyo3(signature = (template, csv, kind="letters", join="auto", missing="error"))]
    fn merge_records(
        py: Python<'_>,
        template: &[u8],
        csv: &[u8],
        kind: &str,
        join: &str,
        missing: &str,
    ) -> PyResult<Py<PyBytes>> {
        let out = super::merge_all(template, csv, kind, join, missing)
            .map_err(|err| runtime(err.render()))?;
        Ok(PyBytes::new_bound(py, &out).unbind())
    }

    #[pyfunction]
    fn write_as_flavor(py: Python<'_>, bytes: &[u8], flavor: &str) -> PyResult<Py<PyBytes>> {
        let out = super::write_as(bytes, flavor).map_err(|err| runtime(err.render()))?;
        Ok(PyBytes::new_bound(py, &out).unbind())
    }

    #[pyfunction]
    fn is_password_protected(bytes: &[u8]) -> bool {
        super::is_encrypted(bytes)
    }

    #[pyfunction]
    fn document_summary(bytes: &[u8]) -> PyResult<Vec<(String, String)>> {
        let rows = super::inspect(bytes).map_err(|err| runtime(err.render()))?;
        Ok(rows
            .into_iter()
            .map(|(key, value)| (key.to_string(), value))
            .collect())
    }

    #[pyfunction]
    fn is_edited(bytes: &[u8]) -> PyResult<bool> {
        super::is_dirty(bytes).map_err(|err| runtime(err.render()))
    }

    /// Read Markdown and return a `.docx` package.
    ///
    /// `images` is a list of `(path, bytes)`. The path matches the Markdown reference.
    #[pyfunction]
    #[pyo3(signature = (markdown, images=None))]
    fn markdown_to_docx(
        py: Python<'_>,
        markdown: &str,
        images: Option<Vec<(String, Vec<u8>)>>,
    ) -> PyResult<Py<PyBytes>> {
        let owned = images.unwrap_or_default();
        let refs: Vec<(&str, &[u8])> = owned
            .iter()
            .map(|(path, bytes)| (path.as_str(), bytes.as_slice()))
            .collect();
        let out = super::markdown_to_docx(markdown, &refs).map_err(|err| runtime(err.render()))?;
        Ok(PyBytes::new_bound(py, &out).unbind())
    }

    /// Read a Markdown file and return a `.docx` package. Images resolve beside the file.
    #[pyfunction]
    fn open_markdown(py: Python<'_>, path: &str) -> PyResult<Py<PyBytes>> {
        let out = super::open_markdown_file(std::path::Path::new(path))
            .map_err(|err| runtime(err.render()))?;
        Ok(PyBytes::new_bound(py, &out).unbind())
    }

    #[pymodule]
    fn _native(module: &Bound<'_, PyModule>) -> PyResult<()> {
        module.add_function(wrap_pyfunction!(merge, module)?)?;
        module.add_function(wrap_pyfunction!(plain_text, module)?)?;
        module.add_function(wrap_pyfunction!(doc_title, module)?)?;
        module.add_function(wrap_pyfunction!(merge_field_names, module)?)?;
        module.add_function(wrap_pyfunction!(part_names, module)?)?;
        module.add_function(wrap_pyfunction!(text_of_story, module)?)?;
        module.add_function(wrap_pyfunction!(story_summary, module)?)?;
        module.add_function(wrap_pyfunction!(merge_record, module)?)?;
        module.add_function(wrap_pyfunction!(merge_records, module)?)?;
        module.add_function(wrap_pyfunction!(write_as_flavor, module)?)?;
        module.add_function(wrap_pyfunction!(is_password_protected, module)?)?;
        module.add_function(wrap_pyfunction!(document_summary, module)?)?;
        module.add_function(wrap_pyfunction!(is_edited, module)?)?;
        module.add_function(wrap_pyfunction!(markdown_to_docx, module)?)?;
        module.add_function(wrap_pyfunction!(open_markdown, module)?)?;
        Ok(())
    }
}
