//! WebAssembly calls the same Rust functions as the other adapters.
//!
//! The `fs` feature stays off, so the module holds bytes and never touches a file.
//! SharedArrayBuffer and threads are not required.

use edgedoc::{
    Budget, CsvOptions, Document, Join, MergeKind, MissingField, PartKind, Records, SortKey,
    StoryRef, WriteOptions,
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

fn error(err: edgedoc::Error) -> AdapterError {
    AdapterError::new(err.code, err.message)
}

/// Merge letters. The bytes match [`edgedoc::Document::merge_letters_bytes`].
pub fn merge_csv_bytes(template: &[u8], csv: &[u8]) -> Result<Vec<u8>, String> {
    Document::merge_letters_bytes(template, csv)
        .map_err(|err| format!("{} {}", err.code, err.message))
}

/// specs/144 DOC2-03: merge a dataset — every parent with its child rows — into one package.
///
/// A dataset is JSON, not CSV, because a child list has nowhere to go in a flat table. The options
/// carry the kind, the join, the missing policy, and the budget the host chose.
///
/// The result is one package: the joined document for `auto`/`page-break`, the catalogue shape for
/// `directory`/`labels`, and the first document when `join` is `none`. A host that files one
/// document per parent passes `from`/`to` (or a one-parent dataset) and calls this once per parent.
pub fn merge_dataset_bytes(
    template: &[u8],
    dataset_json: &str,
    options_json: &str,
) -> Result<Vec<u8>, String> {
    // The budget is read **first**, then applied to the package read as well as to the merge
    // (DOC2-15). Reading with the engine default would let a zip bomb expand before the merge
    // budget ever mattered.
    let options = edgedoc::options_from_json(options_json)?;
    let mut document = Document::read_bytes_with(
        template,
        &edgedoc::ReadOptions {
            budget: options.budget.clone(),
            ..edgedoc::ReadOptions::new()
        },
    )
    .map_err(|err| format!("{} {}", err.code, err.message))?;
    let dataset = edgedoc::dataset_from_json(dataset_json)?;
    let documents = edgedoc::merge_dataset(&document.model(), &dataset, &options)?;
    let first = documents
        .into_iter()
        .next()
        .ok_or_else(|| "merge.no_records The merge produced no documents.".to_string())?;
    document.adopt_merged(first);
    document
        .write_bytes()
        .map_err(|err| format!("{} {}", err.code, err.message))
}

/// The plain text of the first merged document. A check for an author, never a file.
pub fn preview_dataset_text(
    template: &[u8],
    dataset_json: &str,
    options_json: &str,
) -> Result<String, String> {
    let options = edgedoc::options_from_json(options_json)?;
    let document = Document::read_bytes_with(
        template,
        &edgedoc::ReadOptions {
            budget: options.budget.clone(),
            ..edgedoc::ReadOptions::new()
        },
    )
    .map_err(|err| format!("{} {}", err.code, err.message))?;
    let dataset = edgedoc::dataset_from_json(dataset_json)?;
    let documents = edgedoc::merge_dataset(&document.model(), &dataset, &options)?;
    let first = documents
        .into_iter()
        .next()
        .ok_or_else(|| "merge.no_records The merge produced no documents.".to_string())?;
    Ok(first.plain_text(StoryRef::Body))
}

/// Every merge field with its story, table, and row, as JSON text.
pub fn field_index_of(template: &[u8]) -> Result<String, String> {
    let document =
        Document::read_bytes(template).map_err(|err| format!("{} {}", err.code, err.message))?;
    Ok(edgedoc::field_index_json(&document.model()))
}

/// Every zip entry name.
pub fn part_names_of(template: &[u8]) -> Result<Vec<String>, String> {
    let doc =
        Document::read_bytes(template).map_err(|err| format!("{} {}", err.code, err.message))?;
    Ok(doc.part_names())
}

/// Read Markdown and return a `.docx` package. Image bytes are supplied by the caller.
pub fn markdown_to_document(markdown: &str) -> Result<Vec<u8>, AdapterError> {
    let doc = Document::from_markdown(markdown).map_err(error)?;
    doc.write_bytes().map_err(error)
}

/// Build a `.docx` package from a closed block description (specs/144 DOC2-03).
///
/// Markdown cannot carry a merge field: `«Name»` is ordinary text, and `«Lines.Quantity__c»` loses its
/// underscores to emphasis. A host that installs a template therefore needs a way to say "this
/// paragraph is a field" — this is that way, and it is the smallest vocabulary a mail-merge template
/// needs:
///
/// ```json
/// { "blocks": [
///   { "paragraph": { "text": "For ", "field": "Customer", "after": "" } },
///   { "table": [ { "header": "Article", "field": "Lines.Name" },
///                { "header": "Qty",     "field": "Lines.Quantity__c" } ] }
/// ] }
/// ```
///
/// Every `field` becomes a real `MERGEFIELD`, so `mergeFieldNames` on the result reports it and a host
/// can judge its binding against the package it just built.
pub fn document_from_blocks(json: &str) -> Result<Vec<u8>, AdapterError> {
    let spec = edgedoc::blocks::BlocksSpec::parse(json)
        .map_err(|issue| AdapterError::new(issue.code, issue.message))?;
    let doc = spec.build().map_err(error)?;
    doc.write_bytes().map_err(error)
}

/// Read Markdown with images packed as one byte buffer.
///
/// `paths.len()` must equal `lengths.len()`. `bytes` is the images concatenated in
/// the same order, and each length is that image's size.
pub fn markdown_to_document_with_images(
    markdown: &str,
    paths: &[String],
    bytes: &[u8],
    lengths: &[u32],
) -> Result<Vec<u8>, AdapterError> {
    if paths.len() != lengths.len() {
        return Err(AdapterError::new(
            "markdown.image_bytes",
            "Each image path needs one length. Pass the same number of paths and lengths.",
        ));
    }
    let mut images = Vec::new();
    let mut offset = 0usize;
    for (path, length) in paths.iter().zip(lengths) {
        let end = offset.saturating_add(*length as usize);
        let slice = bytes.get(offset..end).ok_or_else(|| {
            AdapterError::new(
                "markdown.image_bytes",
                "The image byte buffer is shorter than the lengths say. Pass one concatenated buffer and a length for each path.",
            )
        })?;
        images.push(edgedoc::MarkdownImage::new(path, slice.to_vec()));
        offset = end;
    }
    let options = edgedoc::MarkdownReadOptions::new().images(images);
    let doc = Document::from_markdown_with(markdown, &options).map_err(error)?;
    doc.write_bytes().map_err(error)
}

/// The body text of Markdown, so a caller can check a read without unpacking a document.
pub fn markdown_text(markdown: &str) -> Result<String, AdapterError> {
    let doc = Document::from_markdown(markdown).map_err(error)?;
    Ok(doc.plain_text(StoryRef::Body))
}

pub fn read_plain(bytes: &[u8]) -> Result<String, String> {
    let doc = Document::read_bytes(bytes).map_err(|err| format!("{} {}", err.code, err.message))?;
    Ok(doc.plain_text(StoryRef::Body))
}

/// The body text of one record, with the fields replaced. The template does not change.
pub fn preview_text(template: &[u8], csv: &[u8], missing: &str) -> Result<String, AdapterError> {
    let doc = Document::read_bytes(template).map_err(error)?;
    let records = Records::from_csv(csv, &CsvOptions::default())
        .map_err(|err| AdapterError::new(err.code, err.message))?;
    let preview = doc
        .preview_with(&records, 0, missing_field(missing)?)
        .map_err(error)?;
    Ok(preview.plain_text(StoryRef::Body))
}

/// The body text, or the text of one story. `story` is a name such as `"body"`.
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

/// The merge field names in body, headers, and footers, in order.
pub fn merge_fields(bytes: &[u8]) -> Result<Vec<String>, AdapterError> {
    let doc = Document::read_bytes(bytes).map_err(error)?;
    Ok(doc.merge_field_names())
}

/// Merge every record and join the results.
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
    batch
        .join(join_mode(join)?)
        .map_err(error)?
        .write_bytes()
        .map_err(error)
}

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

