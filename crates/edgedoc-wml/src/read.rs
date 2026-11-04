use std::sync::Arc;

use edgedoc_limit::Budget;
use edgedoc_model::{
    Anchor, Block, Borders, Cell, CellMargins, CharProps, ContentControl, DocGrid, Field,
    FootnotePr, FormField, FormKind, FrameProps, HeaderRefs, InlineObject, LineBorder,
    LineNumbering, ModelDocument, ParaProps, Paragraph, PartKind, PartMeta, Picture, Row,
    SectionProps, SectionStart, Style, StyleKind, TabStop, Table, TablePosition, PAGE_BREAK,
};
use edgedoc_opc::xml::{self, El};
use edgedoc_opc::{normalize, read_rels, Package, Rel};
use edgedoc_unit::{Emu, Twips};

use crate::error::{WmlError, WmlResult};

struct Ctx<'a> {
    package: &'a Package,
    budget: &'a Budget,
    rels: &'a [Rel],
    base: String,
    source: &'a [u8],
}

pub fn document_from_package(package: &Package, budget: &Budget) -> WmlResult<ModelDocument> {
    let main = package.office_document().map_err(map_opc)?;
    let rels = part_rels(package, &main.name, budget)?;
    let root = xml::parse(&main.bytes, budget).map_err(map_opc)?;
    if root.name == "wordDocument" {
        return Err(WmlError::new(
            "unsupported",
            "Word 2003 XML is not written by this version. Save the file as a .docx package.",
        ));
    }
    let mut doc = ModelDocument::new();
    let ctx = Ctx {
        package,
        budget,
        rels: &rels,
        base: main.name.clone(),
        source: &main.bytes,
    };
    if let Some(body) = root.child_els().find(|el| el.name == "body") {
        doc.body = read_blocks(body, &ctx, &mut doc, 0)?;
        if let Some(sect) = body.child_els().find(|el| el.name == "sectPr") {
            doc.last_section = read_sect(sect, &ctx, &mut doc)?;
        }
    } else {
        return Err(WmlError::new(
            "package.not_word",
            "The main part has no body.",
        ));
    }
    read_settings(&mut doc, &ctx)?;
    read_numbering(&mut doc, &ctx)?;
    read_styles(&mut doc, &ctx)?;
    read_core(&mut doc, package, budget)?;
    read_notes(&mut doc, &ctx)?;
    read_comments(&mut doc, &ctx)?;
    doc.ensure_nonempty();
    Ok(doc)
}

fn read_blocks(
    parent: &El,
    ctx: &Ctx<'_>,
    doc: &mut ModelDocument,
    depth: usize,
) -> WmlResult<Vec<Arc<Block>>> {
    let mut blocks = Vec::new();
    for child in parent.child_els() {
        if is_property_element(&child.name) {
            continue;
        }
        if child.name == "sectPr" {
            continue;
        }
        if child.name == "AlternateContent" {
            blocks.push(Arc::new(read_alternate(child, ctx, doc, depth)?));
            continue;
        }
        if child.name == "sdt" {
            blocks.push(Arc::new(read_control(child, ctx, doc, depth)?));
            continue;
        }
        if child.name == "tbl" {
            blocks.push(Arc::new(Block::Table(read_table(child, ctx, doc, depth)?)));
            continue;
        }
        if child.name == "p" {
            blocks.push(Arc::new(Block::Paragraph(read_paragraph(child, ctx, doc)?)));
            continue;
        }
        if !crate::catalog::is_owned(&child.name) {
            blocks.push(Arc::new(Block::Opaque(child.raw(ctx.source).to_string())));
        }
    }
    Ok(blocks)
}

fn read_alternate(
    el: &El,
    ctx: &Ctx<'_>,
    doc: &mut ModelDocument,
    depth: usize,
) -> WmlResult<Block> {
    let choice = el.child_els().find(|child| child.name == "Choice");
    let fallback = el.child_els().find(|child| child.name == "Fallback");
    let requires = choice
        .and_then(|el| el.attr("Requires"))
        .unwrap_or("")
        .to_string();
    let choice_blocks = match choice {
        Some(choice) => read_blocks(choice, ctx, doc, depth)?,
        None => Vec::new(),
    };
    let fallback_xml = fallback
        .map(|el| el.raw(ctx.source).to_string())
        .unwrap_or_default();
    Ok(Block::Alternate {
        requires,
        choice: choice_blocks,
        fallback_xml,
    })
}

fn read_control(el: &El, ctx: &Ctx<'_>, doc: &mut ModelDocument, depth: usize) -> WmlResult<Block> {
    let pr = el.child_els().find(|child| child.name == "sdtPr");
    let title = text_attr(pr, "alias");
    let tag = text_attr(pr, "tag");
    let lock = text_attr(pr, "lock");
    let lock_content = lock == "sdtContentLocked" || lock == "contentLocked" || lock == "sdtLocked";
    let kind = control_kind(pr);
    let placeholder = pr
        .and_then(|pr| pr.child_els().find(|el| el.name == "placeholder"))
        .and_then(|el| el.child_els().find(|el| el.name == "docPart"))
        .and_then(|el| el.attr("val"))
        .unwrap_or("")
        .to_string();
    let binding_xml = pr
        .and_then(|pr| pr.child_els().find(|el| el.name == "dataBinding"))
        .map(|el| el.raw(ctx.source).to_string());
    let items = read_control_items(pr);
    let date_format = pr
        .and_then(|pr| pr.child_els().find(|el| el.name == "date"))
        .and_then(|el| el.attr("format"))
        .map(str::to_string)
        .or_else(|| {
            pr.and_then(|pr| pr.child_els().find(|el| el.name == "dateFormat"))
                .and_then(|el| el.attr("val"))
                .map(str::to_string)
        });
    let checked = pr
        .and_then(|pr| pr.child_els().find(|el| el.name == "checkbox"))
        .and_then(|el| el.child_els().find(|el| el.name == "checked"))
        .and_then(|el| el.attr("val"))
        .map(|val| val == "1" || val == "true");
    let content = el.child_els().find(|child| child.name == "sdtContent");
    let mut blocks = match content {
        Some(content) => read_blocks(content, ctx, doc, depth)?,
        None => Vec::new(),
    };
    if lock_content {
        for block in &mut blocks {
            if let Block::Paragraph(para) = Arc::make_mut(block) {
                para.locked = true;
            }
        }
    }
    Ok(Block::Control(ContentControl {
        title,
        tag,
        lock_content,
        kind,
        placeholder,
        binding_xml,
        blocks,
        items,
        date_format,
        checked,
    }))
}

