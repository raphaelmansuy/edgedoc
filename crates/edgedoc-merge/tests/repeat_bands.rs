//! Repeat bands (specs/144 DOC2-01, DOC2-02, DOC2-03, DOC2-34).
//!
//! A band is a table row whose merge fields share one `Alias.` prefix. It is cloned once per child
//! of the **current parent**, and the parent cursor does not move — which is the difference between
//! an invoice and one invoice per line (EdgeBase spec `010` probes `CS-05`/`CS-06`).
//!
//! These tests build the model document directly (no package, no zip) so a failure names the merge
//! law rather than a parser.

use std::sync::Arc;

use edgedoc_limit::Budget;
use edgedoc_merge::{finish, MergeKind, MergeRequest, MissingField, Record, Records, Repeats};
use edgedoc_model::{Block, Cell, ModelDocument, Paragraph, Row, StoryRef, Table};

fn para(field: Option<&str>) -> Arc<Block> {
    let mut paragraph = Paragraph::new();
    if let Some(name) = field {
        paragraph = paragraph.merge_field(name);
    }
    Arc::new(Block::Paragraph(paragraph))
}

fn text_para(text: &str, field: &str) -> Arc<Block> {
    Arc::new(Block::Paragraph(
        Paragraph::new().text(text).merge_field(field),
    ))
}

fn cell(field: &str) -> Cell {
    Cell::paragraph("").blocks(vec![para(Some(field))])
}

/// The template the EdgeBase blueprint ships: header, scalars, a line table with a band row, total.
fn template() -> ModelDocument {
    let mut doc = ModelDocument::new();
    doc.body.push(text_para("Invoice ", "Number"));
    doc.body.push(text_para("Customer ", "Customer"));
    let band_row = Row::cells(vec![
        cell("Lines.Name"),
        cell("Lines.Qty"),
        cell("Lines.Amount"),
    ]);
    doc.body.push(Arc::new(Block::Table(
        Table::new().header(["Article", "Qty", "Amount"]),
    )));
    if let Some(Block::Table(table)) = doc.body.last_mut().and_then(|block| Arc::get_mut(block)) {
        table.rows.push(band_row);
    }
    doc.body.push(text_para("Total ", "Total"));
    doc.ensure_nonempty();
    doc
}

#[allow(clippy::expect_used)] // Static test fixtures fail immediately when malformed.
fn parents(rows: &[(&str, &str, &str)]) -> Records {
    let headers = vec!["Number".into(), "Customer".into(), "Total".into()];
    let cells = rows
        .iter()
        .map(|(number, customer, total)| {
            vec![number.to_string(), customer.to_string(), total.to_string()]
        })
        .collect();
    Records::from_rows(headers, cells).expect("parents")
}

#[allow(clippy::expect_used)] // Static test fixtures fail immediately when malformed.
fn children(sets: &[&[(&str, &str, &str)]]) -> Vec<Records> {
    sets.iter()
        .map(|set| {
            let headers = vec!["Name".into(), "Qty".into(), "Amount".into()];
            let cells = set
                .iter()
                .map(|(name, qty, amount)| {
                    vec![name.to_string(), qty.to_string(), amount.to_string()]
                })
                .collect();
            Records::from_rows(headers, cells).expect("children")
        })
        .collect()
}

fn merge(
    template: &ModelDocument,
    parents: Records,
    repeats: Repeats,
    kind: MergeKind,
    missing: MissingField,
) -> Result<Vec<ModelDocument>, String> {
    let mut request = MergeRequest {
        template,
        records: parents,
        kind,
        missing,
        from: 0,
        to: None,
        maps: Vec::new(),
        budget: Budget::default(),
        today: "2026-11-10".to_string(),
        repeats,
        band: None,
        band_parent: None,
        band_row: 0,
    };
    finish(&mut request)
        .map(|batch| batch.documents)
        .map_err(|error| error.code.to_string())
}

/// The body as one line per paragraph and one line per table row, cells separated by `" | "`.
fn lines(doc: &ModelDocument) -> Vec<String> {
    doc.plain_text(StoryRef::Body)
        .lines()
        .map(|line| line.replace('\t', " | "))
        .collect()
}

fn band(sets: &[&[(&str, &str, &str)]]) -> Repeats {
    let mut repeats = Repeats::new();
    repeats.insert("Lines".into(), children(sets));
    repeats
}

#[test]
fn band_repeats_one_row_per_child() {
    let documents = merge(
        &template(),
        parents(&[("Q-1", "Ada", "120.00")]),
        band(&[&[
            ("Widget", "2", "40.00"),
            ("Gadget", "1", "50.00"),
            ("Bolt", "3", "10.00"),
        ]]),
        MergeKind::Letters,
        MissingField::Error,
    )
    .expect("merge");
    assert_eq!(documents.len(), 1, "one parent makes one document");
    let body = lines(&documents[0]);
    for expected in [
        "Widget | 2 | 40.00",
        "Gadget | 1 | 50.00",
        "Bolt | 3 | 10.00",
    ] {
        assert!(
            body.contains(&expected.to_string()),
            "missing {expected}: {body:?}"
        );
    }
    assert_eq!(
        body.iter()
            .filter(|line| line.starts_with("Widget")
                || line.starts_with("Gadget")
                || line.starts_with("Bolt"))
            .count(),
        3,
        "exactly three line rows: {body:?}"
    );
}

