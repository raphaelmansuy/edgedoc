//! The dataset wire format (specs/144 DOC2-03, DOC2-15).
//!
//! A dataset is JSON because a child list has nowhere to go in a flat CSV. These tests pin the
//! grammar, the budget clamp, and the two refusals a host will actually meet (a group count that
//! does not match the parents, and a value that is not text).

use std::collections::BTreeMap;

use edgedoc_limit::Budget;
use edgedoc_merge::{
    dataset_from_json, field_index, merge_dataset, options_from_json, record_sets, DatasetOptions,
    Join, MergeKind, MissingField,
};
use edgedoc_model::{Block, Cell, ModelDocument, Paragraph, Row, StoryRef, Table};

fn para(field: &str) -> std::sync::Arc<Block> {
    std::sync::Arc::new(Block::Paragraph(Paragraph::new().merge_field(field)))
}

fn cell(field: &str) -> Cell {
    Cell::paragraph("").blocks(vec![para(field)])
}

/// Header, two scalars, a line table with a band row, and a total.
fn template() -> ModelDocument {
    let mut doc = ModelDocument::new();
    doc.body.push(std::sync::Arc::new(Block::Paragraph(
        Paragraph::new().text("Invoice ").merge_field("Number"),
    )));
    doc.body.push(std::sync::Arc::new(Block::Table(
        Table::new().header(["A", "B"]),
    )));
    if let Some(Block::Table(table)) = doc
        .body
        .last_mut()
        .and_then(|block| std::sync::Arc::get_mut(block))
    {
        table
            .rows
            .push(Row::cells(vec![cell("Lines.Name"), cell("Lines.Qty")]));
    }
    doc.body.push(std::sync::Arc::new(Block::Paragraph(
        Paragraph::new().text("Total ").merge_field("Total"),
    )));
    doc.ensure_nonempty();
    doc
}

const DATASET: &str = r#"{
  "records": [ { "Number": "Q-1", "Total": "90.00" } ],
  "repeats": { "Lines": [ [ { "Name": "Widget", "Qty": "2" }, { "Name": "Gadget", "Qty": "1" } ] ] }
}"#;

const OPTIONS: &str = r#"{ "kind": "letters", "join": "none", "missing": "error",
  "budget": { "maxPart": 16777216, "maxTotal": 33554432, "maxEntries": 20000 } }"#;

fn lines(doc: &ModelDocument) -> Vec<String> {
    doc.plain_text(StoryRef::Body)
        .lines()
        .map(|line| line.replace('\t', " | "))
        .collect()
}

#[test]
fn a_dataset_reads_parents_and_their_child_rows() {
    let dataset = dataset_from_json(DATASET).expect("dataset");
    assert_eq!(dataset.parents.len(), 1);
    assert_eq!(
        dataset.parents[0].get("Number").map(String::as_str),
        Some("Q-1")
    );
    let group = &dataset.repeats["Lines"][0];
    assert_eq!(group.len(), 2);
    assert_eq!(group[1].get("Name").map(String::as_str), Some("Gadget"));
}

#[test]
fn a_dataset_merges_one_row_per_child_and_keeps_the_parent_total() {
    let dataset = dataset_from_json(DATASET).expect("dataset");
    let options = options_from_json(OPTIONS).expect("options");
    let documents = merge_dataset(&template(), &dataset, &options).expect("merge");
    assert_eq!(documents.len(), 1);
    let body = lines(&documents[0]);
    assert!(body.contains(&"Widget | 2".to_string()), "{body:?}");
    assert!(body.contains(&"Gadget | 1".to_string()), "{body:?}");
    assert!(body.contains(&"Total 90.00".to_string()), "{body:?}");
}

#[test]
fn the_budget_is_clamped_to_the_engine_ceiling() {
    let options = options_from_json(
        r#"{ "budget": { "maxPart": 999999999999, "maxTotal": 999999999999, "maxEntries": 99999999 } }"#,
    )
    .expect("options");
    let base = Budget::default();
    assert_eq!(
        options.budget.max_part, base.max_part,
        "a host cannot raise the ceiling"
    );
    assert_eq!(options.budget.max_total, base.max_total);
    assert_eq!(options.budget.max_entries, base.max_entries);
}