fn read_control_items(pr: Option<&El>) -> Vec<edgedoc_model::ContentItem> {
    let Some(list) = pr.and_then(|pr| {
        pr.child_els()
            .find(|el| el.name == "dropDownList" || el.name == "comboBox")
    }) else {
        return Vec::new();
    };
    list.child_els()
        .filter(|el| el.name == "listItem")
        .map(|el| {
            let value = el.attr("value").unwrap_or("").to_string();
            let display = el
                .attr("displayText")
                .map(str::to_string)
                .unwrap_or_else(|| value.clone());
            edgedoc_model::ContentItem { display, value }
        })
        .filter(|item| !item.value.is_empty() || !item.display.is_empty())
        .collect()
}

fn control_kind(pr: Option<&El>) -> String {
    let Some(pr) = pr else {
        return "richText".into();
    };
    for name in [
        "text",
        "date",
        "dropDownList",
        "comboBox",
        "checkbox",
        "picture",
        "docPartObj",
    ] {
        if pr.child_els().any(|el| el.name == name) {
            return name.to_string();
        }
    }
    "richText".into()
}

fn text_attr(pr: Option<&El>, name: &str) -> String {
    pr.and_then(|pr| pr.child_els().find(|el| el.name == name))
        .and_then(|el| el.attr("val"))
        .unwrap_or("")
        .to_string()
}

fn read_table(el: &El, ctx: &Ctx<'_>, doc: &mut ModelDocument, depth: usize) -> WmlResult<Table> {
    if depth >= ctx.budget.max_table_depth {
        return Err(WmlError::new(
            "model.limit",
            format!(
                "The table nesting is deeper than {}. Flatten the table.",
                ctx.budget.max_table_depth
            ),
        ));
    }
    let mut table = Table::new();
    for child in el.child_els() {
        if child.name == "tblGrid" {
            for col in child.children("gridCol") {
                if let Some(width) = attr_twips(col, "w") {
                    table.widths.push(width);
                }
            }
        } else if child.name == "tr" {
            table.rows.push(read_row(child, ctx, doc, depth)?);
        } else if child.name == "tblPrEx" {
            table.extra.push(child.raw(ctx.source).to_string());
            if let Some(borders) = child.child_els().find(|el| el.name == "tblBorders") {
                table.borders = read_borders(borders);
            }
        } else if child.name == "tblPr" {
            read_table_props(&mut table, child, ctx);
        }
    }
    Ok(table)
}

fn read_table_props(table: &mut Table, pr: &El, ctx: &Ctx<'_>) {
    for child in pr.child_els() {
        match child.name.as_str() {
            "bidiVisual" => table.bidi_visual = true,
            "tblBorders" => table.borders = read_borders(child),
            "tblCellMar" => table.cell_margins = read_margins(child),
            "tblLook" => table.look = child.attr("val").map(str::to_string),
            "tblW" => {
                let kind = child.attr("type").unwrap_or("dxa");
                let value = child.attr("w").unwrap_or("");
                // An automatic width carries no width, so the model keeps no value.
                table.width_kind = if kind == "auto" && value == "0" {
                    None
                } else {
                    Some(format!("{value}:{kind}"))
                };
            }
            "tblStyle" => table.style = child.attr("val").map(str::to_string),
            "jc" => table.align = child.attr("val").map(str::to_string),
            "tblInd" => table.indent = attr_twips(child, "w"),
            "tblLayout" => table.layout = child.attr("type").map(str::to_string),
            "tblCellSpacing" => table.cell_space = attr_twips(child, "w"),
            "tblOverlap" => table.overlap = child.attr("val").map(str::to_string),
            "tblpPr" => table.position = Some(read_table_position(child)),
            _ => table.extra.push(child.raw(ctx.source).to_string()),
        }
    }
}

fn read_table_position(el: &El) -> TablePosition {
    TablePosition {
        left_from_text: attr_i32(el, "leftFromText"),
        right_from_text: attr_i32(el, "rightFromText"),
        top_from_text: attr_i32(el, "topFromText"),
        bottom_from_text: attr_i32(el, "bottomFromText"),
        left_from: el.attr("leftFrom").map(str::to_string),
        top_from: el.attr("topFrom").map(str::to_string),
        h_anchor: el.attr("horzAnchor").map(str::to_string),
        v_anchor: el.attr("vertAnchor").map(str::to_string),
        tblp_x: attr_i32(el, "tblpX"),
        tblp_y: attr_i32(el, "tblpY"),
    }
}

fn read_row(el: &El, ctx: &Ctx<'_>, doc: &mut ModelDocument, depth: usize) -> WmlResult<Row> {
    let mut row = Row {
        cells: Vec::new(),
        header: false,
        height: None,
        height_rule: None,
        cant_split: false,
        hidden: false,
        extra: Vec::new(),
    };
    for child in el.child_els() {
        if child.name == "trPr" {
            for pr in child.child_els() {
                match pr.name.as_str() {
                    "tblHeader" => row.header = val_on(pr),
                    "cantSplit" => row.cant_split = val_on(pr),
                    "hidden" => row.hidden = val_on(pr),
                    "trHeight" => {
                        row.height = attr_twips(pr, "val");
                        row.height_rule = pr.attr("hRule").map(str::to_string);
                    }
                    _ => row.extra.push(pr.raw(ctx.source).to_string()),
                }
            }
        } else if child.name == "tc" {
            let tc_pr = child.child_els().find(|el| el.name == "tcPr");
            let span = tc_pr
                .and_then(|pr| pr.child_els().find(|el| el.name == "gridSpan"))
                .and_then(|el| el.attr("val"))
                .and_then(|val| val.parse::<usize>().ok())
                .unwrap_or(1usize);
            let mut extra = Vec::new();
            if let Some(pr) = tc_pr {
                for item in pr.child_els() {
                    if !matches!(
                        item.name.as_str(),
                        "gridSpan"
                            | "tcW"
                            | "vMerge"
                            | "hMerge"
                            | "vAlign"
                            | "textDirection"
                            | "shd"
                            | "tcMar"
                            | "tcBorders"
                            | "fitText"
                            | "noWrap"
                    ) {
                        extra.push(item.raw(ctx.source).to_string());
                    }
                }
            }
            row.cells.push(Cell {
                blocks: read_blocks(child, ctx, doc, depth + 1)?,
                span: crate::clamp_cols(span, ctx.budget.max_cols),
                width: tc_pr
                    .and_then(|pr| pr.child_els().find(|el| el.name == "tcW"))
                    .and_then(|el| attr_twips(el, "w")),
                v_merge: tc_pr
                    .and_then(|pr| pr.child_els().find(|el| el.name == "vMerge"))
                    .map(|el| el.attr("val").unwrap_or("continue").to_string()),
                h_merge: tc_pr
                    .and_then(|pr| pr.child_els().find(|el| el.name == "hMerge"))
                    .map(|el| el.attr("val").unwrap_or("continue").to_string()),
                v_align: tc_pr
                    .and_then(|pr| pr.child_els().find(|el| el.name == "vAlign"))
                    .and_then(|el| el.attr("val"))
                    .map(str::to_string),
                text_direction: tc_pr
                    .and_then(|pr| pr.child_els().find(|el| el.name == "textDirection"))
                    .and_then(|el| el.attr("val"))
                    .map(str::to_string),
                shading: tc_pr
                    .and_then(|pr| pr.child_els().find(|el| el.name == "shd"))
                    .and_then(|el| el.attr("fill"))
                    .map(str::to_string),
                margins: tc_pr
                    .and_then(|pr| pr.child_els().find(|el| el.name == "tcMar"))
                    .map(read_cell_margins)
                    .unwrap_or_default(),
                borders: tc_pr
                    .and_then(|pr| pr.child_els().find(|el| el.name == "tcBorders"))
                    .map(read_borders)
                    .unwrap_or_default(),
                fit_text: tc_pr
                    .and_then(|pr| pr.child_els().find(|el| el.name == "fitText"))
                    .map(val_on)
                    .unwrap_or(false),
                no_wrap: tc_pr
                    .and_then(|pr| pr.child_els().find(|el| el.name == "noWrap"))
                    .map(val_on)
                    .unwrap_or(false),
                extra,
            });
        }
    }
    Ok(row)
}

