//! The invoice documents, built once for the example binary and once for the tests.
//!
//! Run `make examples` to write the files, or `cargo test -p edgedoc --test examples`
//! to check them in memory.

#![allow(dead_code)]

use std::sync::Arc;

use edgedoc::{
    Block, Cell, CsvOptions, Document, EditOptions, ListKind, MergeKind, MissingField, Mm,
    Paragraph, Pt, Record, Records, Row, SortKey, Table, Twips,
};

use crate::common::{
    body, caption, eyebrow, heading, line, logo, muted, p, page_x_of_y, props, rule, seal, split,
    subheading, totals_fields, MIST, NAVY, SLATE,
};

use crate::Rendered;

/// One parsed order line.
pub struct Line {
    pub description: String,
    pub quantity: f64,
    pub unit: f64,
}

impl Line {
    pub fn amount(&self) -> f64 {
        self.quantity * self.unit
    }
}

pub const CSV: &str = "\
InvoiceNumber,InvoiceDate,DueDate,Reference,Customer,Contact,Address,City,PostalCode,Country,Terms,Notes,Lines
INV-2026-0418,2 October 2026,1 November 2026,PO-88213,Northwind Traders,Amelia Hart,41 Harbour Road,Rotterdam,3011 AA,Netherlands,Net 30,Thank you for the repeat order.,Brand audit:1:4800.00|Design system:2:1250.00|Print run:1:940.00
INV-2026-0419,4 October 2026,3 November 2026,PO-88240,Lumen Health,Dr. Ivan Petrov,8 Rue Pasteur,Lyon,69003,France,Net 30,Two sites,Discovery workshop:1:2200.00|Accessibility review:3:680.00
INV-2026-0420,9 October 2026,8 November 2026,PO-88277,Meridian Freight,Joy Adeyemi,1201 West Loop,Houston,77055,United States,Net 30,Rush delivery requested,Fleet dashboard:1:11400.00|Driver training:4:420.00|Support retainer:1:1800.00
INV-2026-0421,12 October 2026,11 November 2026,PO-88301,Kestrel Studios,Marco Silva,5 Kingsway,Manchester,M1 4AB,United Kingdom,Net 15,Retainer renewal,Retainer October:1:3600.00|Extra sessions:2:540.00
";

pub fn parse_lines(text: &str) -> Vec<Line> {
    text.split('|')
        .filter_map(|chunk| {
            let mut parts = chunk.split(':');
            let description = parts.next()?.trim().to_string();
            let quantity = parts.next()?.trim().parse::<f64>().ok()?;
            let unit = parts.next()?.trim().parse::<f64>().ok()?;
            Some(Line {
                description,
                quantity,
                unit,
            })
        })
        .collect()
}

pub fn money(value: f64) -> String {
    format!("{value:.2}")
}

pub fn subtotal(lines: &[Line]) -> f64 {
    lines.iter().map(Line::amount).sum()
}

fn cells(text: &str) -> Cell {
    Cell::paragraph("").blocks(vec![Arc::new(Block::Paragraph(
        Paragraph::new().text(text),
    ))])
}

fn header_row(labels: [&str; 4]) -> Row {
    let mut row = Row::cells(Vec::new()).header();
    for (text, align) in labels.iter().zip(["left", "right", "right", "right"]) {
        let mut shading = Cell::paragraph("").blocks(vec![Arc::new(Block::Paragraph(
            Paragraph::new().align(align).run(
                edgedoc::RunBuilder::new()
                    .text(text)
                    .bold()
                    .size(Pt(8.5))
                    .color("FFFFFF"),
            ),
        ))]);
        shading.shading = Some(NAVY.into());
        row.cells.push(shading);
    }
    row
}

fn money_table() -> Table {
    let mut table = Table::new().widths([Mm(84.0), Mm(18.0), Mm(26.0), Mm(26.0)]);
    table.borders = common_rule();
    table.cell_margins = edgedoc::CellMargins {
        top: Some(Twips(56)),
        bottom: Some(Twips(56)),
        left: Some(Twips(84)),
        right: Some(Twips(84)),
    };
    table
        .rows
        .push(header_row(["Description", "Qty", "Unit", "Amount"]));
    // A marker row. The generator replaces the table body after the merge.
    table.rows.push(Row::cells([
        cells("LINES-PLACEHOLDER"),
        cells(""),
        cells(""),
        cells(""),
    ]));
    table
}

fn common_rule() -> edgedoc::Borders {
    crate::common::rule()
}

