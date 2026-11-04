# The implementation plan.

> Part of the EdgeDoc specification. Date: 2026-11-04. Requirement ids are normative. See [the index](README.md).

## 16. Implementation plan

Estimates are planning estimates for one engineer who already knows Rust. They are not measurements.

Do the phases in order. Do not start a phase before the previous phase meets its exit test. Adapters start only after the facade exit test passes. That rule keeps merge logic in one crate.

### 16.1 Phase 0. Workspace

1. Create the workspace with the crates in [03-architecture.md](03-architecture.md).
2. Set `edition = "2021"`. This checkout compiles with Rust 1.76. Move to edition 2024 when `rustc` is 1.85 or newer.
3. Set `rust-version` to `1.76`. The WordCraft checkout used for the examples declares `1.95`.
4. Forbid `unsafe_code`.
5. Deny `clippy::unwrap_used`, `expect_used`, `panic`, `todo`, and `unimplemented` in library crates. WordCraft denies those lints in `crates/doc/src/lib.rs`.
6. Add a layer test that rejects a dependency from a lower layer to a higher layer.
7. Add CI for `cargo test`, `cargo clippy`, and `cargo build --target wasm32-unknown-unknown -p edgedoc-wasm`.

Exit test: the layer test fails when `edgedoc-merge` is given a dependency on `edgedoc-opc`.

### 16.2 Phase 1. Limits, units, and OPC

1. Implement `Budget` once.
2. Implement length newtypes once.
3. Implement `PackageSource`, content types, and relationships.
4. Add the hostile ZIP tests `ED-X-04` and `ED-X-03`.

Exit test: a memory package round-trips part bytes and relationship ids.

### 16.3 Phase 2. Model

1. Implement stories, blocks, runs, and positions.
2. Implement `Document::edit` with rollback.
3. Implement opaque markup nodes and `EditReport`.
4. Do not parse XML in this phase.

Exit test: `ED-X-19` and the rollback test pass. A failed closure leaves `plain_text` unchanged.

### 16.4 Phase 3. Catalog, reader, and writer

1. Write the owned catalog once.
2. Implement the reader from the catalog.
3. Implement the writer from the catalog.
4. Add Strict namespace mapping and alternate-content retention.
5. Add passthrough parts and the clean-save hash.

Exit test: `ED-X-07`, `ED-X-08`, `ED-X-12`, `ED-X-13`, `ED-X-15`, and `ED-X-16` pass. A property test covers `ED-S-08`.

### 16.5 Phase 4. Fluent facade

1. Implement the builder in [05-model-and-api.md](05-model-and-api.md) on top of `Document::edit`.
2. Do not add a second paragraph type.
3. Implement `open`, `save`, and the atomic replace path.

Exit test: the invoice sample in [05-model-and-api.md](05-model-and-api.md) writes a package that the reader loads to the same plain text.

### 16.6 Phase 5. Merge

1. Implement `parse_instruction` once.
2. Implement `FieldEval` and the registry.
3. Implement records, CSV, and the spreadsheet reader behind the `sheet` feature.
4. Implement `MergeBatch` and `Join`.
5. Port the WordCraft cursor rules. Do not port `refresh` mutation.

Exit test: `ED-X-21` through `ED-X-32` and `ED-X-44` pass.

### 16.7 Phase 6. Retention of hard markup

1. Add consent for revisions, comments, and equations.
2. Add content controls from `ED-T-12`.
3. Add inline PNG, JPEG, and GIF.
4. Keep EMF and WMF opaque.

Exit test: `ED-X-14`, `ED-X-41`, and `ED-X-42` pass.

### 16.8 Phase 7. Python adapter

1. Add `edgedoc-py` with the methods in [07-adapters.md](07-adapters.md).
2. Generate stubs.
3. Build the wheel tags in `ED-Y-03` with Maturin.
4. Compare package bytes with the Rust test (`ED-X-45`).

Exit test: `pytest` loads the wheel and the byte-parity test passes.

### 16.9 Phase 8. WebAssembly adapter

1. Add `edgedoc-wasm` with the exports in [07-adapters.md](07-adapters.md).
2. Turn off `fs` and `crypt` in the default WASM build.
3. Enforce the size budget in CI.
4. Run the same byte-parity fixture in Node.

Exit test: `ED-W-05` and `ED-X-45` pass.

### 16.10 Phase 9. Encryption

1. Enable `crypt` on the native facade.
2. Implement agile and standard encryption from [MS-OFFCRYPTO](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-offcrypto/3c34d72a-1a61-4b52-a893-196f9157f083).
3. Keep passwords out of `Error`.

Version 1 already detects an encrypted package. No password returns `package.password_required`. A password returns `package.encrypted_unsupported` until this phase can check the verifier. The library does not return a document body.

Exit test: `ED-X-09` and `ED-X-10` pass. A round trip with a known password restores the plain package.

### 16.11 Later work

Do this work in new versions. Do not mix it into version 1.

| Item | Why it waits |
|---|---|
| Strict writer | Many transitional elements have no strict form |
| Word 2003 XML | A second vocabulary. WordCraft rewrites it into a package first |
| Binary `.doc` read | A different parser. Put it in `edgedoc-doc` so the facade stays small |
| `PAGE` and `TOC` evaluation | Those fields need a layout engine. That engine is an editor |
| Chart editing | Passthrough already keeps charts |
| `wasm32-wasip2` | Another adapter after the browser module is stable |
| Data-binding evaluation | XPath into custom XML is a separate engine |

### 16.12 Planning estimates

| Phase | Estimate |
|---|---|
| 0 workspace | 1 week |
| 1 OPC | 2 weeks |
| 2 model | 2 weeks |
| 3 reader and writer | 4 weeks |
| 4 facade | 2 weeks |
| 5 merge | 3 weeks |
| 6 retention | 2 weeks |
| 7 Python | 2 weeks |
| 8 WebAssembly | 1 week |
| 9 encryption | 2 weeks |
| Sum | 21 weeks |

### 16.13 Definition of done for version 1

1. Every requirement in [10-traceability.md](10-traceability.md) has a test, or the row says Manual.
2. The three packages share one version.
3. `ED-X-45` passes.
4. A person opens the invoice sample in Word and confirms that Word shows no repair dialog. Record the Word version in the test note.