fn read_paragraph(el: &El, ctx: &Ctx<'_>, doc: &mut ModelDocument) -> WmlResult<Paragraph> {
    let mut para = Paragraph::new();
    let raw = el.raw(ctx.source).to_string();
    if let Some(ppr) = el.child_els().find(|child| child.name == "pPr") {
        para.props = read_ppr(ppr, ctx);
        // A paragraph-level `w:sectPr` ends the section that this paragraph completes.
        if let Some(sect) = ppr.child_els().find(|child| child.name == "sectPr") {
            let props = read_sect(sect, ctx, doc)?;
            para.section = Some(edgedoc_model::SectionBreak { props });
        }
    }
    let mut field: Option<FieldBuild> = None;
    for child in el.child_els() {
        match child.name.as_str() {
            "pPr" => {}
            "hyperlink" => {
                let target = child
                    .attr("anchor")
                    .map(|anchor| format!("#{anchor}"))
                    .or_else(|| {
                        let id = child.attr("id").unwrap_or("");
                        ctx.rels
                            .iter()
                            .find(|rel| rel.id == id)
                            .map(|rel| rel.target.clone())
                    });
                let before = para.runs.len();
                read_runs(child, &mut para, &mut field, ctx);
                if let Some(target) = target {
                    for run in para.runs.iter_mut().skip(before) {
                        run.props.link = Some(target.clone());
                    }
                }
            }
            "bookmarkStart" => {
                let name = child.attr("name").unwrap_or("").to_string();
                para.push_object(
                    InlineObject::Bookmark { name, start: true },
                    &CharProps::default(),
                );
            }
            "bookmarkEnd" => para.push_object(
                InlineObject::Bookmark {
                    name: String::new(),
                    start: false,
                },
                &CharProps::default(),
            ),
            "ins" | "del" => {
                para.retention.revisions = true;
                read_runs(child, &mut para, &mut field, ctx);
            }
            "oMath" | "oMathPara" => para.retention.equations = true,
            "r" | "fldSimple" => read_run(child, &mut para, &mut field, ctx),
            other if !crate::catalog::is_owned(other) => {
                para.push_object(
                    InlineObject::Opaque(child.raw(ctx.source).to_string()),
                    &CharProps::default(),
                );
            }
            _ => {}
        }
    }
    if field.is_some() {
        para.push_object(InlineObject::Orphan, &CharProps::default());
    }
    if para.retention.any() {
        para.raw_xml = Some(raw);
    }
    Ok(para)
}

fn descendant_text(el: &El) -> String {
    let mut out = el.text();
    for child in el.child_els() {
        out.push_str(&descendant_text(child));
    }
    out
}

struct FieldBuild {
    instr: String,
    result: String,
    in_result: bool,
    dirty: bool,
    locked: bool,
    form: Option<edgedoc_model::FormField>,
}

fn read_runs(el: &El, para: &mut Paragraph, field: &mut Option<FieldBuild>, ctx: &Ctx<'_>) {
    for child in el.child_els() {
        if child.name == "r" || child.name == "fldSimple" {
            read_run(child, para, field, ctx);
        } else if child.name == "ins" || child.name == "del" {
            read_runs(child, para, field, ctx);
        }
    }
}

fn read_run(el: &El, para: &mut Paragraph, field: &mut Option<FieldBuild>, ctx: &Ctx<'_>) {
    if el.name == "fldSimple" {
        let instr = el.attr("instr").unwrap_or("").to_string();
        let cached = descendant_text(el);
        let result = if cached.is_empty() {
            edgedoc_model::Instruction::parse(&instr)
                .merge_name()
                .map(|name| format!("«{name}»"))
                .unwrap_or_default()
        } else {
            cached
        };
        // `w:fldSimple` carries `w:dirty` and `w:fldLock` on the element itself.
        let mut built = Field::new(&instr, &result);
        built.dirty = is_set(el.attr("dirty"));
        built.locked = is_set(el.attr("fldLock"));
        para.push_object(InlineObject::Field(built), &CharProps::default());
        return;
    }
    let props = el
        .child_els()
        .find(|child| child.name == "rPr")
        .map(|pr| read_rpr(pr, ctx))
        .unwrap_or_default();
    for child in el.child_els() {
        match child.name.as_str() {
            "rPr" => {}
            "t" => {
                let text = child.text();
                if let Some(build) = field.as_mut() {
                    if build.in_result {
                        build.result.push_str(&text);
                    } else {
                        para.push_text(&text, &props);
                    }
                } else {
                    para.push_text(&text, &props);
                }
            }
            "instrText" => {
                if let Some(build) = field.as_mut() {
                    build.instr.push_str(&child.text());
                }
            }
            "tab" => para.push_text("\t", &props),
            "cr" => para.push_text("\n", &props),
            "br" => {
                if child.attr("type") == Some("page") {
                    para.push_text(&PAGE_BREAK.to_string(), &props);
                } else {
                    para.push_text("\n", &props);
                }
            }
            "fldChar" => on_fld_char(child, para, field, ctx, &props),
            "drawing" | "pict" | "object" => read_drawing(child, para, &props, ctx),
            // Alternate content inside a run holds a shape or a picture. Keep the
            // markup whole: a container reader would lose the shape's text.
            "AlternateContent" => para.push_object(
                InlineObject::Opaque(child.raw(ctx.source).to_string()),
                &props,
            ),
            "footnoteReference" | "endnoteReference" | "commentReference" => {
                let kind = match child.name.as_str() {
                    "footnoteReference" => "footnote",
                    "endnoteReference" => "endnote",
                    _ => "comment",
                };
                if kind == "comment" {
                    para.retention.comments = true;
                }
                let id = child
                    .attr("id")
                    .and_then(|val| val.parse::<i32>().ok())
                    .unwrap_or(0);
                para.push_object(
                    InlineObject::Note {
                        kind: kind.to_string(),
                        id,
                    },
                    &props,
                );
            }
            other if !crate::catalog::is_owned(other) => {
                para.push_object(
                    InlineObject::Opaque(child.raw(ctx.source).to_string()),
                    &props,
                );
            }
            _ => {}
        }
    }
}

