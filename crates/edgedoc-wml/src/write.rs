use std::sync::Arc;

use edgedoc_model::{
    Block, Borders, CharProps, FormField, FormKind, InlineObject, ListKind, ModelDocument,
    ParaProps, Paragraph, PartKind, Picture, SectionProps, SectionStart, Style, StyleSheet,
    PAGE_BREAK,
};
use edgedoc_opc::escape;

use crate::error::WmlResult;

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const RELS: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const CT: &str = "http://schemas.openxmlformats.org/package/2006/content-types";
const WP: &str = "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing";
const A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const PIC: &str = "http://schemas.openxmlformats.org/drawingml/2006/picture";
const MC: &str = "http://schemas.openxmlformats.org/markup-compatibility/2006";

/// The namespace declarations that every story part root carries.
///
/// A header, a footer, a note, or a comment can hold a drawing, so each one needs the
/// drawing namespaces. Without them Word reports a corrupt file.
const NS: &str = " xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" xmlns:wp=\"http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing\" xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:pic=\"http://schemas.openxmlformats.org/drawingml/2006/picture\" xmlns:mc=\"http://schemas.openxmlformats.org/markup-compatibility/2006\"";

const REL_STYLES: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles";
const REL_SETTINGS: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings";
const REL_NUMBERING: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering";
const REL_FOOTNOTES: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/footnotes";
const REL_ENDNOTES: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/endnotes";
const REL_COMMENTS: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments";
const REL_IMAGE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image";
const REL_HYPERLINK: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink";

pub struct Written {
    pub files: Vec<(String, Vec<u8>)>,
}

struct Rels {
    items: Vec<(String, String, String, bool)>,
}

impl Rels {
    fn new() -> Self {
        Self { items: Vec::new() }
    }

    fn add(&mut self, rel_type: &str, target: &str, external: bool) -> String {
        let id = format!("rId{}", self.items.len() + 1);
        self.items
            .push((id.clone(), rel_type.into(), target.into(), external));
        id
    }

    fn add_id(&mut self, id: &str, rel_type: &str, target: &str, external: bool) {
        if self.items.iter().any(|(have, _, _, _)| have == id) {
            return;
        }
        self.items
            .push((id.to_string(), rel_type.into(), target.into(), external));
    }

    /// Reuse the relationship that already points at this target, so one part has one id.
    fn add_or_reuse(&mut self, rel_type: &str, target: &str, external: bool) -> String {
        if let Some((id, _, _, _)) = self
            .items
            .iter()
            .find(|(_, kind, have, ext)| kind == rel_type && have == target && *ext == external)
        {
            return id.clone();
        }
        self.add(rel_type, target, external)
    }

    fn xml(&self) -> String {
        let mut out = format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"{RELS}\">");
        for (id, kind, target, external) in &self.items {
            let mode = if *external {
                " TargetMode=\"External\""
            } else {
                ""
            };
            out.push_str(&format!(
                "<Relationship Id=\"{}\" Type=\"{}\" Target=\"{}\"{mode}/>",
                escape(id),
                escape(kind),
                escape(target)
            ));
        }
        out.push_str("</Relationships>");
        out
    }
}

/// A numbering definition the writer builds from `Paragraph::list`.
struct Numbering {
    /// `numId` to use for each `(kind, level)` pair.
    ids: Vec<((ListKind, u32), u32)>,
    /// The first free id, above any id that the kept part already uses.
    next_num: u32,
    next_abstract: u32,
}

impl Numbering {
    fn new() -> Self {
        Self {
            ids: Vec::new(),
            next_num: 1,
            next_abstract: 1,
        }
    }

    /// Continue above the ids of a kept numbering part, so no definition is shared.
    fn from_existing(existing: Option<&str>) -> Self {
        let mut numbering = Self::new();
        if let Some(xml) = existing {
            numbering.next_num = highest_id(xml, "w:num w:numId=\"") + 1;
            numbering.next_abstract = highest_id(xml, "w:abstractNum w:abstractNumId=\"") + 1;
        }
        numbering
    }

    fn id_for(&mut self, kind: ListKind, level: u32) -> u32 {
        if let Some((_, id)) = self
            .ids
            .iter()
            .find(|((have, have_level), _)| *have == kind && *have_level == level)
        {
            return *id;
        }
        let id = self.next_num + self.ids.len() as u32;
        self.ids.push(((kind, level), id));
        id
    }

    fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    fn xml(&self, existing: Option<&str>) -> String {
        let mut out = match existing {
            Some(bytes) => strip_tail(bytes, "</w:numbering>"),
            None => format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<w:numbering xmlns:w=\"{W}\">"),
        };
        for (index, ((kind, level), num_id)) in self.ids.iter().enumerate() {
            let abstract_id = self.next_abstract + index as u32;
            out.push_str(&format!(
                "<w:abstractNum w:abstractNumId=\"{abstract_id}\"><w:multiLevelType w:val=\"hybridMultilevel\"/>"
            ));
            for current in 0..=(*level).min(8) {
                let text = kind.level_text(current);
                out.push_str(&format!(
                    "<w:lvl w:ilvl=\"{current}\"><w:start w:val=\"1\"/><w:numFmt w:val=\"{}\"/><w:lvlText w:val=\"{}\"/><w:lvlJc w:val=\"left\"/><w:pPr><w:ind w:left=\"{}\" w:hanging=\"360\"/></w:pPr></w:lvl>",
                    kind.num_fmt(),
                    escape(&text),
                    (current + 1) * 720
                ));
            }
            out.push_str("</w:abstractNum>");
            out.push_str(&format!(
                "<w:num w:numId=\"{num_id}\"><w:abstractNumId w:val=\"{abstract_id}\"/></w:num>"
            ));
        }
        out.push_str("</w:numbering>");
        out
    }
}

/// The highest numeric id that a marker such as `w:num w:numId="` introduces.
fn highest_id(xml: &str, marker: &str) -> u32 {
    let mut highest = 0;
    let mut rest = xml;
    while let Some(pos) = rest.find(marker) {
        let after = &rest[pos + marker.len()..];
        let digits: String = after.chars().take_while(|ch| ch.is_ascii_digit()).collect();
        if let Ok(value) = digits.parse::<u32>() {
            highest = highest.max(value);
        }
        rest = &after[digits.len().max(1)..];
    }
    highest
}

fn strip_tail(xml: &str, tail: &str) -> String {
    match xml.rfind(tail) {
        Some(pos) => xml[..pos].to_string(),
        None => xml.to_string(),
    }
}

/// True when any paragraph in a story points at a numbering definition.
fn document_refers_to_list(doc: &ModelDocument) -> bool {
    fn walk(blocks: &[Arc<Block>]) -> bool {
        blocks.iter().any(|block| match block.as_ref() {
            Block::Paragraph(para) => para.props.num_id.is_some() || para.props.list.is_some(),
            Block::Table(table) => table
                .rows
                .iter()
                .any(|row| row.cells.iter().any(|cell| walk(&cell.blocks))),
            Block::Control(control) => walk(&control.blocks),
            Block::Alternate { choice, .. } => walk(choice),
            Block::Opaque(_) => false,
        })
    }
    walk(&doc.body) || doc.parts.values().any(|part| walk(&part.blocks))
}

pub fn write_document(
    doc: &ModelDocument,
    flavor: &str,
    kept: &[(String, Vec<u8>, Option<String>)],
) -> WmlResult<Written> {
    let mut doc_rels = Rels::new();
    doc_rels.add(REL_STYLES, "styles.xml", false);
    doc_rels.add(REL_SETTINGS, "settings.xml", false);
    let mut media = Vec::new();
    let mut header_files = Vec::new();
    let existing_numbering = doc.numbering_xml.as_deref().or_else(|| {
        kept.iter()
            .find(|(name, _, _)| name.eq_ignore_ascii_case("word/numbering.xml"))
            .map(|(_, bytes, _)| std::str::from_utf8(bytes).unwrap_or(""))
    });
    let mut numbering = Numbering::from_existing(existing_numbering);
    let body = write_body(
        doc,
        &mut doc_rels,
        &mut media,
        &mut header_files,
        &mut numbering,
    )?;
    // A document that already holds a numbering part keeps it. So does a document whose
    // body refers to a list, even when this save builds no new definition.
    let body_lists = document_refers_to_list(doc);
    let keep_numbering = existing_numbering.is_some_and(|xml| !xml.trim().is_empty()) || body_lists;
    if !numbering.is_empty() || keep_numbering {
        doc_rels.add_or_reuse(REL_NUMBERING, "numbering.xml", false);
    }
    if !doc.parts_of(PartKind::Footnote).is_empty() {
        doc_rels.add(REL_FOOTNOTES, "footnotes.xml", false);
    }
    if !doc.parts_of(PartKind::Endnote).is_empty() {
        doc_rels.add(REL_ENDNOTES, "endnotes.xml", false);
    }
    if !doc.parts_of(PartKind::Comment).is_empty() {
        doc_rels.add(REL_COMMENTS, "comments.xml", false);
    }
    retain_relationships(&mut doc_rels, kept, &collect_opaque_ids(doc));
    let document = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<w:document xmlns:w=\"{W}\" xmlns:r=\"{R}\" xmlns:wp=\"{WP}\" xmlns:a=\"{A}\" xmlns:pic=\"{PIC}\" xmlns:mc=\"{MC}\"><w:body>{body}</w:body></w:document>"
    );
    let mut files = vec![
        ("word/document.xml".into(), document.into_bytes()),
        (
            "word/styles.xml".into(),
            styles_xml(&doc.styles).into_bytes(),
        ),
        (
            "word/settings.xml".into(),
            doc.settings_xml
                .clone()
                .map(String::into_bytes)
                .unwrap_or_else(|| kept_or(kept, "word/settings.xml", settings_xml()).into_bytes()),
        ),
        (
            "word/_rels/document.xml.rels".into(),
            doc_rels.xml().into_bytes(),
        ),
        ("docProps/core.xml".into(), core_xml(doc).into_bytes()),
        ("docProps/app.xml".into(), app_xml().into_bytes()),
    ];
    if !numbering.is_empty() || keep_numbering {
        files.push((
            "word/numbering.xml".into(),
            numbering.xml(existing_numbering).into_bytes(),
        ));
    }
    for (name, bytes) in &media {
        files.push((name.clone(), bytes.clone()));
    }
    for (name, bytes) in header_files {
        files.push((name, bytes));
    }
    for (name, bytes) in note_parts(doc, "footnotes", "footnote", PartKind::Footnote)? {
        files.push((name, bytes));
    }
    for (name, bytes) in note_parts(doc, "endnotes", "endnote", PartKind::Endnote)? {
        files.push((name, bytes));
    }
    for (name, bytes) in comment_parts(doc)? {
        files.push((name, bytes));
    }
    let mut pkg_rels = Rels::new();
    pkg_rels.add(edgedoc_opc::OFFICE_DOCUMENT, "word/document.xml", false);
    pkg_rels.add(edgedoc_opc::CORE_PROPS, "docProps/core.xml", false);
    files.push(("_rels/.rels".into(), pkg_rels.xml().into_bytes()));
    let content_types = content_types_xml(flavor, &files, kept);
    files.push(("[Content_Types].xml".into(), content_types.into_bytes()));
    for (name, bytes, _) in kept {
        let key = name.to_ascii_lowercase();
        if files
            .iter()
            .any(|(have, _)| have.eq_ignore_ascii_case(&key) || have.eq_ignore_ascii_case(name))
        {
            continue;
        }
        if key == "word/document.xml"
            || key == "[content_types].xml"
            || key == "_rels/.rels"
            || key.starts_with("word/_rels/")
        {
            continue;
        }
        files.push((name.clone(), bytes.clone()));
    }
    Ok(Written { files })
}

