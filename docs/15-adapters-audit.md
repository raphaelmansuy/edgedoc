# Adapter audit

Date: 2026-11-04.

The crate, the wheel, and the WebAssembly module must expose the same work. This
chapter records what each adapter offers, how each one is tested, and what the audit
against ECMA-376 and WordCraft found.

## The rule

Every adapter calls the facade `edgedoc`. An adapter does not call a lower crate, because
a second copy of the read or the write path would drift from the first.

`layer_dependencies_match_the_plan` in `crates/edgedoc/tests/e2e.rs` enforces this rule.
The audit found one violation and it is fixed: the Python adapter called
`edgedoc-opc::is_encrypted_package` directly. The facade now offers
`Document::is_password_protected`.

## Surface

| Task | Rust | Python | WebAssembly |
|---|---|---|---|
| Read the body text | `plain_text` | `plain_text` | `readDocument` |
| Read one story | `Document::plain_text(StoryRef::Part)` | `text_of_story` | `textOf` |
| Story counts | `Document::parts` | `story_summary` | — |
| Title | `Document::core_props` | `doc_title` | `documentSummary` |
| Summary | `Document::part_names`, `body_len`, `section_break_count` | `document_summary` | `documentSummary` |
| Merge field names | `merge_field_names` | `merge_field_names` | `mergeFieldNames` |
| Part names | `part_names` | `part_names` | `documentSummary` |
| Merge one record | `preview_with` | `merge_record` | `preview` |
| Merge every record | `merge(...).finish().join(...)` | `merge_records` | `mergeRecords` |
| Merge with the default mode | `merge_letters_bytes` | `merge_csv` | `merge` |
| Write another flavour | `write_bytes_with` | `write_as_flavor` | `writeAs` |
| Write without an edit | `write_bytes` | `read_and_write` | `writeDocument` |
| Password detection | `Document::is_password_protected` | `is_password_protected` | `isPasswordProtected` |
| Edited flag | `Document::is_dirty` | `is_edited` | — |

A dash means the task is not exposed. Each dash is a deliberate choice, not an oversight.
A script rarely asks for the story count or the edited flag. A smaller surface is also
easier to keep correct.

## What the audit found

| Defect | Effect | Fix |
|---|---|---|
| The Python adapter called a lower crate | The layer rule was broken | `Document::is_password_protected` on the facade |
| A Python function returned `Vec<u8>` | The caller received a list of integers, not `bytes` | Return `PyBytes` from every writer |
| `preview` hardcoded `MissingField::Error` | A template with a computed field could not preview | `Document::preview_with` takes the mode |
| A flavour change on a clean document returned the input | `writeAs("dotx")` returned docx bytes | A flavour change forces a rewrite |
| The WASM error dropped the code | A caller could not branch on the error | Every message starts with the stable code |
| The WASM module was not built in CI | A broken binding reached the release | CI builds it and runs the examples |
| `wasm-bindgen` resolved above the MSRV | The build failed on Rust 1.76 | The lockfile pins 0.2.100 |

## Against ECMA-376

The adapters hold bytes. They do not parse or write markup, so the schema rules live in
`edgedoc-wml`. Two rules do reach the adapters:

**A ZIP package is not a Word document.** `package.not_zip` covers bytes that are not a
package. `package.not_word` covers a package with no main part. A caller can tell the two
apart because the code is stable.

**An encrypted package is a compound file.** ECMA-376 keeps a protected document in a CFB
container with an `EncryptionInfo` stream. Detection reads the container signature and the
stream name, not the password. A bare CFB signature without that stream is not a
protected document, which is why the check tests both.

## Against WordCraft

WordCraft exposes the same ideas in its command line: `info`, `text`, `inspect`, `convert`,
and `render`. The adapters cover `info`, `text`, and `convert`.

| WordCraft command | Adapter equivalent |
|---|---|
| `info <file>` | `document_summary` and `documentSummary` |
| `text <file>` | `plain_text`, `text_of_story`, and `textOf` |
| `inspect <file>` | `document_summary` with the part count |
| `convert <in> <out>` | `write_as_flavor` for another flavour |
| `render <file> <out.png>` | Not offered. A layout engine is out of scope |

`make verify` uses the WordCraft command line as an independent reader, so the documents
that these adapters produce are checked by another implementation. See
[13-word-validation.md](13-word-validation.md).

## How the adapters are tested

### Python

`examples/python/test_edgedoc.py` holds 17 cases. They run the installed wheel, not the
Rust code, so a binding mistake fails there.

```bash
python3 -m venv .venv
.venv/bin/pip install maturin pytest
make test-python
```

The cases cover each row of the surface table, the three missing-field modes, an unknown
story name, an unknown flavour, and the error code.

### WebAssembly

`examples/wasm/run.mjs` builds the module for `wasm32-unknown-unknown`, converts it with
`wasm-bindgen`, and runs nine examples in Node. It writes `target/examples/wasm-merged.docx`,
which `make verify` then reads with WordCraft.

```bash
cargo install wasm-bindgen-cli --version 0.2.100
make example-wasm
```

The module needs no file system and no threads. The `fs` feature stays off.

## What the adapters do not expose

These need the Rust API, because they take a closure or a borrowed value that crosses no
language boundary well:

- `Document::edit`, which takes a closure over the document model.
- The fluent builders for a paragraph, a table, a style, and a picture.
- A merge `map_value` closure.
- The `Budget` controls.

A script that needs one of these builds the document in Rust and ships the bytes. The
examples follow that rule: the Rust demo crate builds the documents, and the adapters
read, merge, and write them.
