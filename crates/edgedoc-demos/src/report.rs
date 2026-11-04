//! A quarterly business review, with one section per region after the merge.

#![allow(dead_code)]

use std::sync::Arc;

use edgedoc::{
    Block, Borders, Cell, CsvOptions, Document, Join, LineBorder, ListKind, MergeKind, Mm,
    Paragraph, Pt, Record, Records, Row, SortKey, Table, Twips,
};

use crate::common::{
    body, caption, chart, eyebrow, heading, logo, muted, p, page_x_of_y, props, rule, seal, split,
    subheading, INK, LINE, SLATE,
};
use crate::Rendered;

pub const CSV: &str = "\
Region,Head,Revenue,Growth,Deals,Pipeline,Priority,Summary
Northern Europe,Elena Fischer,\"4,820,000\",12.4%,214,\"6,100,000\",Renew the three framework contracts,Retention is the strongest in the group. Two renewals move to multi-year terms. The region grew on the back of two framework renewals and a stronger mid-market mix.
Southern Europe,Marco Rossi,\"3,410,000\",8.1%,186,\"4,450,000\",Close the Milan pilot,The pilot converts in the first week of the next quarter. Growth came from the Milan pilot and from a higher average deal size.
North America,Grace Lee,\"7,960,000\",19.2%,402,\"9,300,000\",Hire two enterprise sellers,Two roles are open and pipeline covers the target twice over. The region leads on new logos and on expansion revenue.
Asia Pacific,Wei Chen,\"2,540,000\",26.8%,131,\"3,800,000\",Open the Singapore office,The lease is signed and the team starts with six people. This is the fastest-growing region on the smallest base.
Latin America,Sofia Alvarez,\"1,780,000\",6.3%,94,\"2,050,000\",Stabilise collections,Two large accounts moved to longer payment terms. Growth held while collections improved in the second half of the quarter.
";

/// A strip of three metrics. Each value is a merge field.
fn metric_strip(metrics: [(&str, &str, &str); 3]) -> Table {
    let mut table = Table::new().widths([Mm(46.0), Mm(46.0), Mm(56.0)]);
    table.cell_margins = edgedoc::CellMargins {
        top: Some(Twips(140)),
        bottom: Some(Twips(140)),
        left: Some(Twips(130)),
        right: Some(Twips(130)),
    };
    table.borders = Borders {
        inside_v: Some(LineBorder {
            val: "single".into(),
            size: Some(2),
            color: Some(LINE.into()),
            space: None,
        }),
        ..Borders::default()
    };
    let mut row = Row::cells(Vec::new());
    for (label, field, note) in metrics {
        let mut cell = Cell::paragraph("");
        cell.blocks = vec![
            Arc::new(p(Paragraph::new().run(
                edgedoc::RunBuilder::new()
                    .text(label)
                    .color(SLATE)
                    .size(Pt(8.0)),
            ))),
            Arc::new(p(
                Paragraph::new().merge_field_styled(field, &props(18.0, INK, true))
            )),
            Arc::new(p(Paragraph::new().run(
                edgedoc::RunBuilder::new()
                    .text(note)
                    .color(SLATE)
                    .size(Pt(8.0)),
            ))),
        ];
        row.cells.push(cell);
    }
    table.rows.push(row);
    table
}

/// A two-column fact list where each value is a merge field.
fn fact_fields(rows: [(&str, &str); 4]) -> Table {
    let mut table = Table::new().widths([Mm(40.0), Mm(108.0)]);
    table.cell_margins = edgedoc::CellMargins {
        top: Some(Twips(44)),
        bottom: Some(Twips(44)),
        left: Some(Twips(0)),
        right: Some(Twips(96)),
    };
    table.borders = Borders {
        inside_h: Some(LineBorder {
            val: "single".into(),
            size: Some(2),
            color: Some(crate::common::MIST.into()),
            space: None,
        }),
        ..Borders::default()
    };
    for (label, field) in rows {
        let label_cell = Cell::paragraph("").blocks(vec![Arc::new(p(muted(label)))]);
        let value_cell =
            Cell::paragraph("").blocks(vec![Arc::new(p(Paragraph::new().merge_field(field)))]);
        table.rows.push(Row::cells([label_cell, value_cell]));
    }
    table
}