fn kept_or(kept: &[(String, Vec<u8>, Option<String>)], name: &str, fallback: String) -> String {
    kept.iter()
        .find(|(have, _, _)| have.eq_ignore_ascii_case(name))
        .map(|(_, bytes, _)| String::from_utf8_lossy(bytes).into_owned())
        .unwrap_or(fallback)
}

fn retain_relationships(
    rels: &mut Rels,
    kept: &[(String, Vec<u8>, Option<String>)],
    opaque_ids: &[String],
) {
    let Some((_, bytes, _)) = kept
        .iter()
        .find(|(name, _, _)| name.eq_ignore_ascii_case("word/_rels/document.xml.rels"))
    else {
        return;
    };
    let Ok(root) = edgedoc_opc::xml::parse(bytes, &edgedoc_limit::Budget::default()) else {
        return;
    };
    for rel in edgedoc_opc::rels_from_el(&root) {
        // A kept drawing points at its original relationship, so that id must stay.
        let kept_image =
            rel.rel_type.ends_with("/image") && opaque_ids.iter().any(|id| *id == rel.id);
        let rebuilt = [
            REL_STYLES,
            REL_SETTINGS,
            REL_FOOTNOTES,
            REL_ENDNOTES,
            REL_COMMENTS,
            REL_HYPERLINK,
        ]
        .iter()
        .any(|kind| {
            rel.rel_type.ends_with(kind.trim_start_matches(
                "http://schemas.openxmlformats.org/officeDocument/2006/relationships",
            ))
        }) || (rel.rel_type.ends_with("/image") && !kept_image);
        if rebuilt
            || rels
                .items
                .iter()
                .any(|(_, _, target, _)| target == &rel.target)
        {
            continue;
        }
        rels.add_id(&rel.id, &rel.rel_type, &rel.target, rel.external);
    }
}

/// Every `r:embed` and `r:id` value that kept markup still points at.
fn collect_opaque_ids(doc: &ModelDocument) -> Vec<String> {
    let mut ids = Vec::new();
    fn scan(xml: &str, ids: &mut Vec<String>) {
        for marker in ["r:embed=\"", "r:id=\"", "r:link=\""] {
            let mut rest = xml;
            while let Some(pos) = rest.find(marker) {
                let after = &rest[pos + marker.len()..];
                match after.find('"') {
                    Some(end) => {
                        ids.push(after[..end].to_string());
                        rest = &after[end..];
                    }
                    None => break,
                }
            }
        }
    }
    fn walk(blocks: &[Arc<Block>], ids: &mut Vec<String>) {
        for block in blocks {
            match block.as_ref() {
                Block::Paragraph(para) => {
                    for object in &para.objects {
                        if let InlineObject::Opaque(xml) = object {
                            scan(xml, ids);
                        }
                    }
                }
                Block::Table(table) => {
                    for row in &table.rows {
                        for cell in &row.cells {
                            walk(&cell.blocks, ids);
                        }
                    }
                }
                Block::Control(control) => walk(&control.blocks, ids),
                Block::Alternate {
                    choice,
                    fallback_xml,
                    ..
                } => {
                    walk(choice, ids);
                    scan(fallback_xml, ids);
                }
                Block::Opaque(xml) => scan(xml, ids),
            }
        }
    }
    walk(&doc.body, &mut ids);
    for part in doc.parts.values() {
        walk(&part.blocks, &mut ids);
    }
    ids
}

fn write_body(
    doc: &ModelDocument,
    rels: &mut Rels,
    media: &mut Vec<(String, Vec<u8>)>,
    headers: &mut Vec<(String, Vec<u8>)>,
    numbering: &mut Numbering,
) -> WmlResult<String> {
    let mut out = String::new();
    if let Some(before) = doc.last_section.extra.iter().find(|_| false) {
        let _ = before;
    }
    write_blocks(&doc.body, &mut out, rels, media, headers, numbering, doc)?;
    out.push_str(&sect_pr(
        &doc.last_section,
        rels,
        headers,
        doc,
        numbering,
        media,
    ));
    Ok(out)
}

