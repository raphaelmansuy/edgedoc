# WordCraft traceability, the requirement index, and the acceptance scenario.

> Part of the EdgeDoc specification. Date: 2026-11-04. Requirement ids are normative. See [the index](README.md).

## 17. Traceability to WordCraft

This spec was read from `/Users/raphaelmansuy/Github/03-working/wordcraft` at version `0.14.0` in that checkout. Copy the cited behavior. Do not copy the cited defect.

| EdgeDoc rule | WordCraft location | What to copy | What to change |
|---|---|---|---|
| Layered crates | `docs/architecture.md` | L0 units, L1 model, L2 docx | Drop layout, render, engine, and UI |
| Document shape | `crates/doc/src/lib.rs` | Stories, `Arc` blocks, passthrough map | Add opaque markup inside a story |
| Positions | `crates/doc/src/position.rs`, `edit.rs` | Character-boundary edits | Hide mutation behind `edit` |
| Empty story | `Document::ensure_nonempty` | One paragraph minimum | Keep |
| Package budgets | `crates/docx/src/package.rs` | `MAX_PART`, `MAX_TOTAL`, `MAX_ENTRIES` | One `Budget` type |
| XML budgets | `crates/docx/src/xml.rs` | Depth, elements, attributes | Reject DTDs in that parser setup |
| Flavors | `Flavor` in `crates/docx/src/lib.rs` | Four content types | Keep |
| Errors | `DocxError`, `DocError` | Password and limit variants | Stable string codes |
| Unknown markup | `crates/docx/src/lib.rs` | Lenient numbers | Do not skip unknown markup. Retain it |
| Field pairs | `crates/doc/src/fields.rs` | Balance orphan markers before save | Keep |
| Merge commands | `crates/engine/src/cmd/mailings.rs` | Field kinds, filters, letters, directory, labels | No session and no JSON command bus |
| Field parse | `mergefield_name`, `condition`, `act` | Name boundary, numeric compare, NEXT, SKIPIF | Registry of `FieldEval` |
| Finish | `finish` in `merge.rs` | Page break, per-record headers, block cap | Fail the cap. Do not return a partial document |
| Preview | `refresh` | Placeholder `«Name»` | Return a new document |
| Safe save | `docs/file-format-parity.md` | Temporary file, then replace | Facade `fs` feature only |
| Spreadsheet caps | `docs/file-format-parity.md` | Cached values and caps | `sheet` feature |
| WASM random | `crates/docx/Cargo.toml` | `getrandom` `wasm_js` | `crypt` feature only |
| Web headers | `packaging/web/README.md` | No `SharedArrayBuffer` | Library module, not the editor |
| Unsafe | workspace `Cargo.toml` | `forbid(unsafe_code)` | Keep |

## 18. Requirement index

