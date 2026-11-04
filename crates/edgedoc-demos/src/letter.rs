//! Personalised letters, a filtered recipient list, and a label directory.

#![allow(dead_code)]

use std::sync::Arc;

use edgedoc::{CsvOptions, Document, MergeKind, MissingField, Mm, Op, Paragraph, Records, Rule};

use crate::common::{body, line, logo, muted, p, page_x_of_y, props, rule, seal, SLATE};
use crate::Rendered;

pub const CSV: &str = "\
FirstName,LastName,Company,Address,City,State,PostalCode,Country,Segment,Spend,LastOrder,Note
Amelia,Hart,Northwind Traders,41 Harbour Road,Rotterdam,,3011 AA,Netherlands,Enterprise,48200,2026-09-28,Ask about the multi-year terms
Ivan,Petrov,Lumen Health,8 Rue Pasteur,Lyon,,69003,France,Enterprise,31100,2026-10-01,Two new sites are live
Joy,Adeyemi,Meridian Freight,1201 West Loop,Houston,TX,77055,United States,Enterprise,96400,2026-10-03,Wants a quarterly review
Marco,Silva,Kestrel Studios,5 Kingsway,Manchester,,M1 4AB,United Kingdom,Growth,12400,2026-08-14,Small team with big ambitions
Wei,Chen,Nimbus Data,88 Anson Road,Singapore,,079909,Singapore,Growth,8600,2026-09-11,Evaluate the pilot
Sofia,Alvarez,Andes Retail,Av. Providencia 1200,Santiago,,7500000,Chile,Growth,5300,2026-07-30,Pricing is the blocker
";

fn paragraph_with(picture: edgedoc::Picture) -> Paragraph {
    Paragraph::new().picture(picture)
}

/// The letter template.
pub fn template() -> Result<Document, edgedoc::Error> {
    let mut builder = Document::builder();
    for style in crate::common::house_styles() {
        builder = builder.style(style);
    }
    builder
        .core(
            edgedoc::CoreProps::new()
                .title("Customer letter")
                .creator("EdgeDoc Studio")
                .subject("Account review")
                .language("en-GB")
                .modified("2026-11-04T09:00:00Z"),
        )
        .page(edgedoc::Page::a4().margins(Mm(24.0)).footer(Mm(14.0)))
        .footer(vec![
            p(Paragraph::new().borders(rule())),
            p(line(vec![("EdgeDoc Studio · 12 Rue de la Paix, 75002 Paris".to_string(), props(8.0, SLATE, false))])),
            p(page_x_of_y(SLATE)),
        ])
        .paragraph(paragraph_with(logo(40.0)))
        .paragraph(Paragraph::new().spacing_after(Mm(10.0)))
        .paragraph(Paragraph::new().field_instr(" GREETINGLINE "))
        .paragraph(Paragraph::new().spacing_after(Mm(4.0)))
        .paragraph(
            body("Thank you for your continued partnership. This letter confirms the account review for ")
                .merge_field("Company")
                .text("."),
        )
        .paragraph(Paragraph::new().spacing_after(Mm(2.0)))
        .paragraph(
            body("Your spend for the year to date is €")
                .merge_field("Spend")
                .text(", and your last order was on ")
                .merge_field("LastOrder")
                .text("."),
        )
        // A conditional paragraph: the engine keeps the text only for one segment.
        .paragraph(Paragraph::new().field_instr(
            r#" IF { MERGEFIELD Segment } = "Enterprise" "As an enterprise account, you have a named success manager and a quarterly review." "" "#,
        ))
        .paragraph(Paragraph::new().spacing_after(Mm(4.0)))
        .paragraph(body("The next step is a short call. Reply to this letter and we will agree a date."))
        .paragraph(Paragraph::new().spacing_after(Mm(10.0)))
        .paragraph(Paragraph::new().field_instr(" MERGEFIELD Note "))
        .paragraph(Paragraph::new().spacing_after(Mm(8.0)))
        .paragraph(body("Yours sincerely,"))
        .paragraph(Paragraph::new().spacing_after(Mm(6.0)))
        .paragraph(paragraph_with(seal(16.0)))
        .paragraph(Paragraph::new().merge_field("FirstName").text(" ").merge_field("LastName"))
        .paragraph(muted("Accounts, EdgeDoc Studio"))
        .build()
}

pub fn records() -> Result<Records, edgedoc::Error> {
    Records::from_csv(CSV.as_bytes(), &CsvOptions::default())
        .map_err(|err| edgedoc::Error::new(err.code, err.message))
}