/// The report template.
pub fn template() -> Result<Document, edgedoc::Error> {
    let mut builder = Document::builder();
    for style in crate::common::house_styles() {
        builder = builder.style(style);
    }
    builder
        .core(
            edgedoc::CoreProps::new()
                .title("Quarterly business review")
                .creator("EdgeDoc Analytics")
                .subject("Q3 2026 regional results")
                .keywords("report, quarterly, revenue, regions")
                .category("Board")
                .content_status("Draft")
                .language("en-GB")
                .modified("2026-11-04T09:00:00Z"),
        )
        .page(edgedoc::Page::a4().margins(Mm(20.0)).header(Mm(12.0)).footer(Mm(14.0)).title_page())
        .header(vec![Block::Table(split(
            vec![p(Paragraph::new().picture(logo(36.0)))],
            vec![p(
                Paragraph::new()
                    .align("right")
                    .run(edgedoc::RunBuilder::new().text("Quarterly business review").color(SLATE).size(Pt(8.5))),
            )],
            Mm(60.0),
            Mm(88.0),
        ))])
        .header_first(vec![p(Paragraph::new())])
        .footer(vec![
            p(Paragraph::new().borders(rule())),
            // This paragraph carries the merge field, so each record gets its own section.
            p(Paragraph::new()
                .run(edgedoc::RunBuilder::new().text("Region: ").color(SLATE).size(Pt(8.0)))
                .merge_field_styled("Region", &props(8.0, INK, true))),
            p(page_x_of_y(SLATE)),
        ])
        // Cover.
        .paragraph(eyebrow("Board paper"))
        .paragraph(Paragraph::new().style("Title").text("Quarterly business review"))
        .paragraph(Paragraph::new().style("Subtitle").text("Q3 2026 · regional results"))
        .paragraph(Paragraph::new().borders(rule()).spacing_before(Mm(4.0)).spacing_after(Mm(8.0)))
        .paragraph(Paragraph::new().picture(logo(76.0)))
        .paragraph(Paragraph::new().spacing_after(Mm(8.0)))
        .paragraph(Paragraph::new().picture(chart(148.0)))
        .paragraph(caption("Figure 1. Revenue by region, indexed to the strongest quarter."))
        .paragraph(Paragraph::new().spacing_after(Mm(6.0)))
        .paragraph(muted("Prepared for the board. Figures are unaudited and in euro."))
        // One section per region starts here.
        .paragraph(Paragraph::new().page_break_before().spacing_before(Mm(0.0)))
        .paragraph(eyebrow("Regional results"))
        .paragraph(Paragraph::new().style("Heading1").merge_field_styled("Region", &props(16.0, INK, true)))
        .paragraph(Paragraph::new().style("Subtitle").text("Led by ").merge_field("Head"))
        .paragraph(Paragraph::new().borders(rule()).spacing_before(Mm(2.0)).spacing_after(Mm(5.0)))
        .block(Block::Table(metric_strip([
            ("Revenue", "Revenue", "Year to date"),
            ("Growth", "Growth", "Against Q2"),
            ("Deals", "Deals", "Closed"),
        ])))
        .paragraph(Paragraph::new().spacing_after(Mm(3.0)))
        .paragraph(subheading("Summary"))
        .paragraph(Paragraph::new().merge_field("Summary"))
        .paragraph(subheading("Numbers"))
        .block(Block::Table(fact_fields([
            ("Revenue", "Revenue"),
            ("Growth", "Growth"),
            ("Deals closed", "Deals"),
            ("Open pipeline", "Pipeline"),
        ])))
        .paragraph(Paragraph::new().spacing_after(Mm(3.0)))
        .paragraph(subheading("Priority for next quarter"))
        .paragraph(Paragraph::new().merge_field("Priority"))
        .paragraph(Paragraph::new().spacing_after(Mm(4.0)))
        .paragraph(subheading("What we will do"))
        .block(Block::Paragraph(Paragraph::new().text("Agree the plan with the regional head.").list(ListKind::Decimal, 0)))
        .block(Block::Paragraph(Paragraph::new().text("Review the pipeline every second week.").list(ListKind::Decimal, 0)))
        .block(Block::Paragraph(Paragraph::new().text("Report the outcome at the next board meeting.").list(ListKind::Decimal, 0)))
        // Closing page.
        .paragraph(Paragraph::new().page_break_before().spacing_before(Mm(0.0)))
        .paragraph(eyebrow("Method"))
        .paragraph(heading("How we measure"))
        .paragraph(body("Revenue is recognised in the month of delivery. Growth compares the quarter with the one before it."))
        .paragraph(body("Pipeline counts every open opportunity that has a named owner and a date."))
        .paragraph(Paragraph::new().spacing_after(Mm(6.0)))
        .block(Block::Table(split(
            vec![
                p(subheading("Prepared by")),
                p(Paragraph::new().picture(seal(18.0)).spacing_before(Mm(2.0))),
                p(caption("EdgeDoc Analytics · 4 November 2026")),
            ],
            vec![
                p(subheading("Distribution")),
                p(muted("Board members and regional heads.")),
                p(muted("Do not forward outside the company.")),
            ],
            Mm(72.0),
            Mm(76.0),
        )))
        .build()
}

/// Build the merged report.
pub fn render() -> Result<Vec<Rendered>, String> {
    let template_bytes = template()
        .map_err(|err| err.to_string())?
        .write_bytes()
        .map_err(|err| err.to_string())?;
    let records = Records::from_csv(CSV.as_bytes(), &CsvOptions::default())
        .map_err(|err| err.to_string())?
        .sort(&[SortKey::new("Region")])
        .map_err(|err| err.to_string())?;
    let merged = Document::read_bytes(&template_bytes)
        .map_err(|err| err.to_string())?
        .merge()
        .records(records)
        .kind(MergeKind::Letters)
        .missing(edgedoc::MissingField::Error)
        .map_value("Revenue", |record: &Record| {
            format!("€{}", record.get("Revenue").unwrap_or("0"))
        })
        .finish()
        .map_err(|err| err.to_string())?
        .join(Join::Auto)
        .map_err(|err| err.to_string())?;
    let bytes = merged.write_bytes().map_err(|err| err.to_string())?;
    Ok(vec![
        Rendered {
            name: "report-template.docx".into(),
            bytes: template_bytes,
        },
        Rendered {
            name: "report.docx".into(),
            bytes,
        },
    ])
}
