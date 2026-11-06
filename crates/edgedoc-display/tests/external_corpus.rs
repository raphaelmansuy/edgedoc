//! Opening/layout checks for the pinned external corpus. Pixel parity uses the independent gate.
#![cfg(feature = "serde")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use edgedoc_display::{DisplayDocument, OpenOptions};
use std::path::Path;

#[test]
#[ignore = "Download the pinned corpus with python3 tools/fetch_external_corpus.py first"]
fn pinned_external_documents_surface_layout_limits_without_crashing() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let manifest: serde_json::Value = serde_json::from_slice(
        &std::fs::read(root.join("tests/corpus/external-manifest.json")).unwrap(),
    )
    .unwrap();
    let entries = manifest["documents"].as_array().unwrap();
    assert!(entries.len() >= 75);
    let inventory: serde_json::Value = serde_json::from_slice(
        &std::fs::read(root.join("target/external-docx/inventory.json"))
            .expect("Run corpus verification first"),
    )
    .unwrap();
    let mut opened = 0;
    let mut rejected = 0;
    let mut failures = Vec::new();
    for entry in entries {
        let id = entry["id"].as_str().unwrap();
        let bytes = std::fs::read(root.join("target/external-docx").join(format!("{id}.docx")))
            .expect("Required fixture missing; run the downloader");
        match DisplayDocument::open(&bytes, &OpenOptions::default()) {
            Err(error) if id == "poi-22-deep-table-cell" && error.code == "xml.limit" => {
                rejected += 1
            }
            Err(error) => failures.push(format!("{id}: {} {}", error.code, error.message)),
            Ok(mut document) => match document.layout() {
                Ok(report) => {
                    opened += 1;
                    assert!(report.pages > 0, "{id} has no pages");
                    if id == "docx4j-26-arabic_numbering" {
                        assert!(
                            document
                                .diagnostics()
                                .iter()
                                .any(|d| d.code == "display.unsupported_layout"
                                    && d.detail == "Complex-script and bidirectional layout"),
                            "The visibly incorrect Arabic layout must report its limitation"
                        );
                    }
                    let expected_text = inventory["documents"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|item| item["id"] == id)
                        .unwrap()["text_characters"]
                        .as_u64()
                        .unwrap();
                    if expected_text > 0 {
                        assert!(
                            !document.all_text().trim().is_empty()
                                || document
                                    .diagnostics()
                                    .iter()
                                    .any(|d| d.code == "display.unsupported_layout"),
                            "{id} silently lost all source text"
                        );
                    }
                }
                Err(error) => failures.push(format!("{id}: {} {}", error.code, error.message)),
            },
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert_eq!(opened, entries.len() - 1);
    assert_eq!(rejected, 1);
}