#[allow(clippy::too_many_arguments)]
fn write_blocks(
    blocks: &[Arc<Block>],
    out: &mut String,
    rels: &mut Rels,
    media: &mut Vec<(String, Vec<u8>)>,
    headers: &mut Vec<(String, Vec<u8>)>,
    numbering: &mut Numbering,
    doc: &ModelDocument,
) -> WmlResult<()> {
    for block in blocks {
        match block.as_ref() {
            Block::Paragraph(para) => {
                out.push_str(&paragraph_xml(para, rels, media, headers, numbering, doc))
            }
            Block::Table(table) => {
                out.push_str(&table_xml(table, rels, media, headers, numbering, doc)?)
            }
            Block::Opaque(xml) => out.push_str(xml),
            Block::Alternate {
                requires,
                choice,
                fallback_xml,
            } => {
                out.push_str(&format!(
                    "<mc:AlternateContent><mc:Choice Requires=\"{}\">",
                    escape(requires)
                ));
                write_blocks(choice, out, rels, media, headers, numbering, doc)?;
                out.push_str("</mc:Choice><mc:Fallback>");
                if fallback_xml.is_empty() {
                    write_blocks(choice, out, rels, media, headers, numbering, doc)?;
                } else {
                    out.push_str(fallback_xml);
                }
                out.push_str("</mc:Fallback></mc:AlternateContent>");
            }
            Block::Control(control) => {
                out.push_str("<w:sdt><w:sdtPr>");
                if !control.title.is_empty() {
                    out.push_str(&format!("<w:alias w:val=\"{}\"/>", escape(&control.title)));
                }
                if !control.tag.is_empty() {
                    out.push_str(&format!("<w:tag w:val=\"{}\"/>", escape(&control.tag)));
                }
                if control.lock_content {
                    out.push_str("<w:lock w:val=\"sdtContentLocked\"/>");
                }
                match control.kind.as_str() {
                    "text" => out.push_str("<w:text/>"),
                    "date" => {
                        out.push_str("<w:date");
                        if let Some(format) = &control.date_format {
                            out.push_str(&format!(
                                " w:fullDate=\"{}\" w:format=\"{}\"",
                                escape(format),
                                escape(format)
                            ));
                        }
                        out.push_str("/>");
                    }
                    "dropDownList" | "comboBox" => {
                        out.push_str(&format!("<w:{}>", control.kind));
                        for item in &control.items {
                            out.push_str(&format!(
                                "<w:listItem w:displayText=\"{}\" w:value=\"{}\"/>",
                                escape(&item.display),
                                escape(&item.value)
                            ));
                        }
                        out.push_str(&format!("</w:{}>", control.kind));
                    }
                    "checkbox" => {
                        out.push_str("<w:checkbox>");
                        out.push_str("<w:checked w:val=\"");
                        out.push_str(if control.checked.unwrap_or(false) {
                            "1"
                        } else {
                            "0"
                        });
                        out.push_str("\"/>");
                        out.push_str("</w:checkbox>");
                    }
                    "picture" => out.push_str("<w:picture/>"),
                    _ => {}
                }
                if !control.placeholder.is_empty() {
                    out.push_str(&format!(
                        "<w:placeholder><w:docPart w:val=\"{}\"/></w:placeholder>",
                        escape(&control.placeholder)
                    ));
                }
                if let Some(binding) = &control.binding_xml {
                    out.push_str(binding);
                }
                out.push_str("</w:sdtPr><w:sdtContent>");
                write_blocks(&control.blocks, out, rels, media, headers, numbering, doc)?;
                out.push_str("</w:sdtContent></w:sdt>");
            }
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn paragraph_xml(
    para: &Paragraph,
    rels: &mut Rels,
    media: &mut Vec<(String, Vec<u8>)>,
    headers: &mut Vec<(String, Vec<u8>)>,
    numbering: &mut Numbering,
    doc: &ModelDocument,
) -> String {
    if let Some(raw) = &para.raw_xml {
        return raw.clone();
    }
    let mut out = String::from("<w:p>");
    out.push_str(&ppr(para, rels, media, headers, numbering, doc));
    let mut object_at = 0usize;
    let mut cursor = 0usize;
    for run in &para.runs {
        let end = (cursor + run.len).min(para.text.len());
        let text = &para.text[cursor..end];
        let piece = if text == "\u{FFFC}" {
            let object = para.objects.get(object_at);
            object_at += 1;
            object_xml(object, &run.props, rels, media)
        } else if text.contains(PAGE_BREAK) {
            let props = rpr(&run.props);
            let mut piece = String::new();
            let mut pieces = text.split(PAGE_BREAK).peekable();
            while let Some(part) = pieces.next() {
                if !part.is_empty() {
                    piece.push_str(&text_runs(part, &props));
                }
                if pieces.peek().is_some() {
                    piece.push_str(&format!("<w:r>{props}<w:br w:type=\"page\"/></w:r>"));
                }
            }
            piece
        } else {
            text_runs(text, &rpr(&run.props))
        };
        out.push_str(&wrap_link(&run.props.link, &piece, rels));
        cursor = end;
    }
    out.push_str("</w:p>");
    out
}

/// Split text at tab and newline characters into `w:tab` and `w:br` runs.
fn text_runs(text: &str, props: &str) -> String {
    if !text.contains('\t') && !text.contains('\n') {
        return format!(
            "<w:r>{props}<w:t xml:space=\"preserve\">{}</w:t></w:r>",
            escape(text)
        );
    }
    let mut out = String::new();
    let mut buffer = String::new();
    for ch in text.chars() {
        match ch {
            '\t' => {
                flush_text(&mut out, &mut buffer, props);
                out.push_str(&format!("<w:r>{props}<w:tab/></w:r>"));
            }
            '\n' => {
                flush_text(&mut out, &mut buffer, props);
                out.push_str(&format!("<w:r>{props}<w:br/></w:r>"));
            }
            other => buffer.push(other),
        }
    }
    flush_text(&mut out, &mut buffer, props);
    out
}

fn flush_text(out: &mut String, buffer: &mut String, props: &str) {
    if buffer.is_empty() {
        return;
    }
    out.push_str(&format!(
        "<w:r>{props}<w:t xml:space=\"preserve\">{}</w:t></w:r>",
        escape(buffer)
    ));
    buffer.clear();
}

fn object_xml(
    object: Option<&InlineObject>,
    props: &CharProps,
    rels: &mut Rels,
    media: &mut Vec<(String, Vec<u8>)>,
) -> String {
    match object {
        Some(InlineObject::Field(field)) => {
            // `w:dirty` and `w:fldLock` sit on the begin marker, as Word writes them.
            let mut begin = String::from("<w:fldChar w:fldCharType=\"begin\"");
            if field.dirty {
                begin.push_str(" w:dirty=\"true\"");
            }
            if field.locked {
                begin.push_str(" w:fldLock=\"true\"");
            }
            match &field.form {
                Some(form) => {
                    begin.push('>');
                    begin.push_str(&form_field_xml(form));
                    begin.push_str("</w:fldChar>");
                }
                None => begin.push_str("/>"),
            }
            format!(
                "<w:r>{}{begin}</w:r><w:r><w:instrText xml:space=\"preserve\">{}</w:instrText></w:r><w:r><w:fldChar w:fldCharType=\"separate\"/></w:r><w:r><w:t>{}</w:t></w:r><w:r><w:fldChar w:fldCharType=\"end\"/></w:r>",
                rpr(props),
                escape(&field.instr),
                escape(&field.result)
            )
        }
        Some(InlineObject::Orphan) => format!(
            "<w:r>{}<w:fldChar w:fldCharType=\"begin\"/></w:r>",
            rpr(props)
        ),
        Some(InlineObject::Opaque(xml)) => xml.clone(),
        Some(InlineObject::Picture(picture)) => {
            let id = add_image(rels, media, picture);
            let drawing = drawing_xml(picture, &id);
            format!("<w:r>{}{drawing}</w:r>", rpr(props))
        }
        Some(InlineObject::Hyperlink { target }) => {
            let id = rels.add(REL_HYPERLINK, target, true);
            format!("<w:hyperlink r:id=\"{id}\"/>")
        }
        Some(InlineObject::Bookmark { name, start }) => {
            if *start {
                format!("<w:bookmarkStart w:id=\"0\" w:name=\"{}\"/>", escape(name))
            } else {
                "<w:bookmarkEnd w:id=\"0\"/>".into()
            }
        }
        Some(InlineObject::Note { kind, id }) => {
            let element = match kind.as_str() {
                "footnote" => "footnoteReference",
                "endnote" => "endnoteReference",
                _ => "commentReference",
            };
            format!(
                "<w:r>{}{}</w:r>",
                rpr(props),
                format_args!("<w:{element} w:id=\"{id}\"/>")
            )
        }
        None => String::new(),
    }
}

/// The `w:ffData` element of a legacy form field, with its settings and help text.
fn form_field_xml(form: &FormField) -> String {
    let mut out = String::from("<w:ffData>");
    if !form.name.is_empty() {
        out.push_str(&format!("<w:name w:val=\"{}\"/>", escape(&form.name)));
    }
    if form.enabled {
        out.push_str("<w:enabled/>");
    }
    if form.calc_on_exit {
        out.push_str("<w:calcOnExit/>");
    }
    match &form.kind {
        FormKind::Text {
            kind,
            default,
            max_length,
            format,
        } => {
            out.push_str("<w:textInput");
            if let Some(kind) = kind {
                out.push_str(&format!(" w:type=\"{}\"", escape(kind)));
            }
            if kind.is_none() && default.is_none() && max_length.is_none() && format.is_none() {
                out.push_str("/>");
            } else {
                out.push('>');
                if let Some(default) = default {
                    out.push_str(&format!("<w:default w:val=\"{}\"/>", escape(default)));
                }
                if let Some(max) = max_length {
                    out.push_str(&format!("<w:maxLength w:val=\"{max}\"/>"));
                }
                if let Some(format) = format {
                    out.push_str(&format!("<w:format w:val=\"{}\"/>", escape(format)));
                }
                out.push_str("</w:textInput>");
            }
        }
        FormKind::CheckBox {
            size,
            size_auto,
            default,
            checked,
        } => {
            out.push_str("<w:checkBox>");
            if let Some(size) = size {
                out.push_str(&format!("<w:size w:val=\"{size}\"/>"));
            }
            if *size_auto {
                out.push_str("<w:sizeAuto/>");
            }
            if let Some(default) = default {
                out.push_str(&format!(
                    "<w:default w:val=\"{}\"/>",
                    if *default { "1" } else { "0" }
                ));
            }
            if let Some(checked) = checked {
                out.push_str(&format!(
                    "<w:checked w:val=\"{}\"/>",
                    if *checked { "1" } else { "0" }
                ));
            }
            out.push_str("</w:checkBox>");
        }
        FormKind::DropDown { result, entries } => {
            out.push_str("<w:ddList>");
            if let Some(result) = result {
                out.push_str(&format!("<w:result w:val=\"{result}\"/>"));
            }
            for entry in entries {
                out.push_str(&format!("<w:listEntry w:val=\"{}\"/>", escape(entry)));
            }
            out.push_str("</w:ddList>");
        }
    }
    if let Some(help) = &form.help {
        out.push_str(&format!(
            "<w:helpText w:val=\"{}\" w:type=\"text\"/>",
            escape(help)
        ));
    }
    if let Some(status) = &form.status {
        out.push_str(&format!(
            "<w:statusText w:val=\"{}\" w:type=\"text\"/>",
            escape(status)
        ));
    }
    out.push_str("</w:ffData>");
    out
}

fn add_image(rels: &mut Rels, media: &mut Vec<(String, Vec<u8>)>, picture: &Picture) -> String {
    // Identical bytes become one part. A merge copies the same logo into several
    // stories, and a package repeats a part only when the bytes differ.
    if let Some((name, _)) = media.iter().find(|(_, bytes)| *bytes == picture.bytes) {
        let target = name.trim_start_matches("word/").to_string();
        return rels.add_or_reuse(REL_IMAGE, &target, false);
    }
    let ext = extension(&picture.content_type);
    // The id must be unique even when a header part has its own media list.
    let mut counter = media.len() + 1;
    let mut file = format!("media/img{counter}.{ext}");
    while media
        .iter()
        .any(|(name, _)| name.eq_ignore_ascii_case(&format!("word/{file}")))
    {
        counter += 1;
        file = format!("media/img{counter}.{ext}");
    }
    let id = rels.add(REL_IMAGE, &file, false);
    media.push((format!("word/{file}"), picture.bytes.clone()));
    id
}

fn extension(content_type: &str) -> &'static str {
    if content_type.contains("jpeg") || content_type.contains("jpg") {
        "jpeg"
    } else if content_type.contains("gif") {
        "gif"
    } else if content_type.contains("bmp") {
        "bmp"
    } else if content_type.contains("svg") {
        "svg"
    } else if content_type.contains("tiff") {
        "tiff"
    } else if content_type.contains("webp") {
        "webp"
    } else if content_type.contains("emf") {
        "emf"
    } else if content_type.contains("wmf") {
        "wmf"
    } else {
        "png"
    }
}

fn drawing_xml(picture: &Picture, id: &str) -> String {
    let width = picture.width.map(|emu| emu.0).unwrap_or(914_400);
    let height = picture.height.map(|emu| emu.0).unwrap_or(914_400);
    let name = escape(picture.name.rsplit('/').next().unwrap_or("image"));
    let descr = picture.alt.as_deref().map(escape).unwrap_or_default();
    let graphic = format!(
        "<a:graphic xmlns:a=\"{A}\"><a:graphicData uri=\"{PIC}\"><pic:pic xmlns:pic=\"{PIC}\"><pic:nvPicPr><pic:cNvPr id=\"0\" name=\"{name}\" descr=\"{descr}\"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip r:embed=\"{id}\"/></pic:blipFill><pic:spPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{width}\" cy=\"{height}\"/></a:xfrm><a:prstGeom prst=\"rect\"/></pic:spPr></pic:pic></a:graphicData></a:graphic>"
    );
    match &picture.anchor {
        None => format!("<w:drawing><wp:inline distT=\"0\" distB=\"0\" distL=\"0\" distR=\"0\"><wp:extent cx=\"{width}\" cy=\"{height}\"/>{graphic}</wp:inline></w:drawing>"),
        Some(anchor) => {
            let wrap = match anchor.wrap.as_deref() {
                Some(value) if value.starts_with("square") => {
                    let side = match value.split(':').nth(1).unwrap_or("bothSides") {
                        "left" => "left",
                        "right" => "right",
                        "largest" => "largest",
                        _ => "bothSides",
                    };
                    format!("<wp:wrapSquare wrapText=\"{}\"/>", escape(side))
                }
                Some("wrapTight") => "<wp:wrapTight wrapText=\"bothSides\"><wp:wrapPolygon><wp:start x=\"0\" y=\"0\"/><wp:lineTo x=\"0\" y=\"10000\"/><wp:lineTo x=\"10000\" y=\"10000\"/><wp:lineTo x=\"10000\" y=\"0\"/><wp:lineTo x=\"0\" y=\"0\"/></wp:wrapPolygon></wp:wrapTight>".into(),
                Some("wrapThrough") => "<wp:wrapThrough wrapText=\"bothSides\"><wp:wrapPolygon><wp:start x=\"0\" y=\"0\"/><wp:lineTo x=\"0\" y=\"10000\"/><wp:lineTo x=\"10000\" y=\"10000\"/><wp:lineTo x=\"10000\" y=\"0\"/><wp:lineTo x=\"0\" y=\"0\"/></wp:wrapPolygon></wp:wrapThrough>".into(),
                Some("wrapTopAndBottom") => "<wp:wrapTopAndBottom/>".into(),
                Some("wrapNone") => "<wp:wrapNone/>".into(),
                _ => "<wp:wrapSquare wrapText=\"bothSides\"/>".into(),
            };
            let horizontal = position_axis(anchor.h_align.as_deref(), anchor.x.map(|emu| emu.0), "H");
            let vertical = position_axis(anchor.v_align.as_deref(), anchor.y.map(|emu| emu.0), "V");
            format!(
                "<w:drawing><wp:anchor distT=\"{}\" distB=\"{}\" distL=\"{}\" distR=\"{}\" simplePos=\"0\" relativeHeight=\"0\" behindDoc=\"{}\" locked=\"0\" layoutInCell=\"1\" allowOverlap=\"1\"><wp:simplePos x=\"0\" y=\"0\"/>{horizontal}{vertical}<wp:extent cx=\"{width}\" cy=\"{height}\"/>{wrap}{graphic}</wp:anchor></w:drawing>",
                anchor.dist_top.map(|emu| emu.0).unwrap_or(0),
                anchor.dist_bottom.map(|emu| emu.0).unwrap_or(0),
                anchor.dist_left.map(|emu| emu.0).unwrap_or(0),
                anchor.dist_right.map(|emu| emu.0).unwrap_or(0),
                if anchor.behind_text { "1" } else { "0" }
            )
        }
    }
}

fn position_axis(align: Option<&str>, offset: Option<i64>, axis: &str) -> String {
    let relative = if axis == "H" { "column" } else { "paragraph" };
    let body = match (align, offset) {
        (Some(value), _) => format!("<wp:align>{}</wp:align>", escape(value)),
        (None, Some(value)) => format!("<wp:posOffset>{value}</wp:posOffset>"),
        (None, None) => "<wp:align>left</wp:align>".to_string(),
    };
    format!("<wp:position{axis} relativeFrom=\"{relative}\">{body}</wp:position{axis}>")
}

fn wrap_link(link: &Option<String>, piece: &str, rels: &mut Rels) -> String {
    let Some(link) = link else {
        return piece.to_string();
    };
    if let Some(anchor) = link.strip_prefix('#') {
        format!(
            "<w:hyperlink w:anchor=\"{}\">{piece}</w:hyperlink>",
            escape(anchor)
        )
    } else {
        let id = rels.add(REL_HYPERLINK, link, true);
        format!("<w:hyperlink r:id=\"{id}\">{piece}</w:hyperlink>")
    }
}

#[allow(clippy::too_many_arguments)]
fn ppr(
    para: &Paragraph,
    rels: &mut Rels,
    media: &mut Vec<(String, Vec<u8>)>,
    headers: &mut Vec<(String, Vec<u8>)>,
    numbering: &mut Numbering,
    doc: &ModelDocument,
) -> String {
    let props = &para.props;
    let mut inner = String::new();
    if let Some(style) = &props.style {
        inner.push_str(&format!("<w:pStyle w:val=\"{}\"/>", escape(style)));
    }
    if props.keep_next {
        inner.push_str("<w:keepNext/>");
    }
    if props.keep_lines {
        inner.push_str("<w:keepLines/>");
    }
    if props.page_break_before {
        inner.push_str("<w:pageBreakBefore/>");
    }
    if !props.widow_control {
        inner.push_str("<w:widowControl w:val=\"0\"/>");
    }
    if props.contextual_spacing {
        inner.push_str("<w:contextualSpacing/>");
    }
    if props.suppress_line_numbers {
        inner.push_str("<w:suppressLineNumbers/>");
    }
    write_borders(&props.borders, "pBdr", &mut inner);
    if let Some(fill) = &props.shading {
        inner.push_str(&format!(
            "<w:shd w:val=\"clear\" w:fill=\"{}\"/>",
            escape(fill)
        ));
    }
    if let Some(level) = props
        .num_id
        .or(if props.list.is_some() { Some(0) } else { None })
    {
        let (id, level) = match props.list {
            Some(spec) => (numbering.id_for(spec.kind, spec.level), spec.level),
            None => (level, props.ilvl.unwrap_or(0)),
        };
        inner.push_str(&format!(
            "<w:numPr><w:ilvl w:val=\"{level}\"/><w:numId w:val=\"{id}\"/></w:numPr>"
        ));
    } else if props.ilvl.is_some() {
        inner.push_str(&format!(
            "<w:numPr><w:ilvl w:val=\"{}\"/><w:numId w:val=\"0\"/></w:numPr>",
            props.ilvl.unwrap_or(0)
        ));
    }
    if !props.tabs.is_empty() {
        inner.push_str("<w:tabs>");
        for stop in &props.tabs {
            inner.push_str(&format!(
                "<w:tab w:val=\"{}\" w:pos=\"{}\"",
                escape(&stop.val),
                stop.pos.0
            ));
            if let Some(leader) = &stop.leader {
                inner.push_str(&format!(" w:leader=\"{}\"", escape(leader)));
            }
            inner.push_str("/>");
        }
        inner.push_str("</w:tabs>");
    }
    if props.indent_left.is_some()
        || props.indent_right.is_some()
        || props.indent_first.is_some()
        || props.indent_left_chars.is_some()
        || props.indent_right_chars.is_some()
        || props.indent_first_chars.is_some()
    {
        inner.push_str("<w:ind");
        if let Some(value) = props.indent_left {
            inner.push_str(&format!(" w:left=\"{}\"", value.0));
        }
        if let Some(value) = props.indent_right {
            inner.push_str(&format!(" w:right=\"{}\"", value.0));
        }
        if let Some(value) = props.indent_first {
            if value.0 < 0 {
                inner.push_str(&format!(" w:hanging=\"{}\"", -value.0));
            } else {
                inner.push_str(&format!(" w:firstLine=\"{}\"", value.0));
            }
        }
        if let Some(value) = props.indent_left_chars {
            inner.push_str(&format!(" w:leftChars=\"{value}\""));
        }
        if let Some(value) = props.indent_right_chars {
            inner.push_str(&format!(" w:rightChars=\"{value}\""));
        }
        if let Some(value) = props.indent_first_chars {
            if value < 0 {
                inner.push_str(&format!(" w:hangingChars=\"{}\"", -value));
            } else {
                inner.push_str(&format!(" w:firstLineChars=\"{value}\""));
            }
        }
        inner.push_str("/>");
    }
    if props.spacing_before.is_some() || props.spacing_after.is_some() || props.line.is_some() {
        inner.push_str("<w:spacing");
        if let Some(value) = props.spacing_before {
            inner.push_str(&format!(" w:before=\"{}\"", value.0));
        }
        if let Some(value) = props.spacing_after {
            inner.push_str(&format!(" w:after=\"{}\"", value.0));
        }
        if let Some(value) = props.line {
            inner.push_str(&format!(" w:line=\"{}\"", value.0));
        }
        if let Some(rule) = &props.line_rule {
            inner.push_str(&format!(" w:lineRule=\"{}\"", escape(rule)));
        }
        inner.push_str("/>");
    }
    if let Some(frame) = &props.frame {
        inner.push_str("<w:framePr");
        if let Some(value) = &frame.drop_cap {
            inner.push_str(&format!(" w:dropCap=\"{}\"", escape(value)));
        }
        if let Some(value) = frame.lines {
            inner.push_str(&format!(" w:lines=\"{value}\""));
        }
        if let Some(value) = &frame.wrap {
            inner.push_str(&format!(" w:wrap=\"{}\"", escape(value)));
        }
        if let Some(value) = frame.x {
            inner.push_str(&format!(" w:x=\"{}\"", value.0));
        }
        if let Some(value) = frame.y {
            inner.push_str(&format!(" w:y=\"{}\"", value.0));
        }
        if let Some(value) = &frame.h_anchor {
            inner.push_str(&format!(" w:hAnchor=\"{}\"", escape(value)));
        }
        if let Some(value) = &frame.v_anchor {
            inner.push_str(&format!(" w:vAnchor=\"{}\"", escape(value)));
        }
        if let Some(value) = frame.h_space {
            inner.push_str(&format!(" w:hSpace=\"{}\"", value.0));
        }
        if let Some(value) = frame.v_space {
            inner.push_str(&format!(" w:vSpace=\"{}\"", value.0));
        }
        if let Some(value) = frame.width {
            inner.push_str(&format!(" w:w=\"{}\"", value.0));
        }
        if let Some(value) = frame.height {
            inner.push_str(&format!(" w:h=\"{}\"", value.0));
        }
        inner.push_str("/>");
        for extra in &frame.extra {
            inner.push_str(extra);
        }
    }
    if let Some(align) = &props.align {
        inner.push_str(&format!("<w:jc w:val=\"{}\"/>", escape(align)));
    }
    if let Some(level) = props.outline {
        inner.push_str(&format!("<w:outlineLvl w:val=\"{level}\"/>"));
    }
    if props.bidi {
        inner.push_str("<w:bidi/>");
    }
    let mark = rpr(&props.mark);
    if !mark.is_empty() {
        inner.push_str(&mark);
    }
    for extra in &props.extra {
        inner.push_str(extra);
    }
    if let Some(section) = &para.section {
        // A section break in a paragraph carries its own headers and pictures, so it
        // writes into the same collections as the body.
        inner.push_str(&sect_pr(
            &section.props,
            rels,
            headers,
            doc,
            numbering,
            media,
        ));
    }
    if inner.is_empty() {
        String::new()
    } else {
        format!("<w:pPr>{inner}</w:pPr>")
    }
}

fn rpr(props: &CharProps) -> String {
    let mut inner = String::new();
    if props.style.is_some() {
        inner.push_str(&format!(
            "<w:rStyle w:val=\"{}\"/>",
            escape(props.style.as_deref().unwrap_or(""))
        ));
    }
    if props.font.is_some()
        || props.font_hansi.is_some()
        || props.font_cs.is_some()
        || props.font_ea.is_some()
    {
        inner.push_str("<w:rFonts");
        if let Some(font) = &props.font {
            inner.push_str(&format!(" w:ascii=\"{}\"", escape(font)));
        }
        if let Some(font) = props.font_hansi.as_ref().or(props.font.as_ref()) {
            inner.push_str(&format!(" w:hAnsi=\"{}\"", escape(font)));
        }
        if let Some(font) = &props.font_cs {
            inner.push_str(&format!(" w:cs=\"{}\"", escape(font)));
        }
        if let Some(font) = &props.font_ea {
            inner.push_str(&format!(" w:eastAsia=\"{}\"", escape(font)));
        }
        inner.push_str("/>");
    }
    if props.bold {
        inner.push_str("<w:b/>");
    }
    if let Some(value) = props.bold_cs {
        inner.push_str(if value {
            "<w:bCs/>"
        } else {
            "<w:bCs w:val=\"0\"/>"
        });
    }
    if props.italic {
        inner.push_str("<w:i/>");
    }
    if let Some(value) = props.italic_cs {
        inner.push_str(if value {
            "<w:iCs/>"
        } else {
            "<w:iCs w:val=\"0\"/>"
        });
    }
    if props.caps {
        inner.push_str("<w:caps/>");
    }
    if props.small_caps {
        inner.push_str("<w:smallCaps/>");
    }
    if props.strike {
        inner.push_str("<w:strike/>");
    }
    if props.double_strike {
        inner.push_str("<w:dstrike/>");
    }
    if props.outline {
        inner.push_str("<w:outline/>");
    }
    if props.shadow {
        inner.push_str("<w:shadow/>");
    }
    if props.emboss {
        inner.push_str("<w:emboss/>");
    }
    if props.imprint {
        inner.push_str("<w:imprint/>");
    }
    if props.no_proof {
        inner.push_str("<w:noProof/>");
    }
    if let Some(value) = props.snap_to_grid {
        inner.push_str(if value {
            "<w:snapToGrid/>"
        } else {
            "<w:snapToGrid w:val=\"0\"/>"
        });
    }
    if props.vanish {
        inner.push_str("<w:vanish/>");
    }
    if props.web_hidden {
        inner.push_str("<w:webHidden/>");
    }
    if let Some(color) = &props.color {
        inner.push_str(&format!("<w:color w:val=\"{}\"/>", escape(color)));
    }
    if let Some(spacing) = props.spacing {
        inner.push_str(&format!("<w:spacing w:val=\"{spacing}\"/>"));
    }
    if props.scale.is_some() {
        inner.push_str(&format!("<w:w w:val=\"{}\"/>", props.scale.unwrap_or(100)));
    }
    if let Some(kern) = props.kern {
        inner.push_str(&format!("<w:kern w:val=\"{kern}\"/>"));
    }
    if let Some(position) = props.position {
        inner.push_str(&format!("<w:position w:val=\"{position}\"/>"));
    }
    if let Some(size) = props.size_pt {
        inner.push_str(&format!(
            "<w:sz w:val=\"{}\"/>",
            (size * 2.0).round() as i32
        ));
    }
    if let Some(size) = props.size_cs {
        inner.push_str(&format!(
            "<w:szCs w:val=\"{}\"/>",
            (size * 2.0).round() as i32
        ));
    }
    if let Some(highlight) = &props.highlight {
        inner.push_str(&format!("<w:highlight w:val=\"{}\"/>", escape(highlight)));
    }
    if props.underline || props.underline_val.is_some() {
        let val = props.underline_val.as_deref().unwrap_or("single");
        inner.push_str(&format!("<w:u w:val=\"{}\"", escape(val)));
        if let Some(color) = &props.underline_color {
            inner.push_str(&format!(" w:color=\"{}\"", escape(color)));
        }
        inner.push_str("/>");
    }
    if let Some(fill) = &props.shading {
        inner.push_str(&format!(
            "<w:shd w:val=\"clear\" w:fill=\"{}\"/>",
            escape(fill)
        ));
    }
    if let Some(border) = &props.border {
        inner.push_str(&format!("<w:bdr w:val=\"{}\"", escape(&border.val)));
        if let Some(size) = border.size {
            inner.push_str(&format!(" w:sz=\"{size}\""));
        }
        if let Some(color) = &border.color {
            inner.push_str(&format!(" w:color=\"{}\"", escape(color)));
        }
        if let Some(space) = border.space {
            inner.push_str(&format!(" w:space=\"{space}\""));
        }
        inner.push_str("/>");
    }
    if let Some(layout) = &props.east_asian_layout {
        inner.push_str("<w:eastAsianLayout");
        if let Some(id) = layout.id {
            inner.push_str(&format!(" w:id=\"{id}\""));
        }
        if let Some(combine) = layout.combine {
            inner.push_str(if combine {
                " w:combine=\"1\""
            } else {
                " w:combine=\"0\""
            });
        }
        if let Some(brackets) = &layout.combine_brackets {
            inner.push_str(&format!(" w:combineBrackets=\"{}\"", escape(brackets)));
        }
        if let Some(vertical) = layout.vertical {
            inner.push_str(if vertical {
                " w:vert=\"1\""
            } else {
                " w:vert=\"0\""
            });
        }
        if let Some(compress) = layout.vertical_compress {
            inner.push_str(if compress {
                " w:vertCompress=\"1\""
            } else {
                " w:vertCompress=\"0\""
            });
        }
        inner.push_str("/>");
    }
    if let Some(width) = props.fit_text {
        inner.push_str(&format!("<w:fitText w:val=\"{}\"/>", width.0));
    }
    if let Some(align) = &props.vert_align {
        inner.push_str(&format!("<w:vertAlign w:val=\"{}\"/>", escape(align)));
    }
    if props.rtl {
        inner.push_str("<w:rtl/>");
    }
    if props.complex_script {
        inner.push_str("<w:cs/>");
    }
    if props.lang.is_some() || props.lang_bidi.is_some() || props.lang_ea.is_some() {
        inner.push_str("<w:lang");
        if let Some(lang) = &props.lang {
            inner.push_str(&format!(" w:val=\"{}\"", escape(lang)));
        }
        if let Some(lang) = &props.lang_bidi {
            inner.push_str(&format!(" w:bidi=\"{}\"", escape(lang)));
        }
        if let Some(lang) = &props.lang_ea {
            inner.push_str(&format!(" w:eastAsia=\"{}\"", escape(lang)));
        }
        inner.push_str("/>");
    }
    for extra in &props.extra {
        inner.push_str(extra);
    }
    if inner.is_empty() {
        String::new()
    } else {
        format!("<w:rPr>{inner}</w:rPr>")
    }
}

fn table_xml(
    table: &edgedoc_model::Table,
    rels: &mut Rels,
    media: &mut Vec<(String, Vec<u8>)>,
    headers: &mut Vec<(String, Vec<u8>)>,
    numbering: &mut Numbering,
    doc: &ModelDocument,
) -> WmlResult<String> {
    let mut out = String::from("<w:tbl><w:tblPr>");
    // ECMA-376 fixes the order: style, position, overlap, direction, width, then the rest.
    if let Some(style) = &table.style {
        out.push_str(&format!("<w:tblStyle w:val=\"{}\"/>", escape(style)));
    }
    if let Some(position) = &table.position {
        out.push_str("<w:tblpPr");
        for (name, value) in [
            ("leftFromText", position.left_from_text),
            ("rightFromText", position.right_from_text),
            ("topFromText", position.top_from_text),
            ("bottomFromText", position.bottom_from_text),
            ("tblpX", position.tblp_x),
            ("tblpY", position.tblp_y),
        ] {
            if let Some(value) = value {
                out.push_str(&format!(" w:{name}=\"{value}\""));
            }
        }
        for (name, value) in [
            ("leftFrom", position.left_from.as_deref()),
            ("topFrom", position.top_from.as_deref()),
            ("horzAnchor", position.h_anchor.as_deref()),
            ("vertAnchor", position.v_anchor.as_deref()),
        ] {
            if let Some(value) = value {
                out.push_str(&format!(" w:{name}=\"{}\"", escape(value)));
            }
        }
        out.push_str("/>");
    }
    if let Some(overlap) = &table.overlap {
        out.push_str(&format!("<w:tblOverlap w:val=\"{}\"/>", escape(overlap)));
    }
    match &table.width_kind {
        Some(spec) => {
            let (value, kind) = spec.split_once(':').unwrap_or((spec.as_str(), "dxa"));
            out.push_str(&format!(
                "<w:tblW w:w=\"{}\" w:type=\"{}\"/>",
                escape(value),
                escape(kind)
            ));
        }
        None => out.push_str("<w:tblW w:w=\"0\" w:type=\"auto\"/>"),
    }
    if table.bidi_visual {
        out.push_str("<w:bidiVisual/>");
    }
    if let Some(align) = &table.align {
        out.push_str(&format!("<w:jc w:val=\"{}\"/>", escape(align)));
    }
    if let Some(indent) = table.indent {
        out.push_str(&format!("<w:tblInd w:w=\"{}\" w:type=\"dxa\"/>", indent.0));
    }
    if let Some(layout) = &table.layout {
        out.push_str(&format!("<w:tblLayout w:type=\"{}\"/>", escape(layout)));
    }
    if let Some(space) = table.cell_space {
        out.push_str(&format!(
            "<w:tblCellSpacing w:w=\"{}\" w:type=\"dxa\"/>",
            space.0
        ));
    }
    write_borders(&table.borders, "tblBorders", &mut out);
    write_margins(&table.cell_margins, &mut out);
    if let Some(look) = &table.look {
        out.push_str(&format!("<w:tblLook w:val=\"{}\"/>", escape(look)));
    }
    for extra in &table.extra {
        if !extra.starts_with("<w:tblPrEx") {
            out.push_str(extra);
        }
    }
    out.push_str("</w:tblPr>");
    for extra in &table.extra {
        if extra.starts_with("<w:tblPrEx") {
            out.push_str(extra);
        }
    }
    out.push_str("<w:tblGrid>");
    for width in &table.widths {
        out.push_str(&format!("<w:gridCol w:w=\"{}\"/>", width.0));
    }
    out.push_str("</w:tblGrid>");
    for row in &table.rows {
        out.push_str("<w:tr>");
        if row.header
            || row.height.is_some()
            || row.cant_split
            || row.hidden
            || !row.extra.is_empty()
        {
            out.push_str("<w:trPr>");
            // The schema order is cantSplit, trHeight, tblHeader, hidden.
            if row.cant_split {
                out.push_str("<w:cantSplit/>");
            }
            if let Some(height) = row.height {
                let rule = row.height_rule.as_deref().unwrap_or("atLeast");
                out.push_str(&format!(
                    "<w:trHeight w:val=\"{}\" w:hRule=\"{}\"/>",
                    height.0,
                    escape(rule)
                ));
            }
            if row.header {
                out.push_str("<w:tblHeader/>");
            }
            if row.hidden {
                out.push_str("<w:hidden/>");
            }
            for extra in &row.extra {
                out.push_str(extra);
            }
            out.push_str("</w:trPr>");
        }
        for cell in &row.cells {
            out.push_str("<w:tc>");
            let has_props = cell.span > 1
                || cell.width.is_some()
                || cell.v_merge.is_some()
                || cell.h_merge.is_some()
                || cell.v_align.is_some()
                || cell.text_direction.is_some()
                || cell.shading.is_some()
                || cell.margins.top.is_some()
                || cell.margins.bottom.is_some()
                || cell.margins.left.is_some()
                || cell.margins.right.is_some()
                || !cell.borders.is_empty()
                || cell.fit_text
                || cell.no_wrap
                || !cell.extra.is_empty();
            if has_props {
                out.push_str("<w:tcPr>");
                if cell.span > 1 {
                    out.push_str(&format!("<w:gridSpan w:val=\"{}\"/>", cell.span));
                }
                if let Some(width) = cell.width {
                    out.push_str(&format!("<w:tcW w:w=\"{}\" w:type=\"dxa\"/>", width.0));
                }
                if let Some(merge) = &cell.v_merge {
                    if merge == "continue" {
                        out.push_str("<w:vMerge/>");
                    } else {
                        out.push_str(&format!("<w:vMerge w:val=\"{}\"/>", escape(merge)));
                    }
                }
                if let Some(merge) = &cell.h_merge {
                    if merge == "continue" {
                        out.push_str("<w:hMerge/>");
                    } else {
                        out.push_str(&format!("<w:hMerge w:val=\"{}\"/>", escape(merge)));
                    }
                }
                write_borders(&cell.borders, "tcBorders", &mut out);
                if let Some(fill) = &cell.shading {
                    out.push_str(&format!(
                        "<w:shd w:val=\"clear\" w:fill=\"{}\"/>",
                        escape(fill)
                    ));
                }
                if cell.no_wrap {
                    out.push_str("<w:noWrap/>");
                }
                write_cell_margins(&cell.margins, &mut out);
                if let Some(direction) = &cell.text_direction {
                    out.push_str(&format!(
                        "<w:textDirection w:val=\"{}\"/>",
                        escape(direction)
                    ));
                }
                if let Some(align) = &cell.v_align {
                    out.push_str(&format!("<w:vAlign w:val=\"{}\"/>", escape(align)));
                }
                if cell.fit_text {
                    out.push_str("<w:fitText w:val=\"1\"/>");
                }
                for extra in &cell.extra {
                    out.push_str(extra);
                }
                out.push_str("</w:tcPr>");
            }
            write_blocks(&cell.blocks, &mut out, rels, media, headers, numbering, doc)?;
            if cell.blocks.is_empty() {
                out.push_str("<w:p/>");
            }
            out.push_str("</w:tc>");
        }
        out.push_str("</w:tr>");
    }
    out.push_str("</w:tbl>");
    Ok(out)
}

fn sect_pr(
    props: &SectionProps,
    rels: &mut Rels,
    headers: &mut Vec<(String, Vec<u8>)>,
    doc: &ModelDocument,
    numbering: &mut Numbering,
    media: &mut Vec<(String, Vec<u8>)>,
) -> String {
    let mut out = String::from("<w:sectPr>");
    let mut refs = String::new();
    let story_refs = [
        (&props.headers.default, "header", "default"),
        (&props.headers.first, "header", "first"),
        (&props.headers.even, "header", "even"),
        (&props.footers.default, "footer", "default"),
        (&props.footers.first, "footer", "first"),
        (&props.footers.even, "footer", "even"),
    ];
    for (id, kind, which) in story_refs {
        if let Some(markup) = story_ref(id, kind, which, rels, headers, doc, numbering, media) {
            refs.push_str(&markup);
        }
    }
    // ECMA-376 gives `w:sectPr` a fixed child order. The references come first,
    // then the section type, then the page setup. Word rejects another order.
    out.push_str(&refs);
    write_note_pr(&props.footnotes, "footnotePr", &mut out);
    write_note_pr(&props.endnotes, "endnotePr", &mut out);
    let start = match props.start {
        SectionStart::Continuous => "continuous",
        SectionStart::EvenPage => "evenPage",
        SectionStart::OddPage => "oddPage",
        SectionStart::NextPage => "nextPage",
    };
    out.push_str(&format!("<w:type w:val=\"{start}\"/>"));
    out.push_str(&format!(
        "<w:pgSz w:w=\"{}\" w:h=\"{}\"{}/>",
        props.page_w.0,
        props.page_h.0,
        if props.landscape {
            " w:orient=\"landscape\""
        } else {
            ""
        }
    ));
    out.push_str(&format!(
        "<w:pgMar w:top=\"{}\" w:right=\"{}\" w:bottom=\"{}\" w:left=\"{}\" w:header=\"{}\" w:footer=\"{}\" w:gutter=\"{}\"/>",
        props.margin_top.0, props.margin_right.0, props.margin_bottom.0, props.margin_left.0, props.header_dist.0, props.footer_dist.0, props.gutter.0
    ));
    write_borders(&props.page_borders, "pgBorders", &mut out);
    if let Some(numbering) = &props.line_numbering {
        out.push_str(&format!(
            "<w:lnNumType w:countBy=\"{}\" w:start=\"{}\"",
            numbering.count_by, numbering.start
        ));
        if let Some(restart) = &numbering.restart {
            out.push_str(&format!(" w:restart=\"{}\"", escape(restart)));
        }
        if let Some(distance) = numbering.distance {
            out.push_str(&format!(" w:distance=\"{}\"", distance.0));
        }
        out.push_str("/>");
    }
    if props.page_start.is_some() || props.page_fmt.is_some() {
        out.push_str("<w:pgNumType");
        if let Some(start) = props.page_start {
            out.push_str(&format!(" w:start=\"{start}\""));
        }
        if let Some(fmt) = &props.page_fmt {
            out.push_str(&format!(" w:fmt=\"{}\"", escape(fmt)));
        }
        out.push_str("/>");
    }
    if props.columns > 1 || props.column_space.is_some() || props.separate_columns {
        out.push_str(&format!("<w:cols w:num=\"{}\"", props.columns.max(1)));
        if let Some(space) = props.column_space {
            out.push_str(&format!(" w:space=\"{}\"", space.0));
        }
        if props.separate_columns {
            out.push_str(" w:sep=\"1\"");
        }
        out.push_str("/>");
    }
    if props.title_page {
        out.push_str("<w:titlePg/>");
    }
    if props.bidi {
        out.push_str("<w:bidi/>");
    }
    if props.rtl_gutter {
        out.push_str("<w:rtlGutter/>");
    }
    if let Some(direction) = &props.text_direction {
        out.push_str(&format!(
            "<w:textDirection w:val=\"{}\"/>",
            escape(direction)
        ));
    }
    if let Some(align) = &props.v_align {
        out.push_str(&format!("<w:vAlign w:val=\"{}\"/>", escape(align)));
    }
    if let Some(grid) = &props.doc_grid {
        out.push_str("<w:docGrid");
        if let Some(kind) = &grid.grid_type {
            out.push_str(&format!(" w:type=\"{}\"", escape(kind)));
        }
        if let Some(pitch) = grid.line_pitch {
            out.push_str(&format!(" w:linePitch=\"{pitch}\""));
        }
        if let Some(space) = grid.char_space {
            out.push_str(&format!(" w:charSpace=\"{space}\""));
        }
        out.push_str("/>");
    }
    for extra in &props.extra {
        out.push_str(extra);
    }
    out.push_str("</w:sectPr>");
    out
}

fn write_note_pr(props: &edgedoc_model::FootnotePr, tag: &str, out: &mut String) {
    if props.num_fmt.is_none()
        && props.num_start.is_none()
        && props.num_restart.is_none()
        && props.pos.is_none()
    {
        return;
    }
    out.push_str(&format!("<w:{tag}>"));
    if let Some(pos) = &props.pos {
        out.push_str(&format!("<w:pos w:val=\"{}\"/>", escape(pos)));
    }
    if let Some(fmt) = &props.num_fmt {
        out.push_str(&format!("<w:numFmt w:val=\"{}\"/>", escape(fmt)));
    }
    if let Some(start) = props.num_start {
        out.push_str(&format!("<w:numStart w:val=\"{start}\"/>"));
    }
    if let Some(restart) = &props.num_restart {
        out.push_str(&format!("<w:numRestart w:val=\"{}\"/>", escape(restart)));
    }
    out.push_str(&format!("</w:{tag}>"));
}

/// One `w:headerReference` or `w:footerReference`, with the part it points at.
#[allow(clippy::too_many_arguments)]
fn story_ref(
    id: &Option<u32>,
    kind: &str,
    which: &str,
    rels: &mut Rels,
    headers: &mut Vec<(String, Vec<u8>)>,
    doc: &ModelDocument,
    numbering: &mut Numbering,
    media: &mut Vec<(String, Vec<u8>)>,
) -> Option<String> {
    let id = id.as_ref()?;
    let part = doc.parts.get(id)?;
    let file = format!("{kind}{id}.xml");
    let rel_type =
        format!("http://schemas.openxmlformats.org/officeDocument/2006/relationships/{kind}");
    let rid = rels.add_or_reuse(&rel_type, &file, false);
    let tag = if kind == "header" { "hdr" } else { "ftr" };
    // Several sections can reference the same header. The part is written once,
    // because two parts with one name make the package invalid.
    let already_written = headers
        .iter()
        .any(|(name, _)| name.eq_ignore_ascii_case(&format!("word/{file}")));
    if already_written {
        let element = if kind == "header" {
            "headerReference"
        } else {
            "footerReference"
        };
        return Some(format!("<w:{element} w:type=\"{which}\" r:id=\"{rid}\"/>"));
    }
    let mut inner = String::new();
    // A header or a footer has its own relationships. Its pictures join the
    // package-level media list, so two stories never write the same part name.
    let mut part_rels = Rels::new();
    let mut nested_headers = Vec::new();
    write_blocks(
        &part.blocks,
        &mut inner,
        &mut part_rels,
        media,
        &mut nested_headers,
        numbering,
        doc,
    )
    .ok();
    headers.push((
        format!("word/{file}"),
        format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><w:{tag}{NS}>{inner}</w:{tag}>").into_bytes(),
    ));
    if !part_rels.items.is_empty() {
        headers.push((
            format!("word/_rels/{file}.rels"),
            part_rels.xml().into_bytes(),
        ));
    }
    let element = if kind == "header" {
        "headerReference"
    } else {
        "footerReference"
    };
    Some(format!("<w:{element} w:type=\"{which}\" r:id=\"{rid}\"/>"))
}

fn note_parts(
    doc: &ModelDocument,
    file: &str,
    element: &str,
    kind: PartKind,
) -> WmlResult<Vec<(String, Vec<u8>)>> {
    let ids = doc.parts_of(kind);
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let mut inner = String::new();
    let mut files = Vec::new();
    let mut part_rels = Rels::new();
    for id in ids {
        let Some(part) = doc.parts.get(&id) else {
            continue;
        };
        let note_id = part.meta.id.unwrap_or(id as i32);
        inner.push_str(&format!("<w:{element} w:id=\"{note_id}\""));
        if let Some(note_type) = &part.meta.note_type {
            inner.push_str(&format!(" w:type=\"{}\"", escape(note_type)));
        }
        if let Some(author) = &part.meta.author {
            inner.push_str(&format!(" w:author=\"{}\"", escape(author)));
        }
        if let Some(date) = &part.meta.date {
            inner.push_str(&format!(" w:date=\"{}\"", escape(date)));
        }
        if let Some(initials) = &part.meta.initials {
            inner.push_str(&format!(" w:initials=\"{}\"", escape(initials)));
        }
        inner.push('>');
        let mut numbering = Numbering::new();
        write_blocks(
            &part.blocks,
            &mut inner,
            &mut part_rels,
            &mut files,
            &mut Vec::new(),
            &mut numbering,
            doc,
        )?;
        inner.push_str(&format!("</w:{element}>"));
    }
    let root = if element == "footnote" {
        "footnotes"
    } else {
        "endnotes"
    };
    let xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><w:{root}{NS}>{inner}</w:{root}>"
    );
    files.push((format!("word/{file}.xml"), xml.into_bytes()));
    if !part_rels.items.is_empty() {
        files.push((
            format!("word/_rels/{file}.xml.rels"),
            part_rels.xml().into_bytes(),
        ));
    }
    Ok(files)
}

fn comment_parts(doc: &ModelDocument) -> WmlResult<Vec<(String, Vec<u8>)>> {
    let ids = doc.parts_of(PartKind::Comment);
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let mut inner = String::new();
    let mut files = Vec::new();
    let mut part_rels = Rels::new();
    for id in ids {
        let Some(part) = doc.parts.get(&id) else {
            continue;
        };
        let comment_id = part.meta.id.unwrap_or(id as i32);
        inner.push_str(&format!("<w:comment w:id=\"{comment_id}\""));
        if let Some(author) = &part.meta.author {
            inner.push_str(&format!(" w:author=\"{}\"", escape(author)));
        }
        if let Some(date) = &part.meta.date {
            inner.push_str(&format!(" w:date=\"{}\"", escape(date)));
        }
        if let Some(initials) = &part.meta.initials {
            inner.push_str(&format!(" w:initials=\"{}\"", escape(initials)));
        }
        inner.push('>');
        let mut numbering = Numbering::new();
        write_blocks(
            &part.blocks,
            &mut inner,
            &mut part_rels,
            &mut files,
            &mut Vec::new(),
            &mut numbering,
            doc,
        )?;
        inner.push_str("</w:comment>");
    }
    let xml = format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><w:comments{NS}>{inner}</w:comments>");
    files.push(("word/comments.xml".into(), xml.into_bytes()));
    if !part_rels.items.is_empty() {
        files.push((
            "word/_rels/comments.xml.rels".into(),
            part_rels.xml().into_bytes(),
        ));
    }
    Ok(files)
}

fn write_borders(borders: &Borders, tag: &str, out: &mut String) {
    if borders.is_empty() {
        return;
    }
    out.push_str(&format!("<w:{tag}>"));
    let items = [
        ("top", &borders.top),
        ("left", &borders.left),
        ("bottom", &borders.bottom),
        ("right", &borders.right),
        ("insideH", &borders.inside_h),
        ("insideV", &borders.inside_v),
        ("between", &borders.between),
        ("bar", &borders.bar),
        ("tl2br", &borders.tl2_br),
        ("tr2bl", &borders.tr2_bl),
    ];
    for (name, line) in items {
        let Some(line) = line else { continue };
        out.push_str(&format!("<w:{name} w:val=\"{}\"", escape(&line.val)));
        if let Some(size) = line.size {
            out.push_str(&format!(" w:sz=\"{size}\""));
        }
        if let Some(space) = line.space {
            out.push_str(&format!(" w:space=\"{space}\""));
        }
        if let Some(color) = &line.color {
            out.push_str(&format!(" w:color=\"{}\"", escape(color)));
        }
        out.push_str("/>");
    }
    out.push_str(&format!("</w:{tag}>"));
}

fn write_margins(margins: &edgedoc_model::CellMargins, out: &mut String) {
    if margins.top.is_none()
        && margins.left.is_none()
        && margins.bottom.is_none()
        && margins.right.is_none()
    {
        return;
    }
    out.push_str("<w:tblCellMar>");
    for (name, value) in [
        ("top", margins.top),
        ("left", margins.left),
        ("bottom", margins.bottom),
        ("right", margins.right),
    ] {
        if let Some(value) = value {
            out.push_str(&format!("<w:{name} w:w=\"{}\" w:type=\"dxa\"/>", value.0));
        }
    }
    out.push_str("</w:tblCellMar>");
}

fn write_cell_margins(margins: &edgedoc_model::CellMargins, out: &mut String) {
    if margins.top.is_none()
        && margins.left.is_none()
        && margins.bottom.is_none()
        && margins.right.is_none()
    {
        return;
    }
    out.push_str("<w:tcMar>");
    for (name, value) in [
        ("top", margins.top),
        ("left", margins.left),
        ("bottom", margins.bottom),
        ("right", margins.right),
    ] {
        if let Some(value) = value {
            out.push_str(&format!("<w:{name} w:w=\"{}\" w:type=\"dxa\"/>", value.0));
        }
    }
    out.push_str("</w:tcMar>");
}

fn styles_xml(styles: &StyleSheet) -> String {
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>");
    out.push_str(&format!("<w:styles xmlns:w=\"{W}\">"));
    if let Some(defaults) = &styles.doc_defaults_xml {
        out.push_str(defaults);
    }
    if let Some(latent) = &styles.latent_styles_xml {
        out.push_str(latent);
    }
    let has_normal = styles
        .styles
        .iter()
        .any(|style| style.id.eq_ignore_ascii_case("Normal"))
        || styles
            .raw_styles
            .iter()
            .any(|(id, _)| id.eq_ignore_ascii_case("Normal"));
    if !has_normal {
        out.push_str("<w:style w:type=\"paragraph\" w:default=\"1\" w:styleId=\"Normal\"><w:name w:val=\"Normal\"/></w:style>");
    }
    for style in &styles.styles {
        match &style.raw_xml {
            Some(raw) => out.push_str(raw),
            None => out.push_str(&style_xml(style)),
        }
    }
    for (id, raw) in &styles.raw_styles {
        if styles
            .styles
            .iter()
            .any(|style| style.id.eq_ignore_ascii_case(id))
        {
            continue;
        }
        out.push_str(raw);
    }
    for extra in &styles.extra {
        out.push_str(extra);
    }
    out.push_str("</w:styles>");
    out
}

fn style_xml(style: &Style) -> String {
    let mut out = format!(
        "<w:style w:type=\"{}\" w:styleId=\"{}\">",
        style.kind.as_str(),
        escape(&style.id)
    );
    out.push_str(&format!("<w:name w:val=\"{}\"/>", escape(&style.name)));
    if let Some(based) = &style.based_on {
        out.push_str(&format!("<w:basedOn w:val=\"{}\"/>", escape(based)));
    }
    if let Some(next) = &style.next {
        out.push_str(&format!("<w:next w:val=\"{}\"/>", escape(next)));
    }
    if let Some(link) = &style.link {
        out.push_str(&format!("<w:link w:val=\"{}\"/>", escape(link)));
    }
    if style.semi_hidden {
        out.push_str("<w:semiHidden/>");
    }
    if let Some(priority) = style.priority {
        out.push_str(&format!("<w:uiPriority w:val=\"{priority}\"/>"));
    }
    if style.q_format {
        out.push_str("<w:qFormat/>");
    }
    let para = ppr_of(&style.para_props);
    if !para.is_empty() {
        out.push_str(&para);
    }
    let run = rpr(&style.char_props);
    if !run.is_empty() {
        out.push_str(&run);
    }
    out.push_str("</w:style>");
    out
}

fn ppr_of(props: &ParaProps) -> String {
    let para = Paragraph::with_props(props.clone());
    let mut rels = Rels::new();
    let doc = ModelDocument::new();
    let mut numbering = Numbering::new();
    let mut media = Vec::new();
    let mut headers = Vec::new();
    ppr(
        &para,
        &mut rels,
        &mut media,
        &mut headers,
        &mut numbering,
        &doc,
    )
}

fn settings_xml() -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><w:settings xmlns:w=\"{W}\"><w:defaultTabStop w:val=\"708\"/><w:compat/></w:settings>"
    )
}