fn read_form_field(el: &El, ctx: &Ctx<'_>) -> FormField {
    let mut form = FormField::default();
    for child in el.child_els() {
        match child.name.as_str() {
            "name" => form.name = child.attr("val").unwrap_or("").to_string(),
            "enabled" => form.enabled = is_on(child.attr("val")),
            "calcOnExit" => form.calc_on_exit = is_on(child.attr("val")),
            "helpText" => form.help = child.attr("val").map(str::to_string),
            "statusText" => form.status = child.attr("val").map(str::to_string),
            "textInput" => {
                form.kind = FormKind::Text {
                    kind: child.attr("type").map(str::to_string),
                    default: child
                        .child_els()
                        .find(|item| item.name == "default")
                        .and_then(|item| item.attr("val"))
                        .map(str::to_string),
                    max_length: child
                        .child_els()
                        .find(|item| item.name == "maxLength")
                        .and_then(|item| item.attr("val"))
                        .and_then(|val| val.parse().ok()),
                    format: child
                        .child_els()
                        .find(|item| item.name == "format")
                        .and_then(|item| item.attr("val"))
                        .map(str::to_string),
                }
            }
            "checkBox" => {
                let size = child
                    .child_els()
                    .find(|item| item.name == "size")
                    .and_then(|item| item.attr("val"))
                    .and_then(|val| val.parse().ok());
                let default = child
                    .child_els()
                    .find(|item| item.name == "default")
                    .and_then(|item| item.attr("val"))
                    .map(|val| is_on(Some(val)));
                let checked = child
                    .child_els()
                    .find(|item| item.name == "checked")
                    .and_then(|item| item.attr("val"))
                    .map(|val| is_on(Some(val)));
                form.kind = FormKind::CheckBox {
                    size,
                    size_auto: child.child_els().any(|item| item.name == "sizeAuto"),
                    default,
                    checked,
                }
            }
            "ddList" => {
                let result = child
                    .child_els()
                    .find(|item| item.name == "result")
                    .and_then(|item| item.attr("val"))
                    .and_then(|val| val.parse().ok());
                let entries = child
                    .child_els()
                    .filter(|item| item.name == "listEntry")
                    .filter_map(|item| item.attr("val").map(str::to_string))
                    .collect();
                form.kind = FormKind::DropDown { result, entries }
            }
            other if !crate::catalog::is_owned(other) => {
                let _ = ctx;
            }
            _ => {}
        }
    }
    form
}

fn on_fld_char(
    el: &El,
    para: &mut Paragraph,
    field: &mut Option<FieldBuild>,
    ctx: &Ctx<'_>,
    props: &CharProps,
) {
    match el.attr("fldCharType").unwrap_or("") {
        "begin" => {
            *field = Some(FieldBuild {
                instr: String::new(),
                result: String::new(),
                in_result: false,
                dirty: is_set(el.attr("dirty")),
                locked: is_set(el.attr("fldLock")),
                form: el
                    .child_els()
                    .find(|child| child.name == "ffData")
                    .map(|child| read_form_field(child, ctx)),
            })
        }
        "separate" => {
            if let Some(build) = field.as_mut() {
                build.in_result = true;
            }
        }
        "end" => match field.take() {
            Some(build) => {
                let result = if build.result.is_empty() {
                    edgedoc_model::Instruction::parse(&build.instr)
                        .merge_name()
                        .map(|name| format!("«{name}»"))
                        .unwrap_or_default()
                } else {
                    build.result
                };
                let mut built = Field::new(&build.instr, &result);
                built.dirty = build.dirty;
                built.locked = build.locked;
                built.form = build.form;
                para.push_object(InlineObject::Field(built), props);
            }
            None => para.push_object(InlineObject::Orphan, props),
        },
        _ => {}
    }
}

/// A Word boolean attribute. An element with no attribute is on, because the default
/// of `w:enabled`, `w:calcOnExit`, and the like is true when the attribute is absent.
fn is_on(value: Option<&str>) -> bool {
    match value {
        None => true,
        Some(text) => !matches!(text, "0" | "false" | "off"),
    }
}

/// A Word boolean attribute where absence means off, such as `w:fldChar/@w:dirty`.
fn is_set(value: Option<&str>) -> bool {
    matches!(value, Some("1") | Some("true") | Some("on"))
}

fn read_drawing(el: &El, para: &mut Paragraph, props: &CharProps, ctx: &Ctx<'_>) {
    let embed = find_attr(el, "embed").or_else(|| find_attr(el, "id"));
    let target = embed
        .as_ref()
        .and_then(|id| ctx.rels.iter().find(|rel| &rel.id == id))
        .map(|rel| join_target(&ctx.base, &rel.target));
    let part = target.as_ref().and_then(|name| ctx.package.part(name));
    let content_type = part
        .and_then(|part| part.content_type.clone())
        .unwrap_or_default();
    let metafile = content_type.contains("emf")
        || content_type.contains("wmf")
        || target.as_deref().unwrap_or("").ends_with(".emf")
        || target.as_deref().unwrap_or("").ends_with(".wmf");
    // A shape, a text box, or a group has no image relationship. Its markup carries
    // the shape and any text it holds, so keep the element whole.
    let has_shape = contains_element(el, "wsp")
        || contains_element(el, "wgp")
        || contains_element(el, "txbxContent");
    if metafile || el.name == "object" || el.name == "pict" || (target.is_none() && has_shape) {
        para.push_object(InlineObject::Opaque(el.raw(ctx.source).to_string()), props);
        return;
    }
    let bytes = part.map(|part| part.bytes.clone()).unwrap_or_default();
    let width = find_attr(el, "cx")
        .or_else(|| find_attr(el, "width"))
        .and_then(|val| val.parse::<i64>().ok())
        .map(Emu);
    let height = find_attr(el, "cy")
        .or_else(|| find_attr(el, "height"))
        .and_then(|val| val.parse::<i64>().ok())
        .map(Emu);
    let alt = find_attr(el, "descr").or_else(|| find_attr(el, "title"));
    let anchor = read_anchor(el);
    para.push_object(
        InlineObject::Picture(Picture {
            name: target.unwrap_or_else(|| "image.png".into()),
            bytes,
            content_type,
            width,
            height,
            alt,
            anchor,
        }),
        props,
    );
}