/// Build the template, one letter per enterprise recipient, and a directory of all records.
pub fn render() -> Result<Vec<Rendered>, String> {
    let template_bytes = template()
        .map_err(|err| err.to_string())?
        .write_bytes()
        .map_err(|err| err.to_string())?;
    let all = records().map_err(|err| err.to_string())?;
    let selected = all
        .clone()
        .filter(&[Rule::new("Segment", Op::Equal, "Enterprise")])
        .map_err(|err| err.to_string())?;

    let batch = Document::read_bytes(&template_bytes)
        .map_err(|err| err.to_string())?
        .merge()
        .records(selected.clone())
        .kind(MergeKind::Letters)
        .missing(MissingField::Placeholder)
        .finish()
        .map_err(|err| err.to_string())?;

    let mut out = vec![Rendered {
        name: "letter-template.docx".into(),
        bytes: template_bytes.clone(),
    }];
    for index in 0..batch.len() {
        let Some(doc) = batch.document(index) else {
            continue;
        };
        let name = format!(
            "letters/{}-{}",
            selected.cell(index, "LastName").unwrap_or_default(),
            selected.cell(index, "FirstName").unwrap_or_default()
        );
        let bytes = doc.write_bytes().map_err(|err| err.to_string())?;
        out.push(Rendered {
            name: format!("{name}.docx"),
            bytes,
        });
    }

    let directory = directory_document()
        .map_err(|err| err.to_string())?
        .write_bytes()
        .map_err(|err| err.to_string())?;
    out.push(Rendered {
        name: "directory.docx".into(),
        bytes: directory,
    });
    Ok(out)
}

/// A three-column sheet of address labels, one per record.
///
/// A directory is a layout job: every record becomes a label cell. It is not a merge
/// of the letter, because the letter body would fill every cell.
pub fn directory_document() -> Result<edgedoc::Document, edgedoc::Error> {
    let records = records().map_err(|err| edgedoc::Error::new(err.code, err.message))?;
    let mut table =
        edgedoc::Table::new().widths([edgedoc::Mm(58.0), edgedoc::Mm(58.0), edgedoc::Mm(58.0)]);
    table.cell_margins = edgedoc::CellMargins {
        top: Some(edgedoc::Twips(160)),
        bottom: Some(edgedoc::Twips(160)),
        left: Some(edgedoc::Twips(120)),
        right: Some(edgedoc::Twips(120)),
    };
    table.borders = edgedoc::Borders {
        inside_h: Some(edgedoc::LineBorder {
            val: "dotted".into(),
            size: Some(4),
            color: Some("CBD5E1".into()),
            space: None,
        }),
        inside_v: Some(edgedoc::LineBorder {
            val: "dotted".into(),
            size: Some(4),
            color: Some("CBD5E1".into()),
            space: None,
        }),
        ..edgedoc::Borders::default()
    };
    let mut row = edgedoc::Row::cells(Vec::new());
    for index in 0..records.len() {
        let mut blocks = Vec::new();
        let name = format!(
            "{} {}",
            records.cell(index, "FirstName").unwrap_or_default(),
            records.cell(index, "LastName").unwrap_or_default()
        );
        blocks.push(Arc::new(edgedoc::Block::Paragraph(
            Paragraph::new().run(edgedoc::RunBuilder::new().text(name.trim()).bold()),
        )));
        for field in ["Company", "Address", "City", "Country"] {
            let value = records.cell(index, field).unwrap_or_default();
            if !value.is_empty() {
                blocks.push(Arc::new(edgedoc::Block::Paragraph(
                    Paragraph::new().text(&value),
                )));
            }
        }
        row.cells.push(edgedoc::Cell::paragraph("").blocks(blocks));
        if row.cells.len() == 3 {
            table.rows.push(row);
            row = edgedoc::Row::cells(Vec::new());
        }
    }
    if !row.cells.is_empty() {
        while row.cells.len() < 3 {
            row.cells.push(edgedoc::Cell::paragraph(""));
        }
        table.rows.push(row);
    }

    let mut builder = edgedoc::Document::builder();
    for style in crate::common::house_styles() {
        builder = builder.style(style);
    }
    builder
        .core(
            edgedoc::CoreProps::new()
                .title("Recipient directory")
                .creator("EdgeDoc Studio")
                .language("en-GB"),
        )
        .page(edgedoc::Page::a4().margins(Mm(18.0)))
        .paragraph(Paragraph::new().style("Heading1").text("Directory"))
        .paragraph(
            Paragraph::new().spacing_after(Mm(4.0)).run(
                edgedoc::RunBuilder::new()
                    .text("One label per record, three per row.")
                    .size(edgedoc::Pt(9.0))
                    .color("64748B"),
            ),
        )
        .table(table)
        .build()
}