fn core_xml(doc: &ModelDocument) -> String {
    let mut out = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><cp:coreProperties xmlns:cp=\"http://schemas.openxmlformats.org/package/2006/metadata/core-properties\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\" xmlns:dcterms=\"http://purl.org/dc/terms/\" xmlns:dcmitype=\"http://purl.org/dc/dcmitype/\" xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\">",
    );
    let core = &doc.core;
    for (tag, value) in [
        ("dc:title", &core.title),
        ("dc:subject", &core.subject),
        ("dc:creator", &core.creator),
        ("cp:keywords", &core.keywords),
        ("dc:description", &core.description),
        ("cp:lastModifiedBy", &core.last_modified_by),
        ("cp:revision", &core.revision),
    ] {
        if !value.is_empty() {
            out.push_str(&format!("<{tag}>{}</{tag}>", escape(value)));
        }
    }
    for (tag, value) in [
        ("dcterms:created", &core.created),
        ("dcterms:modified", &core.modified),
    ] {
        if !value.is_empty() {
            out.push_str(&format!(
                "<{tag} xsi:type=\"dcterms:W3CDTF\">{}</{tag}>",
                escape(value)
            ));
        }
    }
    for (tag, value) in [
        ("cp:category", &core.category),
        ("cp:contentStatus", &core.content_status),
        ("dc:identifier", &core.identifier),
        ("dc:language", &core.language),
        ("cp:version", &core.version),
    ] {
        if !value.is_empty() {
            out.push_str(&format!("<{tag}>{}</{tag}>", escape(value)));
        }
    }
    out.push_str("</cp:coreProperties>");
    out
}

