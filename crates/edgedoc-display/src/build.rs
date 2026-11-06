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
    FloatAnchor, FlowBlock, FlowCell, FlowNote, FlowParagraph, FlowRow, FlowSection, FlowTable,
    RevisionKind, SectionStart, TableWidth, VAlign, WrapMode,
};
use edgedoc_text::Align;

use crate::{Diagnostic, OutlineItem, Result, Severity};

/// A document ready for the flow crate.
pub struct Prepared {
    /// The sections, each with the blocks that it covers.
    pub sections: Vec<edgedoc_flow::SectionGroup>,
    pub footnotes_each_page: bool,
}

/// Prepare a document. See `DP-P-14`.
pub fn prepare(
    doc: &Document,
    fonts: &edgedoc_fonts::FontDb,
    diagnostics: &mut Vec<Diagnostic>,
    note_pages: &std::collections::BTreeMap<i32, u32>,
) -> Result<Prepared> {
    FIELD_METADATA.with(|metadata| {
        let core = doc.core_props();
        *metadata.borrow_mut() = (core.title.clone(), core.creator.clone());
    });
    let known: Vec<String> = fonts.faces().iter().map(|f| f.family.clone()).collect();
    let mut cascade = Cascade::new(doc.styles(), &known);
    configure_default_style(doc, &mut cascade);
    let settings = DisplaySettings::of(doc);
    let defaults = doc
        .default_formatting()
        .map_err(|e| crate::Error::new(e.code, e.message))?;
    cascade.set_default_properties(&defaults.0, &defaults.1);
    cascade.set_default_tab(settings.default_tab);
    cascade.set_compatibility_mode(settings.compatibility_mode);
    load_numbering(doc, &mut cascade);
    let mut source = body_blocks(doc);
    set_note_numbers(&source, doc, &settings, note_pages);
    let mut flat = Vec::new();
    flatten_sections(&source, &mut flat);
    let footnotes_each_page = settings.footnotes.num_restart.as_deref() == Some("eachPage")
        || doc.section().footnotes.num_restart.as_deref() == Some("eachPage")
        || flat.iter().any(|block| matches!(block, Block::Paragraph(para) if para.section.as_ref().is_some_and(|section| section.props.footnotes.num_restart.as_deref() == Some("eachPage"))));
    build_footnote_stories(doc, fonts, diagnostics, &settings, &defaults);
    let (endnote_stories, endnote_separator) =
        build_endnote_stories(doc, fonts, diagnostics, &settings, &defaults);
    let parts = build_story_parts(doc, fonts, diagnostics, &settings, &defaults);
    unstyle_toc_links(&mut source, doc.styles());
    diagnose_layout_gaps(doc, &source, diagnostics);
    let mut flattened = Vec::new();
    flatten_sections(&source, &mut flattened);
    let mut body = Vec::new();
    let mut groups: Vec<edgedoc_flow::SectionGroup> = Vec::new();
    let mut first = 0;
    let mut inherited = FlowSection::default();
    let mut pending_endnotes = Vec::new();
    let mut seen_endnotes = std::collections::BTreeSet::new();
    for (position, block) in flattened.iter().copied().enumerate() {
        let next_start = flattened[position + 1..]
            .iter()
            .find_map(|block| match block {
                Block::Paragraph(p) => p.section.as_ref().map(|s| s.props.start),
                _ => None,
            })
            .unwrap_or(doc.section().start);
        // Wordcraft consumes an empty section-break paragraph as a boundary. Giving it a
        // line box adds a blank row and unbalances the preceding columns.
        let boundary_only = matches!(block, Block::Paragraph(p) if p.section.is_some()
            && p.text.is_empty() && p.objects.is_empty()
            && next_start == edgedoc_model::SectionStart::Continuous);
        collect_endnote_ids(
            block,
            &mut pending_endnotes,
            &mut seen_endnotes,
            &endnote_stories,
        );
        if !boundary_only {
            build_one(&mut cascade, block, &mut body, diagnostics, true);
        }
        if let Block::Paragraph(p) = block {
            if let Some(brk) = &p.section {
                // sectPr describes the section ending here, including how that section began.
                let section = resolved_section(&brk.props, &settings, &parts, &inherited);
                let mut blocks = body[first..].to_vec();
                if brk
                    .props
                    .endnotes
                    .pos
                    .as_deref()
                    .or(settings.endnotes.pos.as_deref())
                    == Some("sectEnd")
                {
                    append_endnotes(
                        &mut blocks,
                        &mut pending_endnotes,
                        &endnote_stories,
                        &endnote_separator,
                    );
                }
                groups.push(edgedoc_flow::SectionGroup {
                    blocks,
                    section: section.clone(),
                });
                first = body.len();
                inherited = section;
            }
        }
    }
    let section = resolved_section(doc.section(), &settings, &parts, &inherited);
    let mut remaining = body[first..].to_vec();
    append_endnotes(
        &mut remaining,
        &mut pending_endnotes,
        &endnote_stories,
        &endnote_separator,
    );
    if remaining.is_empty() && groups.is_empty() {
        remaining.push(FlowBlock::Paragraph(empty_paragraph(&mut cascade)));
    }
    groups.push(edgedoc_flow::SectionGroup {
        blocks: remaining,
        section,
    });
    for group in &mut groups {
        join_simple_paragraph_borders(&mut group.blocks);
    }
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
    Ok(Prepared {
        sections: groups,
        footnotes_each_page,
    })
}

// Wordcraft joins adjacent paragraphs with identical border sets and indents. Restrict this
// implementation to direct top/bottom borders: side-border and shading continuity and the
// between edge need their own handling before they can join safely.
fn join_simple_paragraph_borders(blocks: &mut [FlowBlock]) {
    for index in 1..blocks.len() {
        let (before, after) = blocks.split_at_mut(index);
        let (Some(FlowBlock::Paragraph(a)), Some(FlowBlock::Paragraph(b))) =
            (before.last_mut(), after.first_mut())
        else {
            continue;
        };
        if a.border_group_safe
            && b.border_group_safe
            && a.para.borders == b.para.borders
            && a.para.indent_left == b.para.indent_left
            && a.para.indent_right == b.para.indent_right
        {
            a.border_join_below = true;
            b.border_join_above = true;
        }
    }
}

/// A deliberately narrow subset of table styles whose layout properties are implemented.
/// Unknown markup and conditional regions stay diagnosed until they are covered.
fn supported_plain_table_style(sheet: &edgedoc_model::StyleSheet, id: &str) -> bool {
    use edgedoc_opc::xml::{self, El};
    fn attrs(el: &El, names: &[&str]) -> bool {
        el.attrs
            .iter()
            .all(|(name, _)| names.contains(&name.as_str()))
    }
    fn table_properties(el: &El) -> bool {
        el.attrs.is_empty()
            && el.child_els().all(|child| match child.name.as_str() {
                "tblInd" => {
                    attrs(child, &["w", "type"])
                        && child.attr("w") == Some("0")
                        && child
                            .attr("type")
                            .map_or(true, |v| matches!(v, "dxa" | "nil"))
                        && child.child_els().next().is_none()
                }
                "tblCellMar" => {
                    child.attrs.is_empty()
                        && child.child_els().all(|margin| {
                            matches!(
                                margin.name.as_str(),
                                "top" | "bottom" | "left" | "right" | "start" | "end"
                            ) && attrs(margin, &["w", "type"])
                                && margin.attr("type").map_or(true, |v| v == "dxa")
                                && margin
                                    .attr("w")
                                    .and_then(|v| v.parse::<u32>().ok())
                                    .is_some_and(|value| value <= 31_680)
                                && margin.child_els().next().is_none()
                        })
                }
                "tblBorders" => {
                    child.attrs.is_empty()
                        && child.child_els().all(|border| {
                            matches!(
                                border.name.as_str(),
                                "top"
                                    | "bottom"
                                    | "left"
                                    | "right"
                                    | "start"
                                    | "end"
                                    | "insideH"
                                    | "insideV"
                            ) && attrs(border, &["val", "sz", "space", "color"])
                                && border.attr("val") == Some("single")
                                && border.attr("space").map_or(true, |v| v == "0")
                                && border
                                    .attr("sz")
                                    .map_or(true, |v| v.parse::<u32>().is_ok_and(|size| size <= 96))
                                && border
                                    .attr("color")
                                    .map_or(true, |v| v == "auto" || Rgba::parse_hex(v).is_some())
                                && border.child_els().next().is_none()
                        })
                }
                _ => false,
            })
    }
    let mut seen = Vec::new();
    let mut next = Some(id);
    while let Some(name) = next {
        if seen.len() >= 32
            || seen
                .iter()
                .any(|prior: &&str| prior.eq_ignore_ascii_case(name))
        {
            return false;
        }
        seen.push(name);
        let Some(style) = sheet.by_id(name) else {
            return false;
        };
        if style.kind != edgedoc_model::StyleKind::Table
            || !style.table_regions.is_empty()
            || style.char_props != edgedoc_model::CharProps::default()
        {
            return false;
        }
        let mut para = style.para_props.clone();
        if [para.spacing_before, para.spacing_after, para.line]
            .into_iter()
            .flatten()
            .any(|value| !(0..=31_680).contains(&value.0))
            || para
                .line_rule
                .as_deref()
                .is_some_and(|rule| !matches!(rule, "auto" | "exact" | "atLeast"))
        {
            return false;
        }
        para.spacing_before = None;
        para.spacing_after = None;
        para.line = None;
        para.line_rule = None;
        if para != edgedoc_model::ParaProps::default() {
            return false;
        }
        if let Some(table) = &style.table_props {
            let borders = &table.borders;
            if borders.start != borders.left
                || borders.end != borders.right
                || borders.between.is_some()
                || borders.bar.is_some()
                || borders.tl2_br.is_some()
                || borders.tr2_bl.is_some()
                || [
                    &borders.top,
                    &borders.bottom,
                    &borders.left,
                    &borders.right,
                    &borders.inside_h,
                    &borders.inside_v,
                ]
                .into_iter()
                .flatten()
                .any(|border| {
                    border.val != "single"
                        || border.space.is_some_and(|space| space != 0)
                        || border.size.is_some_and(|size| !(0..=96).contains(&size))
                        || border.color.as_deref().is_some_and(|color| {
                            color != "auto" && Rgba::parse_hex(color).is_none()
                        })
                })
                || [
                    table.cell_margins.top,
                    table.cell_margins.bottom,
                    table.cell_margins.left,
                    table.cell_margins.right,
                ]
                .into_iter()
                .flatten()
                .any(|margin| !(0..=31_680).contains(&margin.0))
            {
                return false;
            }
            let mut remaining = table.clone();
            remaining.borders = Default::default();
            remaining.cell_margins = Default::default();
            if remaining.indent.is_some_and(|value| value.0 == 0) {
                remaining.indent = None;
            }
            if remaining != Table::new() {
                return false;
            }
        }
        let Some(root) = style
            .raw_xml
            .as_deref()
            .and_then(|raw| xml::parse(raw.as_bytes(), &edgedoc::Budget::default()).ok())
        else {
            return false;
        };
        if !root.child_els().all(|child| match child.name.as_str() {
            "name" | "basedOn" | "uiPriority" | "semiHidden" | "unhideWhenUsed" | "qFormat"
            | "rsid" => true,
            "pPr" => {
                child.attrs.is_empty()
                    && child.child_els().all(|p| {
                        p.name == "spacing"
                            && attrs(p, &["before", "after", "line", "lineRule"])
                            && ["before", "after", "line"].into_iter().all(|key| {
                                p.attr(key).map_or(true, |value| {
                                    value.parse::<u32>().is_ok_and(|n| n <= 31_680)
                                })
                            })
                            && p.attr("lineRule")
                                .map_or(true, |rule| matches!(rule, "auto" | "exact" | "atLeast"))
                            && p.child_els().next().is_none()
                    })
            }
            "tblPr" => table_properties(child),
            _ => false,
        }) {
            return false;
        }
        next = style.based_on.as_deref();
    }
    true
}