#[test]
fn band_keeps_parent_scalar() {
    // This is the probe that fails with a `directory`: the parent cursor walks off the record list,
    // so `Total` prints blank. A band freezes the parent.
    let documents = merge(
        &template(),
        parents(&[("Q-1", "Ada", "90.00")]),
        band(&[&[("Widget", "2", "40.00"), ("Gadget", "1", "50.00")]]),
        MergeKind::Letters,
        MissingField::Error,
    )
    .expect("merge");
    let body = lines(&documents[0]);
    assert!(body.contains(&"Invoice Q-1".to_string()), "{body:?}");
    assert!(body.contains(&"Customer Ada".to_string()), "{body:?}");
    assert!(
        body.contains(&"Total 90.00".to_string()),
        "the parent total must be present with two children: {body:?}"
    );
}

#[test]
fn band_zero_children_removes_the_row_and_keeps_the_header() {
    let documents = merge(
        &template(),
        parents(&[("Q-1", "Ada", "0.00")]),
        band(&[&[]]),
        MergeKind::Letters,
        MissingField::Error,
    )
    .expect("merge");
    let body = lines(&documents[0]);
    assert!(
        body.contains(&"Article | Qty | Amount".to_string()),
        "header kept: {body:?}"
    );
    assert!(
        !body.iter().any(|line| line.contains("«Lines.")),
        "no leftover placeholder row: {body:?}"
    );
    assert!(body.contains(&"Total 0.00".to_string()), "{body:?}");
}

#[test]
fn band_absent_for_one_parent_keeps_the_other_parents_lines() {
    let mut repeats = Repeats::new();
    let mut by_parent = children(&[&[("Widget", "2", "40.00")]]);
    by_parent.push(
        Records::from_rows(vec!["Name".into(), "Qty".into(), "Amount".into()], vec![])
            .expect("empty"),
    );
    repeats.insert("Lines".into(), by_parent);
    let documents = merge(
        &template(),
        parents(&[("Q-1", "Ada", "40.00"), ("Q-2", "Grace", "0.00")]),
        repeats,
        MergeKind::Letters,
        MissingField::Error,
    )
    .expect("merge");
    assert_eq!(documents.len(), 2);
    assert!(lines(&documents[0]).contains(&"Widget | 2 | 40.00".to_string()));
    assert!(
        !lines(&documents[1])
            .iter()
            .any(|line| line.contains("Widget")),
        "the second parent has no lines: {:?}",
        lines(&documents[1])
    );
}

#[test]
fn band_two_parents_make_two_documents_each_with_its_own_lines() {
    let documents = merge(
        &template(),
        parents(&[("Q-1", "Ada", "90.00"), ("Q-2", "Grace", "50.00")]),
        band(&[
            &[("Widget", "2", "40.00"), ("Gadget", "1", "50.00")],
            &[("Bolt", "5", "10.00")],
        ]),
        MergeKind::Letters,
        MissingField::Error,
    )
    .expect("merge");
    assert_eq!(documents.len(), 2);
    let first = lines(&documents[0]);
    let second = lines(&documents[1]);
    assert!(
        first.contains(&"Widget | 2 | 40.00".to_string()),
        "{first:?}"
    );
    assert!(
        !first.contains(&"Bolt | 5 | 10.00".to_string()),
        "the first invoice must not borrow the second's line: {first:?}"
    );
    assert!(
        second.contains(&"Bolt | 5 | 10.00".to_string()),
        "{second:?}"
    );
    assert!(second.contains(&"Total 50.00".to_string()), "{second:?}");
}

#[test]
fn band_field_outside_a_table_reads_the_first_child() {
    let mut doc = ModelDocument::new();
    doc.body.push(text_para("First line: ", "Lines.Name"));
    doc.ensure_nonempty();
    let documents = merge(
        &doc,
        parents(&[("Q-1", "Ada", "90.00")]),
        band(&[&[("Widget", "2", "40.00")]]),
        MergeKind::Letters,
        MissingField::Error,
    )
    .expect("merge");
    assert!(lines(&documents[0]).contains(&"First line: Widget".to_string()));
}

#[test]
fn band_mixed_prefix_is_refused() {
    let mut doc = ModelDocument::new();
    doc.body
        .push(Arc::new(Block::Table(Table::new().header(["A", "B"]))));
    if let Some(Block::Table(table)) = doc.body.last_mut().and_then(|block| Arc::get_mut(block)) {
        table
            .rows
            .push(Row::cells(vec![cell("Lines.Name"), cell("Other.Name")]));
    }
    doc.ensure_nonempty();
    let mut repeats = band(&[&[("Widget", "2", "40.00")]]);
    repeats.insert("Other".into(), children(&[&[("Bolt", "1", "1.00")]]));
    let error = merge(
        &doc,
        parents(&[("Q-1", "Ada", "90.00")]),
        repeats,
        MergeKind::Letters,
        MissingField::Error,
    );
    assert_eq!(error, Err("merge.mixed_band".into()));
}