/// The template. Two pages: the invoice, then the terms.
pub fn template() -> Result<Document, edgedoc::Error> {
    let letterhead = split(
        vec![p(Paragraph::new().picture(logo(46.0)))],
        vec![
            p(Paragraph::new().align("right").run(
                edgedoc::RunBuilder::new()
                    .text("EdgeDoc Studio")
                    .bold()
                    .color(NAVY)
                    .size(Pt(10.0)),
            )),
            p(Paragraph::new().align("right").run(
                edgedoc::RunBuilder::new()
                    .text("12 Rue de la Paix, 75002 Paris")
                    .color(SLATE)
                    .size(Pt(8.5)),
            )),
            p(Paragraph::new().align("right").run(
                edgedoc::RunBuilder::new()
                    .text("billing@edgedoc.example")
                    .color(SLATE)
                    .size(Pt(8.5)),
            )),
        ],
        Mm(74.0),
        Mm(74.0),
    );
    let footer = vec![
        p(Paragraph::new().borders(rule())),
        p(line(vec![(
            "EdgeDoc Studio · VAT FR 32 404 833 048 · terms and bank details are on page 2"
                .to_string(),
            props(8.0, SLATE, false),
        )])),
        p(page_x_of_y(SLATE)),
    ];

    let mut builder = Document::builder();
    for style in crate::common::house_styles() {
        builder = builder.style(style);
    }
    builder
        .core(
            edgedoc::CoreProps::new()
                .title("Invoice")
                .creator("EdgeDoc Studio")
                .subject("Tax invoice template")
                .keywords("invoice, template, mail merge")
                .category("Finance")
                .content_status("Approved")
                .language("en-GB")
                .modified("2026-11-04T09:00:00Z"),
        )
        .page(
            edgedoc::Page::a4()
                .margins(Mm(16.0))
                .header(Mm(10.0))
                .footer(Mm(12.0))
                .page_numbers(1, "decimal"),
        )
        .header(vec![
            Block::Table(letterhead),
            p(Paragraph::new().borders(rule()).spacing_after(Mm(5.0))),
        ])
        .footer(footer)
        .paragraph(eyebrow("Tax invoice"))
        .paragraph(
            Paragraph::new()
                .style("Title")
                .text("Invoice ")
                .merge_field("InvoiceNumber"),
        )
        .paragraph(
            Paragraph::new()
                .style("Subtitle")
                .text("Issued ")
                .merge_field("InvoiceDate")
                .text("  ·  due ")
                .merge_field("DueDate"),
        )
        .paragraph(
            Paragraph::new()
                .borders(rule())
                .spacing_before(Mm(3.0))
                .spacing_after(Mm(5.0)),
        )
        .block(Block::Table(split(
            vec![
                p(subheading("Billed to")),
                p(Paragraph::new().merge_field("Customer")),
                p(Paragraph::new().merge_field("Contact")),
                p(Paragraph::new().merge_field("Address")),
                p(Paragraph::new().merge_field("City")),
                p(Paragraph::new().merge_field("Country")),
            ],
            vec![
                p(subheading("Reference")),
                p(Paragraph::new().merge_field("Reference")),
                p(Paragraph::new().spacing_after(Mm(2.0))),
                p(subheading("Terms")),
                p(Paragraph::new().merge_field("Terms")),
            ],
            Mm(84.0),
            Mm(64.0),
        )))
        .paragraph(Paragraph::new().spacing_after(Mm(4.0)))
        .block(Block::Table(money_table()))
        .paragraph(Paragraph::new().spacing_after(Mm(1.0)))
        .block(Block::Table(totals_fields(&[
            ("Subtotal", "Subtotal", false),
            ("VAT 21%", "VAT", false),
            ("Total due", "Total", true),
        ])))
        .paragraph(Paragraph::new().page_break_before().spacing_before(Mm(0.0)))
        .paragraph(eyebrow("Terms"))
        .paragraph(heading("How to pay"))
        .paragraph(body(
            "Pay the total by the due date. Quote the invoice number with the transfer.",
        ))
        .block(Block::Paragraph(
            Paragraph::new()
                .text("Transfer to FR76 3000 4000 0300 0000 1234 567.")
                .list(ListKind::Bullet, 0),
        ))
        .block(Block::Paragraph(
            Paragraph::new()
                .text("Send the remittance advice to billing@edgedoc.example.")
                .list(ListKind::Bullet, 0),
        ))
        .block(Block::Paragraph(
            Paragraph::new()
                .text("Late payment carries interest at the statutory rate.")
                .list(ListKind::Bullet, 0),
        ))
        .paragraph(Paragraph::new().spacing_after(Mm(5.0)))
        .paragraph(subheading("Notes"))
        .paragraph(Paragraph::new().merge_field("Notes"))
        .paragraph(Paragraph::new().spacing_after(Mm(10.0)))
        .block(Block::Table(split(
            vec![
                p(subheading("Approved by")),
                p(Paragraph::new().picture(seal(19.0)).spacing_before(Mm(2.0))),
                p(caption("Accounts receivable · EdgeDoc Studio")),
            ],
            vec![
                p(subheading("Questions")),
                p(body(
                    "Reply to billing@edgedoc.example. Quote the invoice number in the subject.",
                )),
                p(muted("This invoice is valid without a signature.")),
            ],
            Mm(70.0),
            Mm(78.0),
        )))
        .build()
}