fn read_anchor(el: &El) -> Option<Anchor> {
    let mut found = None;
    for child in el.child_els() {
        match child.name.as_str() {
            "anchor" => {
                let wrap = child
                    .child_els()
                    .find(|item| {
                        matches!(
                            item.name.as_str(),
                            "wrapSquare"
                                | "wrapTight"
                                | "wrapThrough"
                                | "wrapTopAndBottom"
                                | "wrapNone"
                        )
                    })
                    .map(|item| match item.name.as_str() {
                        "wrapSquare" => {
                            format!("square:{}", item.attr("wrapText").unwrap_or("bothSides"))
                        }
                        other => other.to_string(),
                    });
                found = Some(Anchor {
                    behind_text: child
                        .attr("behindDoc")
                        .map(|val| val == "1" || val == "true")
                        .unwrap_or(false),
                    h_anchor: child.attr("relativeFromH").map(str::to_string),
                    v_anchor: child.attr("relativeFromV").map(str::to_string),
                    h_align: axis_align(child, "positionH"),
                    v_align: axis_align(child, "positionV"),
                    x: axis_offset(child, "positionH"),
                    y: axis_offset(child, "positionV"),
                    wrap,
                    dist_top: find_attr(child, "distT")
                        .and_then(|val| val.parse::<i64>().ok())
                        .map(Emu),
                    dist_bottom: find_attr(child, "distB")
                        .and_then(|val| val.parse::<i64>().ok())
                        .map(Emu),
                    dist_left: find_attr(child, "distL")
                        .and_then(|val| val.parse::<i64>().ok())
                        .map(Emu),
                    dist_right: find_attr(child, "distR")
                        .and_then(|val| val.parse::<i64>().ok())
                        .map(Emu),
                });
            }
            "inline" => {}
            _ => {}
        }
    }
    found
}

/// `wp:align` carries its value as element text, so read both forms.
fn axis_align(el: &El, axis: &str) -> Option<String> {
    let axis = el.child_els().find(|item| item.name == axis)?;
    let align = axis.child_els().find(|inner| inner.name == "align")?;
    let text = align.text().trim().to_string();
    if text.is_empty() {
        align.attr("val").map(str::to_string)
    } else {
        Some(text)
    }
}

fn axis_offset(el: &El, axis: &str) -> Option<Emu> {
    let axis = el.child_els().find(|item| item.name == axis)?;
    let offset = axis.child_els().find(|inner| inner.name == "posOffset")?;
    offset.text().trim().parse::<i64>().ok().map(Emu)
}

/// True when the subtree holds an element with this local name.
fn contains_element(el: &El, name: &str) -> bool {
    el.child_els()
        .any(|child| child.name == name || contains_element(child, name))
}

fn find_attr(el: &El, name: &str) -> Option<String> {
    if let Some(value) = el.attr(name) {
        return Some(value.to_string());
    }
    for child in el.child_els() {
        if let Some(found) = find_attr(child, name) {
            return Some(found);
        }
    }
    None
}

fn read_rpr(el: &El, ctx: &Ctx<'_>) -> CharProps {
    let mut props = CharProps::default();
    for child in el.child_els() {
        match child.name.as_str() {
            "b" => props.bold = val_on(child),
            "i" => props.italic = val_on(child),
            "bCs" => props.bold_cs = Some(val_on(child)),
            "iCs" => props.italic_cs = Some(val_on(child)),
            "u" => {
                props.underline = val_on(child);
                props.underline_val = child.attr("val").map(str::to_string);
                props.underline_color = child.attr("color").map(str::to_string);
            }
            "strike" => props.strike = val_on(child),
            "dstrike" => props.double_strike = val_on(child),
            "caps" => props.caps = val_on(child),
            "smallCaps" => props.small_caps = val_on(child),
            "emboss" => props.emboss = val_on(child),
            "imprint" => props.imprint = val_on(child),
            "outline" => props.outline = val_on(child),
            "shadow" => props.shadow = val_on(child),
            "noProof" => props.no_proof = val_on(child),
            "snapToGrid" => props.snap_to_grid = Some(val_on(child)),
            "spacing" => props.spacing = attr_i32(child, "val"),
            "kern" => props.kern = child.attr("val").and_then(|val| val.parse::<u32>().ok()),
            "position" => props.position = attr_i32(child, "val"),
            "w" => props.scale = child.attr("val").and_then(|val| val.parse::<u32>().ok()),
            "bdr" => props.border = Some(read_border(child)),
            "eastAsianLayout" => props.east_asian_layout = Some(read_east_asian_layout(child)),
            "fitText" => {
                props.fit_text = child
                    .attr("val")
                    .and_then(|val| val.parse::<u32>().ok())
                    .map(|twips| edgedoc_unit::Twips(twips as i32))
            }
            "sz" => {
                props.size_pt = child
                    .attr("val")
                    .and_then(|val| val.parse::<f32>().ok())
                    .map(|half| half / 2.0)
            }
            "szCs" => {
                props.size_cs = child
                    .attr("val")
                    .and_then(|val| val.parse::<f32>().ok())
                    .map(|half| half / 2.0)
            }
            "rFonts" => {
                props.font = child.attr("ascii").map(str::to_string);
                props.font_hansi = child.attr("hAnsi").map(str::to_string);
                props.font_cs = child.attr("cs").map(str::to_string);
                props.font_ea = child.attr("eastAsia").map(str::to_string);
                if props.font.is_none() {
                    props.font = props.font_hansi.clone();
                }
            }
            "color" => props.color = child.attr("val").map(str::to_string),
            "highlight" => props.highlight = child.attr("val").map(str::to_string),
            "shd" => props.shading = child.attr("fill").map(str::to_string),
            "rtl" => props.rtl = val_on(child),
            "cs" => props.complex_script = val_on(child),
            "vanish" => props.vanish = val_on(child),
            "webHidden" => props.web_hidden = val_on(child),
            "vertAlign" => props.vert_align = child.attr("val").map(str::to_string),
            "lang" => {
                props.lang = child.attr("val").map(str::to_string);
                props.lang_bidi = child.attr("bidi").map(str::to_string);
                props.lang_ea = child.attr("eastAsia").map(str::to_string);
                if props.lang.is_none() {
                    props.lang = props.lang_bidi.clone();
                }
            }
            "rStyle" => props.style = child.attr("val").map(str::to_string),
            other if !crate::catalog::is_owned(other) => {
                props.extra.push(child.raw(ctx.source).to_string())
            }
            _ => {}
        }
    }
    props
}

