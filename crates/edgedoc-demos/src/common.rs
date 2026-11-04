//! Shared style helpers for the example documents.
//!
//! These helpers keep the examples short. They are not part of the public API.
//! Each example compiles this module on its own, so a helper that only one example
//! calls would otherwise warn.
#![allow(dead_code)]

use std::sync::Arc;

use edgedoc::{
    Block, Borders, Cell, CellMargins, Field, InlineObject, LineBorder, Mm, Paragraph, Pt, Row,
    RunBuilder, Style, Table, Twips,
};

pub const INK: &str = "111827";
pub const NAVY: &str = "1E3A8A";
pub const INDIGO: &str = "4F46E5";
pub const SLATE: &str = "64748B";
pub const MIST: &str = "F1F5F9";
pub const LINE: &str = "CBD5E1";
pub const WHITE: &str = "FFFFFF";

pub const LOGO: &[u8] = include_bytes!("../assets/logo.png");
pub const CHART: &[u8] = include_bytes!("../assets/chart.png");
pub const SEAL: &[u8] = include_bytes!("../assets/seal.png");

/// The style sheet that every example starts from.
pub fn house_styles() -> Vec<Style> {
    vec![
        Style::paragraph("Normal")
            .named("Normal")
            .font("Aptos")
            .size(Pt(10.5)),
        Style::paragraph("Title")
            .named("Title")
            .size(Pt(32.0))
            .bold()
            .color(NAVY)
            .font("Aptos Display"),
        Style::paragraph("Subtitle")
            .named("Subtitle")
            .size(Pt(13.0))
            .color(SLATE)
            .font("Aptos"),
        Style::paragraph("Heading1")
            .named("heading 1")
            .size(Pt(16.0))
            .bold()
            .color(INK)
            .font("Aptos Display"),
        Style::paragraph("Heading2")
            .named("heading 2")
            .size(Pt(12.0))
            .bold()
            .color(NAVY)
            .font("Aptos Display"),
        Style::paragraph("Caption")
            .named("caption")
            .size(Pt(8.5))
            .italic()
            .color(SLATE),
        Style::character("Eyebrow")
            .named("Eyebrow")
            .size(Pt(8.5))
            .bold()
            .color(INDIGO),
        Style::table("Grid").named("EdgeDoc Grid"),
    ]
}

/// A thin line under a block of text.
pub fn rule() -> Borders {
    Borders {
        bottom: Some(LineBorder {
            val: "single".into(),
            size: Some(6),
            color: Some(LINE.into()),
            space: Some(1),
        }),
        ..Borders::default()
    }
}

/// A small uppercase label above a heading.
pub fn eyebrow(text: &str) -> Paragraph {
    Paragraph::new()
        .run(RunBuilder::new().text(text).style("Eyebrow"))
        .spacing_before(Twips(0))
        .spacing_after(Twips(100))
        .keep_next()
}

pub fn heading(text: &str) -> Paragraph {
    Paragraph::new()
        .style("Heading1")
        .text(text)
        .spacing_before(Mm(6.0))
        .spacing_after(Mm(2.5))
        .keep_next()
}

pub fn subheading(text: &str) -> Paragraph {
    Paragraph::new()
        .style("Heading2")
        .text(text)
        .spacing_before(Mm(4.0))
        .spacing_after(Mm(1.5))
        .keep_next()
}

pub fn body(text: &str) -> Paragraph {
    Paragraph::new()
        .text(text)
        .spacing_after(Mm(2.5))
        .line_spacing(Twips(280), "auto")
}

pub fn muted(text: &str) -> Paragraph {
    Paragraph::new().run(RunBuilder::new().text(text).color(SLATE).size(Pt(9.0)))
}

pub fn caption(text: &str) -> Paragraph {
    Paragraph::new()
        .style("Caption")
        .text(text)
        .spacing_before(Mm(1.5))
        .spacing_after(Mm(4.0))
}

/// A page-number field with a cached result, so the file reads well before Word updates it.
pub fn page_field() -> Paragraph {
    let mut para = Paragraph::new();
    para.push_object(
        InlineObject::Field(Field::new(" PAGE ", "1")),
        &edgedoc::CharProps::default(),
    );
    para
}

pub fn page_count_field() -> Paragraph {
    let mut para = Paragraph::new();
    para.push_object(
        InlineObject::Field(Field::new(" NUMPAGES ", "1")),
        &edgedoc::CharProps::default(),
    );
    para
}

/// `Page 1 of 1`, with both fields cached so the file reads well before Word updates it.
pub fn page_x_of_y(colour: &str) -> Paragraph {
    let style = props(8.0, colour, false);
    let mut para = Paragraph::new();
    para.push_text("Page ", &style);
    para.push_object(InlineObject::Field(Field::new(" PAGE ", "1")), &style);
    para.push_text(" of ", &style);
    para.push_object(InlineObject::Field(Field::new(" NUMPAGES ", "1")), &style);
    para
}