fn app_xml() -> String {
    "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Properties xmlns=\"http://schemas.openxmlformats.org/officeDocument/2006/extended-properties\"><Application>EdgeDoc</Application></Properties>".into()
}

fn content_types_xml(
    flavor: &str,
    files: &[(String, Vec<u8>)],
    kept: &[(String, Vec<u8>, Option<String>)],
) -> String {
    let main = match flavor {
        "docm" => "application/vnd.ms-word.document.macroEnabled.main+xml",
        "dotx" => {
            "application/vnd.openxmlformats-officedocument.wordprocessingml.template.main+xml"
        }
        "dotm" => "application/vnd.ms-word.template.macroEnabledTemplate.main+xml",
        _ => "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml",
    };
    let mut out = format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Types xmlns=\"{CT}\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Default Extension=\"png\" ContentType=\"image/png\"/><Default Extension=\"jpeg\" ContentType=\"image/jpeg\"/><Default Extension=\"gif\" ContentType=\"image/gif\"/><Default Extension=\"bmp\" ContentType=\"image/bmp\"/><Default Extension=\"tiff\" ContentType=\"image/tiff\"/><Default Extension=\"webp\" ContentType=\"image/webp\"/><Default Extension=\"emf\" ContentType=\"image/x-emf\"/><Default Extension=\"wmf\" ContentType=\"image/x-wmf\"/><Default Extension=\"bin\" ContentType=\"application/vnd.ms-office.vbaProject\"/>");
    out.push_str(&format!(
        "<Override PartName=\"/word/document.xml\" ContentType=\"{main}\"/>"
    ));
    out.push_str("<Override PartName=\"/word/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml\"/>");
    out.push_str("<Override PartName=\"/word/settings.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml\"/>");
    out.push_str("<Override PartName=\"/docProps/core.xml\" ContentType=\"application/vnd.openxmlformats-package.core-properties+xml\"/>");
    out.push_str("<Override PartName=\"/docProps/app.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.extended-properties+xml\"/>");
    for (name, _) in files {
        if name.starts_with("word/header") {
            out.push_str(&format!("<Override PartName=\"/{name}\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml\"/>"));
        } else if name.starts_with("word/footer") {
            out.push_str(&format!("<Override PartName=\"/{name}\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.footer+xml\"/>"));
        } else if name == "word/numbering.xml" {
            out.push_str("<Override PartName=\"/word/numbering.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml\"/>");
        } else if name == "word/footnotes.xml" {
            out.push_str("<Override PartName=\"/word/footnotes.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.footnotes+xml\"/>");
        } else if name == "word/endnotes.xml" {
            out.push_str("<Override PartName=\"/word/endnotes.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.endnotes+xml\"/>");
        } else if name == "word/comments.xml" {
            out.push_str("<Override PartName=\"/word/comments.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.comments+xml\"/>");
        }
    }
    for (name, _, kind) in kept {
        if let Some(kind) = kind {
            if !out.contains(&format!("PartName=\"/{}\"", name.trim_start_matches('/'))) {
                out.push_str(&format!(
                    "<Override PartName=\"/{}\" ContentType=\"{}\"/>",
                    name.trim_start_matches('/'),
                    escape(kind)
                ));
            }
        }
    }
    out.push_str("</Types>");
    out
}
