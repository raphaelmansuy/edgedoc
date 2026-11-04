//! Cached spreadsheet values. Formulas are not evaluated.

use edgedoc_limit::Budget;
use edgedoc_merge::Records;
use edgedoc_opc::xml::{self, El};
use edgedoc_opc::{read_zip, write_zip};

use crate::error::Error;

pub fn records_from_xlsx(bytes: &[u8], budget: &Budget) -> Result<Records, Error> {
    let package = read_zip(bytes, budget)?;
    let sheet = package.part("xl/worksheets/sheet1.xml").ok_or_else(|| {
        Error::new(
            "package.missing_part",
            "The workbook has no xl/worksheets/sheet1.xml part.",
        )
    })?;
    budget
        .part(sheet.bytes.len() as u64, "sheet")
        .map_err(Error::from)?;
    if sheet.bytes.len() as u64 > budget.max_sheet_part {
        return Err(Error::new(
            "package.limit",
            "The worksheet part is larger than the sheet limit.",
        ));
    }
    let shared = shared_strings(
        package
            .part("xl/sharedStrings.xml")
            .map(|part| part.bytes.as_slice())
            .unwrap_or(&[]),
        budget,
    )?;
    let root = xml::parse(&sheet.bytes, budget)?;
    let mut rows = Vec::new();
    let mut cells = 0usize;
    let mut text_bytes = 0u64;
    for row_el in root
        .child_els()
        .filter(|el| el.name == "sheetData")
        .flat_map(|data| data.child_els())
        .filter(|el| el.name == "row")
    {
        if rows.len() >= budget.max_sheet_rows {
            return Err(Error::new(
                "package.limit",
                "The worksheet has more rows than the sheet limit.",
            ));
        }
        let mut row = Vec::new();
        for cell in row_el.child_els().filter(|el| el.name == "c") {
            if row.len() >= budget.max_sheet_cols {
                return Err(Error::new(
                    "package.limit",
                    "The worksheet has more columns than the sheet limit.",
                ));
            }
            cells += 1;
            if cells > budget.max_sheet_cells {
                return Err(Error::new(
                    "package.limit",
                    "The worksheet has more cells than the sheet limit.",
                ));
            }
            let value = cell_value(cell, &shared);
            if value.chars().count() > budget.max_cell_chars {
                return Err(Error::new(
                    "package.limit",
                    "A cell is longer than the cell limit.",
                ));
            }
            text_bytes = text_bytes.saturating_add(value.len() as u64);
            if text_bytes > budget.max_sheet_text {
                return Err(Error::new(
                    "package.limit",
                    "The worksheet text is larger than the sheet limit.",
                ));
            }
            row.push(value);
        }
        rows.push(row);
    }
    let mut table = rows.into_iter();
    let headers = table.next().unwrap_or_default();
    let body: Vec<Vec<String>> = table.collect();
    Ok(Records::from_rows(headers, body)?)
}

fn cell_value(cell: &El, shared: &[String]) -> String {
    let cached = cell.child_els().find(|el| el.name == "v").map(deep_text);
    let inline = cell.child_els().find(|el| el.name == "is").map(deep_text);
    let raw = cached.or(inline).unwrap_or_default();
    if cell.attr("t") == Some("s") {
        return raw
            .parse::<usize>()
            .ok()
            .and_then(|index| shared.get(index))
            .cloned()
            .unwrap_or_default();
    }
    raw
}

fn shared_strings(bytes: &[u8], budget: &Budget) -> Result<Vec<String>, Error> {
    if bytes.is_empty() {
        return Ok(Vec::new());
    }
    let root = xml::parse(bytes, budget)?;
    Ok(root
        .child_els()
        .filter(|el| el.name == "si")
        .map(deep_text)
        .collect())
}

fn deep_text(el: &El) -> String {
    let mut out = el.text();
    for child in el.child_els() {
        out.push_str(&deep_text(child));
    }
    out
}

pub fn xlsx_with_cached_formula() -> Result<Vec<u8>, Error> {
    let sheet = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData>
<row r="1"><c r="A1" t="inlineStr"><is><t>Name</t></is></c><c r="B1" t="inlineStr"><is><t>Amount</t></is></c></row>
<row r="2"><c r="A2" t="inlineStr"><is><t>Ada</t></is></c><c r="B2"><f>1+1</f><v>2</v></c></row>
</sheetData></worksheet>"#;
    Ok(write_zip(&[
        ("[Content_Types].xml".into(), br#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/></Types>"#.to_vec()),
        ("xl/worksheets/sheet1.xml".into(), sheet.to_vec()),
    ])?)
}
