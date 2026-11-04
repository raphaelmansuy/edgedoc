//! Convert the Word model into flow blocks.
//!
//! This is the one place that reads a [`edgedoc_model::Block`] and a [`StyleSheet`]. The flow
//! crate sees computed values only, so it has no knowledge of Word markup. See `DP-A-02`.

use edgedoc::Document;
use edgedoc_geom::Rgba;
use edgedoc_model::{Block, InlineObject, Paragraph, PartKind, SectionProps, Table};
use edgedoc_style::{Border, BorderStyle, Cascade, ComputedPara, ComputedRun, NumDef, NumLevel};
use edgedoc_text::RunStyle;

use edgedoc_flow::{
    FloatAnchor, FlowBlock, FlowCell, FlowParagraph, FlowRow, FlowSection, FlowTable, RevisionKind,
    SectionStart, VAlign, WrapMode,
};
use edgedoc_text::Align;

use crate::{Diagnostic, OutlineItem, Result, Severity};

/// A document ready for the flow crate.
pub struct Prepared {
    /// Every block of the body, in document order.
    pub body: Vec<FlowBlock>,
    /// The sections, each with the blocks that it covers.
    pub sections: Vec<edgedoc_flow::SectionGroup>,
}

/// Prepare a document. See `DP-P-14`.
pub fn prepare(
    doc: &Document,
    fonts: &edgedoc_fonts::FontDb,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Prepared> {
    let known: Vec<String> = fonts.faces().iter().map(|f| f.family.clone()).collect();
    let mut cascade = Cascade::new(doc.styles(), &known);
    cascade.set_default_tab(default_tab(doc));
    load_numbering(doc, &mut cascade);
    let built = build_sections(doc, fonts, diagnostics);
    let base_section = built.base;
    let headers = built.parts.headers;
    let footers = built.parts.footers;
    let mut body = Vec::new();
    let source = body_blocks(doc);
    build_blocks(&mut cascade, source, &mut body, diagnostics);
    // Split the body at each section break. A paragraph carries the break of the section that it
    // ends, which is the Word rule. See `DP-S-24`.
    let mut groups: Vec<edgedoc_flow::SectionGroup> = Vec::new();
    let mut current: Vec<FlowBlock> = Vec::new();
    let mut section = base_section;
    for (block_index, block) in body.iter().enumerate() {
        current.push(block.clone());
        // The source paragraph at this index holds the break, when there is one.
        let ends = matches!(
            doc.block_at(block_index),
            Some(Block::Paragraph(p)) if p.section.is_some()
        );
        if ends {
            let mut next = section.clone();
            if let Some(Block::Paragraph(p)) = doc.block_at(block_index) {
                if let Some(brk) = &p.section {
                    next = section_of(&brk.props, groups.len() + 1);
                    next.header = headers.first().cloned().flatten();
                    next.footer = footers.first().cloned().flatten();
                }
            }
            next.start = edgedoc_flow::SectionStart::NextPage;
            groups.push(edgedoc_flow::SectionGroup {
                blocks: std::mem::take(&mut current),
                section: section.clone(),
            });
            section = next;
        }
    }
    if !current.is_empty() || groups.is_empty() {
        groups.push(edgedoc_flow::SectionGroup {
            blocks: current,
            section: section.clone(),
        });
    }
    // A document with no explicit break is one group, which is the common case.
    let _ = &mut cascade;
    for d in cascade.diagnostics.iter() {
        diagnostics.push(Diagnostic {
            code: d.code,
            severity: if d.code == "display.font_substituted" {
                Severity::Warning
            } else {
                Severity::Info
            },
            page: None,
            detail: d.detail.clone(),
        });
    }
    if body.is_empty() {
        body.push(FlowBlock::Paragraph(empty_paragraph(&mut cascade)));
    }
    Ok(Prepared {
        body,
        sections: groups,
    })
}

/// The body blocks of a document, which the model keeps private.
fn body_blocks(doc: &Document) -> Vec<Block> {
    let mut out = Vec::new();
    let mut i = 0usize;
    while let Some(block) = doc.block_at(i) {
        out.push(block.clone());
        i += 1;
    }
    out
}

fn empty_paragraph(cascade: &mut Cascade) -> FlowParagraph {
    let para = Paragraph::new();
    paragraph_of(&para, cascade, 0)
}

fn default_tab(doc: &Document) -> f32 {
    // `w:defaultTabStop` is 720 twips (36 pt) in every Word default.
    let _ = doc;
    36.0
}

/// Read the numbering part into the cascade. See `DP-S-14` and `DP-S-15`.
fn load_numbering(doc: &Document, cascade: &mut Cascade) {
    let Some(xml) = doc.numbering_xml() else {
        return;
    };
    // A small reader that takes `w:num` and `w:abstractNum` in order. The document library
    // already validated the XML, so this reader looks for the two element shapes it needs.
    let mut abstracts: Vec<(u32, NumDef)> = Vec::new();
    let mut nums: Vec<(u32, u32)> = Vec::new();
    for chunk in xml.split("<w:abstractNum").skip(1) {
        let id = attr_u32(chunk, "w:abstractNumId").unwrap_or(0);
        let mut def = NumDef {
            abstract_id: id,
            levels: Default::default(),
        };
        for level_chunk in chunk.split("<w:lvl ").skip(1) {
            let ilvl = attr_u32(level_chunk, "w:ilvl").unwrap_or(0);
            let format = attr_str(level_chunk, "w:numFmt").unwrap_or_else(|| "decimal".into());
            let text = element_text(level_chunk, "w:lvlText").unwrap_or_default();
            let start = attr_u32(level_chunk, "w:start").unwrap_or(1);
            let indent_left = attr_i32(level_chunk, "w:left")
                .map(|v| v as f32 / 20.0)
                .unwrap_or(0.0);
            let indent_hanging = attr_i32(level_chunk, "w:hanging")
                .map(|v| v as f32 / 20.0)
                .unwrap_or(0.0);
            let is_legal = level_chunk.contains("<w:isLgl");
            def.levels.insert(
                ilvl,
                NumLevel {
                    format,
                    text,
                    start,
                    indent_left,
                    indent_hanging,
                    font: None,
                    suffix: "tab".into(),
                    is_legal,
                },
            );
            if def.levels.len() > 64 {
                break;
            }
        }
        abstracts.push((id, def));
    }
    for chunk in xml.split("<w:num ").skip(1) {
        let num_id = attr_u32(chunk, "w:numId").unwrap_or(0);
        let abstract_id = attr_u32(chunk, "w:abstractNumId").unwrap_or(0);
        nums.push((num_id, abstract_id));
        if nums.len() > 4096 {
            break;
        }
    }
    for (num_id, abstract_id) in nums {
        if let Some((_, def)) = abstracts.iter().find(|(id, _)| *id == abstract_id) {
            cascade.add_numbering(num_id, def.clone());
        }
    }
}

fn attr_str(chunk: &str, name: &str) -> Option<String> {
    let needle = format!("{name}=\"");
    let start = chunk.find(&needle)? + needle.len();
    let rest = chunk.get(start..)?;
    let end = rest.find('"')?;
    Some(rest.get(..end)?.to_string())
}

fn attr_u32(chunk: &str, name: &str) -> Option<u32> {
    attr_str(chunk, name)?.trim().parse().ok()
}

fn attr_i32(chunk: &str, name: &str) -> Option<i32> {
    attr_str(chunk, name)?.trim().parse().ok()
}

fn element_text(chunk: &str, name: &str) -> Option<String> {
    let open = format!("<{name} ");
    let start = chunk.find(&open)?;
    let rest = chunk.get(start..)?;
    let val = attr_str(rest, "w:val")?;
    Some(val)
}

/// Build the sections of a document. See `DP-S-23`.
/// A header or footer part, built once for the whole document.
struct StoryParts {
    headers: Vec<Option<Vec<FlowBlock>>>,
    footers: Vec<Option<Vec<FlowBlock>>>,
}

/// The page box and the story parts that a build resolves once.
struct DocumentParts {
    base: FlowSection,
    parts: StoryParts,
}

fn build_sections(
    doc: &Document,
    fonts: &edgedoc_fonts::FontDb,
    diagnostics: &mut Vec<Diagnostic>,
) -> DocumentParts {
    let mut cascade = Cascade::new(
        doc.styles(),
        &fonts
            .faces()
            .iter()
            .map(|f| f.family.clone())
            .collect::<Vec<_>>(),
    );
    let mut main = section_of(doc.section(), 0);
    // The header and footer parts, in document order.
    let headers = doc.parts(PartKind::Header);
    let footers = doc.parts(PartKind::Footer);
    let mut header_blocks: Vec<Option<Vec<FlowBlock>>> = Vec::new();
    let mut footer_blocks: Vec<Option<Vec<FlowBlock>>> = Vec::new();
    for id in &headers {
        let mut blocks = Vec::new();
        for block in doc.part_blocks(*id) {
            let mut built = Vec::new();
            build_one(&mut cascade, &block, &mut built, diagnostics);
            blocks.extend(built);
        }
        header_blocks.push(Some(blocks));
    }
    for id in &footers {
        let mut blocks = Vec::new();
        for block in doc.part_blocks(*id) {
            let mut built = Vec::new();
            build_one(&mut cascade, &block, &mut built, diagnostics);
            blocks.extend(built);
        }
        footer_blocks.push(Some(blocks));
    }
    // The header references: the first header is the default, then first and even when set.
    if let Some(Some(h)) = header_blocks.first() {
        main.header = Some(h.clone());
    }
    if let Some(Some(f)) = footer_blocks.first() {
        main.footer = Some(f.clone());
    }
    if main.title_page {
        // The second header part is the first-page header.
        if let Some(Some(h)) = header_blocks.get(1) {
            main.header_first = Some(h.clone());
        }
        if let Some(Some(f)) = footer_blocks.get(1) {
            main.footer_first = Some(f.clone());
        }
    }
    if main.even_and_odd {
        if let Some(Some(h)) = header_blocks.get(2) {
            main.header_even = Some(h.clone());
        }
        if let Some(Some(f)) = footer_blocks.get(2) {
            main.footer_even = Some(f.clone());
        }
    }
    DocumentParts {
        base: main,
        parts: StoryParts {
            headers: header_blocks,
            footers: footer_blocks,
        },
    }
}

/// The section fields that a header set needs, which `FlowSection` keeps beside the page box.
struct SectionFlags {
    title_page: bool,
    even_and_odd: bool,
}

impl SectionFlags {
    fn of(_s: &SectionProps) -> Self {
        Self {
            title_page: false,
            even_and_odd: false,
        }
    }
}

fn section_of(s: &SectionProps, index: usize) -> FlowSection {
    let _ = index;
    let flags = SectionFlags::of(s);
    FlowSection {
        page_w: edgedoc_geom::twips(s.page_w.0),
        page_h: edgedoc_geom::twips(s.page_h.0),
        margin_top: edgedoc_geom::twips(s.margin_top.0),
        margin_bottom: edgedoc_geom::twips(s.margin_bottom.0),
        margin_left: edgedoc_geom::twips(s.margin_left.0),
        margin_right: edgedoc_geom::twips(s.margin_right.0),
        header_dist: edgedoc_geom::twips(s.header_dist.0),
        footer_dist: edgedoc_geom::twips(s.footer_dist.0),
        gutter: edgedoc_geom::twips(s.gutter.0),
        columns: s.columns.max(1),
        column_space: s.column_space.map_or(21.2, |v| edgedoc_geom::twips(v.0)),
        start: match s.start {
            edgedoc_model::SectionStart::NextPage => SectionStart::NextPage,
            edgedoc_model::SectionStart::Continuous => SectionStart::Continuous,
            edgedoc_model::SectionStart::OddPage => SectionStart::OddPage,
            edgedoc_model::SectionStart::EvenPage => SectionStart::EvenPage,
        },
        header: None,
        footer: None,
        header_first: None,
        footer_first: None,
        header_even: None,
        footer_even: None,
        page_start: s.page_start,
        page_fmt: s.page_fmt.clone().unwrap_or_else(|| "decimal".into()),
        v_align: s.v_align.as_deref().map_or(VAlign::Top, VAlign::parse),
        title_page: flags.title_page,
        even_and_odd: flags.even_and_odd,
    }
}

/// Build a list of blocks into flow blocks. The `fonts` argument is kept for a later version that
/// resolves an embedded face per run; version 1 resolves faces through the thread-local map that
/// `set_face_hints` publishes.
fn build_blocks(
    cascade: &mut Cascade,
    source: Vec<Block>,
    out: &mut Vec<FlowBlock>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for block in &source {
        build_one(cascade, block, out, diagnostics);
    }
}

fn build_one(
    cascade: &mut Cascade,
    block: &Block,
    out: &mut Vec<FlowBlock>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match block {
        Block::Paragraph(para) => {
            // A paragraph that holds only pictures becomes one picture block per picture. The
            // paragraph itself would reserve the picture height as a line, and the picture would
            // then take that height again, which doubles the space. See `DP-L-33`.
            let standalone = !para.objects.is_empty() && para.text.chars().all(|c| c == '\u{FFFC}');
            if standalone
                && para
                    .objects
                    .iter()
                    .all(|o| matches!(o, InlineObject::Picture(_)))
            {
                for (i, object) in para.objects.iter().enumerate() {
                    if let InlineObject::Picture(picture) = object {
                        let w = picture.width.map_or(120.0, |v| edgedoc_geom::emu(v.0));
                        let h = picture.height.map_or(80.0, |v| edgedoc_geom::emu(v.0));
                        if w < 1.0 || h < 1.0 {
                            continue;
                        }
                        out.push(FlowBlock::Picture {
                            path: format!("body[{}].picture[{i}]", out.len()),
                            width: w.clamp(1.0, 2000.0),
                            height: h.clamp(1.0, 2000.0),
                            alt: picture.alt.clone().unwrap_or_default(),
                            resource: resource_id(picture),
                            align: picture
                                .anchor
                                .as_ref()
                                .map_or(Align::Left, |a| align_of(a.h_align.as_deref())),
                        });
                    }
                }
                return;
            }
            out.push(FlowBlock::Paragraph(paragraph_of(para, cascade, out.len())));
            // A picture inside a paragraph with text keeps its own block, after the paragraph.
            for (i, object) in para.objects.iter().enumerate() {
                match object {
                    InlineObject::Picture(picture) => {
                        let w = picture.width.map_or(120.0, |v| edgedoc_geom::emu(v.0));
                        let h = picture.height.map_or(80.0, |v| edgedoc_geom::emu(v.0));
                        if w < 1.0 || h < 1.0 {
                            continue;
                        }
                        out.push(FlowBlock::Picture {
                            path: format!("body[{}].picture[{i}]", out.len()),
                            width: w.clamp(1.0, 2000.0),
                            height: h.clamp(1.0, 2000.0),
                            alt: picture.alt.clone().unwrap_or_default(),
                            resource: resource_id(picture),
                            align: Align::Left,
                        });
                    }
                    InlineObject::Opaque(kind) => {
                        diagnostics.push(Diagnostic {
                            code: "display.object_placeholder",
                            severity: Severity::Info,
                            page: None,
                            detail: format!("A {kind} object paints as a placeholder"),
                        });
                    }
                    _ => {}
                }
            }
        }
        Block::Table(table) => {
            out.push(FlowBlock::Table(table_of(table, cascade, out.len())));
        }
        Block::Control(control) => {
            for inner in &control.blocks {
                build_one(cascade, inner, out, diagnostics);
            }
        }
        Block::Alternate { choice, .. } => {
            for inner in choice {
                build_one(cascade, inner, out, diagnostics);
            }
        }
        Block::Opaque(_) => {}
    }
}

fn block_path_len(index: usize) -> String {
    format!("body[{index}]")
}

fn align_of(name: Option<&str>) -> Align {
    match name {
        Some("center") => Align::Center,
        Some("right") | Some("end") => Align::Right,
        _ => Align::Left,
    }
}

/// Build one paragraph. See `DP-S-29`.
pub fn paragraph_of(para: &Paragraph, cascade: &mut Cascade, index: usize) -> FlowParagraph {
    let computed = cascade.paragraph(para);
    // Resolve one run style per source run, then expand each inline object into the text that it
    // shows: a field result becomes characters, and a picture stays a marker that takes room.
    // See `DP-L-11` and `DP-G-13`.
    let mut styles: Vec<RunStyle> = Vec::new();
    let mut source_runs: Vec<(usize, usize, u32)> = Vec::new();
    let mut offset = 0usize;
    for run in &para.runs {
        let end = (offset + run.len).min(para.text.len());
        let computed_run = cascade.run(para, &run.props);
        styles.push(run_style_of(&computed_run));
        source_runs.push((offset, end.max(offset), (styles.len() - 1) as u32));
        offset = end;
    }
    if styles.is_empty() {
        let computed_run = cascade.run(para, &para.props.mark);
        styles.push(run_style_of(&computed_run));
        source_runs.push((0, para.text.len(), 0));
    }
    if let Some((_, end, style)) = source_runs.last().copied() {
        if end < para.text.len() {
            source_runs.push((end, para.text.len(), style));
        }
    }
    // The style of every byte of the source text.
    let mut style_of_byte: Vec<u32> = vec![0; para.text.len() + 1];
    for (start, end, style) in &source_runs {
        for slot in style_of_byte
            .iter_mut()
            .take((*end).min(para.text.len()) + 1)
            .skip(*start)
        {
            *slot = *style;
        }
    }
    // Expand the text, and collect the extents of the objects that remain.
    let mut text = String::with_capacity(para.text.len() + 32);
    let mut runs: Vec<(usize, usize, u32)> = Vec::new();
    let mut objects: Vec<(f32, f32)> = Vec::new();
    let mut object_iter = para.objects.iter();
    for (i, ch) in para.text.char_indices() {
        let style = style_of_byte.get(i).copied().unwrap_or(0);
        if ch == '\u{FFFC}' {
            match object_iter.next() {
                Some(InlineObject::Field(field)) => {
                    // A page field takes the page number, which the flow writes. A cached result
                    // paints as text. See `DP-L-11`.
                    let shown = field_result(field);
                    let start = text.len();
                    text.push_str(&shown);
                    push_run(&mut runs, start, text.len(), style);
                }
                Some(InlineObject::Picture(picture)) => {
                    let start = text.len();
                    text.push('\u{FFFC}');
                    let w = picture.width.map_or(60.0, |v| edgedoc_geom::emu(v.0));
                    let h = picture.height.map_or(18.0, |v| edgedoc_geom::emu(v.0));
                    objects.push((h.max(8.0), 0.0));
                    let _ = w;
                    push_run(&mut runs, start, text.len(), style);
                }
                Some(InlineObject::Note { .. }) => {
                    // A note reference paints a small raised mark.
                    let start = text.len();
                    text.push('1');
                    objects.push((0.0, 0.0));
                    push_run(&mut runs, start, text.len(), style);
                }
                Some(_) | None => {
                    let start = text.len();
                    text.push('\u{FFFC}');
                    objects.push((12.0, 0.0));
                    push_run(&mut runs, start, text.len(), style);
                }
            }
            continue;
        }
        let start = text.len();
        text.push(ch);
        push_run(&mut runs, start, text.len(), style);
    }
    // The flow numbers a list from a running counter, so a paragraph only carries the level.
    // See `DP-S-14`.
    let list = computed.list.clone();
    let label = if list.is_none() {
        list_label(&computed, cascade)
    } else {
        None
    };
    let has_page_field = para_has_field(para, "PAGE");
    let has_total_field = para_has_field(para, "NUMPAGES");
    let mut links = Vec::new();
    for link in &para.links {
        links.push((0usize, text.len(), link.clone()));
    }
    FlowParagraph {
        path: block_path_len(index),
        text,
        runs,
        run_styles: styles,
        para: computed.clone(),
        label,
        list,
        page_break_before: para.props.page_break_before,
        has_page_field,
        has_total_field,
        objects,
        links,
        outline: computed.outline_level,
        notes: Vec::new(),
        revision: revision_of(para),
    }
}

/// The marker that a `PAGE` field leaves in the text, which the flow replaces.
pub const PAGE_NUMBER: char = '\u{F000}';
/// The marker that a `NUMPAGES` field leaves in the text.
pub const PAGE_TOTAL: char = '\u{F001}';

/// Append a run range, merging with the previous range when the style matches.
fn push_run(runs: &mut Vec<(usize, usize, u32)>, start: usize, end: usize, style: u32) {
    match runs.last_mut() {
        Some((_, prev_end, prev_style)) if *prev_style == style && *prev_end == start => {
            *prev_end = end;
        }
        _ => runs.push((start, end, style)),
    }
}

/// The text that a field shows. A page field keeps a placeholder digit, which the flow replaces.
fn field_result(field: &edgedoc_model::Field) -> String {
    let upper = field.instr.to_ascii_uppercase();
    if upper.contains("NUMPAGES") || upper.contains("SECTIONPAGES") {
        // A total paints a marker that the flow replaces once the page count is known.
        // See DP-L-12.
        return PAGE_TOTAL.to_string();
    }
    if upper.contains("PAGE") && !upper.contains("PAGEREF") {
        // The page number marker, which the flow replaces per page. See DP-L-11.
        return PAGE_NUMBER.to_string();
    }
    if field.result.is_empty() {
        return "\u{00A0}".into();
    }
    field.result.clone()
}

/// One computed run as a flow run style. The colour, the underline, and the highlight travel with
/// it, so a heading paints blue and a muted note paints grey. See `DP-S-08`.
fn run_style_of(run: &ComputedRun) -> RunStyle {
    let face = face_id(run);
    let mut style = edgedoc_paint::lower::run_style_of(run, face);
    style.baseline_shift = baseline_shift(run);
    // A face with no bold sibling is emboldened, and a face with no italic sibling is slanted,
    // which is what Word does. See `DP-F-13`.
    style.synth_bold = run.bold && !FACE_MATCHES.with(|m| m.borrow().bold.contains(&face));
    style.synth_italic = run.italic && !FACE_MATCHES.with(|m| m.borrow().italic.contains(&face));
    style
}

thread_local! {
    /// The faces that already carry a bold or an italic style, so a run does not synthesize one.
    static FACE_MATCHES: std::cell::RefCell<StyleSets> =
        std::cell::RefCell::new(StyleSets::default());
}

#[derive(Default)]
struct StyleSets {
    bold: std::collections::BTreeSet<u32>,
    italic: std::collections::BTreeSet<u32>,
}

fn revision_of(para: &Paragraph) -> Option<RevisionKind> {
    if para.retention.revisions {
        Some(RevisionKind::Insert)
    } else {
        None
    }
}

fn para_has_field(para: &Paragraph, name: &str) -> bool {
    for object in &para.objects {
        if let InlineObject::Field(field) = object {
            if field.instr.to_ascii_uppercase().contains(name) {
                return true;
            }
        }
    }
    false
}

/// The list label text for a paragraph. See `DP-S-14` and `DP-S-21`.
fn list_label(computed: &ComputedPara, cascade: &Cascade) -> Option<String> {
    let list = computed.list.as_ref()?;
    let def = cascade.numbering().get(&list.num_id)?;
    let level = def.levels.get(&list.level)?;
    if let Some(bullet) = level.bullet() {
        return Some(bullet.to_string());
    }
    if level.text.is_empty() {
        return None;
    }
    // A level text of `%1.` takes the counter that the flow assigns. Version 1 paints the
    // start value, because a counter needs the paragraph order that the flow holds.
    let counter = edgedoc_style::format_counter(level.start, &level.format, level.is_legal);
    Some(level.text.replace("%1", &counter).replace("%2", &counter))
}

/// The face id for a computed run, through the font database.
fn face_id(run: &ComputedRun) -> u32 {
    // A family has one face per style, so the key is the family and the style. A document that
    // asks for bold finds the bold face, and only a family with no bold face takes the synthetic
    // path. See `DP-F-02` and `DP-F-13`.
    FACE_HINTS.with(|h| {
        let map = h.borrow();
        for name in [&run.face_name, &run.requested_face] {
            let key = (name.to_ascii_lowercase(), run.bold, run.italic);
            if let Some(id) = map.get(&key) {
                return *id;
            }
        }
        // The same family in another style is better than the fallback face.
        for name in [&run.face_name, &run.requested_face] {
            let family = name.to_ascii_lowercase();
            for (b, i) in [(run.bold, run.italic), (false, false), (run.bold, false)] {
                if let Some(id) = map.get(&(family.clone(), b, i)) {
                    return *id;
                }
            }
        }
        0
    })
}

thread_local! {
    /// A family name to face id map, filled by the facade before a build. This keeps the style
    /// crate free of a font database, which `DP-A-06` requires.
    static FACE_HINTS: std::cell::RefCell<
        std::collections::BTreeMap<(String, bool, bool), u32>,
    > = std::cell::RefCell::new(std::collections::BTreeMap::new());
}

/// Publish the face ids that a build needs.
pub fn set_face_hints(fonts: &edgedoc_fonts::FontDb) {
    FACE_HINTS.with(|h| {
        let mut map = h.borrow_mut();
        map.clear();
        for face in fonts.faces() {
            map.insert(
                (face.family.to_ascii_lowercase(), face.bold, face.italic),
                face.id,
            );
            map.insert(
                (face.requested.to_ascii_lowercase(), face.bold, face.italic),
                face.id,
            );
        }
    });
    FACE_MATCHES.with(|m| {
        let mut sets = m.borrow_mut();
        sets.bold.clear();
        sets.italic.clear();
        for face in fonts.faces() {
            if face.bold {
                sets.bold.insert(face.id);
            }
            if face.italic {
                sets.italic.insert(face.id);
            }
        }
    });
}

fn baseline_shift(run: &ComputedRun) -> f32 {
    use edgedoc_style::VertAlign;
    match run.vert_align {
        Some(VertAlign::Superscript) => run.size * 0.33,
        Some(VertAlign::Subscript) => -run.size * 0.14,
        None => run.baseline_shift,
    }
}

/// Build a table. See `DP-L-34`.
fn table_of(table: &Table, cascade: &mut Cascade, index: usize) -> FlowTable {
    let mut rows = Vec::new();
    for row in &table.rows {
        let mut cells = Vec::new();
        for cell in &row.cells {
            let mut blocks = Vec::new();
            for block in &cell.blocks {
                match block.as_ref() {
                    Block::Paragraph(p) => {
                        blocks.push(FlowBlock::Paragraph(paragraph_of(p, cascade, blocks.len())))
                    }
                    Block::Table(t) => blocks.push(FlowBlock::Table(table_of(t, cascade, 0))),
                    Block::Control(c) => {
                        for inner in &c.blocks {
                            if let Block::Paragraph(p) = inner.as_ref() {
                                blocks.push(FlowBlock::Paragraph(paragraph_of(p, cascade, 0)));
                            }
                        }
                    }
                    _ => {}
                }
            }
            cells.push(FlowCell {
                blocks,
                span: cell.span.max(1),
                width: cell.width.map(|w| edgedoc_geom::twips(w.0)),
                v_align: cell.v_align.as_deref().map_or(VAlign::Top, VAlign::parse),
                shading: cell.shading.as_deref().and_then(Rgba::parse_hex),
                margins: [
                    cell.margins.top.map_or(0.0, |v| edgedoc_geom::twips(v.0)),
                    cell.margins.left.map_or(5.4, |v| edgedoc_geom::twips(v.0)),
                    cell.margins
                        .bottom
                        .map_or(0.0, |v| edgedoc_geom::twips(v.0)),
                    cell.margins.right.map_or(5.4, |v| edgedoc_geom::twips(v.0)),
                ],
                borders: [
                    cell.borders.top.as_ref().map(border_of),
                    cell.borders.left.as_ref().map(border_of),
                    cell.borders.bottom.as_ref().map(border_of),
                    cell.borders.right.as_ref().map(border_of),
                ],
                v_merge_continue: cell.v_merge.as_deref() == Some("continue"),
                v_merge_start: cell.v_merge.as_deref() == Some("restart"),
            });
        }
        rows.push(FlowRow {
            cells,
            header: row.header,
            height: row.height.map(|h| edgedoc_geom::twips(h.0)),
            height_exact: row.height_rule.as_deref() == Some("exact"),
            cant_split: row.cant_split,
        });
    }
    FlowTable {
        path: format!("body[{}]", index),
        rows,
        widths: table
            .widths
            .iter()
            .map(|w| edgedoc_geom::twips(w.0))
            .collect(),
        align: table.align.as_deref().map_or(Align::Left, |a| match a {
            "center" => Align::Center,
            "right" | "end" => Align::Right,
            _ => Align::Left,
        }),
        indent: table.indent.map_or(0.0, |v| edgedoc_geom::twips(v.0)),
        layout_fixed: table.layout.as_deref() == Some("fixed"),
        cell_margins: [
            table
                .cell_margins
                .top
                .map_or(0.0, |v| edgedoc_geom::twips(v.0)),
            table
                .cell_margins
                .left
                .map_or(5.4, |v| edgedoc_geom::twips(v.0)),
            table
                .cell_margins
                .bottom
                .map_or(0.0, |v| edgedoc_geom::twips(v.0)),
            table
                .cell_margins
                .right
                .map_or(5.4, |v| edgedoc_geom::twips(v.0)),
        ],
        borders: [
            table.borders.top.as_ref().map(border_of),
            table.borders.left.as_ref().map(border_of),
            table.borders.bottom.as_ref().map(border_of),
            table.borders.right.as_ref().map(border_of),
        ],
        inside_h: table.borders.inside_h.as_ref().map(border_of),
        inside_v: table.borders.inside_v.as_ref().map(border_of),
        floating: table.position.is_some(),
        anchor: table.position.as_ref().map(|p| FloatAnchor {
            behind_text: false,
            wrap: WrapMode::Square,
            offset_x: p.tblp_x.map(|v| v as f32 / 20.0),
            offset_y: p.tblp_y.map(|v| v as f32 / 20.0),
            dist_top: 0.0,
            dist_bottom: 0.0,
            dist_left: 9.0,
            dist_right: 9.0,
        }),
        bidi: table.bidi_visual,
    }
}

fn border_of(line: &edgedoc_model::LineBorder) -> Border {
    Border {
        width: line.size.map_or(0.5, |s| s as f32 / 8.0),
        color: line
            .color
            .as_deref()
            .and_then(Rgba::parse_hex)
            .unwrap_or(Rgba::BLACK),
        style: BorderStyle::parse(&line.val),
        space: line.space.map_or(0.0, |s| s as f32),
    }
}

/// The outline of a document, from its heading paragraphs. See `DP-V-37`.
pub fn outline_of(doc: &Document) -> Vec<OutlineItem> {
    let mut out = Vec::new();
    let mut i = 0usize;
    while let Some(block) = doc.block_at(i) {
        if let Block::Paragraph(p) = block {
            let level = p
                .props
                .outline
                .or_else(|| {
                    p.props
                        .style
                        .as_deref()
                        .and_then(edgedoc_model::heading_level)
                })
                .or_else(|| doc.paragraph_props(i).and_then(|props| props.outline));
            if let Some(level) = level {
                let title = p.text.trim().to_string();
                if !title.is_empty() {
                    out.push(OutlineItem {
                        title,
                        level: level.clamp(0, 8) as u8,
                        path: block_path_len(i),
                    });
                }
            }
        }
        i += 1;
    }
    out
}

/// Register a picture and return its resource id. The id indexes the resource table that the
/// facade exposes, so a painter can fetch the bytes. See `DP-P-10`.
pub fn resource_id(picture: &edgedoc_model::Picture) -> u32 {
    RESOURCES.with(|r| {
        let mut map = r.borrow_mut();
        for (id, (name, _)) in map.iter() {
            if name == &picture.name {
                return *id;
            }
        }
        let id = map.len() as u32 + 1;
        map.insert(id, (picture.name.clone(), picture.bytes.clone()));
        id
    })
}

thread_local! {
    /// The image bytes, by resource id, so the painter can decode one.
    static RESOURCES: std::cell::RefCell<std::collections::BTreeMap<u32, (String, Vec<u8>)>> =
        std::cell::RefCell::new(std::collections::BTreeMap::new());
}

/// The bytes of a resource, for a painter that decodes an image.
pub fn resource_bytes(id: u32) -> Option<Vec<u8>> {
    RESOURCES.with(|r| r.borrow().get(&id).map(|(_, bytes)| bytes.clone()))
}

/// Every resource, so a viewer can build its own table.
pub fn resources() -> Vec<(u32, String, Vec<u8>)> {
    RESOURCES.with(|r| {
        r.borrow()
            .iter()
            .map(|(id, (name, bytes))| (*id, name.clone(), bytes.clone()))
            .collect()
    })
}

/// Clear the resource registry, which a test does between documents.
pub fn clear_resources() {
    RESOURCES.with(|r| r.borrow_mut().clear());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> edgedoc_fonts::FontDb {
        let mut db = edgedoc_fonts::FontDb::new();
        crate::fonts::load_builtin_fonts(&mut db);
        set_face_hints(&db);
        db
    }

    #[test]
    fn a_builder_document_becomes_flow_blocks() {
        let doc = edgedoc::Document::builder()
            .paragraph(edgedoc::Paragraph::new().text("Hello"))
            .build()
            .expect("build");
        let db = db();
        let mut diags = Vec::new();
        let prepared = prepare(&doc, &db, &mut diags).expect("prepare");
        assert!(!prepared.body.is_empty());
        assert!(!prepared.sections.is_empty());
        match prepared.body.first() {
            Some(FlowBlock::Paragraph(p)) => assert_eq!(p.text, "Hello"),
            other => assert!(
                matches!(other, Some(FlowBlock::Paragraph(_))),
                "a paragraph"
            ),
        }
    }

    #[test]
    fn an_a4_page_comes_through_in_points() {
        let doc = edgedoc::Document::builder()
            .page(edgedoc::Page::a4())
            .paragraph(edgedoc::Paragraph::new().text("A4"))
            .build()
            .expect("build");
        let db = db();
        let mut diags = Vec::new();
        let prepared = prepare(&doc, &db, &mut diags).expect("prepare");
        let s = &prepared.sections[0].section;
        assert!((s.page_w - 595.0).abs() < 2.0, "w {}", s.page_w);
        assert!((s.page_h - 842.0).abs() < 2.0, "h {}", s.page_h);
    }

    #[test]
    fn a_table_becomes_a_flow_table() {
        let doc = edgedoc::Document::builder()
            .table(edgedoc::Table::new().header(["A", "B"]).row(["1", "2"]))
            .build()
            .expect("build");
        let db = db();
        let mut diags = Vec::new();
        let prepared = prepare(&doc, &db, &mut diags).expect("prepare");
        let table = prepared.body.iter().find_map(|b| match b {
            FlowBlock::Table(t) => Some(t),
            _ => None,
        });
        let table = table.expect("a table");
        assert_eq!(table.rows.len(), 2);
        assert!(table.rows[0].header);
    }

    #[test]
    fn a_heading_reaches_the_outline() {
        let doc = edgedoc::Document::builder()
            .paragraph(
                edgedoc::Paragraph::new()
                    .style("Heading1")
                    .text("Chapter one"),
            )
            .paragraph(edgedoc::Paragraph::new().text("Body"))
            .build()
            .expect("build");
        let outline = outline_of(&doc);
        assert_eq!(outline.len(), 1);
        assert_eq!(outline[0].title, "Chapter one");
    }

    #[test]
    fn a_run_with_a_size_carries_it() {
        let doc = edgedoc::Document::builder()
            .paragraph(
                edgedoc::Paragraph::new().run(
                    edgedoc::RunBuilder::new()
                        .text("Big")
                        .size(edgedoc::Pt(18.0)),
                ),
            )
            .build()
            .expect("build");
        let db = db();
        let mut diags = Vec::new();
        let prepared = prepare(&doc, &db, &mut diags).expect("prepare");
        let para = prepared.body.iter().find_map(|b| match b {
            FlowBlock::Paragraph(p) => Some(p),
            _ => None,
        });
        let para = para.expect("a paragraph");
        assert!(para.run_styles.iter().any(|s| (s.size - 18.0).abs() < 0.01));
    }
}