// These paragraph switches only request behavior when on. An explicit off value is safe
// to ignore; unknown attributes, malformed XML, and enabled switches remain diagnosed.
fn disabled_paragraph_switch(raw: &str) -> bool {
    let Ok(element) = edgedoc_opc::xml::parse(raw.as_bytes(), &edgedoc::Budget::default()) else {
        return false;
    };
    matches!(
        element.name.as_str(),
        "autoSpaceDE" | "autoSpaceDN" | "adjustRightInd"
    ) && element.attrs.len() == 1
        && matches!(element.attr("val"), Some("0" | "false" | "off"))
        && element.children.is_empty()
}

/// Surface known fidelity gaps when the document actually requests them. Preserving markup
/// during a DOCX save does not establish that it was rendered.
fn diagnose_layout_gaps(doc: &Document, body: &[Block], diagnostics: &mut Vec<Diagnostic>) {
    use std::collections::BTreeSet;
    fn scan(
        block: &Block,
        styles: &edgedoc_model::StyleSheet,
        features: &mut BTreeSet<&'static str>,
    ) {
        match block {
            Block::Paragraph(p) => {
                if p.props.bidi
                    || p.runs
                        .iter()
                        .any(|run| run.props.rtl || run.props.complex_script)
                    || p.text.to_string().chars().any(|ch| {
                        matches!(ch,
                        '\u{0590}'..='\u{08FF}' | '\u{FB1D}'..='\u{FDFF}' |
                        '\u{FE70}'..='\u{FEFF}' | '\u{1EE00}'..='\u{1EEFF}')
                    })
                {
                    features.insert("Complex-script and bidirectional layout");
                }
                if p.props.frame.is_some() {
                    features.insert("Paragraph frames and drop caps");
                }
                if p.props
                    .extra
                    .iter()
                    .any(|raw| !disabled_paragraph_switch(raw))
                {
                    features.insert("Extended paragraph properties");
                }
                if p.retention.revisions {
                    features.insert("Tracked revision presentation");
                }
                for run in &p.runs {
                    let r = &run.props;
                    if r.emboss || r.imprint || r.outline || r.shadow || r.border.is_some() {
                        features.insert("Character effects and borders");
                    }
                    if r.east_asian_layout.is_some()
                        || r.fit_text.is_some()
                        || r.extra.iter().any(|raw| {
                            !edgedoc_opc::xml::parse(raw.as_bytes(), &edgedoc::Budget::default())
                                .ok()
                                .is_some_and(|element| {
                                    element.name == "u" && element.attr("val").is_none()
                                })
                        })
                    {
                        features.insert("Extended character typography");
                    }
                }
                for object in &p.objects {
                    match object {
                        InlineObject::Picture(pic) if pic.anchor.is_some() => {
                            features.insert("Floating picture positioning and text wrap");
                        }
                        InlineObject::Note { .. } => {
                            features.insert("Footnote, endnote, and comment stories");
                        }
                        InlineObject::Opaque(xml) if !nonpainting_markup(xml) => {
                            features.insert("Embedded drawings, equations, and text boxes");
                        }
                        InlineObject::FieldStart { .. } => {
                            features.insert(
                                "Fields other than page and section counters use cached results",
                            );
                        }
                        InlineObject::Field(field) => {
                            // Wordcraft displays CREATEDATE's stored result unchanged; it does
                            // not recalculate it from the package metadata during layout.
                            let command = field
                                .instr
                                .split_whitespace()
                                .next()
                                .unwrap_or("")
                                .to_ascii_uppercase();
                            if !matches!(
                                command.as_str(),
                                "PAGE"
                                    | "NUMPAGES"
                                    | "SECTION"
                                    | "SECTIONPAGES"
                                    | "ASK"
                                    | "SET"
                                    | "TITLE"
                                    | "AUTHOR"
                                    | "FILENAME"
                                    | "CREATEDATE"
                            ) {
                                features.insert("Fields other than page and section counters use cached results");
                            }
                        }
                        _ => {}
                    }
                }
            }
            Block::Table(t) => {
                if t.position.is_some() {
                    features.insert("Floating table positioning and text wrap");
                }
                if t.style
                    .as_deref()
                    .is_some_and(|id| !supported_plain_table_style(styles, id))
                {
                    features.insert("Conditional table styles");
                }
                if t.cell_space.is_some_and(|v| v.0 != 0) {
                    features.insert("Table cell spacing");
                }
                for row in &t.rows {
                    for cell in &row.cells {
                        if cell.v_merge.is_some() {
                            features.insert("Vertically merged table cells");
                        }
                        if cell.text_direction.is_some() || cell.fit_text || cell.no_wrap {
                            features.insert("Table cell text direction and fitting");
                        }
                        for b in &cell.blocks {
                            scan(b, styles, features);
                        }
                    }
                }
            }
            Block::Control(c) => {
                for b in &c.blocks {
                    scan(b, styles, features);
                }
            }
            Block::Alternate { choice, .. } => {
                for b in choice {
                    scan(b, styles, features);
                }
            }
            Block::Opaque(_) => {
                features.insert("Unmodeled block markup");
            }
        }
    }
    fn section(s: &SectionProps, features: &mut BTreeSet<&'static str>) {
        if s.doc_grid
            .as_ref()
            .and_then(|grid| grid.grid_type.as_deref())
            .is_some_and(|kind| kind != "default")
        {
            features.insert("Document grid");
        }
        if s.line_numbering.is_some() {
            features.insert("Line numbering");
        }
        if !s.page_borders.is_empty() {
            features.insert("Page borders");
        }
        if s.text_direction.as_deref().is_some_and(|v| v != "lrTb") {
            features.insert("Vertical section text");
        }
        if s.v_align.as_deref() == Some("both") {
            features.insert("Vertical page justification");
        }
    }
    let mut features = BTreeSet::new();
    section(doc.section(), &mut features);
    for block in body {
        scan(block, doc.styles(), &mut features);
        if let Block::Paragraph(p) = block {
            if let Some(break_) = &p.section {
                section(&break_.props, &mut features);
            }
        }
    }
    for kind in [PartKind::Header, PartKind::Footer] {
        for id in doc.parts(kind) {
            for block in doc.part_blocks(id) {
                scan(&block, doc.styles(), &mut features);
            }
        }
    }
    diagnostics.extend(features.into_iter().map(|feature| Diagnostic {
        code: "display.unsupported_layout",
        severity: Severity::Warning,
        page: None,
        detail: feature.to_string(),
    }));
}

fn configure_default_style(doc: &Document, cascade: &mut Cascade) {
    if !doc.has_styles_part() {
        for name in ["Header", "Footer"] {
            let mut style = edgedoc_model::Style::paragraph(name);
            style.para_props.spacing_after = Some(edgedoc_model::Twips(0));
            style.para_props.line = Some(edgedoc_model::Twips(240));
            style.para_props.line_rule = Some("auto".into());
            style.para_props.tabs = vec![
                edgedoc_model::TabStop {
                    val: "center".into(),
                    pos: edgedoc_model::Twips(4680),
                    leader: None,
                },
                edgedoc_model::TabStop {
                    val: "right".into(),
                    pos: edgedoc_model::Twips(9360),
                    leader: None,
                },
            ];
            cascade.add_fallback_style(style);
        }
    }
    if let Some(xml) = doc.theme_xml() {
        if let Ok(root) = edgedoc_opc::xml::parse(xml.as_bytes(), &edgedoc::Budget::default()) {
            if let Some(scheme) = root
                .children("themeElements")
                .next()
                .and_then(|e| e.children("fontScheme").next())
            {
                for (name, slot) in [("majorFont", "+mj-lt"), ("minorFont", "+mn-lt")] {
                    if let Some(family) = scheme
                        .children(name)
                        .next()
                        .and_then(|e| e.children("latin").next())
                        .and_then(|e| e.attr("typeface"))
                    {
                        cascade.set_theme_font(slot, family);
                    }
                }
            }
        }
    }
    if doc.styles().by_id("Normal").is_some() {
        return;
    }
    for style in &doc.styles().styles {
        if style.kind != edgedoc_model::StyleKind::Paragraph {
            continue;
        }
        if let Some(raw) = &style.raw_xml {
            if let Ok(root) = edgedoc_opc::xml::parse(raw.as_bytes(), &edgedoc::Budget::default()) {
                if matches!(root.attr("default"), Some("1" | "true" | "on")) {
                    cascade.set_default_style(&style.id);
                    break;
                }
            }
        }
    }
}

/// These elements annotate text but have no print-layout extent. Keep their markup in the
/// model; do not replace proofing/permission/bookmark ranges with visible object boxes.
fn nonpainting_markup(raw: &str) -> bool {
    edgedoc_opc::xml::parse(raw.as_bytes(), &edgedoc::Budget::default()).is_ok_and(|root| {
        matches!(
            root.name.as_str(),
            "proofErr"
                | "permStart"
                | "permEnd"
                | "commentRangeStart"
                | "commentRangeEnd"
                | "lastRenderedPageBreak"
        )
    })
}

fn flatten_sections<'a>(blocks: &'a [Block], out: &mut Vec<&'a Block>) {
    for block in blocks {
        match block {
            Block::Control(control) => {
                for inner in &control.blocks {
                    flatten_sections(std::slice::from_ref(inner.as_ref()), out);
                }
            }
            Block::Alternate { choice, .. } => {
                for inner in choice {
                    flatten_sections(std::slice::from_ref(inner.as_ref()), out);
                }
            }
            _ => out.push(block),
        }
    }
}