fn read_ppr(el: &El, ctx: &Ctx<'_>) -> ParaProps {
    let mut props = ParaProps::default();
    for child in el.child_els() {
        match child.name.as_str() {
            "pStyle" => props.style = child.attr("val").map(str::to_string),
            "jc" => props.align = child.attr("val").map(str::to_string),
            "ind" => {
                props.indent_left =
                    attr_twips(child, "left").or_else(|| attr_twips(child, "start"));
                props.indent_right =
                    attr_twips(child, "right").or_else(|| attr_twips(child, "end"));
                props.indent_first = attr_twips(child, "firstLine")
                    .or_else(|| attr_twips(child, "hanging").map(|tw| Twips(-tw.0)));
                props.indent_left_chars =
                    attr_i32(child, "leftChars").or_else(|| attr_i32(child, "startChars"));
                props.indent_right_chars =
                    attr_i32(child, "rightChars").or_else(|| attr_i32(child, "endChars"));
                props.indent_first_chars = attr_i32(child, "firstLineChars")
                    .or_else(|| attr_i32(child, "hangingChars").map(|value| -value));
            }
            "spacing" => {
                props.spacing_before = attr_twips(child, "before");
                props.spacing_after = attr_twips(child, "after");
                props.line = attr_twips(child, "line");
                props.line_rule = child.attr("lineRule").map(str::to_string);
            }
            "shd" => props.shading = child.attr("fill").map(str::to_string),
            "pBdr" => props.borders = read_borders(child),
            "rPr" => props.mark = read_rpr(child, ctx),
            "numPr" => {
                props.ilvl = nested_val(child, "ilvl");
                props.num_id = nested_val(child, "numId");
            }
            "tabs" => {
                for tab in child.child_els().filter(|item| item.name == "tab") {
                    let pos = attr_twips(tab, "pos").unwrap_or(Twips(0));
                    props.tabs.push(TabStop {
                        val: tab.attr("val").unwrap_or("left").to_string(),
                        pos,
                        leader: tab.attr("leader").map(str::to_string),
                    });
                }
            }
            "framePr" => props.frame = Some(read_frame(child, ctx)),
            "keepNext" => props.keep_next = val_on(child),
            "keepLines" => props.keep_lines = val_on(child),
            "pageBreakBefore" => props.page_break_before = val_on(child),
            "widowControl" => props.widow_control = val_on(child),
            "contextualSpacing" => props.contextual_spacing = val_on(child),
            "suppressLineNumbers" => props.suppress_line_numbers = val_on(child),
            "outlineLvl" => props.outline = child.attr("val").and_then(|val| val.parse().ok()),
            "bidi" => props.bidi = val_on(child),
            other if other != "sectPr" && !crate::catalog::is_owned(other) => {
                props.extra.push(child.raw(ctx.source).to_string())
            }
            _ => {}
        }
    }
    props
}

fn read_east_asian_layout(el: &El) -> edgedoc_model::EastAsianLayout {
    edgedoc_model::EastAsianLayout {
        id: el.attr("id").and_then(|val| val.parse().ok()),
        combine: el.attr("combine").map(|val| is_on(Some(val))),
        combine_brackets: el.attr("combineBrackets").map(str::to_string),
        vertical: el.attr("vert").map(|val| is_on(Some(val))),
        vertical_compress: el.attr("vertCompress").map(|val| is_on(Some(val))),
    }
}

fn read_frame(el: &El, ctx: &Ctx<'_>) -> FrameProps {
    FrameProps {
        drop_cap: el.attr("dropCap").map(str::to_string),
        lines: el.attr("lines").and_then(|val| val.parse::<u32>().ok()),
        wrap: el.attr("wrap").map(str::to_string),
        x: attr_twips(el, "x"),
        y: attr_twips(el, "y"),
        h_anchor: el.attr("hAnchor").map(str::to_string),
        v_anchor: el.attr("vAnchor").map(str::to_string),
        h_space: attr_twips(el, "hSpace"),
        v_space: attr_twips(el, "vSpace"),
        width: attr_twips(el, "w"),
        height: attr_twips(el, "h"),
        extra: el
            .child_els()
            .filter(|child| !crate::catalog::is_owned(&child.name))
            .map(|child| child.raw(ctx.source).to_string())
            .collect(),
    }
}

fn nested_val(el: &El, name: &str) -> Option<u32> {
    el.child_els()
        .find(|child| child.name == name)
        .and_then(|child| child.attr("val"))
        .and_then(|val| val.parse().ok())
}

fn val_on(el: &El) -> bool {
    !matches!(el.attr("val"), Some("0") | Some("false") | Some("off"))
}

/// Property containers that may sit beside a block. They are read by the block reader.
fn is_property_element(name: &str) -> bool {
    matches!(
        name,
        "tcPr"
            | "tblPr"
            | "tblPrEx"
            | "trPr"
            | "pPr"
            | "rPr"
            | "sdtPr"
            | "sdtEndPr"
            | "tblGrid"
            | "footnotePr"
            | "endnotePr"
            | "numPr"
            | "sectPrChange"
    )
}

fn read_sect(el: &El, ctx: &Ctx<'_>, doc: &mut ModelDocument) -> WmlResult<SectionProps> {
    let mut props = SectionProps::default();
    for child in el.child_els() {
        match child.name.as_str() {
            "pgSz" => {
                if let Some(width) = attr_twips(child, "w") {
                    props.page_w = width;
                }
                if let Some(height) = attr_twips(child, "h") {
                    props.page_h = height;
                }
                props.landscape = child.attr("orient") == Some("landscape");
            }
            "pgMar" => {
                if let Some(value) = attr_twips(child, "top") {
                    props.margin_top = value;
                }
                if let Some(value) = attr_twips(child, "bottom") {
                    props.margin_bottom = value;
                }
                if let Some(value) = attr_twips(child, "left") {
                    props.margin_left = value;
                }
                if let Some(value) = attr_twips(child, "right") {
                    props.margin_right = value;
                }
                if let Some(value) = attr_twips(child, "header") {
                    props.header_dist = value;
                }
                if let Some(value) = attr_twips(child, "footer") {
                    props.footer_dist = value;
                }
                if let Some(value) = attr_twips(child, "gutter") {
                    props.gutter = value;
                }
            }
            "cols" => {
                props.columns = child
                    .attr("num")
                    .and_then(|val| val.parse().ok())
                    .unwrap_or(1);
                props.column_space = attr_twips(child, "space");
                props.separate_columns = child
                    .attr("sep")
                    .map(|val| val == "1" || val == "true")
                    .unwrap_or(false);
            }
            "pgNumType" => {
                props.page_start = child.attr("start").and_then(|val| val.parse().ok());
                props.page_fmt = child.attr("fmt").map(str::to_string);
            }
            "titlePg" => props.title_page = val_on(child),
            "bidi" => props.bidi = val_on(child),
            "rtlGutter" => props.rtl_gutter = val_on(child),
            "textDirection" => props.text_direction = child.attr("val").map(str::to_string),
            "vAlign" => props.v_align = child.attr("val").map(str::to_string),
            "lnNumType" => {
                props.line_numbering = Some(LineNumbering {
                    count_by: child
                        .attr("countBy")
                        .and_then(|val| val.parse().ok())
                        .unwrap_or(1),
                    start: child
                        .attr("start")
                        .and_then(|val| val.parse().ok())
                        .unwrap_or(1),
                    restart: child.attr("restart").map(str::to_string),
                    distance: attr_twips(child, "distance"),
                });
            }
            "pgBorders" => props.page_borders = read_borders(child),
            "docGrid" => {
                props.doc_grid = Some(DocGrid {
                    grid_type: child.attr("type").map(str::to_string),
                    line_pitch: attr_i32(child, "linePitch"),
                    char_space: attr_i32(child, "charSpace"),
                });
            }
            "footnotePr" => props.footnotes = read_note_pr(child),
            "endnotePr" => props.endnotes = read_note_pr(child),
            "headerReference" => {
                attach_story(&mut props.headers, child, ctx, doc, PartKind::Header)?
            }
            "footerReference" => {
                attach_story(&mut props.footers, child, ctx, doc, PartKind::Footer)?
            }
            "type" => {
                props.start = match child.attr("val").unwrap_or("nextPage") {
                    "continuous" => SectionStart::Continuous,
                    "evenPage" => SectionStart::EvenPage,
                    "oddPage" => SectionStart::OddPage,
                    _ => SectionStart::NextPage,
                };
            }
            other if !crate::catalog::is_owned(other) => {
                props.extra.push(child.raw(ctx.source).to_string())
            }
            _ => {}
        }
    }
    Ok(props)
}

