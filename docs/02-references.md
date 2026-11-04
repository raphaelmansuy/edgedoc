# Normative sources, terms, and conformance classes.

> Part of the EdgeDoc specification. Date: 2026-11-04. Requirement ids are normative. See [the index](README.md).

## 3. Normative references

A later edition applies only when this clause names it. Implementation notes from Microsoft override ECMA-376 where Word and the standard disagree. [MS-OI29500](https://learn.microsoft.com/en-us/openspecs/office_standards/ms-oi29500/1fd4a662-8623-49c0-82f0-18fa91b413b8) revision 25.0 (2026-08-18) is that note set.

| Id | Document | Role |
|---|---|---|
| N1 | [ECMA-376](https://ecma-international.org/publications-and-standards/standards/ecma-376/) 5th edition | Package, markup, compatibility, transitional features |
| N2 | [ISO/IEC 29500-1:2016](https://www.iso.org/standard/71691.html) | Public markup reference. Edition 5 was under publication on 2026-11-04 |
| N3 | [MS-DOCX](https://learn.microsoft.com/en-us/openspecs/office_standards/ms-docx/b839fe1f-e1ca-4fa6-8c26-5954d0abbccd) 23.0 | Word extensions |
| N4 | [MS-OI29500](https://learn.microsoft.com/en-us/openspecs/office_standards/ms-oi29500/1fd4a662-8623-49c0-82f0-18fa91b413b8) 25.0 | Where Office differs from ISO/IEC 29500 |
| N5 | [MS-OFFCRYPTO](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-offcrypto/3c34d72a-1a61-4b52-a893-196f9157f083) 14.0 | Encrypted packages |
| N6 | [MS-DOC](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-doc/ccd7b486-7881-484c-8e4e-14f25f9bcead) | Binary `.doc`. Read-only, later crate |
| N7 | [Open XML SDK](https://learn.microsoft.com/en-us/office/open-xml/open-xml-sdk) | Behavior reference, not a dependency |
| N8 | [Word field codes](https://support.microsoft.com/en-us/office/list-of-field-codes-in-word-1ad6d91a-55a7-4a8d-b535-cf7888659a51) | Field instruction text |
| N9 | [Rust API guidelines](https://rust-lang.github.io/api-guidelines/checklist.html) | Public Rust API |
| N10 | [Semantic Versioning 2.0.0](https://semver.org/) | Crate, wheel, and npm versions |
| N11 | [PyO3](https://pyo3.rs/) 0.29 and [Maturin](https://www.maturin.rs/) 1.14.1 or newer | Python wheels |
| N12 | [wasm-bindgen](https://rustwasm.github.io/docs/wasm-bindgen/) | Browser module |
| N13 | [RFC 4180](https://www.rfc-editor.org/rfc/rfc4180) | CSV records |

`ED-N-01`. Clause 18 maps each requirement to one of these sources, or marks the requirement as an EdgeDoc rule.

### 3.1 Rejected bases

The Open XML SDK and the Rust crate `ooxmlsdk` expose generated schema types. That shape round-trips a package. It is a poor API for an invoice. EdgeDoc does not use generated schema types as its public API.

`docx-rs` is a builder with a weak reader and no merge engine. `python-docx` drops unknown parts. WordCraft is an editor. EdgeDoc copies WordCraft's layer split, package limits, and merge rules. EdgeDoc does not copy the engine, the layout crate, or the user interface.

## 4. Terms

Each term has one meaning in this spec and in the code.

| Term | Meaning |
|---|---|
| package | The ZIP container defined by ECMA-376 Part 2 |
| part | One named item in a package |
| document | The in-memory model |
| story | An ordered list of blocks. The body is a story. A header is a story |
| block | A paragraph or a table |
| run | A contiguous span of characters with one set of character properties |
| field | A Word field. It has an instruction and a cached result |
| merge field | A field whose instruction keyword is `MERGEFIELD` |
| template | A document that holds merge fields |
| record | One row of merge data |
| result document | A new document produced by a merge |
| passthrough part | A part stored as bytes and written back unchanged |
| opaque markup | Markup the model does not own, stored in document order |
| facade | The `edgedoc` crate |
| adapter | The Python crate or the WebAssembly crate |
| policy | A value that selects a behavior |
| limit | A maximum the library refuses to pass |
| flavor | `docx`, `docm`, `dotx`, or `dotm` |
| owned element | An element the model can edit |
| conformance class | A named set of requirements the library claims |

Do not call a package a file when the clause is about parts. Do not call a record a row in the API. The CSV parser may say row for the text line. The merge API says record.

## 5. Conformance

### 5.1 Classes

| Class | Id prefix | Claim |
|---|---|---|
| P package | `ED-P` | Read and write an OPC package, relationships, and content types |
| T text | `ED-T` | Paragraphs, runs, tables, styles, numbering, sections, headers, and footers |
| M merge | `ED-M` | Merge fields for letters, a directory, and labels |
| R retention | `ED-R` | Unedited parts stay byte-identical. Edited stories report any dropped markup |
| S safety | `ED-S` | Hostile input returns `Err`. It does not panic |

`ED-S-02`. Version 1 claims classes P, T, M, R, and S. It does not claim page parity with Word.

### 5.2 What "Word compatible" means

`ED-R-01`. A document that the library wrote from the owned set opens in Word without a repair dialog. The check is manual. CI uses schema checks and round-trip equality. Word output is not committed. WordCraft uses the same gate (`docs/file-format-parity.md`).

`ED-R-02`. Byte identity is required only for an unedited package and for passthrough parts. Semantic equality is required for the owned set.
