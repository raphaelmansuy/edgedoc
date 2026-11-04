//! A price list that merges a table row by row.
//!
//! The merge engine replaces a field in any paragraph, including a paragraph inside a
//! table cell. This demo puts a merge field in every cell of a table, so the whole
//! table comes from the record list: one catalogue per region, one row per product.

#![allow(dead_code)]

use std::sync::Arc;

use edgedoc::{
    Block, Borders, Cell, CsvOptions, Document, Join, LineBorder, MergeKind, MissingField, Mm,
    Paragraph, Pt, Records, Row, Table, Twips,
};

use crate::common::{
    caption, eyebrow, logo, muted, p, page_x_of_y, props, rule, split, subheading, INK, MIST, NAVY,
    SLATE,
};
use crate::Rendered;

/// The catalogue. Every column of every row is a merge field.
pub const CSV: &str = "\
Region,Currency,Product,Code,Pack,Unit,Price,Trend
Northern Europe,EUR,Design system,DS-100,1,1,1250.00,up
Northern Europe,EUR,Brand audit,BA-210,1,1,4800.00,up
Northern Europe,EUR,Print run,PR-330,1,500,940.00,flat
Southern Europe,EUR,Discovery workshop,DW-010,1,1,2200.00,up
Southern Europe,EUR,Accessibility review,AR-140,1,1,680.00,up
North America,USD,Fleet dashboard,FD-500,1,1,11400.00,up
North America,USD,Driver training,DT-120,1,4,420.00,flat
North America,USD,Support retainer,SR-900,1,1,1800.00,down
Asia Pacific,SGD,Design system,DS-100,1,1,1800.00,up
Asia Pacific,SGD,Pilot licence,PL-050,1,1,900.00,up
";

/// A cell whose single paragraph carries a merge field.
fn field_cell(field: &str, align: &str, bold: bool) -> Cell {
    let mut text = Paragraph::new().align(align);
    let style = if bold {
        props(10.0, INK, true)
    } else {
        props(10.0, INK, false)
    };
    text = text.merge_field_styled(field, &style);
    Cell::paragraph("").blocks(vec![Arc::new(Block::Paragraph(text))])
}

/// The header row of the table, in white on navy.
fn header_row(labels: [&str; 6]) -> Row {
    let mut row = Row::cells(Vec::new()).header();
    for (index, label) in labels.iter().enumerate() {
        let align = if index >= 4 { "right" } else { "left" };
        let mut cell = Cell::paragraph("");
        cell.shading = Some(NAVY.into());
        cell.blocks = vec![Arc::new(Block::Paragraph(
            Paragraph::new().align(align).run(
                edgedoc::RunBuilder::new()
                    .text(label)
                    .bold()
                    .size(Pt(8.5))
                    .color("FFFFFF"),
            ),
        ))];
        row.cells.push(cell);
    }
    row
}

