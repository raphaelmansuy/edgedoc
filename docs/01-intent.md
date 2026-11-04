# Why EdgeDoc exists, and what version 1 includes.

> Part of the EdgeDoc specification. Date: 2026-11-04. Requirement ids are normative. See [the index](README.md).

## 1. Five whys

### 1.1 Why build a Word library now

Invoices, letters, and contracts still move as `.docx` files. A program that creates those files must read a template, fill fields, and save a file that Word opens. A full editor does not fit a server job or a browser form.

### 1.2 Why current libraries lose the template

`python-docx` and `docx-rs` keep a small model. They read the parts they know. They drop the rest on save. A chart, a signature line, a content control, or a theme can disappear. The file is no longer the template the business approved.

WordCraft does not drop whole parts. `Document::passthrough` keeps source parts as bytes (`crates/doc/src/lib.rs`). The DOCX reader skips unknown markup in a part. The crate docs state that rule (`crates/docx/src/lib.rs`).

### 1.3 Why a second small model is the wrong center

A `.docx` file is a package of parts. It is not only a list of paragraphs. ECMA-376 Part 2 defines the package. Part 1 defines WordprocessingML. Microsoft adds elements in [MS-DOCX](https://learn.microsoft.com/en-us/openspecs/office_standards/ms-docx/b839fe1f-e1ca-4fa6-8c26-5954d0abbccd). A library that stores only paragraphs cannot save a template that already exists.

### 1.4 Why one Rust core must feed Python and WebAssembly

The same invoice merge runs in a Rust service, a Python job, and a browser. Three merge engines drift. Field rules, size limits, and error codes then disagree. One core, with two adapters, keeps those rules identical.

### 1.5 Why the public API is a facade over an explicit model

Creation, editing, and mail merge are different jobs. WordCraft already splits them. The document is `wordcraft_doc::Document`. Merge state lives on the session, not on the document (`MergeState` in `crates/engine/src/cmd/mailings.rs`). The preview path still writes field results back into the open document (`refresh` in `crates/engine/src/cmd/mailings/merge.rs`).

EdgeDoc keeps the split. EdgeDoc does not write preview text into the template. The root need is one standalone library that reads, edits, merges, and saves Word files without becoming an editor.

## 2. Scope

### 2.1 In scope

EdgeDoc reads and writes WordprocessingML packages. It offers a fluent API to build and edit a document. It merges records into a template and returns new documents. It publishes the same behavior as a Rust crate, a PyPI wheel, and a WebAssembly module.

`ED-S-01`. The product is a library. It has no window, no ribbon, and no layout engine.

### 2.2 Out of scope for version 1

Version 1 leaves this work out:

- Pagination, page drawing, and PDF export.
- Spelling checks and VBA execution.
- Evaluation of `PAGE`, `NUMPAGES`, `TOC`, and `REF`. Those fields keep their cached result.
- A writer for the Word 97-2003 binary format.
- Spreadsheet formula evaluation.
- Custom-XML data-binding evaluation.

### 2.3 Readers this spec assumes

The primary reader is a programmer who calls the library. The secondary reader is a reviewer who checks a requirement against a test. Chapters 01 to 08 and chapter 10 are descriptive. Chapter 09 is a procedure.