fn read_note_pr(el: &El) -> FootnotePr {
    FootnotePr {
        num_fmt: nested_attr(el, "numFmt", "val"),
        num_start: nested_attr(el, "numStart", "val").and_then(|val| val.parse().ok()),
        num_restart: nested_attr(el, "numRestart", "val"),
        pos: child_attr(el, "pos", "val"),
    }
}

fn child_attr(el: &El, name: &str, attr: &str) -> Option<String> {
    el.child_els()
        .find(|child| child.name == name)
        .and_then(|child| child.attr(attr))
        .map(str::to_string)
}

fn nested_attr(el: &El, name: &str, attr: &str) -> Option<String> {
    child_attr(el, name, attr)
}

fn attach_story(
    refs: &mut HeaderRefs,
    el: &El,
    ctx: &Ctx<'_>,
    doc: &mut ModelDocument,
    kind: PartKind,
) -> WmlResult<()> {
    let id = el.attr("id").unwrap_or("");
    let Some(rel) = ctx.rels.iter().find(|rel| rel.id == id) else {
        return Ok(());
    };
    let name = join_target(&ctx.base, &rel.target);
    let blocks = if let Some(part) = ctx.package.part(&name) {
        let root = xml::parse(&part.bytes, ctx.budget).map_err(map_opc)?;
        let rels = part_rels(ctx.package, &part.name, ctx.budget)?;
        let nested = Ctx {
            package: ctx.package,
            budget: ctx.budget,
            rels: &rels,
            base: part.name.clone(),
            source: &part.bytes,
        };
        read_blocks(&root, &nested, doc, 0)?
    } else {
        Vec::new()
    };
    let part_id = doc.add_part(kind, blocks);
    match el.attr("type").unwrap_or("default") {
        "first" => refs.first = Some(part_id),
        "even" => refs.even = Some(part_id),
        _ => refs.default = Some(part_id),
    }
    Ok(())
}

fn read_borders(el: &El) -> Borders {
    let mut borders = Borders::default();
    for child in el.child_els() {
        let line = read_border(child);
        match child.name.as_str() {
            "top" => borders.top = Some(line),
            "left" | "start" => borders.left = Some(line),
            "bottom" => borders.bottom = Some(line),
            "right" | "end" => borders.right = Some(line),
            "insideH" => borders.inside_h = Some(line),
            "insideV" => borders.inside_v = Some(line),
            "between" => borders.between = Some(line),
            "bar" => borders.bar = Some(line),
            "tl2br" => borders.tl2_br = Some(line),
            "tr2bl" => borders.tr2_bl = Some(line),
            _ => {}
        }
    }
    if let Some(start) = borders.left.clone() {
        borders.start = Some(start);
    }
    if let Some(end) = borders.right.clone() {
        borders.end = Some(end);
    }
    borders
}

fn read_border(el: &El) -> LineBorder {
    LineBorder {
        val: el.attr("val").unwrap_or("single").to_string(),
        size: el.attr("sz").and_then(|val| val.parse().ok()),
        color: el.attr("color").map(str::to_string),
        space: el.attr("space").and_then(|val| val.parse().ok()),
    }
}

fn read_margins(el: &El) -> CellMargins {
    CellMargins {
        top: el
            .child_els()
            .find(|child| child.name == "top")
            .and_then(|child| attr_twips(child, "w")),
        left: el
            .child_els()
            .find(|child| child.name == "left" || child.name == "start")
            .and_then(|child| attr_twips(child, "w")),
        bottom: el
            .child_els()
            .find(|child| child.name == "bottom")
            .and_then(|child| attr_twips(child, "w")),
        right: el
            .child_els()
            .find(|child| child.name == "right" || child.name == "end")
            .and_then(|child| attr_twips(child, "w")),
    }
}

fn read_cell_margins(el: &El) -> CellMargins {
    read_margins(el)
}

fn attr_twips(el: &El, name: &str) -> Option<Twips> {
    el.attr(name)
        .and_then(|val| val.parse::<i32>().ok())
        .map(Twips)
}

fn attr_i32(el: &El, name: &str) -> Option<i32> {
    el.attr(name).and_then(|val| val.parse::<i32>().ok())
}

fn read_styles(doc: &mut ModelDocument, ctx: &Ctx<'_>) -> WmlResult<()> {
    let Some(rel) = ctx
        .rels
        .iter()
        .find(|rel| rel.rel_type.ends_with("/styles"))
    else {
        return Ok(());
    };
    let name = join_target(&ctx.base, &rel.target);
    let Some(part) = ctx.package.part(&name) else {
        return Ok(());
    };
    let root = xml::parse(&part.bytes, ctx.budget).map_err(map_opc)?;
    for child in root.child_els() {
        match child.name.as_str() {
            "docDefaults" => doc.styles.doc_defaults_xml = Some(child.raw(&part.bytes).to_string()),
            "latentStyles" => {
                doc.styles.latent_styles_xml = Some(child.raw(&part.bytes).to_string())
            }
            "style" => {
                let id = child.attr("styleId").unwrap_or("").to_string();
                let name = child
                    .child_els()
                    .find(|el| el.name == "name")
                    .and_then(|el| el.attr("val"))
                    .unwrap_or(&id)
                    .to_string();
                let mut style = Style::paragraph(&id);
                style.name = name;
                style.kind = StyleKind::parse(child.attr("type").unwrap_or("paragraph"));
                style.q_format = val_on(
                    child
                        .child_els()
                        .find(|el| el.name == "qFormat")
                        .unwrap_or(child),
                );
                style.raw_xml = Some(child.raw(&part.bytes).to_string());
                for item in child.child_els() {
                    match item.name.as_str() {
                        "basedOn" => style.based_on = item.attr("val").map(str::to_string),
                        "next" => style.next = item.attr("val").map(str::to_string),
                        "link" => style.link = item.attr("val").map(str::to_string),
                        "semiHidden" => style.semi_hidden = val_on(item),
                        "uiPriority" => style.priority = attr_i32(item, "val"),
                        "pPr" => style.para_props = read_ppr(item, ctx),
                        "rPr" => style.char_props = read_rpr(item, ctx),
                        _ => {}
                    }
                }
                doc.styles.styles.push(style);
            }
            other => doc.styles.extra.push(other.to_string()),
        }
    }
    Ok(())
}