/// The catalogue template. The table has one row of fields, repeated by the merge.
fn template() -> Result<Document, edgedoc::Error> {
    let mut table =
        Table::new().widths([Mm(46.0), Mm(20.0), Mm(20.0), Mm(18.0), Mm(18.0), Mm(22.0)]);
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
    table.cell_margins = edgedoc::CellMargins {
        top: Some(Twips(52)),
        bottom: Some(Twips(52)),
        left: Some(Twips(80)),
        right: Some(Twips(80)),
    };
    table.rows.push(header_row([
        "Product", "Code", "Pack", "Unit", "Price", "Trend",
    ]));
    table.rows.push(Row::cells([
        field_cell("Product", "left", false),
        field_cell("Code", "left", false),
        field_cell("Pack", "right", false),
        field_cell("Unit", "right", false),
        field_cell("Price", "right", true),
        field_cell("Trend", "right", false),
    ]));

    Document::builder()
        .core(
            edgedoc::CoreProps::new()
                .title("Price list")
                .creator("EdgeDoc Studio")
                .subject("Regional catalogue template")
                .keywords("price list, table, merge")
                .category("Sales")
                .language("en-GB")
                .modified("2026-11-04T09:00:00Z"),
        )
        .page(edgedoc::Page::a4().margins(Mm(18.0)).footer(Mm(12.0)))
        .footer(vec![
            p(Paragraph::new().borders(rule())),
            p(Paragraph::new()
                .run(
                    edgedoc::RunBuilder::new()
                        .text("Price list · ")
                        .color(SLATE)
                        .size(Pt(8.0)),
                )
                .merge_field_styled("Region", &props(8.0, INK, true))),
            p(page_x_of_y(SLATE)),
        ])
        .header(vec![Block::Table(split(
            vec![p(Paragraph::new().picture(logo(40.0)))],
            vec![p(Paragraph::new().align("right").run(
                edgedoc::RunBuilder::new()
                    .text("Regional price list")
                    .color(SLATE)
                    .size(Pt(8.5)),
            ))],
            Mm(64.0),
            Mm(84.0),
        ))])
        .paragraph(eyebrow("Catalogue"))
        .paragraph(
            Paragraph::new()
                .style("Heading1")
                .merge_field_styled("Region", &props(16.0, INK, true)),
        )
        .paragraph(
            Paragraph::new()
                .style("Subtitle")
                .text("All prices in ")
                .merge_field("Currency"),
        )
        .paragraph(
            Paragraph::new()
                .borders(rule())
                .spacing_before(Mm(2.0))
                .spacing_after(Mm(4.0)),
        )
        .block(Block::Table(table))
        .paragraph(Paragraph::new().spacing_after(Mm(3.0)))
        .paragraph(caption(
            "Prices exclude VAT. The trend column compares this quarter with the last.",
        ))
        .paragraph(Paragraph::new().spacing_after(Mm(4.0)))
        .paragraph(subheading("How to order"))
        .block(Block::Paragraph(
            Paragraph::new()
                .text("Quote the product code and the pack size.")
                .list(edgedoc::ListKind::Decimal, 0),
        ))
        .block(Block::Paragraph(
            Paragraph::new()
                .text("Send the order to sales@edgedoc.example.")
                .list(edgedoc::ListKind::Decimal, 0),
        ))
        .block(Block::Paragraph(
            Paragraph::new()
                .text("We confirm the price and the delivery date in one working day.")
                .list(edgedoc::ListKind::Decimal, 0),
        ))
        .paragraph(Paragraph::new().spacing_after(Mm(4.0)))
        .paragraph(muted(
            "The catalogue follows the current exchange rate. A quotation is valid for 30 days.",
        ))
        .build()
}

/// One catalogue per region, with every row of that region in the table.
pub fn render_by_region() -> Result<Vec<Rendered>, String> {
    let template_bytes = template()
        .map_err(|err| err.to_string())?
        .write_bytes()
        .map_err(|err| err.to_string())?;
    let all =
        Records::from_csv(CSV.as_bytes(), &CsvOptions::default()).map_err(|err| err.to_string())?;
    let regions: Vec<String> = {
        let mut names: Vec<String> = Vec::new();
        for index in 0..all.len() {
            let region = all.cell(index, "Region").unwrap_or_default();
            if !names.iter().any(|have| have.eq_ignore_ascii_case(&region)) {
                names.push(region);
            }
        }
        names
    };

    let mut out = vec![Rendered {
        name: "price-list-template.docx".into(),
        bytes: template_bytes.clone(),
    }];
    for region in regions {
        let rows = all
            .clone()
            .filter(&[edgedoc::Rule::new("Region", edgedoc::Op::Equal, &region)])
            .map_err(|err| err.to_string())?;
        let merged = Document::read_bytes(&template_bytes)
            .map_err(|err| err.to_string())?
            .merge()
            .records(rows)
            .kind(MergeKind::Directory)
            .missing(MissingField::Error)
            .map_value("Price", |record: &edgedoc::Record| {
                let symbol = match record.get("Currency").unwrap_or("") {
                    "USD" => "$",
                    "SGD" => "S$",
                    _ => "€",
                };
                format!("{symbol}{}", record.get("Price").unwrap_or("0"))
            })
            .map_value("Trend", |record: &edgedoc::Record| {
                match record.get("Trend").unwrap_or("") {
                    "up" => "\u{25B2}".to_string(),
                    "down" => "\u{25BC}".to_string(),
                    _ => "\u{25A0}".to_string(),
                }
            })
            .finish()
            .map_err(|err| err.to_string())?
            .join(Join::Auto)
            .map_err(|err| err.to_string())?;
        let slug = region.replace(' ', "-").to_ascii_lowercase();
        let bytes = merged.write_bytes().map_err(|err| err.to_string())?;
        out.push(Rendered {
            name: format!("price-lists/{slug}.docx"),
            bytes,
        });
    }
    Ok(out)
}