/// A paragraph built from parts, so a line can mix styles and a field.
pub fn line(parts: Vec<(String, edgedoc::CharProps)>) -> Paragraph {
    let mut para = Paragraph::new();
    for (text, props) in parts {
        para.push_text(&text, &props);
    }
    para
}

pub fn props(size: f32, colour: &str, bold: bool) -> edgedoc::CharProps {
    let mut props = edgedoc::CharProps {
        size_pt: Some(size),
        color: Some(colour.into()),
        ..edgedoc::CharProps::default()
    };
    props.bold = bold;
    props
}

/// A label on the left and a value on the right, with no outer border.
pub fn split(left: Vec<Block>, right: Vec<Block>, left_width: Mm, right_width: Mm) -> Table {
    let mut table = Table::new().widths([left_width, right_width]);
    table.cell_margins = CellMargins {
        top: Some(Twips(0)),
        bottom: Some(Twips(0)),
        left: Some(Twips(0)),
        right: Some(Twips(120)),
    };
    let mut first = Cell::paragraph("");
    first.blocks = left.into_iter().map(Arc::new).collect();
    let mut second = Cell::paragraph("");
    second.blocks = right.into_iter().map(Arc::new).collect();
    table.rows.push(Row::cells([first, second]));
    table
}

/// A block that is one paragraph.
pub fn p(para: Paragraph) -> Block {
    Block::Paragraph(para)
}

/// A key and value strip: a light label column and a plain value column.
pub fn facts(rows: &[(&str, &str)]) -> Table {
    let mut table = Table::new().widths([Mm(34.0), Mm(114.0)]);
    table.cell_margins = CellMargins {
        top: Some(Twips(40)),
        bottom: Some(Twips(40)),
        left: Some(Twips(0)),
        right: Some(Twips(96)),
    };
    table.borders = Borders {
        inside_h: Some(LineBorder {
            val: "single".into(),
            size: Some(2),
            color: Some(MIST.into()),
            space: None,
        }),
        ..Borders::default()
    };
    for (label, value) in rows {
        let label_cell = Cell::paragraph("").blocks(vec![Arc::new(p(muted(label)))]);
        let value_cell =
            Cell::paragraph("").blocks(vec![Arc::new(p(Paragraph::new().text(value)))]);
        table.rows.push(Row::cells([label_cell, value_cell]));
    }
    table
}

/// A line-item table with a navy header row and a right-aligned money column.
pub fn money_table(header: [&str; 4], rows: &[(&str, &str, &str, &str)]) -> Table {
    let mut table = Table::new().widths([Mm(84.0), Mm(18.0), Mm(26.0), Mm(26.0)]);
    table.cell_margins = CellMargins {
        top: Some(Twips(56)),
        bottom: Some(Twips(56)),
        left: Some(Twips(84)),
        right: Some(Twips(84)),
    };
    table.borders = Borders {
        top: Some(LineBorder {
            val: "single".into(),
            size: Some(8),
            color: Some(NAVY.into()),
            space: None,
        }),
        bottom: Some(LineBorder {
            val: "single".into(),
            size: Some(8),
            color: Some(NAVY.into()),
            space: None,
        }),
        inside_h: Some(LineBorder {
            val: "single".into(),
            size: Some(2),
            color: Some(MIST.into()),
            space: None,
        }),
        ..Borders::default()
    };
    let mut header_row = Row::cells(Vec::new()).header();
    for (index, text) in header.iter().enumerate() {
        let align = if index == 0 { "left" } else { "right" };
        let mut cell = Cell::paragraph("");
        cell.shading = Some(NAVY.into());
        cell.blocks = vec![Arc::new(p(Paragraph::new().align(align).run(
            RunBuilder::new()
                .text(text)
                .bold()
                .size(Pt(8.5))
                .color(WHITE),
        )))];
        header_row.cells.push(cell);
    }
    table.rows.push(header_row);
    for (index, (description, quantity, unit, amount)) in rows.iter().enumerate() {
        let mut row = Row::cells(Vec::new());
        let cells = [
            (description.to_string(), "left"),
            (quantity.to_string(), "right"),
            (unit.to_string(), "right"),
            (amount.to_string(), "right"),
        ];
        for (text, align) in cells {
            let mut cell = Cell::paragraph("");
            cell.blocks = vec![Arc::new(p(Paragraph::new().text(&text).align(align)))];
            if index % 2 == 1 {
                cell.shading = Some(MIST.into());
            }
            row.cells.push(cell);
        }
        table.rows.push(row);
    }
    table
}