/// A small summary as `key=value` lines, so the caller needs no JSON parser.
pub fn summary(bytes: &[u8]) -> Result<String, AdapterError> {
    let doc = Document::read_bytes(bytes).map_err(error)?;
    Ok(format!(
        "title={}\ncreator={}\nflavor={:?}\nparts={}\nmerge_fields={}\nbody_blocks={}\nsection_breaks={}\npictures={}",
        doc.core_props().title,
        doc.core_props().creator,
        doc.flavor(),
        doc.part_names().len(),
        doc.merge_field_names().len(),
        doc.body_len(),
        doc.section_break_count(),
        doc.pictures().len()
    ))
}

/// Write the package in another flavour.
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

#[cfg(feature = "wasm")]
mod bind {
    use wasm_bindgen::prelude::*;

    fn message(err: super::AdapterError) -> JsValue {
        JsValue::from(err.render())
    }

    #[wasm_bindgen(js_name = readDocument)]
    pub fn read_document(bytes: &[u8]) -> Result<String, JsValue> {
        super::read_plain(bytes).map_err(JsValue::from)
    }

    #[wasm_bindgen(js_name = writeDocument)]
    pub fn write_document(bytes: &[u8]) -> Result<Vec<u8>, JsValue> {
        edgedoc::Document::read_bytes(bytes)
            .and_then(|doc| doc.write_bytes())
            .map_err(|err| JsValue::from(format!("{} {}", err.code, err.message)))
    }

    #[wasm_bindgen]
    pub fn merge(template: &[u8], csv: &[u8]) -> Result<Vec<u8>, JsValue> {
        super::merge_csv_bytes(template, csv).map_err(JsValue::from)
    }

    /// Merge every record with a kind, a join mode, and a missing-field mode.
    #[wasm_bindgen(js_name = mergeRecords)]
    pub fn merge_records(
        template: &[u8],
        csv: &[u8],
        kind: &str,
        join: &str,
        missing: &str,
    ) -> Result<Vec<u8>, JsValue> {
        super::merge_all(template, csv, kind, join, missing).map_err(message)
    }