/// Word's TOC links keep their targets, but their built-in Hyperlink character
/// style is suppressed in the cached TOC result. Work on the display copy so
/// serializing the source document retains its original formatting.
fn unstyle_toc_links(blocks: &mut [Block], styles: &edgedoc_model::StyleSheet) {
    use std::collections::HashSet;
    use std::sync::Arc;

    let mut ids: HashSet<String> = styles
        .styles
        .iter()
        .filter(|style| {
            style.kind == edgedoc_model::StyleKind::Character
                && style.name.eq_ignore_ascii_case("Hyperlink")
        })
        .map(|style| style.id.clone())
        .collect();
    ids.insert("Hyperlink".to_string());

    fn walk(blocks: &mut [Block], ids: &HashSet<String>, ranges: &mut Vec<bool>) {
        for block in blocks {
            match block {
                Block::Paragraph(para) => {
                    if ranges.is_empty()
                        && !para
                            .objects
                            .iter()
                            .any(|object| matches!(object, InlineObject::FieldStart { .. }))
                    {
                        continue;
                    }
                    let mut objects = para.objects.iter();
                    let mut runs = para.runs.iter();
                    let mut current = runs.next();
                    let mut end = current.map_or(0, |run| run.len);
                    let mut adjusted: Vec<edgedoc_model::Run> = Vec::new();
                    let mut changed = false;
                    let mut last_run_end = end;
                    for (offset, ch) in para.text.char_indices() {
                        while offset >= end {
                            current = runs.next();
                            let Some(run) = current else { break };
                            end = end.saturating_add(run.len);
                        }
                        let Some(run) = current else { break };
                        if ch == edgedoc_model::OBJ {
                            match objects.next() {
                                Some(InlineObject::FieldStart { instr, .. }) => ranges.push(
                                    instr
                                        .split_whitespace()
                                        .next()
                                        .is_some_and(|s| s.eq_ignore_ascii_case("TOC")),
                                ),
                                Some(InlineObject::FieldEnd) => {
                                    ranges.pop();
                                }
                                _ => {}
                            }
                        }
                        let mut props = run.props.clone();
                        if ranges.iter().any(|is_toc| *is_toc)
                            && props.link.is_some()
                            && props.style.as_ref().is_some_and(|id| ids.contains(id))
                        {
                            props.style = None;
                            changed = true;
                        }
                        if let Some(last) = adjusted
                            .last_mut()
                            .filter(|last| last.props == props && last_run_end == end)
                        {
                            last.len += ch.len_utf8();
                        } else {
                            adjusted.push(edgedoc_model::Run {
                                len: ch.len_utf8(),
                                props,
                            });
                        }
                        last_run_end = end;
                    }
                    if changed
                        && adjusted.iter().map(|run| run.len).sum::<usize>() == para.text.len()
                    {
                        para.runs = adjusted;
                    }
                }
                Block::Table(table) => {
                    for row in &mut table.rows {
                        for cell in &mut row.cells {
                            for block in &mut cell.blocks {
                                walk(std::slice::from_mut(Arc::make_mut(block)), ids, ranges);
                            }
                        }
                    }
                }
                Block::Control(control) => {
                    for block in &mut control.blocks {
                        walk(std::slice::from_mut(Arc::make_mut(block)), ids, ranges);
                    }
                }
                Block::Alternate { choice, .. } => {
                    for block in choice {
                        walk(std::slice::from_mut(Arc::make_mut(block)), ids, ranges);
                    }
                }
                Block::Opaque(_) => {}
            }
        }
    }
    walk(blocks, &ids, &mut Vec::new());
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

/// Number body references in reading order, using the prior layout's pages for page restarts.
fn set_note_numbers(
    blocks: &[Block],
    doc: &Document,
    settings: &DisplaySettings,
    pages: &std::collections::BTreeMap<i32, u32>,
) {
    use std::collections::{BTreeMap, BTreeSet};
    struct State<'a> {
        next: [u32; 2],
        marks: BTreeMap<(String, i32), String>,
        available: BTreeSet<(String, i32)>,
        settings: &'a DisplaySettings,
        pages: &'a BTreeMap<i32, u32>,
        last_footnote_page: Option<u32>,
    }
    fn walk(blocks: &[Block], section: &SectionProps, state: &mut State<'_>) {
        for block in blocks {
            match block {
                Block::Paragraph(para) => {
                    for object in &para.objects {
                        if let InlineObject::Note { kind, id } = object {
                            let (index, props, defaults, default_fmt) = match kind.as_str() {
                                "footnote" => {
                                    (0, &section.footnotes, &state.settings.footnotes, "decimal")
                                }
                                "endnote" => {
                                    (1, &section.endnotes, &state.settings.endnotes, "lowerRoman")
                                }
                                _ => continue,
                            };
                            let key = (kind.clone(), *id);
                            if !state.available.contains(&key) || state.marks.contains_key(&key) {
                                continue;
                            }
                            if index == 0 {
                                if let Some(page) = state.pages.get(id).copied() {
                                    if props
                                        .num_restart
                                        .as_deref()
                                        .or(defaults.num_restart.as_deref())
                                        == Some("eachPage")
                                        && state.last_footnote_page.is_some_and(|last| last != page)
                                    {
                                        state.next[index] = props
                                            .num_start
                                            .or(defaults.num_start)
                                            .unwrap_or(1)
                                            .clamp(1, 32_767);
                                    }
                                    state.last_footnote_page = Some(page);
                                }
                            }
                            let n = state.next[index];
                            state.next[index] = n.saturating_add(1);
                            state.marks.insert(
                                key,
                                edgedoc_flow::format_page_number(
                                    n,
                                    props
                                        .num_fmt
                                        .as_deref()
                                        .or(defaults.num_fmt.as_deref())
                                        .unwrap_or(default_fmt),
                                ),
                            );
                        }
                    }
                }
                Block::Table(table) => {
                    for row in &table.rows {
                        for cell in &row.cells {
                            for block in &cell.blocks {
                                walk(std::slice::from_ref(block.as_ref()), section, state);
                            }
                        }
                    }
                }
                Block::Control(control) => {
                    for block in &control.blocks {
                        walk(std::slice::from_ref(block.as_ref()), section, state);
                    }
                }
                Block::Alternate { choice, .. } => {
                    for block in choice {
                        walk(std::slice::from_ref(block.as_ref()), section, state);
                    }
                }
                Block::Opaque(_) => {}
            }
        }
    }
    let mut available = BTreeSet::new();
    for (kind, part_kind) in [
        ("footnote", PartKind::Footnote),
        ("endnote", PartKind::Endnote),
    ] {
        for part in doc.parts(part_kind) {
            if let Some(id) = doc.part_meta(part).and_then(|meta| meta.id) {
                available.insert((kind.to_string(), id));
            }
        }
    }
    let mut flat = Vec::new();
    flatten_sections(blocks, &mut flat);
    let mut sections = vec![doc.section(); flat.len()];
    let mut following = doc.section();
    for (index, block) in flat.iter().enumerate().rev() {
        if let Block::Paragraph(para) = block {
            if let Some(section) = &para.section {
                following = &section.props;
            }
        }
        sections[index] = following;
    }
    let mut state = State {
        next: [1, 1],
        marks: BTreeMap::new(),
        available,
        settings,
        pages,
        last_footnote_page: None,
    };
    let mut initialized = false;
    let mut section_start = true;
    for (block, section) in flat.into_iter().zip(sections) {
        if section_start {
            for (index, (props, defaults)) in [
                (&section.footnotes, &settings.footnotes),
                (&section.endnotes, &settings.endnotes),
            ]
            .into_iter()
            .enumerate()
            {
                if !initialized
                    || props
                        .num_restart
                        .as_deref()
                        .or(defaults.num_restart.as_deref())
                        == Some("eachSect")
                {
                    state.next[index] = props
                        .num_start
                        .or(defaults.num_start)
                        .unwrap_or(1)
                        .clamp(1, 32_767);
                }
            }
            initialized = true;
            section_start = false;
        }
        walk(std::slice::from_ref(block), section, &mut state);
        if matches!(block, Block::Paragraph(para) if para.section.is_some()) {
            section_start = true;
        }
    }
    NOTE_NUMBERS.with(|numbers| *numbers.borrow_mut() = state.marks);
}