/// A three-column total strip below a money table.
fn totals(lines: &[(&str, &str, bool)]) -> Table {
    let mut table = Table::new().widths([Mm(96.0), Mm(28.0), Mm(26.0)]);
    table.cell_margins = CellMargins {
        top: Some(Twips(40)),
        bottom: Some(Twips(40)),
        left: Some(Twips(0)),
        right: Some(Twips(84)),
    };
    for (label, value, strong) in lines {
        let mut row = Row::cells(Vec::new());
        row.cells.push(Cell::paragraph(""));
        let mut label_cell = Cell::paragraph("");
        let mut value_cell = Cell::paragraph("");
        let label_props = if *strong {
            props(11.0, INK, true)
        } else {
            props(10.0, SLATE, false)
        };
        let value_props = if *strong {
            props(12.0, NAVY, true)
        } else {
            props(10.0, INK, false)
        };
        label_cell.blocks = vec![Arc::new(Block::Paragraph(
            line(vec![(label.to_string(), label_props)]).align("right"),
        ))];
        value_cell.blocks = vec![Arc::new(Block::Paragraph(
            line(vec![(value.to_string(), value_props)]).align("right"),
        ))];
        if *strong {
            for cell in [&mut label_cell, &mut value_cell] {
                cell.borders = Borders {
                    top: Some(LineBorder {
                        val: "single".into(),
                        size: Some(8),
                        color: Some(NAVY.into()),
                        space: None,
                    }),
                    ..Borders::default()
                };
            }
        }
        row.cells.push(label_cell);
        row.cells.push(value_cell);
        table.rows.push(row);
    }
    table
}

/// A total strip with each value as a merge field.
pub fn totals_fields(lines: &[(&str, &str, bool)]) -> Table {
    let mut table = Table::new().widths([Mm(96.0), Mm(28.0), Mm(26.0)]);
    table.cell_margins = CellMargins {
        top: Some(Twips(40)),
        bottom: Some(Twips(40)),
        left: Some(Twips(0)),
        right: Some(Twips(84)),
    };
    for (label, field, strong) in lines {
        let mut row = Row::cells(Vec::new());
        row.cells.push(Cell::paragraph(""));
        let mut label_cell = Cell::paragraph("");
        let mut value_cell = Cell::paragraph("");
        let label_props = if *strong {
            props(11.0, INK, true)
        } else {
            props(10.0, SLATE, false)
        };
        let value_props = if *strong {
            props(12.0, NAVY, true)
        } else {
            props(10.0, INK, false)
        };
        label_cell.blocks = vec![Arc::new(Block::Paragraph(
            line(vec![(label.to_string(), label_props)]).align("right"),
        ))];
        value_cell.blocks = vec![Arc::new(Block::Paragraph(
            Paragraph::new()
                .align("right")
                .merge_field_styled(field, &value_props),
        ))];
        if *strong {
            for cell in [&mut label_cell, &mut value_cell] {
                cell.borders = Borders {
                    top: Some(LineBorder {
                        val: "single".into(),
                        size: Some(8),
                        color: Some(NAVY.into()),
                        space: None,
                    }),
                    ..Borders::default()
                };
            }
        }
        row.cells.push(label_cell);
        row.cells.push(value_cell);
        table.rows.push(row);
    }
    table
}

pub fn logo(width_mm: f32) -> edgedoc::Picture {
    edgedoc::Picture::png(LOGO.to_vec())
        .size(
            edgedoc::Emu((width_mm * 36_000.0) as i64),
            edgedoc::Emu((width_mm * 36_000.0 * 96.0 / 420.0) as i64),
        )
        .alt("EdgeDoc")
}

pub fn chart(width_mm: f32) -> edgedoc::Picture {
    edgedoc::Picture::png(CHART.to_vec())
        .size(
            edgedoc::Emu((width_mm * 36_000.0) as i64),
            edgedoc::Emu((width_mm * 36_000.0 * 340.0 / 760.0) as i64),
        )
        .alt("Quarterly revenue by region")
}

pub fn seal(width_mm: f32) -> edgedoc::Picture {
    edgedoc::Picture::png(SEAL.to_vec())
        .size(
            edgedoc::Emu((width_mm * 36_000.0) as i64),
            edgedoc::Emu((width_mm * 36_000.0) as i64),
        )
        .alt("Approved")
}

pub fn bullets(items: &[&str]) -> Vec<Block> {
    items
        .iter()
        .map(|item| {
            p(Paragraph::new()
                .text(item)
                .list(edgedoc::ListKind::Bullet, 0))
        })
        .collect()
}

pub fn numbered(items: &[&str]) -> Vec<Block> {
    items
        .iter()
        .map(|item| {
            p(Paragraph::new()
                .text(item)
                .list(edgedoc::ListKind::Decimal, 0))
        })
        .collect()
}