#[test]
fn band_missing_value_names_the_field() {
    // A child set with no `Amount` column at all, under the error policy.
    let mut repeats = Repeats::new();
    repeats.insert(
        "Lines".into(),
        vec![Records::from_rows(
            vec!["Name".into(), "Qty".into()],
            vec![vec!["Widget".to_string(), "2".to_string()]],
        )
        .expect("children")],
    );
    let error = merge(
        &template(),
        parents(&[("Q-1", "Ada", "90.00")]),
        repeats,
        MergeKind::Letters,
        MissingField::Error,
    );
    assert_eq!(error, Err("merge.unknown_field".into()));
}

#[test]
fn band_placeholder_policy_keeps_the_name() {
    let mut repeats = Repeats::new();
    repeats.insert(
        "Lines".into(),
        vec![Records::from_rows(
            vec!["Name".into(), "Qty".into()],
            vec![vec!["Widget".to_string(), "2".to_string()]],
        )
        .expect("children")],
    );
    let documents = merge(
        &template(),
        parents(&[("Q-1", "Ada", "90.00")]),
        repeats,
        MergeKind::Letters,
        MissingField::Placeholder,
    )
    .expect("merge");
    let body = lines(&documents[0]);
    assert!(
        body.iter().any(|line| line.contains("«Lines.Amount»")),
        "{body:?}"
    );
}

#[test]
fn an_undeclared_alias_reads_as_a_missing_field() {
    let mut doc = ModelDocument::new();
    doc.body.push(text_para("", "Ghost.Name"));
    doc.ensure_nonempty();
    let error = merge(
        &doc,
        parents(&[("Q-1", "Ada", "90.00")]),
        Repeats::new(),
        MergeKind::Letters,
        MissingField::Error,
    );
    assert_eq!(error, Err("merge.unknown_field".into()));
}

#[test]
fn a_catalogue_merge_still_repeats_the_first_field_row_per_record() {
    // The directory law survives the band change: one row per record, not per child.
    let mut doc = ModelDocument::new();
    doc.body.push(Arc::new(Block::Table(
        Table::new().header(["Customer", "Number"]),
    )));
    if let Some(Block::Table(table)) = doc.body.last_mut().and_then(|block| Arc::get_mut(block)) {
        table
            .rows
            .push(Row::cells(vec![cell("Customer"), cell("Number")]));
    }
    doc.ensure_nonempty();
    let documents = merge(
        &doc,
        parents(&[("Q-1", "Ada", "90.00"), ("Q-2", "Grace", "50.00")]),
        Repeats::new(),
        MergeKind::Directory,
        MissingField::Error,
    )
    .expect("merge");
    assert_eq!(documents.len(), 1, "a directory is one document");
    let body = lines(&documents[0]);
    assert!(body.contains(&"Ada | Q-1".to_string()), "{body:?}");
    assert!(body.contains(&"Grace | Q-2".to_string()), "{body:?}");
}

#[test]
fn a_record_is_a_map_of_named_text() {
    let records = parents(&[("Q-1", "Ada", "90.00")]);
    let record: Record = records.record(0);
    assert_eq!(record.get("Number"), Some("Q-1"));
    assert_eq!(
        record.get("customer"),
        Some("Ada"),
        "a lookup is case-insensitive"
    );
}

#[test]
fn a_decimal_string_is_inserted_verbatim() {
    let mut doc = ModelDocument::new();
    doc.body.push(text_para("Total ", "Total"));
    doc.ensure_nonempty();
    let documents = merge(
        &doc,
        parents(&[("Q-1", "Ada", "12400.005")]),
        Repeats::new(),
        MergeKind::Letters,
        MissingField::Error,
    )
    .expect("merge");
    let text = documents[0].plain_text(StoryRef::Body);
    assert!(text.contains("Total 12400.005"), "{text}");
}

#[test]
fn a_clean_document_that_only_gains_a_band_row_still_merges() {
    // The template has no scalar fields at all: only the band. It must still print one row per child
    // and not fail on the absent parent headers.
    let mut doc = ModelDocument::new();
    doc.body
        .push(Arc::new(Block::Table(Table::new().header(["Article"]))));
    if let Some(Block::Table(table)) = doc.body.last_mut().and_then(|block| Arc::get_mut(block)) {
        table.rows.push(Row::cells(vec![cell("Lines.Name")]));
    }
    doc.ensure_nonempty();
    let documents = merge(
        &doc,
        parents(&[("Q-1", "Ada", "90.00")]),
        band(&[&[("Widget", "2", "40.00"), ("Gadget", "1", "50.00")]]),
        MergeKind::Letters,
        MissingField::Error,
    )
    .expect("merge");
    let body = lines(&documents[0]);
    assert_eq!(
        body.iter()
            .filter(|line| line == &"Widget" || line == &"Gadget")
            .count(),
        2,
        "{body:?}"
    );
}