| Id | Clause | Source |
|---|---|---|
| `ED-S-01` | 2.1 | EdgeDoc rule |
| `ED-S-02` | 5.1 | EdgeDoc rule |
| `ED-S-03` | 7 | XML parser practice. No external entities |
| `ED-S-04` | 7 | `ED-E-02` |
| `ED-S-05` | 7 | N5, WordCraft `zeroize` |
| `ED-S-06` | 7 | WordCraft workspace lint |
| `ED-S-07` | 7 | EdgeDoc rule |
| `ED-S-08` | 15 | N9 property tests |
| `ED-N-01` | 3 | This clause |
| `ED-A-01` | 6.1 | WordCraft layer rule |
| `ED-A-02` | 6.1 | DRY |
| `ED-A-03` | 6.2 | Dependency inversion |
| `ED-A-04` | 6.3 | DRY catalog |
| `ED-A-05` | 6.3 | DRY field parser |
| `ED-A-06` | 6.3 | DRY budgets |
| `ED-A-07` | 6.3 | DRY write path |
| `ED-A-08` | 6.3 | WordCraft label tables |
| `ED-A-09` | 7.1 | N9 feature flags |
| `ED-P-01` | 8.1 | N1 Part 2 |
| `ED-P-02` | 8.1 | N1 Part 2, WordCraft `rt::OFFICE_DOC` |
| `ED-P-03` | 8.1 | N1 |
| `ED-P-04` | 8.1 | Dependency inversion |
| `ED-P-05` | 8.2 | N1 content types, WordCraft `Flavor` |
| `ED-P-06` | 8.2 | N3 macro packages |
| `ED-P-07` | 8.3 | N1 Strict and Transitional |
| `ED-P-08` | 8.3 | EdgeDoc version 1 limit |
| `ED-P-09` | 8.3 | N1 Part 3 |
| `ED-P-10` | 8.4 | N5 |
| `ED-P-11` | 8.4 | EdgeDoc feature flag |
| `ED-P-12` | 10.3 | EdgeDoc rule |
| `ED-P-13` | 10.3 | EdgeDoc rule |
| `ED-T-01` | 6.4 | WordCraft `Document` |
| `ED-T-02` | 6.4 | N9 `Send` and `Sync` |
| `ED-T-03` | 6.5 | EdgeDoc rule |
| `ED-T-04` | 6.5 | `ED-R-06` |
| `ED-T-05` | 9.1 | WordCraft `Pos` |
| `ED-T-06` | 9.1 | WordCraft `ensure_nonempty` |
| `ED-T-07` | 9.1 | WordCraft `Paragraph` |
| `ED-T-08` | 9.2 | N1 Part 1 WordprocessingML subset |
| `ED-T-09` | 9.2 | N4, WordCraft #5077 |
| `ED-T-10` | 9.2 | N4, WordCraft #4696 |
| `ED-T-11` | 9.2 | Word limit, WordCraft `MAX_COLS` |
| `ED-T-12` | 9.3 | N1, N3 content controls |
| `ED-T-13` | 9.3 | N3 data binding |
| `ED-R-01` | 5.2 | Manual Word check |
| `ED-R-02` | 5.2 | EdgeDoc rule |
| `ED-R-03` | 9.4 | EdgeDoc rule, WordCraft glossary fingerprint |
| `ED-R-04` | 9.4 | WordCraft `passthrough` |
| `ED-R-05` | 9.4 | WordCraft settings merge |
| `ED-R-06` | 9.4 | EdgeDoc rule |
| `ED-F-01` | 10.1 | N9 builders |
| `ED-F-02` | 10.1 | WordCraft `geom` units |
| `ED-F-03` | 10.1 | N8 `MERGEFIELD`, WordCraft `quote` |
| `ED-F-04` | 10.2 | `ED-T-03` |
| `ED-F-05` | 10.2 | N9 `Result` |
| `ED-F-06` | 10.2 | WordCraft size clamp |
| `ED-F-07` | 10.3 | `ED-A-09` |
| `ED-F-08` | 10.3 | WordCraft safe save |
| `ED-F-09` | 10.3 | WordCraft safe save |
| `ED-F-10` | 10.4 | N9 getters |
| `ED-F-14` | 10.5 | Markdown export and read |
| `ED-F-15` | 10.5 | Markdown image paths and limits |
| `ED-M-01` | 11.1 | WordCraft `MergeState` |
| `ED-M-02` | 11.1 | EdgeDoc correction of `refresh` |
| `ED-M-03` | 11.2 | EdgeDoc rule |
| `ED-M-04` | 11.2 | N13, WordCraft CSV |
| `ED-M-05` | 11.2 | WordCraft sheet caps |
| `ED-M-06` | 11.2 | N13 |
| `ED-M-07` | 11.3 | WordCraft walker |
| `ED-M-08` | 11.3 | N8, open/closed principle |
| `ED-M-09` | 11.3 | WordCraft `address_map` |
| `ED-M-10` | 11.3 | WordCraft `condition` |
| `ED-M-11` | 11.3 | EdgeDoc default. WordCraft uses empty |
| `ED-M-12` | 11.3 | N8 `MERGEFORMAT` |
| `ED-M-13` | 11.4 | WordCraft `finish` |
| `ED-M-14` | 11.4 | WordCraft `per_record` |
| `ED-M-15` | 11.4 | WordCraft directory and labels |
| `ED-M-16` | 11.4 | WordCraft cap, stricter result |
| `ED-M-17` | 11.4 | EdgeDoc correction of `refresh` |
| `ED-M-18` | 11.5 | WordCraft `ENVELOPE_SIZES` |
| `ED-M-19` | 11.5 | WordCraft `MAX_LABEL_TEXT` |
| `ED-M-20` | 11.6 | WordCraft `mailings.filterRecipients` |
| `ED-M-21` | 11.6 | WordCraft `MergeState.filter` and `MergeState.sort` |
| `ED-E-01` | 12 | N9 `Result` |
| `ED-E-02` | 12 | Error-message pattern in clause 12 |
| `ED-E-03` | 12 | N11 |
| `ED-Y-01` | 13 | N10 |
| `ED-Y-02` | 13 | N11 |
| `ED-Y-03` | 13 | N11 stable ABI |
| `ED-Y-04` | 13 | N11 distribution |
| `ED-Y-05` | 13 | `ED-A-02` |
| `ED-Y-06` | 13 | DRY stubs |
| `ED-Y-07` | 13 | `ED-T-02` |
| `ED-W-01` | 14 | N10, N12 |
| `ED-W-02` | 14 | `ED-A-02` |
| `ED-W-03` | 14 | No filesystem in the browser |
| `ED-W-04` | 14 | WordCraft `wasm_js` |
| `ED-W-05` | 14 | Size budget |
| `ED-W-06` | 14 | WordCraft `packaging/web/README.md` |
| `ED-W-07` | 14 | N12. WASI is later |
| `ED-X-01` to `ED-X-45` | 15 | Tests named in that table |

## 19. Acceptance scenario

A caller builds the invoice in [05-model-and-api.md](05-model-and-api.md), or opens a template that contains `«InvoiceNumber»` and `«Amount»`. The caller passes two records. `MissingField::Error` is set. One record lacks `Amount`.

The merge returns `merge.unknown_field`. It returns no package. The caller sets `MissingField::Empty` and merges again. The library returns two letter sections. The header field, when present, differs per section. `write_bytes` from Rust, from Python, and from WebAssembly returns the same bytes. Word opens that package and does not show a repair dialog.