/// The body index of the line-item table, found by its header text.
pub fn money_table_index(doc: &Document) -> Option<usize> {
    for index in 0..doc.body_len() {
        let Some(table) = doc.table(index) else {
            continue;
        };
        let Some(row) = table.rows.first() else {
            continue;
        };
        let Some(cell) = row.cells.first() else {
            continue;
        };
        let Some(block) = cell.blocks.first() else {
            continue;
        };
        if let Block::Paragraph(para) = block.as_ref() {
            if para.plain_text() == "Description" {
                return Some(index);
            }
        }
    }
    None
}

fn row_cells(line: &Line) -> Vec<Cell> {
    [
        (line.description.clone(), "left"),
        (format!("{}", line.quantity), "right"),
        (money(line.unit), "right"),
        (money(line.amount()), "right"),
    ]
    .into_iter()
    .map(|(text, align)| {
        Cell::paragraph("").blocks(vec![Arc::new(Block::Paragraph(
            Paragraph::new().text(&text).align(align),
        ))])
    })
    .collect()
}

/// Replace the marker row with one row per order line.
pub fn fill_lines(doc: &mut Document, lines: &[Line]) -> Result<(), edgedoc::Error> {
    let index = money_table_index(doc).ok_or_else(|| {
        edgedoc::Error::new("model.bad_path", "The template has no line-item table.")
    })?;
    doc.edit(EditOptions::default(), |edit| {
        edit.body().table_at(index, |table| {
            table.rows.truncate(1);
            for (position, line) in lines.iter().enumerate() {
                let mut row = Row::cells(row_cells(line));
                if position % 2 == 1 {
                    for cell in &mut row.cells {
                        cell.shading = Some(MIST.into());
                    }
                }
                table.rows.push(row);
            }
            Ok(())
        })
    })?;
    Ok(())
}

pub fn records() -> Result<Records, edgedoc::Error> {
    Records::from_csv(CSV.as_bytes(), &CsvOptions::default())
        .map_err(|err| edgedoc::Error::new(err.code, err.message))
}

/// Build the template and every invoice.
pub fn render() -> Result<Vec<Rendered>, String> {
    let template = template().map_err(|err| err.to_string())?;
    let template_bytes = template.write_bytes().map_err(|err| err.to_string())?;
    let mut out = vec![Rendered {
        name: "invoice-template.docx".into(),
        bytes: template_bytes.clone(),
    }];

    let records = records()
        .map_err(|err| err.to_string())?
        .sort(&[SortKey::new("InvoiceNumber")])
        .map_err(|err| err.to_string())?;

    let merged = Document::read_bytes(&template_bytes)
        .map_err(|err| err.to_string())?
        .merge()
        .records(records.clone())
        .kind(MergeKind::Letters)
        .missing(MissingField::Empty)
        .map_value("Subtotal", |record: &Record| {
            money(subtotal(&parse_lines(record.get("Lines").unwrap_or(""))))
        })
        .map_value("VAT", |record: &Record| {
            money(subtotal(&parse_lines(record.get("Lines").unwrap_or(""))) * 0.21)
        })
        .map_value("Total", |record: &Record| {
            money(subtotal(&parse_lines(record.get("Lines").unwrap_or(""))) * 1.21)
        })
        .finish()
        .map_err(|err| err.to_string())?;

    for index in 0..merged.len() {
        let Some(mut doc) = merged.document(index) else {
            continue;
        };
        let lines = parse_lines(&records.cell(index, "Lines").unwrap_or_default());
        fill_lines(&mut doc, &lines).map_err(|err| err.to_string())?;
        let number = records.cell(index, "InvoiceNumber").unwrap_or_default();
        let bytes = doc.write_bytes().map_err(|err| err.to_string())?;
        out.push(Rendered {
            name: format!("invoices/{number}.docx"),
            bytes,
        });
    }
    Ok(out)
}