    /// Merge a **dataset**: every parent with its child rows (specs/144 DOC2-03).
    ///
    /// `dataset_json` is `{ "records": [{…}], "repeats": { "Alias": [[{…}]] } }` and
    /// `options_json` is `{ kind, join, missing, budget, from, to }`. The host passes a budget that
    /// is tighter than the engine's own ceiling (DOC2-15).
    ///
    /// Returns one package: the joined document when `join` is `auto` or `page-break`, the catalogue
    /// shape for `directory`/`labels`, and the first document when `join` is `none` and there is one
    /// parent. A host that files one document per parent calls this once per parent with `from`/`to`.
    #[wasm_bindgen(js_name = mergeDataset)]
    pub fn merge_dataset(
        template: &[u8],
        dataset_json: &str,
        options_json: &str,
    ) -> Result<Vec<u8>, JsValue> {
        super::merge_dataset_bytes(template, dataset_json, options_json).map_err(JsValue::from)
    }

    /// The plain text of the first merged document of a dataset. A check, never a file.
    #[wasm_bindgen(js_name = previewDataset)]
    pub fn preview_dataset(
        template: &[u8],
        dataset_json: &str,
        options_json: &str,
    ) -> Result<String, JsValue> {
        super::preview_dataset_text(template, dataset_json, options_json).map_err(JsValue::from)
    }

    /// Build a package from a closed block description (specs/144 DOC2-03).
    ///
    /// Markdown cannot carry a merge field, so a host that installs a template needs this: paragraphs
    /// of text and one field, and tables whose data row is fields.
    #[wasm_bindgen(js_name = documentFromBlocks)]
    pub fn document_from_blocks(json: &str) -> Result<Vec<u8>, JsValue> {
        super::document_from_blocks(json).map_err(message)
    }

    /// Every merge field with the story, table, and row it sits in. JSON text.
    ///
    /// specs/144 DOC2-02: activate uses this to prove a repeat band's fields share one table row.
    #[wasm_bindgen(js_name = mergeFieldIndex)]
    pub fn merge_field_index(template: &[u8]) -> Result<String, JsValue> {
        super::field_index_of(template).map_err(JsValue::from)
    }

    /// Every zip entry name. A host checks the ceiling before it merges.
    #[wasm_bindgen(js_name = partNames)]
    pub fn part_names(template: &[u8]) -> Result<Vec<String>, JsValue> {
        super::part_names_of(template).map_err(JsValue::from)
    }

    /// The body text, or the text of one story.
    #[wasm_bindgen(js_name = textOf)]
    pub fn text_of(bytes: &[u8], story: &str) -> Result<String, JsValue> {
        super::text_of(bytes, story).map_err(message)
    }

    #[wasm_bindgen(js_name = mergeFieldNames)]
    pub fn merge_field_names(bytes: &[u8]) -> Result<Vec<String>, JsValue> {
        super::merge_fields(bytes).map_err(message)
    }

    /// A summary as `key=value` lines.
    #[wasm_bindgen(js_name = documentSummary)]
    pub fn document_summary(bytes: &[u8]) -> Result<String, JsValue> {
        super::summary(bytes).map_err(message)
    }

    /// Write the package in another flavour.
    #[wasm_bindgen(js_name = writeAs)]
    pub fn write_as_flavor(bytes: &[u8], flavor: &str) -> Result<Vec<u8>, JsValue> {
        super::write_as(bytes, flavor).map_err(message)
    }

    /// The plain-text preview of one record. The template is not changed.
    ///
    /// `missing` is `"error"`, `"empty"`, or `"placeholder"`. When it is omitted, a
    /// field with no column is an error.
    #[wasm_bindgen]
    pub fn preview(
        template: &[u8],
        csv: &[u8],
        missing: Option<String>,
    ) -> Result<String, JsValue> {
        super::preview_text(template, csv, missing.as_deref().unwrap_or("error")).map_err(message)
    }

    /// True when the bytes hold a package that a password protects.
    #[wasm_bindgen(js_name = isPasswordProtected)]
    pub fn is_password_protected(bytes: &[u8]) -> bool {
        edgedoc::Document::is_password_protected(bytes)
    }

    /// Read Markdown and return a `.docx` package.
    #[wasm_bindgen(js_name = markdownToDocument)]
    pub fn markdown_to_document(markdown: &str) -> Result<Vec<u8>, JsValue> {
        super::markdown_to_document(markdown).map_err(message)
    }

    /// Read Markdown. Image bytes are one buffer, split by `lengths` in path order.
    #[wasm_bindgen(js_name = markdownToDocumentWithImages)]
    pub fn markdown_to_document_with_images(
        markdown: &str,
        paths: Vec<String>,
        bytes: &[u8],
        lengths: Vec<u32>,
    ) -> Result<Vec<u8>, JsValue> {
        super::markdown_to_document_with_images(markdown, &paths, bytes, &lengths).map_err(message)
    }
}