fn read_settings(doc: &mut ModelDocument, ctx: &Ctx<'_>) -> WmlResult<()> {
    let Some(rel) = ctx
        .rels
        .iter()
        .find(|rel| rel.rel_type.ends_with("/settings"))
    else {
        return Ok(());
    };
    let name = join_target(&ctx.base, &rel.target);
    if let Some(part) = ctx.package.part(&name) {
        doc.settings_xml = Some(String::from_utf8_lossy(&part.bytes).into_owned());
    }
    Ok(())
}

fn read_numbering(doc: &mut ModelDocument, ctx: &Ctx<'_>) -> WmlResult<()> {
    let Some(rel) = ctx
        .rels
        .iter()
        .find(|rel| rel.rel_type.ends_with("/numbering"))
    else {
        return Ok(());
    };
    let name = join_target(&ctx.base, &rel.target);
    if let Some(part) = ctx.package.part(&name) {
        doc.numbering_xml = Some(String::from_utf8_lossy(&part.bytes).into_owned());
    }
    Ok(())
}

fn read_core(doc: &mut ModelDocument, package: &Package, budget: &Budget) -> WmlResult<()> {
    let Some(part) = package
        .parts
        .values()
        .find(|part| part.name.ends_with("docProps/core.xml"))
    else {
        return Ok(());
    };
    let root = xml::parse(&part.bytes, budget).map_err(map_opc)?;
    for child in root.child_els() {
        let text = descendant_text(child);
        match child.name.as_str() {
            "title" => doc.core.title = text,
            "subject" => doc.core.subject = text,
            "creator" => doc.core.creator = text,
            "keywords" => doc.core.keywords = text,
            "description" => doc.core.description = text,
            "lastModifiedBy" => doc.core.last_modified_by = text,
            "revision" => doc.core.revision = text,
            "created" => doc.core.created = text,
            "modified" => doc.core.modified = text,
            "category" => doc.core.category = text,
            "contentStatus" => doc.core.content_status = text,
            "identifier" => doc.core.identifier = text,
            "language" => doc.core.language = text,
            "version" => doc.core.version = text,
            _ => {}
        }
    }
    Ok(())
}

/// Footnotes, endnotes, and their separators become story parts.
fn read_notes(doc: &mut ModelDocument, ctx: &Ctx<'_>) -> WmlResult<()> {
    for (rel_suffix, kind) in [
        ("footnotes", PartKind::Footnote),
        ("endnotes", PartKind::Endnote),
    ] {
        let Some(rel) = ctx
            .rels
            .iter()
            .find(|rel| rel.rel_type.ends_with(&format!("/{rel_suffix}")))
        else {
            continue;
        };
        let name = join_target(&ctx.base, &rel.target);
        let Some(part) = ctx.package.part(&name) else {
            continue;
        };
        let root = xml::parse(&part.bytes, ctx.budget).map_err(map_opc)?;
        let rels = part_rels(ctx.package, &part.name, ctx.budget)?;
        for note in root.child_els() {
            if note.name != "footnote" && note.name != "endnote" {
                continue;
            }
            let nested = Ctx {
                package: ctx.package,
                budget: ctx.budget,
                rels: &rels,
                base: part.name.clone(),
                source: &part.bytes,
            };
            let blocks = read_blocks(note, &nested, doc, 0)?;
            let meta = PartMeta {
                id: note.attr("id").and_then(|val| val.parse().ok()),
                author: note.attr("author").map(str::to_string),
                date: note.attr("date").map(str::to_string),
                initials: note.attr("initials").map(str::to_string),
                note_type: note.attr("type").map(str::to_string),
            };
            doc.add_part_with(kind, blocks, meta);
        }
    }
    Ok(())
}

/// Comments become story parts. The body keeps the comment marks.
fn read_comments(doc: &mut ModelDocument, ctx: &Ctx<'_>) -> WmlResult<()> {
    let Some(rel) = ctx
        .rels
        .iter()
        .find(|rel| rel.rel_type.ends_with("/comments"))
    else {
        return Ok(());
    };
    let name = join_target(&ctx.base, &rel.target);
    let Some(part) = ctx.package.part(&name) else {
        return Ok(());
    };
    let root = xml::parse(&part.bytes, ctx.budget).map_err(map_opc)?;
    let rels = part_rels(ctx.package, &part.name, ctx.budget)?;
    for comment in root.child_els() {
        if comment.name != "comment" {
            continue;
        }
        let nested = Ctx {
            package: ctx.package,
            budget: ctx.budget,
            rels: &rels,
            base: part.name.clone(),
            source: &part.bytes,
        };
        let blocks = read_blocks(comment, &nested, doc, 0)?;
        let meta = PartMeta {
            id: comment.attr("id").and_then(|val| val.parse().ok()),
            author: comment.attr("author").map(str::to_string),
            date: comment.attr("date").map(str::to_string),
            initials: comment.attr("initials").map(str::to_string),
            note_type: None,
        };
        doc.add_part_with(PartKind::Comment, blocks, meta);
    }
    Ok(())
}

fn part_rels(package: &Package, name: &str, budget: &Budget) -> WmlResult<Vec<Rel>> {
    let file = name.rsplit('/').next().unwrap_or(name);
    let dir = name.rsplit_once('/').map(|(dir, _)| dir).unwrap_or("");
    let rels_name = if dir.is_empty() {
        format!("_rels/{file}.rels")
    } else {
        format!("{dir}/_rels/{file}.rels")
    };
    match package.part(&rels_name) {
        Some(part) => read_rels(&part.bytes, budget).map_err(map_opc),
        None => Ok(Vec::new()),
    }
}

pub fn join_target(base: &str, target: &str) -> String {
    if target.starts_with('/') {
        return normalize(target);
    }
    let dir = base.rsplit_once('/').map(|(dir, _)| dir).unwrap_or("");
    if dir.is_empty() {
        normalize(target)
    } else {
        normalize(&format!("{dir}/{target}"))
    }
}

fn map_opc(err: edgedoc_opc::OpcError) -> WmlError {
    WmlError::new(err.code, err.message)
}