thread_local! {
    static FIELD_METADATA: std::cell::RefCell<(String, String)> = const {
        std::cell::RefCell::new((String::new(), String::new()))
    };
    static NOTE_NUMBERS: std::cell::RefCell<
        std::collections::BTreeMap<(String, i32), String>,
    > = const { std::cell::RefCell::new(std::collections::BTreeMap::new()) };
    static FOOTNOTE_STORIES: std::cell::RefCell<
        std::collections::BTreeMap<i32, std::sync::Arc<[FlowBlock]>>,
    > = const { std::cell::RefCell::new(std::collections::BTreeMap::new()) };
    static FOOTNOTE_SEPARATOR: std::cell::RefCell<Option<std::sync::Arc<[FlowBlock]>>> = const { std::cell::RefCell::new(None) };
    static FOOTNOTE_CONTINUATION_SEPARATOR: std::cell::RefCell<Option<std::sync::Arc<[FlowBlock]>>> = const { std::cell::RefCell::new(None) };
    static FOOTNOTE_CONTINUATION_NOTICE: std::cell::RefCell<Option<std::sync::Arc<[FlowBlock]>>> = const { std::cell::RefCell::new(None) };
    static CURRENT_NOTE_MARK: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

fn build_footnote_stories(
    doc: &Document,
    fonts: &edgedoc_fonts::FontDb,
    diagnostics: &mut Vec<Diagnostic>,
    settings: &DisplaySettings,
    defaults: &(edgedoc_model::CharProps, edgedoc_model::ParaProps),
) {
    FOOTNOTE_STORIES.with(|stories| stories.borrow_mut().clear());
    FOOTNOTE_SEPARATOR.with(|separator| *separator.borrow_mut() = None);
    FOOTNOTE_CONTINUATION_SEPARATOR.with(|separator| *separator.borrow_mut() = None);
    FOOTNOTE_CONTINUATION_NOTICE.with(|notice| *notice.borrow_mut() = None);
    let known = fonts
        .faces()
        .iter()
        .map(|face| face.family.clone())
        .collect::<Vec<_>>();
    let mut cascade = Cascade::new(doc.styles(), &known);
    configure_default_style(doc, &mut cascade);
    cascade.set_default_tab(settings.default_tab);
    cascade.set_compatibility_mode(settings.compatibility_mode);
    cascade.set_default_properties(&defaults.0, &defaults.1);
    load_numbering(doc, &mut cascade);
    for part in doc.parts(PartKind::Footnote) {
        let kind = doc
            .part_meta(part)
            .and_then(|meta| meta.note_type.as_deref());
        if kind == Some("continuationNotice") {
            let mut blocks = Vec::new();
            for source in doc.part_blocks(part) {
                build_one(&mut cascade, &source, &mut blocks, diagnostics, false);
            }
            for (index, block) in blocks.iter_mut().enumerate() {
                if let FlowBlock::Paragraph(para) = block {
                    para.path = format!("footnote-continuation-notice[{index}]");
                }
            }
            FOOTNOTE_CONTINUATION_NOTICE.with(|notice| *notice.borrow_mut() = Some(blocks.into()));
            continue;
        }
        let continuation = match kind {
            Some("separator") => false,
            Some("continuationSeparator") => true,
            _ => continue,
        };
        let mut blocks = Vec::new();
        for source in doc.part_blocks(part) {
            let has_rule = matches!(&source, Block::Paragraph(para) if para.objects.iter().any(|object| matches!(object, InlineObject::NoteSeparator { continuation: value } if *value == continuation)));
            let start = blocks.len();
            build_one(&mut cascade, &source, &mut blocks, diagnostics, false);
            for (index, block) in blocks.iter_mut().enumerate().skip(start) {
                if let FlowBlock::Paragraph(para) = block {
                    para.path = match (continuation, has_rule) {
                        (true, true) => format!("footnote-continuation-separator[{index}]"),
                        (false, true) => format!("footnote-separator[{index}]"),
                        (true, false) => {
                            format!("footnote-continuation-separator-content[{index}]")
                        }
                        (false, false) => format!("footnote-separator-content[{index}]"),
                    };
                }
            }
        }
        if continuation {
            FOOTNOTE_CONTINUATION_SEPARATOR
                .with(|separator| *separator.borrow_mut() = Some(blocks.into()));
        } else {
            FOOTNOTE_SEPARATOR.with(|separator| *separator.borrow_mut() = Some(blocks.into()));
        }
    }
    for part in doc.parts(PartKind::Footnote) {
        let Some(meta) = doc.part_meta(part) else {
            continue;
        };
        if meta
            .note_type
            .as_deref()
            .is_some_and(|kind| kind != "normal")
        {
            continue;
        }
        let Some(id) = meta.id else { continue };
        let mark = NOTE_NUMBERS
            .with(|numbers| numbers.borrow().get(&("footnote".to_string(), id)).cloned());
        let Some(mark) = mark else { continue };
        CURRENT_NOTE_MARK.with(|current| *current.borrow_mut() = Some(mark));
        let mut blocks = Vec::new();
        for source in doc.part_blocks(part) {
            build_one(&mut cascade, &source, &mut blocks, diagnostics, false);
        }
        CURRENT_NOTE_MARK.with(|current| *current.borrow_mut() = None);
        for (index, block) in blocks.iter_mut().enumerate() {
            let path = format!("footnote[{id}][{index}]");
            match block {
                FlowBlock::Paragraph(para) => para.path = path,
                FlowBlock::Table(table) => assign_cell_paths(table, &path),
                _ => {}
            }
        }
        FOOTNOTE_STORIES.with(|stories| {
            stories.borrow_mut().insert(id, blocks.into());
        });
    }
}

fn build_endnote_stories(
    doc: &Document,
    fonts: &edgedoc_fonts::FontDb,
    diagnostics: &mut Vec<Diagnostic>,
    settings: &DisplaySettings,
    defaults: &(edgedoc_model::CharProps, edgedoc_model::ParaProps),
) -> (
    std::collections::BTreeMap<i32, Vec<FlowBlock>>,
    std::sync::Arc<[FlowBlock]>,
) {
    let known = fonts
        .faces()
        .iter()
        .map(|face| face.family.clone())
        .collect::<Vec<_>>();
    let mut cascade = Cascade::new(doc.styles(), &known);
    configure_default_style(doc, &mut cascade);
    cascade.set_default_tab(settings.default_tab);
    cascade.set_compatibility_mode(settings.compatibility_mode);
    cascade.set_default_properties(&defaults.0, &defaults.1);
    load_numbering(doc, &mut cascade);
    let mut stories = std::collections::BTreeMap::new();
    let mut separator = Vec::new();
    for part in doc.parts(PartKind::Endnote) {
        let Some(meta) = doc.part_meta(part) else {
            continue;
        };
        if meta.note_type.as_deref() == Some("separator") {
            for source in doc.part_blocks(part) {
                let has_rule = matches!(&source, Block::Paragraph(para) if para.objects.iter().any(|object| matches!(object, InlineObject::NoteSeparator { continuation: false })));
                let start = separator.len();
                build_one(&mut cascade, &source, &mut separator, diagnostics, false);
                for (index, block) in separator.iter_mut().enumerate().skip(start) {
                    if let FlowBlock::Paragraph(para) = block {
                        para.path = if has_rule {
                            format!("footnote-separator[{index}]")
                        } else {
                            format!("endnote-separator-content[{index}]")
                        };
                    }
                }
            }
            continue;
        }
        if meta
            .note_type
            .as_deref()
            .is_some_and(|kind| kind != "normal")
        {
            continue;
        }
        let Some(id) = meta.id else { continue };
        let mark = NOTE_NUMBERS
            .with(|numbers| numbers.borrow().get(&("endnote".to_string(), id)).cloned());
        let Some(mark) = mark else { continue };
        CURRENT_NOTE_MARK.with(|current| *current.borrow_mut() = Some(mark));
        let mut blocks = Vec::new();
        for source in doc.part_blocks(part) {
            build_one(&mut cascade, &source, &mut blocks, diagnostics, false);
        }
        CURRENT_NOTE_MARK.with(|current| *current.borrow_mut() = None);
        for (index, block) in blocks.iter_mut().enumerate() {
            let path = format!("endnote[{id}][{index}]");
            match block {
                FlowBlock::Paragraph(para) => para.path = path,
                FlowBlock::Table(table) => assign_cell_paths(table, &path),
                _ => {}
            }
        }
        stories.insert(id, blocks);
    }
    (stories, separator.into())
}

fn collect_endnote_ids(
    block: &Block,
    pending: &mut Vec<i32>,
    seen: &mut std::collections::BTreeSet<i32>,
    stories: &std::collections::BTreeMap<i32, Vec<FlowBlock>>,
) {
    match block {
        Block::Paragraph(para) => {
            for object in &para.objects {
                if let InlineObject::Note { kind, id } = object {
                    if kind == "endnote" && stories.contains_key(id) && seen.insert(*id) {
                        pending.push(*id);
                    }
                }
            }
        }
        Block::Table(table) => {
            for row in &table.rows {
                for cell in &row.cells {
                    for child in &cell.blocks {
                        collect_endnote_ids(child, pending, seen, stories);
                    }
                }
            }
        }
        Block::Control(control) => {
            for child in &control.blocks {
                collect_endnote_ids(child, pending, seen, stories);
            }
        }
        Block::Alternate { choice, .. } => {
            for child in choice {
                collect_endnote_ids(child, pending, seen, stories);
            }
        }
        Block::Opaque(_) => {}
    }
}

fn append_endnotes(
    blocks: &mut Vec<FlowBlock>,
    pending: &mut Vec<i32>,
    stories: &std::collections::BTreeMap<i32, Vec<FlowBlock>>,
    separator: &std::sync::Arc<[FlowBlock]>,
) {
    if pending.is_empty() {
        return;
    }
    blocks.push(FlowBlock::EndnoteSeparator {
        path: format!("endnote-separator[{}]", pending[0]),
        blocks: separator.clone(),
    });
    for id in pending.drain(..) {
        if let Some(story) = stories.get(&id) {
            blocks.extend(story.iter().cloned());
        }
    }
}

fn empty_paragraph(cascade: &mut Cascade) -> FlowParagraph {
    let para = Paragraph::new();
    paragraph_of(&para, cascade, 0)
}

#[derive(Default)]
struct DisplaySettings {
    even_and_odd: bool,
    mirror_margins: bool,
    default_tab: f32,
    compatibility_mode: u32,
    footnotes: edgedoc_model::FootnotePr,
    endnotes: edgedoc_model::FootnotePr,
}

impl DisplaySettings {
    fn of(doc: &Document) -> Self {
        let mut out = Self {
            default_tab: 36.0,
            compatibility_mode: 12,
            ..Self::default()
        };
        if let Some(xml) = doc.settings_xml() {
            if let Ok(root) = edgedoc_opc::xml::parse(xml.as_bytes(), &edgedoc::Budget::default()) {
                for el in root.child_els() {
                    let enabled = !matches!(el.attr("val"), Some("0" | "false" | "off"));
                    match el.name.as_str() {
                        "evenAndOddHeaders" => out.even_and_odd = enabled,
                        "mirrorMargins" => out.mirror_margins = enabled,
                        "footnotePr" | "endnotePr" => {
                            let props = if el.name == "footnotePr" {
                                &mut out.footnotes
                            } else {
                                &mut out.endnotes
                            };
                            for child in el.child_els() {
                                match child.name.as_str() {
                                    "numFmt" => {
                                        props.num_fmt = child.attr("val").map(str::to_string)
                                    }
                                    "numStart" => {
                                        props.num_start =
                                            child.attr("val").and_then(|v| v.parse().ok())
                                    }
                                    "numRestart" => {
                                        props.num_restart = child.attr("val").map(str::to_string)
                                    }
                                    "pos" => props.pos = child.attr("val").map(str::to_string),
                                    _ => {}
                                }
                            }
                        }
                        "defaultTabStop" => {
                            if let Some(v) = el.attr("val").and_then(|v| v.parse::<u32>().ok()) {
                                out.default_tab = (v as f32 / 20.0).max(1.0);
                            }
                        }
                        "compat" => {
                            if let Some(mode) = el
                                .children("compatSetting")
                                .find(|setting| {
                                    setting.attr("name") == Some("compatibilityMode")
                                        && setting.attr("uri").map_or(true, |uri| {
                                            uri == "http://schemas.microsoft.com/office/word"
                                        })
                                })
                                .and_then(|setting| setting.attr("val"))
                                .and_then(|value| value.parse::<u32>().ok())
                            {
                                out.compatibility_mode = mode.clamp(11, 99);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        out
    }
}

/// Read the numbering part into the cascade. See `DP-S-14` and `DP-S-15`.
fn load_numbering(doc: &Document, cascade: &mut Cascade) {
    let Some(xml) = doc.numbering_xml() else {
        return;
    };
    let Ok(root) = edgedoc_opc::xml::parse(xml.as_bytes(), &edgedoc::Budget::default()) else {
        return;
    };
    let mut abstracts = std::collections::BTreeMap::new();
    for element in root.children("abstractNum") {
        let Some(id) = element
            .attr("abstractNumId")
            .and_then(|s| s.parse::<u32>().ok())
        else {
            continue;
        };
        let mut def = NumDef {
            abstract_id: id,
            ..NumDef::default()
        };
        for level in element.children("lvl") {
            let Some(index) = level
                .attr("ilvl")
                .and_then(|s| s.parse::<u32>().ok())
                .filter(|v| *v < 9)
            else {
                continue;
            };
            def.levels.insert(index, read_num_level(level));
        }
        abstracts.insert(id, def);
    }
    for element in root.children("num") {
        let Some(id) = element.attr("numId").and_then(|s| s.parse::<u32>().ok()) else {
            continue;
        };
        let Some(abstract_id) =
            child_value(element, "abstractNumId").and_then(|s| s.parse::<u32>().ok())
        else {
            continue;
        };
        let Some(mut def) = abstracts.get(&abstract_id).cloned() else {
            continue;
        };
        for over in element.children("lvlOverride") {
            let Some(level) = over
                .attr("ilvl")
                .and_then(|s| s.parse::<u32>().ok())
                .filter(|v| *v < 9)
            else {
                continue;
            };
            if let Some(replacement) = over.children("lvl").next() {
                def.levels.insert(level, read_num_level(replacement));
                if let Some(start) =
                    child_value(replacement, "start").and_then(|s| s.parse::<u32>().ok())
                {
                    def.start_overrides.insert(level, start);
                }
            }
            if let Some(start) =
                child_value(over, "startOverride").and_then(|s| s.parse::<u32>().ok())
            {
                def.levels.entry(level).or_default().start = start;
                def.start_overrides.insert(level, start);
            }
        }
        cascade.add_numbering(id, def);
    }
}

fn child_value<'a>(element: &'a edgedoc_opc::xml::El, name: &'a str) -> Option<&'a str> {
    element.children(name).next()?.attr("val")
}

fn read_num_level(element: &edgedoc_opc::xml::El) -> NumLevel {
    let indent = element
        .children("pPr")
        .next()
        .and_then(|p| p.children("ind").next());
    let to_points = |name: &str| {
        indent
            .and_then(|p| p.attr(name))
            .and_then(|s| s.parse::<i32>().ok())
            .map_or(0.0, |v| v as f32 / 20.0)
    };
    let legal = element
        .children("isLgl")
        .next()
        .is_some_and(|e| !matches!(e.attr("val"), Some("0" | "false" | "off")));
    let run_props = element.children("rPr").next();
    let toggle = |name| {
        run_props
            .and_then(|r| r.children(name).next())
            .map(|v| !matches!(v.attr("val"), Some("0" | "false" | "off")))
    };
    NumLevel {
        format: child_value(element, "numFmt").unwrap_or("decimal").into(),
        text: child_value(element, "lvlText").unwrap_or("%1.").into(),
        start: child_value(element, "start")
            .and_then(|s| s.parse().ok())
            .unwrap_or(1),
        indent_left: to_points("left"),
        indent_hanging: if indent.is_some_and(|p| p.attr("hanging").is_some()) {
            to_points("hanging")
        } else {
            -to_points("firstLine")
        },
        tab: element
            .children("pPr")
            .next()
            .and_then(|p| p.children("tabs").next())
            .and_then(|tabs| tabs.children("tab").find(|t| t.attr("val") == Some("num")))
            .and_then(|t| t.attr("pos"))
            .and_then(|s| s.parse::<i32>().ok())
            .map(|v| v as f32 / 20.0),
        font: element
            .children("rPr")
            .next()
            .and_then(|p| p.children("rFonts").next())
            .and_then(|p| p.attr("ascii"))
            .map(str::to_string),
        suffix: child_value(element, "suff").unwrap_or("tab").into(),
        justification: child_value(element, "lvlJc").unwrap_or("left").into(),
        is_legal: legal,
        caps: element
            .children("rPr")
            .next()
            .and_then(|r| r.children("caps").next())
            .is_some_and(|caps| !matches!(caps.attr("val"), Some("0" | "false" | "off"))),
        bold: toggle("b"),
        italic: toggle("i"),
        size: run_props
            .and_then(|r| child_value(r, "sz"))
            .and_then(|v| v.parse::<f32>().ok())
            .map(|v| v / 2.0),
    }
}

/// Story parts are addressed by model id, never by their order in the package.
fn build_story_parts(
    doc: &Document,
    fonts: &edgedoc_fonts::FontDb,
    diagnostics: &mut Vec<Diagnostic>,
    settings: &DisplaySettings,
    defaults: &(edgedoc_model::CharProps, edgedoc_model::ParaProps),
) -> std::collections::BTreeMap<u32, Vec<FlowBlock>> {
    let mut cascade = Cascade::new(
        doc.styles(),
        &fonts
            .faces()
            .iter()
            .map(|f| f.family.clone())
            .collect::<Vec<_>>(),
    );
    configure_default_style(doc, &mut cascade);
    cascade.set_default_tab(settings.default_tab);
    cascade.set_compatibility_mode(settings.compatibility_mode);
    cascade.set_default_properties(&defaults.0, &defaults.1);
    load_numbering(doc, &mut cascade);
    let mut parts = std::collections::BTreeMap::new();
    for kind in [PartKind::Header, PartKind::Footer] {
        for id in doc.parts(kind) {
            let mut source = doc.part_blocks(id);
            let mut blocks = Vec::new();
            for block in &mut source {
                let footer_frame = if kind == PartKind::Footer {
                    match block {
                        Block::Paragraph(para) => align_margin_footer_page_field(para),
                        _ => false,
                    }
                } else {
                    false
                };
                let index = blocks.len();
                build_one(&mut cascade, block, &mut blocks, diagnostics, false);
                if footer_frame {
                    if let Some(FlowBlock::Paragraph(para)) = blocks.get_mut(index) {
                        para.para.footer_page_frame = true;
                    }
                }
            }
            for (index, block) in blocks.iter_mut().enumerate() {
                let path = format!("part[{id}]/block[{index}]");
                match block {
                    FlowBlock::Table(table) => assign_cell_paths(table, &path),
                    FlowBlock::Paragraph(para) => para.path = path,
                    _ => {}
                }
            }
            parts.insert(id, blocks);
        }
    }
    parts
}

/// A common Word footer uses an auto-sized margin-aligned frame containing
/// only a PAGE field. In this bounded case, aligning the paragraph against the
/// margin gives the same position; general frame wrapping remains diagnosed.
fn align_margin_footer_page_field(para: &mut Paragraph) -> bool {
    let Some(frame) = &para.props.frame else {
        return false;
    };
    let page_field = matches!(para.objects.as_slice(), [InlineObject::Field(field)]
        if field.instr.split_whitespace().next().is_some_and(|s| s.eq_ignore_ascii_case("PAGE")));
    let only_field = para
        .text
        .chars()
        .all(|c| c == edgedoc_model::OBJ || c.is_whitespace());
    if page_field
        && only_field
        && frame.h_anchor.as_deref() == Some("margin")
        && frame.x_align.as_deref() == Some("right")
        && frame.drop_cap.is_none()
        && frame.width.is_none()
        && frame.height.is_none()
        && frame.x.is_none()
        && frame.y.map_or(true, |y| (-1..=1).contains(&y.0))
        && frame.y_align.is_none()
        && frame.h_space.is_none()
        && frame.v_space.is_none()
    {
        para.props.align = Some("right".into());
        true
    } else {
        false
    }
}

fn resolved_section(
    props: &SectionProps,
    settings: &DisplaySettings,
    parts: &std::collections::BTreeMap<u32, Vec<FlowBlock>>,
    previous: &FlowSection,
) -> FlowSection {
    let mut section = section_of(props);
    section.compatibility_mode = settings.compatibility_mode;
    section.even_and_odd = settings.even_and_odd;
    section.mirror_margins = settings.mirror_margins;
    // Missing references mean "link to previous", independently for each variant.
    let get = |id: Option<u32>, inherited: &Option<Vec<FlowBlock>>| {
        id.map(|id| parts.get(&id).cloned().unwrap_or_default())
            .or_else(|| inherited.clone())
    };
    section.header = get(props.headers.default, &previous.header);
    section.header_first = get(props.headers.first, &previous.header_first);
    section.header_even = get(props.headers.even, &previous.header_even);
    section.footer = get(props.footers.default, &previous.footer);
    section.footer_first = get(props.footers.first, &previous.footer_first);
    section.footer_even = get(props.footers.even, &previous.footer_even);
    section
}

fn section_of(s: &SectionProps) -> FlowSection {
    // Wordcraft normalizes signed page-margin distances before laying out the page.
    let limit = edgedoc_geom::twips(s.page_w.0.max(s.page_h.0));
    let margin = |value| edgedoc_geom::twips(value).abs().min(limit);
    FlowSection {
        compatibility_mode: 15,
        page_w: edgedoc_geom::twips(s.page_w.0),
        page_h: edgedoc_geom::twips(s.page_h.0),
        margin_top: margin(s.margin_top.0),
        margin_bottom: margin(s.margin_bottom.0),
        margin_left: margin(s.margin_left.0),
        margin_right: margin(s.margin_right.0),
        header_dist: margin(s.header_dist.0),
        footer_dist: margin(s.footer_dist.0),
        gutter: margin(s.gutter.0),
        columns: s.columns.max(1),
        column_space: s.column_space.map_or(36.0, |v| edgedoc_geom::twips(v.0)),
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
        title_page: s.title_page,
        even_and_odd: false,
        mirror_margins: false,
    }
}

fn build_one(
    cascade: &mut Cascade,
    block: &Block,
    out: &mut Vec<FlowBlock>,
    diagnostics: &mut Vec<Diagnostic>,
    page_anchors: bool,
) {
    match block {
        Block::Paragraph(para) => {
            // Floating pictures still use separate blocks until their wrap positioning is
            // supported. Inline pictures stay in the paragraph, including picture-only ones,
            // so their line retains the paragraph mark's descent and spacing.
            let standalone = !para.objects.is_empty() && para.text.chars().all(|c| c == '\u{FFFC}');
            if standalone
                && para.objects.iter().all(
                    |o| matches!(o, InlineObject::Picture(picture) if picture.anchor.is_some() && !(page_anchors && page_anchor(picture).is_some())),
                )
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
                            src: picture_source(picture),
                            align: picture
                                .anchor
                                .as_ref()
                                .map_or(Align::Left, |a| align_of(a.h_align.as_deref())),
                        });
                    }
                }
                return;
            }
            out.push(FlowBlock::Paragraph(paragraph_of_inner(
                para,
                cascade,
                out.len(),
                page_anchors,
            )));
            // Floating pictures retain their separate block until wrap positioning is supported.
            for (i, object) in para.objects.iter().enumerate() {
                match object {
                    InlineObject::Picture(picture)
                        if picture.anchor.is_some()
                            && !(page_anchors && page_anchor(picture).is_some()) =>
                    {
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
                            src: picture_source(picture),
                            align: Align::Left,
                        });
                    }
                    InlineObject::Opaque(kind) if !nonpainting_markup(kind) => {
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
                build_one(cascade, inner, out, diagnostics, page_anchors);
            }
        }
        Block::Alternate { choice, .. } => {
            for inner in choice {
                build_one(cascade, inner, out, diagnostics, page_anchors);
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

fn picture_source(picture: &edgedoc_model::Picture) -> [f32; 4] {
    let [left, top, right, bottom] = picture
        .crop
        .map(|value| value.clamp(0, 100_000) as f32 / 100_000.0);
    [
        left,
        top,
        (1.0 - left - right).max(0.0),
        (1.0 - top - bottom).max(0.0),
    ]
}

// Wordcraft aligns page-relative, non-wrapping floats independently of the line box.
// Restrict this first implementation to behind-text pictures in the body.
fn page_anchor(picture: &edgedoc_model::Picture) -> Option<edgedoc_flow::PageAnchorPicture> {
    let a = picture.anchor.as_ref()?;
    if !a.behind_text
        || a.wrap.as_deref() != Some("wrapNone")
        || a.h_anchor.as_deref() != Some("page")
        || a.v_anchor.as_deref() != Some("page")
    {
        return None;
    }
    fn axis(
        align: Option<&str>,
        offset: Option<f32>,
        vertical: bool,
    ) -> Option<edgedoc_flow::PageAnchorAxis> {
        match align {
            Some("left" | "top") => Some(edgedoc_flow::PageAnchorAxis::Start),
            Some("center") => Some(edgedoc_flow::PageAnchorAxis::Center),
            Some("right") if !vertical => Some(edgedoc_flow::PageAnchorAxis::End),
            Some("bottom") if vertical => Some(edgedoc_flow::PageAnchorAxis::End),
            None => Some(edgedoc_flow::PageAnchorAxis::Offset(offset.unwrap_or(0.0))),
            _ => None,
        }
    }
    Some(edgedoc_flow::PageAnchorPicture {
        resource: resource_id(picture),
        src: picture_source(picture),
        width: picture
            .width
            .map_or(60.0, |v| edgedoc_geom::emu(v.0))
            .clamp(1.0, 2000.0),
        height: picture
            .height
            .map_or(18.0, |v| edgedoc_geom::emu(v.0))
            .clamp(1.0, 2000.0),
        alt: picture.alt.clone().unwrap_or_default(),
        x: axis(
            a.h_align.as_deref(),
            a.x.map(|v| edgedoc_geom::emu(v.0)),
            false,
        )?,
        y: axis(
            a.v_align.as_deref(),
            a.y.map(|v| edgedoc_geom::emu(v.0)),
            true,
        )?,
    })
}

/// Build one paragraph. See `DP-S-29`.
pub fn paragraph_of(para: &Paragraph, cascade: &mut Cascade, index: usize) -> FlowParagraph {
    paragraph_of_inner(para, cascade, index, false)
}

fn paragraph_of_inner(
    para: &Paragraph,
    cascade: &mut Cascade,
    index: usize,
    page_anchors: bool,
) -> FlowParagraph {
    let mut computed = cascade.paragraph(para);
    if let Some(list) = &mut computed.list {
        if list.format == "bullet" {
            if let Some(font) = &list.font {
                list.text = edgedoc_fonts::symbols::map_text(font, &list.text);
            }
        }
    }
    if computed.list.is_some() {
        let mut mark = cascade.run(para, &para.props.mark);
        if let Some(list) = &computed.list {
            if let Some(bold) = list.bold {
                mark.bold = bold;
            }
            if let Some(italic) = list.italic {
                mark.italic = italic;
            }
            if let Some(size) = list.size {
                mark.size = size;
            }
            if let Some(font) = &list.font {
                mark.face_name = font.clone();
                mark.requested_face = font.clone();
            }
        }
        computed.label_fallback_face = face_id(&mark);
        computed.label_size = Some(mark.size);
        computed.label_kerning = mark.kern.is_some_and(|threshold| mark.size >= threshold);
    }
    computed.label_face = computed
        .list
        .as_ref()
        .and_then(|list| list.font.as_ref().map(|font| (list, font)))
        .and_then(|(list, font)| {
            FACE_HINTS.with(|hints| {
                hints
                    .borrow()
                    .get(&(
                        font.to_ascii_lowercase(),
                        list.bold.unwrap_or(false),
                        list.italic.unwrap_or(false),
                    ))
                    .copied()
            })
        });
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
    let mut inline_pictures = Vec::new();
    let mut page_pictures = Vec::new();
    let mut notes = Vec::new();
    let mut hyphen_styles = std::collections::BTreeMap::new();
    let mut small_caps_styles = std::collections::BTreeMap::new();
    let mut object_iter = para.objects.iter();
    for (i, ch) in para.text.char_indices() {
        let style = style_of_byte.get(i).copied().unwrap_or(0);
        if ch == '\u{FFFC}' {
            match object_iter.next() {
                Some(InlineObject::Field(field)) => {
                    // A page field takes the page number, which the flow writes. A cached result
                    // paints as text. See `DP-L-11`.
                    let shown = field_result(field);
                    if shown.is_empty() {
                        continue;
                    }
                    let start = text.len();
                    text.push_str(&shown);
                    push_run(&mut runs, start, text.len(), style);
                }
                Some(InlineObject::Picture(picture)) => {
                    let start = text.len();
                    text.push('\u{FFFC}');
                    let w = picture.width.map_or(60.0, |v| edgedoc_geom::emu(v.0));
                    let h = picture.height.map_or(18.0, |v| edgedoc_geom::emu(v.0));
                    if picture.anchor.is_none() {
                        let width = w.clamp(1.0, 2000.0);
                        let height = h.clamp(1.0, 2000.0);
                        let [left, top, right, bottom] = picture
                            .effect_extent
                            .map(|extent| extent.map(|value| edgedoc_geom::emu(value.0).max(0.0)))
                            .unwrap_or([0.0; 4]);
                        objects.push((width + left + right, height + top + bottom));
                        inline_pictures.push(Some(edgedoc_flow::InlinePicture {
                            resource: resource_id(picture),
                            src: picture_source(picture),
                            width,
                            height,
                            alt: picture.alt.clone().unwrap_or_default(),
                            effect_left: left,
                            effect_bottom: bottom,
                        }));
                    } else if let Some(anchored) =
                        page_anchors.then(|| page_anchor(picture)).flatten()
                    {
                        page_pictures.push((objects.len(), anchored));
                        objects.push((0.0, 0.0));
                        inline_pictures.push(None);
                    } else {
                        objects.push((h.max(8.0), 0.0));
                        inline_pictures.push(None);
                    }
                    push_run(&mut runs, start, text.len(), style);
                }
                Some(
                    InlineObject::Bookmark { .. }
                    | InlineObject::Hyperlink { .. }
                    | InlineObject::FieldStart { .. }
                    | InlineObject::FieldEnd
                    | InlineObject::InlineControlBoundary(_),
                ) => {}
                Some(InlineObject::Opaque(raw)) if nonpainting_markup(raw) => {}
                Some(InlineObject::PositionalTab {
                    alignment,
                    relative_to,
                    leader,
                }) => {
                    let mut tab_style = styles
                        .get(style as usize)
                        .cloned()
                        .unwrap_or_else(|| run_style_of(&ComputedRun::default()));
                    tab_style.positional_tab = Some(edgedoc_text::PositionalTab {
                        align: edgedoc_text::TabAlign::parse(alignment),
                        margin_relative: relative_to == "margin",
                        leader: leader.as_deref().and_then(|value| match value {
                            "dot" => Some('.'),
                            "hyphen" => Some('-'),
                            "underscore" | "heavy" => Some('_'),
                            "middleDot" => Some('\u{B7}'),
                            _ => None,
                        }),
                    });
                    let index = styles.len() as u32;
                    styles.push(tab_style);
                    let start = text.len();
                    text.push('\t');
                    push_run(&mut runs, start, text.len(), index);
                }
                Some(InlineObject::Note { kind, id }) => {
                    if kind == "comment" {
                        continue;
                    }
                    // A note reference paints its mark in its own run style.
                    let shown = if kind == "footnoteMark" || kind == "endnoteMark" {
                        CURRENT_NOTE_MARK.with(|mark| mark.borrow().clone())
                    } else {
                        NOTE_NUMBERS
                            .with(|numbers| numbers.borrow().get(&(kind.clone(), *id)).cloned())
                    };
                    if shown.is_none() && matches!(kind.as_str(), "footnote" | "endnote") {
                        continue;
                    }
                    let shown = shown.unwrap_or_else(|| "1".to_string());
                    let props = para
                        .runs
                        .get(style as usize)
                        .map_or(&para.props.mark, |run| &run.props);
                    let mut computed_mark = cascade.run(para, props);
                    computed_mark.vert_align = Some(edgedoc_style::VertAlign::Superscript);
                    let mark_style = styles.len() as u32;
                    styles.push(run_style_of(&computed_mark));
                    let start = text.len();
                    text.push_str(&shown);
                    push_run(&mut runs, start, text.len(), mark_style);
                    if kind == "footnote" && CURRENT_NOTE_MARK.with(|mark| mark.borrow().is_none())
                    {
                        if let Some(blocks) =
                            FOOTNOTE_STORIES.with(|stories| stories.borrow().get(id).cloned())
                        {
                            let separator = FOOTNOTE_SEPARATOR
                                .with(|separator| separator.borrow().clone())
                                .unwrap_or_default();
                            let continuation_separator = FOOTNOTE_CONTINUATION_SEPARATOR
                                .with(|separator| separator.borrow().clone())
                                .unwrap_or_default();
                            let continuation_notice = FOOTNOTE_CONTINUATION_NOTICE
                                .with(|notice| notice.borrow().clone())
                                .unwrap_or_default();
                            notes.push(FlowNote {
                                id: *id,
                                offset: start,
                                blocks,
                                separator,
                                continuation_separator,
                                continuation_notice,
                            });
                        }
                    }
                }
                Some(InlineObject::NoteSeparator { .. }) => {}
                Some(_) | None => {
                    let start = text.len();
                    text.push('\u{FFFC}');
                    objects.push((12.0, 0.0));
                    inline_pictures.push(None);
                    push_run(&mut runs, start, text.len(), style);
                }
            }
            continue;
        }
        let start = text.len();
        text.push(ch);
        let style = if ch == '\u{2011}' {
            let original = styles.get(style as usize);
            let fallback =
                original.and_then(|s| HYPHEN_FALLBACKS.with(|h| h.borrow().get(&s.face).copied()));
            if let Some((face, source_bold, source_italic)) = fallback {
                *hyphen_styles.entry(style).or_insert_with(|| {
                    let mut alternate = styles[style as usize].clone();
                    alternate.face = face;
                    alternate.synth_bold |= source_bold;
                    alternate.synth_italic |= source_italic;
                    let index = styles.len() as u32;
                    styles.push(alternate);
                    index
                })
            } else {
                style
            }
        } else {
            style
        };
        let style = if styles
            .get(style as usize)
            .is_some_and(|s| s.caps == edgedoc_text::CapsMode::Small)
        {
            let small = ch.is_lowercase();
            *small_caps_styles.entry((style, small)).or_insert_with(|| {
                let mut alternate = styles[style as usize].clone();
                alternate.caps = edgedoc_text::CapsMode::All;
                if small {
                    alternate.size *= 0.8;
                    alternate.line_size *= 0.8;
                }
                let index = styles.len() as u32;
                styles.push(alternate);
                index
            })
        } else {
            style
        };
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
    let has_total_field = para_has_field(para, "NUMPAGES") || para_has_field(para, "SECTIONPAGES");
    let mut links = Vec::new();
    for link in &para.links {
        links.push((0usize, text.len(), link.clone()));
    }
    let mark_style = styles.len() as u32;
    let mark = cascade.run(para, &para.props.mark);
    styles.push(run_style_of(&mark));
    FlowParagraph {
        path: block_path_len(index),
        text,
        runs,
        run_styles: styles,
        mark_style,
        para: computed.clone(),
        label,
        list,
        page_break_before: para.props.page_break_before,
        has_page_field,
        has_total_field,
        objects,
        inline_pictures,
        page_pictures,
        border_group_safe: para.props.borders_present
            && para.props.borders.between.is_none()
            && para.props.borders.bar.is_none()
            && para.props.borders.left.is_none()
            && para.props.borders.right.is_none()
            && para.props.borders.start.is_none()
            && para.props.borders.end.is_none()
            && para.props.borders.inside_h.is_none()
            && para.props.borders.inside_v.is_none()
            && para.props.borders.tl2_br.is_none()
            && para.props.borders.tr2_bl.is_none()
            && (computed.borders.top.is_some() || computed.borders.bottom.is_some())
            && computed.shading.is_none(),
        border_join_above: false,
        border_join_below: false,
        links,
        outline: computed.outline_level,
        notes,
        revision: revision_of(para),
    }
}

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
    let command = field
        .instr
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_ascii_uppercase();
    match command.as_str() {
        "NUMPAGES" => return edgedoc_flow::TOTAL_MARKER.to_string(),
        "SECTIONPAGES" => return edgedoc_flow::SECTION_TOTAL_MARKER.to_string(),
        "SECTION" => return edgedoc_flow::SECTION_MARKER.to_string(),
        "PAGE" => return edgedoc_flow::PAGE_MARKER.to_string(),
        // Assignment fields set a bookmark; their cached result is never displayed.
        "ASK" | "SET" => return String::new(),
        "TITLE" if field.result.is_empty() => {
            return FIELD_METADATA
                .with(|metadata| field_case(&field.instr, metadata.borrow().0.clone()));
        }
        "AUTHOR" if field.result.is_empty() => {
            return FIELD_METADATA
                .with(|metadata| field_case(&field.instr, metadata.borrow().1.clone()));
        }
        "FILENAME" if field.result.is_empty() => {
            return field_case(&field.instr, "Document1".to_string());
        }
        _ => {}
    }
    field.result.clone()
}

/// Apply Word's capitalization switches in order, ignoring quoted field arguments.
fn field_case(instr: &str, mut text: String) -> String {
    let mut chars = instr.chars().peekable();
    let mut quoted = false;
    while let Some(ch) = chars.next() {
        match ch {
            '\\' if quoted => {
                chars.next();
            }
            '"' => quoted = !quoted,
            '\\' if chars.peek() == Some(&'*') => {
                chars.next();
                while chars.next_if(|c| c.is_whitespace()).is_some() {}
                let mut switch = String::new();
                while let Some(c) = chars.next_if(|c| !c.is_whitespace() && *c != '\\' && *c != '"')
                {
                    switch.push(c);
                }
                match switch.to_ascii_lowercase().as_str() {
                    "upper" => text = text.to_uppercase(),
                    "lower" => text = text.to_lowercase(),
                    "caps" | "firstcap" => {
                        let first_only = switch.eq_ignore_ascii_case("firstcap");
                        let mut word_start = true;
                        let mut done = false;
                        let mut result = String::with_capacity(text.len());
                        for c in text.chars() {
                            if c.is_whitespace() {
                                word_start = true;
                                result.push(c);
                            } else if word_start && !done && c.is_alphanumeric() {
                                result.extend(c.to_uppercase());
                                word_start = false;
                                done = first_only;
                            } else {
                                if c.is_alphanumeric() {
                                    word_start = false;
                                }
                                result.push(c);
                            }
                        }
                        text = result;
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    text
}

/// One computed run as a flow run style. The colour, the underline, and the highlight travel with
/// it, so a heading paints blue and a muted note paints grey. See `DP-S-08`.
fn run_style_of(run: &ComputedRun) -> RunStyle {
    let face = face_id(run);
    let mut style = edgedoc_paint::lower::run_style_of(run, face);
    style.baseline_shift = baseline_shift(run);
    if run.vert_align.is_some() {
        style.size *= 0.65;
    }
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
            if field
                .instr
                .split_whitespace()
                .next()
                .is_some_and(|v| v.eq_ignore_ascii_case(name))
            {
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
        for family in edgedoc_fonts::substitute_candidates(&run.face_name) {
            for (bold, italic) in [
                (run.bold, run.italic),
                (run.bold, false),
                (false, run.italic),
                (false, false),
            ] {
                if let Some(id) = map.get(&(family.to_ascii_lowercase(), bold, italic)) {
                    return *id;
                }
            }
        }
        FACE_FALLBACKS.with(|fallbacks| {
            fallbacks
                .borrow()
                .get(&(run.bold, run.italic))
                .copied()
                .unwrap_or(0)
        })
    })
}

thread_local! {
    /// A family name to face id map, filled by the facade before a build. This keeps the style
    /// crate free of a font database, which `DP-A-06` requires.
    static FACE_HINTS: std::cell::RefCell<
        std::collections::BTreeMap<(String, bool, bool), u32>,
    > = const { std::cell::RefCell::new(std::collections::BTreeMap::new()) };
    static FACE_FALLBACKS: std::cell::RefCell<
        std::collections::BTreeMap<(bool, bool), u32>,
    > = const { std::cell::RefCell::new(std::collections::BTreeMap::new()) };
    /// A bounded fallback for the nonbreaking hyphen when a face omits that glyph.
    static HYPHEN_FALLBACKS: std::cell::RefCell<
        std::collections::BTreeMap<u32, (u32, bool, bool)>,
    > = const { std::cell::RefCell::new(std::collections::BTreeMap::new()) };
}

/// Publish the face ids that a build needs.
pub fn set_face_hints(fonts: &edgedoc_fonts::FontDb) {
    FACE_FALLBACKS.with(|fallbacks| {
        let mut map = fallbacks.borrow_mut();
        map.clear();
        if let Some(fallback) = fonts.fallback() {
            for bold in [false, true] {
                for italic in [false, true] {
                    if let Some(id) = fonts.resolve(&fallback.family, bold, italic) {
                        map.insert((bold, italic), id);
                    }
                }
            }
        }
    });
    HYPHEN_FALLBACKS.with(|h| {
        let mut map = h.borrow_mut();
        map.clear();
        let fallback = fonts.faces().iter().find(|face| {
            face.family.eq_ignore_ascii_case("Source Sans 3")
                && !face.bold
                && !face.italic
                && face.glyph_for('\u{2011}').is_some()
        });
        if let Some(fallback) = fallback {
            for face in fonts.faces() {
                if face.glyph_for('\u{2011}').is_none() {
                    map.insert(face.id, (fallback.id, face.bold, face.italic));
                }
            }
        }
    });
    FACE_HINTS.with(|h| {
        let mut map = h.borrow_mut();
        map.clear();
        for (family, bold, italic, id) in fonts.registered_faces() {
            map.insert((family.to_ascii_lowercase(), bold, italic), id);
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
        Some(VertAlign::Superscript) => run.baseline_shift + run.size * 0.33,
        Some(VertAlign::Subscript) => run.baseline_shift - run.size * 0.14,
        None => run.baseline_shift,
    }
}

fn table_band_sizes(chain: &[edgedoc_model::Style]) -> (usize, usize) {
    let band_size = |name: &str| {
        chain
            .iter()
            .rev()
            .filter_map(|style| style.table_props.as_ref())
            .flat_map(|props| &props.extra)
            .find_map(|raw| {
                let root =
                    edgedoc_opc::xml::parse(raw.as_bytes(), &edgedoc::Budget::default()).ok()?;
                (root.name == name).then(|| {
                    root.attr("val")
                        .and_then(|value| value.parse::<usize>().ok())
                        .unwrap_or(0)
                        .min(1000)
                })
            })
            .unwrap_or(0)
    };
    (
        band_size("tblStyleRowBandSize"),
        band_size("tblStyleColBandSize"),
    )
}

/// Conditional table parts in Wordcraft's precedence order, lowest first.
fn table_regions(
    table: &Table,
    (row_size, col_size): (usize, usize),
    row: usize,
    col: usize,
    span: usize,
    cols: usize,
) -> Vec<&'static str> {
    let bits = table
        .look
        .as_deref()
        .and_then(|value| u32::from_str_radix(value, 16).ok())
        .unwrap_or(0x04A0);
    let opts = &table.look_options;
    let first_row = opts.first_row.unwrap_or(bits & 0x0020 != 0);
    let last_row = opts.last_row.unwrap_or(bits & 0x0040 != 0);
    let first_col = opts.first_column.unwrap_or(bits & 0x0080 != 0);
    let last_col = opts.last_column.unwrap_or(bits & 0x0100 != 0);
    let rows_banded = !opts.no_h_band.unwrap_or(bits & 0x0200 != 0);
    let cols_banded = !opts.no_v_band.unwrap_or(bits & 0x0400 != 0);
    let header = first_row && row == 0;
    let total = last_row && row + 1 == table.rows.len() && table.rows.len() > 1;
    let first = first_col && col == 0;
    let last = last_col && col.saturating_add(span) >= cols && cols > 1;
    let mut regions = vec!["wholeTable"];
    if cols_banded && !first && col_size > 0 {
        regions.push(
            if (col.saturating_sub(usize::from(first_col)) / col_size) % 2 == 0 {
                "band1Vert"
            } else {
                "band2Vert"
            },
        );
    }
    if rows_banded && !header && row_size > 0 {
        regions.push(
            if (row.saturating_sub(usize::from(first_row)) / row_size) % 2 == 0 {
                "band1Horz"
            } else {
                "band2Horz"
            },
        );
    }
    if first {
        regions.push("firstCol");
    }
    if last {
        regions.push("lastCol");
    }
    if header {
        regions.push("firstRow");
    }
    if total {
        regions.push("lastRow");
    }
    if header && last {
        regions.push("neCell");
    }
    if header && first {
        regions.push("nwCell");
    }
    if total && last {
        regions.push("seCell");
    }
    if total && first {
        regions.push("swCell");
    }
    regions
}

/// Build a table. See `DP-L-34`.
fn row_grid_skip(row: &edgedoc_model::Row, name: &str) -> usize {
    row.extra
        .iter()
        .find_map(|raw| {
            let root = edgedoc_opc::xml::parse(raw.as_bytes(), &edgedoc::Budget::default()).ok()?;
            (root.name == name)
                .then(|| root.attr("val"))?
                .and_then(|v| v.parse::<usize>().ok())
        })
        .unwrap_or(0)
        .min(edgedoc::Budget::default().max_cols)
}

fn table_of(table: &Table, cascade: &mut Cascade, index: usize) -> FlowTable {
    let chain = table
        .style
        .as_deref()
        .map_or_else(Vec::new, |id| cascade.table_style_chain(id));
    let mut styled = table.clone();
    for style in chain.iter().rev() {
        for region in style
            .table_regions
            .iter()
            .rev()
            .filter(|region| region.kind == "wholeTable")
        {
            macro_rules! inherit_region {
                ($target:expr, $source:expr) => {
                    if $target.is_none() {
                        $target = $source.clone();
                    }
                };
            }
            inherit_region!(styled.borders.top, region.table_borders.top);
            inherit_region!(styled.borders.left, region.table_borders.left);
            inherit_region!(styled.borders.bottom, region.table_borders.bottom);
            inherit_region!(styled.borders.right, region.table_borders.right);
            inherit_region!(styled.borders.inside_h, region.table_borders.inside_h);
            inherit_region!(styled.borders.inside_v, region.table_borders.inside_v);
        }
        if let Some(inherited) = &style.table_props {
            macro_rules! inherit {
                ($target:expr, $source:expr) => {
                    if $target.is_none() {
                        $target = $source.clone();
                    }
                };
            }
            inherit!(styled.borders.top, inherited.borders.top);
            inherit!(styled.borders.left, inherited.borders.left);
            inherit!(styled.borders.bottom, inherited.borders.bottom);
            inherit!(styled.borders.right, inherited.borders.right);
            inherit!(styled.borders.inside_h, inherited.borders.inside_h);
            inherit!(styled.borders.inside_v, inherited.borders.inside_v);
            inherit!(styled.cell_margins.top, inherited.cell_margins.top);
            inherit!(styled.cell_margins.left, inherited.cell_margins.left);
            inherit!(styled.cell_margins.bottom, inherited.cell_margins.bottom);
            inherit!(styled.cell_margins.right, inherited.cell_margins.right);
        }
    }
    let table = &styled;
    let prior_diagnostics = cascade.diagnostics.len();
    let mut cell_cascade = cascade.clone();
    cell_cascade.apply_table_style_defaults(&chain);
    let band_sizes = table_band_sizes(&chain);
    let table_shading = chain
        .iter()
        .rev()
        .filter_map(|style| style.table_props.as_ref())
        .flat_map(|props| &props.extra)
        .find_map(|raw| {
            let root = edgedoc_opc::xml::parse(raw.as_bytes(), &edgedoc::Budget::default()).ok()?;
            (root.name == "shd")
                .then(|| root.attr("fill"))
                .flatten()
                .filter(|fill| Rgba::parse_hex(fill).is_some())
                .map(str::to_string)
        });
    let mut rows = Vec::new();
    let columns = table.widths.len().max(
        table
            .rows
            .iter()
            .map(|row| {
                row_grid_skip(row, "gridBefore")
                    + row.cells.iter().map(|cell| cell.span.max(1)).sum::<usize>()
                    + row_grid_skip(row, "gridAfter")
            })
            .max()
            .unwrap_or(0),
    );
    for (row_index, row) in table.rows.iter().enumerate() {
        let mut cells = Vec::new();
        let grid_before = row_grid_skip(row, "gridBefore");
        let grid_after = row_grid_skip(row, "gridAfter");
        let mut column = grid_before;
        for cell in &row.cells {
            let span = cell.span.max(1);
            let regions = table_regions(table, band_sizes, row_index, column, span, columns);
            let defaults = cell_cascade.table_defaults();
            let mut shading = table_shading.clone();
            let mut v_align = None;
            let mut region_borders: [Option<edgedoc_model::LineBorder>; 4] = Default::default();
            for name in regions {
                for style in &chain {
                    for region in style
                        .table_regions
                        .iter()
                        .filter(|region| region.kind == name)
                    {
                        cell_cascade.apply_table_region_defaults(region);
                        let borders = &region.borders;
                        let row_part =
                            matches!(name, "firstRow" | "lastRow" | "band1Horz" | "band2Horz");
                        let col_part =
                            matches!(name, "firstCol" | "lastCol" | "band1Vert" | "band2Vert");
                        let edges = [
                            if col_part && row_index > 0 {
                                &borders.between
                            } else {
                                &borders.top
                            },
                            if row_part && column > 0 {
                                &borders.inside_v
                            } else {
                                &borders.left
                            },
                            if col_part && row_index + 1 < table.rows.len() {
                                &borders.between
                            } else {
                                &borders.bottom
                            },
                            if row_part && column + span < columns {
                                &borders.inside_v
                            } else {
                                &borders.right
                            },
                        ];
                        for (target, source) in region_borders.iter_mut().zip(edges) {
                            if source.is_some() {
                                *target = source.clone();
                            }
                        }
                        if region
                            .shading
                            .as_deref()
                            .and_then(Rgba::parse_hex)
                            .is_some()
                        {
                            shading = region.shading.clone();
                        }
                        if region.v_align.is_some() {
                            v_align = region.v_align.clone();
                        }
                    }
                }
            }
            column += span;
            let mut blocks = Vec::new();
            for block in &cell.blocks {
                match block.as_ref() {
                    Block::Paragraph(p) => blocks.push(FlowBlock::Paragraph(paragraph_of(
                        p,
                        &mut cell_cascade,
                        blocks.len(),
                    ))),
                    Block::Table(t) => blocks.push(FlowBlock::Table(table_of(t, cascade, 0))),
                    Block::Control(c) => {
                        for inner in &c.blocks {
                            if let Block::Paragraph(p) = inner.as_ref() {
                                blocks.push(FlowBlock::Paragraph(paragraph_of(
                                    p,
                                    &mut cell_cascade,
                                    0,
                                )));
                            }
                        }
                    }
                    _ => {}
                }
            }
            cell_cascade.restore_table_defaults(defaults);
            cells.push(FlowCell {
                blocks,
                span: cell.span.max(1),
                width: cell.width.map(|w| edgedoc_geom::twips(w.0)),
                v_align: cell
                    .v_align
                    .as_deref()
                    .or(v_align.as_deref())
                    .map_or(VAlign::Top, VAlign::parse),
                shading: cell
                    .shading
                    .as_deref()
                    .and_then(Rgba::parse_hex)
                    .or_else(|| shading.as_deref().and_then(Rgba::parse_hex)),
                margins: [
                    cell.margins
                        .top
                        .or(table.cell_margins.top)
                        .map_or(0.0, |v| edgedoc_geom::twips(v.0)),
                    cell.margins
                        .left
                        .or(table.cell_margins.left)
                        .map_or(5.4, |v| edgedoc_geom::twips(v.0)),
                    cell.margins
                        .bottom
                        .or(table.cell_margins.bottom)
                        .map_or(0.0, |v| edgedoc_geom::twips(v.0)),
                    cell.margins
                        .right
                        .or(table.cell_margins.right)
                        .map_or(5.4, |v| edgedoc_geom::twips(v.0)),
                ],
                borders: [
                    cell.borders
                        .top
                        .as_ref()
                        .or(region_borders[0].as_ref())
                        .map(border_of),
                    cell.borders
                        .left
                        .as_ref()
                        .or(region_borders[1].as_ref())
                        .map(border_of),
                    cell.borders
                        .bottom
                        .as_ref()
                        .or(region_borders[2].as_ref())
                        .map(border_of),
                    cell.borders
                        .right
                        .as_ref()
                        .or(region_borders[3].as_ref())
                        .map(border_of),
                ],
                v_merge_continue: cell.v_merge.as_deref() == Some("continue"),
                v_merge_start: cell.v_merge.as_deref() == Some("restart"),
            });
        }
        rows.push(FlowRow {
            cells,
            grid_before,
            grid_after,
            exception_borders: [
                row.exception_borders.top.as_ref().map(border_of),
                row.exception_borders.left.as_ref().map(border_of),
                row.exception_borders.bottom.as_ref().map(border_of),
                row.exception_borders.right.as_ref().map(border_of),
                row.exception_borders.inside_h.as_ref().map(border_of),
                row.exception_borders.inside_v.as_ref().map(border_of),
            ],
            header: row.header,
            height: row.height.map(|h| edgedoc_geom::twips(h.0)),
            height_exact: row.height_rule.as_deref() == Some("exact"),
            cant_split: row.cant_split,
        });
    }
    cascade
        .diagnostics
        .extend(cell_cascade.diagnostics.into_iter().skip(prior_diagnostics));
    let legacy_inset = if cascade.compatibility_mode() < 15
        && !matches!(table.align.as_deref(), Some("center" | "right" | "end"))
    {
        rows.first()
            .and_then(|row| row.cells.first())
            .map_or(5.4, |cell| cell.margins[1])
    } else {
        0.0
    };
    // Modern Word positions a start-aligned table's grid half a border width in
    // from the margin, because its first cell border is centered on that grid.
    let modern_border_inset = if cascade.compatibility_mode() >= 15 {
        rows.first()
            .and_then(|row| row.cells.first())
            .and_then(|cell| cell.borders[edgedoc_flow::SIDE_LEFT])
            .or_else(|| table.borders.left.as_ref().map(border_of))
            .filter(|border| border.style.is_visible())
            .map_or(0.0, |border| border.width.clamp(0.0, 12.0) / 2.0)
    } else {
        0.0
    };
    let mut result = FlowTable {
        path: format!("body[{}]", index),
        rows,
        widths: table
            .widths
            .iter()
            .map(|w| edgedoc_geom::twips(w.0))
            .collect(),
        auto_width: table.width_kind.is_none(),
        preferred_width: table.width_kind.as_deref().and_then(|value| {
            let (number, kind) = value.split_once(':')?;
            let literal_percent = number.ends_with('%');
            let number = number
                .trim_end_matches('%')
                .parse::<f32>()
                .ok()
                .filter(|n| n.is_finite() && *n > 0.0)?;
            match kind {
                "dxa" => Some(TableWidth::Points((number / 20.0).min(100_000.0))),
                "pct" => Some(TableWidth::Fraction(
                    (number / if literal_percent { 100.0 } else { 5000.0 }).min(100.0),
                )),
                _ => None,
            }
        }),
        align: table.align.as_deref().map_or(Align::Left, |a| match a {
            "center" => Align::Center,
            "right" | "end" => Align::Right,
            _ => Align::Left,
        }),
        indent: table.indent.map_or(0.0, |v| edgedoc_geom::twips(v.0)) - legacy_inset
            + modern_border_inset,
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
    };
    let path = result.path.clone();
    assign_cell_paths(&mut result, &path);
    result
}

fn assign_cell_paths(table: &mut FlowTable, path: &str) {
    table.path = path.to_string();
    for (row_index, row) in table.rows.iter_mut().enumerate() {
        for (cell_index, cell) in row.cells.iter_mut().enumerate() {
            for (block_index, block) in cell.blocks.iter_mut().enumerate() {
                let child =
                    format!("{path}/row[{row_index}]/cell[{cell_index}]/block[{block_index}]");
                match block {
                    FlowBlock::Paragraph(p) => p.path = child,
                    FlowBlock::Table(nested) => assign_cell_paths(nested, &child),
                    _ => {}
                }
            }
        }
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
        const { std::cell::RefCell::new(std::collections::BTreeMap::new()) };
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

    #[test]
    fn plain_table_style_accepts_only_modeled_properties() {
        let mut sheet = edgedoc_model::StyleSheet::default();
        let mut style = edgedoc_model::Style::table("Grid");
        style.raw_xml = Some(r#"<w:style><w:tblPr><w:tblBorders><w:top w:val="single" w:sz="4"/></w:tblBorders></w:tblPr></w:style>"#.into());
        sheet.push(style.clone());
        assert!(supported_plain_table_style(&sheet, "Grid"));
        // A nil zero indent carries the same modeled value as a zero dxa indent.
        style.raw_xml = Some(
            r#"<w:style><w:tblPr><w:tblInd w:w="0" w:type="nil"/></w:tblPr></w:style>"#.into(),
        );
        sheet.push(style.clone());
        assert!(supported_plain_table_style(&sheet, "Grid"));
        style.raw_xml = Some(
            r#"<w:style><w:tblPr><w:tblInd w:w="1" w:type="nil"/></w:tblPr></w:style>"#.into(),
        );
        sheet.push(style.clone());
        assert!(!supported_plain_table_style(&sheet, "Grid"));
        style.raw_xml = Some(r#"<w:style><w:tblStylePr w:type="firstRow"/></w:style>"#.into());
        sheet.push(style.clone());
        assert!(!supported_plain_table_style(&sheet, "Grid"));
        style.raw_xml = Some(r#"<w:style><w:tblPr><w:tblBorders><w:top w:val="single" w:themeColor="accent1"/></w:tblBorders></w:tblPr></w:style>"#.into());
        sheet.push(style.clone());
        assert!(!supported_plain_table_style(&sheet, "Grid"));
        style.raw_xml = Some("<w:style/>".into());
        style.char_props.bold = true;
        sheet.push(style.clone());
        assert!(!supported_plain_table_style(&sheet, "Grid"));
        style.char_props.bold = false;
        let mut table = Table::new();
        table.borders.top = Some(edgedoc_model::LineBorder {
            val: "double".into(),
            ..Default::default()
        });
        style.table_props = Some(table);
        sheet.push(style);
        assert!(!supported_plain_table_style(&sheet, "Grid"));
        assert!(!supported_plain_table_style(&sheet, "Missing"));
    }

    #[test]
    fn field_case_switches_apply_in_order_outside_quotes() {
        assert_eq!(
            field_case(r"TITLE \* Lower \* Caps", "WEEKLY REPORT".into()),
            "Weekly Report"
        );
        assert_eq!(
            field_case(r"TITLE \* Lower \* FirstCap", "WEEKLY REPORT".into()),
            "Weekly report"
        );
        assert_eq!(
            field_case(r#"TITLE "\* Upper" \* Lower"#, "MiXeD".into()),
            "mixed"
        );
    }

    #[test]
    fn toc_link_style_is_suppressed_only_inside_the_range_on_the_display_copy() {
        use edgedoc_model::{CharProps, InlineObject, Style, StyleSheet};
        let link = CharProps {
            style: Some("localized-link".into()),
            link: Some("#entry".into()),
            ..CharProps::default()
        };
        let normal = CharProps::default();
        let mut first = Paragraph::new();
        first.push_text("outside", &link);
        first.push_object(
            InlineObject::FieldStart {
                instr: " TOC ".into(),
                dirty: false,
                locked: false,
            },
            &normal,
        );
        first.push_text("inside", &link);
        let mut second = Paragraph::new();
        second.push_text("continued", &link);
        second.push_object(InlineObject::FieldEnd, &normal);
        second.push_text("after", &link);
        let mut style = Style::character("localized-link");
        style.name = "Hyperlink".into();
        let styles = StyleSheet {
            styles: vec![style],
            ..StyleSheet::default()
        };
        let mut blocks = vec![Block::Paragraph(first), Block::Paragraph(second)];
        let original = blocks.clone();
        unstyle_toc_links(&mut blocks, &styles);
        assert!(matches!(&blocks[0], Block::Paragraph(_)));
        let Block::Paragraph(first) = &blocks[0] else {
            return;
        };
        assert!(matches!(&blocks[1], Block::Paragraph(_)));
        let Block::Paragraph(second) = &blocks[1] else {
            return;
        };
        assert_eq!(first.runs[0].props.style.as_deref(), Some("localized-link"));
        assert_eq!(first.runs[2].props.style, None);
        assert_eq!(first.runs[2].props.link.as_deref(), Some("#entry"));
        assert_eq!(second.runs[0].props.style, None);
        assert_eq!(
            second.runs[2].props.style.as_deref(),
            Some("localized-link")
        );
        assert!(matches!(&original[0], Block::Paragraph(_)));
        let Block::Paragraph(original_first) = &original[0] else {
            return;
        };
        assert_eq!(
            original_first.runs[2].props.style.as_deref(),
            Some("localized-link")
        );
    }

    fn db() -> edgedoc_fonts::FontDb {
        let mut db = edgedoc_fonts::FontDb::new();
        crate::fonts::load_builtin_fonts(&mut db);
        set_face_hints(&db);
        db
    }

    #[test]
    fn unknown_family_uses_the_fallback_family_in_the_requested_style() {
        let db = db();
        let run = ComputedRun {
            requested_face: "Missing font family".into(),
            face_name: "Missing font family".into(),
            bold: true,
            ..ComputedRun::default()
        };
        let face = db.face(face_id(&run));
        assert!(face.is_some_and(|face| face.bold));
    }

    #[test]
    fn a_builder_document_becomes_flow_blocks() {
        let doc = edgedoc::Document::builder()
            .paragraph(edgedoc::Paragraph::new().text("Hello"))
            .build()
            .expect("build");
        let db = db();
        let mut diags = Vec::new();
        let prepared =
            prepare(&doc, &db, &mut diags, &std::collections::BTreeMap::new()).expect("prepare");
        assert!(!prepared.sections[0].blocks.is_empty());
        assert!(!prepared.sections.is_empty());
        match prepared.sections[0].blocks.first() {
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
        let prepared =
            prepare(&doc, &db, &mut diags, &std::collections::BTreeMap::new()).expect("prepare");
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
        let prepared =
            prepare(&doc, &db, &mut diags, &std::collections::BTreeMap::new()).expect("prepare");
        let table = prepared
            .sections
            .iter()
            .flat_map(|s| &s.blocks)
            .find_map(|b| match b {
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
        let prepared =
            prepare(&doc, &db, &mut diags, &std::collections::BTreeMap::new()).expect("prepare");
        let para = prepared
            .sections
            .iter()
            .flat_map(|s| &s.blocks)
            .find_map(|b| match b {
                FlowBlock::Paragraph(p) => Some(p),
                _ => None,
            });
        let para = para.expect("a paragraph");
        assert!(para.run_styles.iter().any(|s| (s.size - 18.0).abs() < 0.01));
    }
}