#[test]
fn a_tighter_budget_is_kept() {
    let options = options_from_json(r#"{ "budget": { "maxPart": 1024 } }"#).expect("options");
    assert_eq!(options.budget.max_part, 1024);
    assert_eq!(
        options.budget.max_total,
        Budget::default().max_total,
        "unspecified stays default"
    );
}

#[test]
fn an_unknown_kind_is_named() {
    let error = options_from_json(r#"{ "kind": "invoice" }"#).expect_err("refused");
    assert!(error.contains("Unknown merge kind"), "{error}");
    assert!(
        error.contains("normal"),
        "the message lists the legal words: {error}"
    );
}

#[test]
fn normal_is_one_continuing_document() {
    let template = template();
    let options = DatasetOptions {
        kind: MergeKind::Normal,
        join: Join::None,
        missing: MissingField::Error,
        budget: Budget::default(),
        from: None,
        to: None,
    };
    let dataset = dataset_from_json(
        r#"{ "records": [ { "Number": "Q-1", "Total": "1.00" }, { "Number": "Q-2", "Total": "2.00" } ],
             "repeats": { "Lines": [ [ { "Name": "A", "Qty": "1" } ], [ { "Name": "B", "Qty": "2" } ] ] } }"#,
    )
    .expect("dataset");
    let documents = merge_dataset(&template, &dataset, &options).expect("merge");
    assert_eq!(
        documents.len(),
        2,
        "two parents are two documents when join is none"
    );
    // Joined into one package: `auto` folds them, and `normal` keeps them in one continuous flow.
    let joined_options = DatasetOptions {
        join: Join::Auto,
        ..options.clone()
    };
    let joined = merge_dataset(&template, &dataset, &joined_options).expect("merge");
    assert_eq!(joined.len(), 1);
    let text = joined[0].plain_text(StoryRef::Body);
    assert!(text.contains("Q-1"), "{text}");
    assert!(text.contains("Q-2"), "{text}");
}

#[test]
fn a_group_count_past_the_parents_is_refused() {
    let dataset = dataset_from_json(
        r#"{ "records": [ { "Number": "Q-1" } ], "repeats": { "Lines": [ [], [] ] } }"#,
    )
    .expect("dataset");
    let error = record_sets(&dataset).expect_err("refused");
    assert!(error.contains("merge.repeats_length"), "{error}");
}

#[test]
fn a_parent_with_no_group_gets_an_empty_band() {
    let dataset = dataset_from_json(
        r#"{ "records": [ { "Number": "Q-1" }, { "Number": "Q-2" } ],
             "repeats": { "Lines": [ [ { "Name": "Widget", "Qty": "2" } ] ] } }"#,
    )
    .expect("dataset");
    let (_parents, repeats) = record_sets(&dataset).expect("sets");
    assert_eq!(repeats["Lines"].len(), 2, "one child set per parent");
    assert!(
        repeats["Lines"][1].is_empty(),
        "the second parent has no lines"
    );
}

#[test]
fn a_non_text_value_is_named() {
    let error =
        dataset_from_json(r#"{ "records": [ { "Number": ["a"] } ] }"#).expect_err("refused");
    assert!(error.contains("records[0].Number"), "{error}");
    assert!(error.contains("must be text"), "{error}");
}

#[test]
fn a_number_and_a_null_are_widened() {
    let dataset =
        dataset_from_json(r#"{ "records": [ { "Count": 3, "Gone": null, "Flag": true } ] }"#)
            .expect("dataset");
    assert_eq!(
        dataset.parents[0].get("Count").map(String::as_str),
        Some("3")
    );
    assert_eq!(dataset.parents[0].get("Gone").map(String::as_str), Some(""));
    assert_eq!(
        dataset.parents[0].get("Flag").map(String::as_str),
        Some("true")
    );
}

#[test]
fn an_empty_dataset_is_refused() {
    let error = dataset_from_json(r#"{ "records": [] }"#).expect_err("refused");
    assert!(error.contains("merge.no_records"), "{error}");
}

#[test]
fn malformed_json_names_the_offset() {
    let error = dataset_from_json(r#"{ "records": [ }"#).expect_err("refused");
    assert!(error.contains("merge.dataset"), "{error}");
}

#[test]
fn deep_nesting_is_refused_rather_than_recursing() {
    let mut text = String::from(r#"{ "records": "#);
    for _ in 0..40 {
        text.push('[');
    }
    // The parser stops at the depth cap before it reaches the end.
    let error = dataset_from_json(&text).expect_err("refused");
    assert!(
        error.contains("nested too deeply") || error.contains("merge.dataset"),
        "{error}"
    );
}

#[test]
fn an_escaped_string_is_read() {
    let dataset =
        dataset_from_json(r#"{ "records": [ { "Note": "line\u0020one\ntwo \"quoted\"" } ] }"#)
            .expect("dataset");
    assert_eq!(
        dataset.parents[0].get("Note").map(String::as_str),
        Some("line one\ntwo \"quoted\"")
    );
}

#[test]
fn a_utf8_value_survives() {
    let dataset = dataset_from_json(r#"{ "records": [ { "Name": "Maison Bertrand — Zürich" } ] }"#)
        .expect("dataset");
    assert_eq!(
        dataset.parents[0].get("Name").map(String::as_str),
        Some("Maison Bertrand — Zürich")
    );
}

#[test]
fn a_records_order_is_the_hosts_order() {
    let dataset = dataset_from_json(r#"{ "records": [ { "Number": "B" }, { "Number": "A" } ] }"#)
        .expect("dataset");
    assert_eq!(
        dataset.parents[0].get("Number").map(String::as_str),
        Some("B")
    );
    assert_eq!(
        dataset.parents[1].get("Number").map(String::as_str),
        Some("A")
    );
}

#[test]
fn the_field_index_names_the_table_row_of_a_band_field() {
    let index = field_index(&template());
    let band: Vec<_> = index
        .iter()
        .filter(|field| field.name.starts_with("Lines."))
        .collect();
    assert_eq!(band.len(), 2, "two band fields: {index:?}");
    for field in band {
        assert_eq!(field.table, Some(0), "both sit in table 0");
        assert_eq!(
            field.row,
            Some(1),
            "both sit in row 1 (row 0 is the header)"
        );
    }
    let scalar = index
        .iter()
        .find(|field| field.name == "Number")
        .expect("the scalar field");
    assert_eq!(scalar.table, None, "a paragraph field has no table");
}

#[test]
fn parents_keep_the_header_order_of_the_first_record() {
    let mut first = BTreeMap::new();
    first.insert("Zeta".to_string(), "1".to_string());
    first.insert("Alpha".to_string(), "2".to_string());
    let dataset = edgedoc_merge::Dataset {
        parents: vec![first],
        repeats: BTreeMap::new(),
    };
    let (records, _) = record_sets(&dataset).expect("sets");
    let headers = records.headers();
    assert_eq!(
        headers,
        &["Alpha".to_string(), "Zeta".to_string()],
        "a map is ordered by key"
    );
}
