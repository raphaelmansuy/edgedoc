# Word format coverage

Date: 2026-11-04.

This chapter records what EdgeDoc does with the Word format. It also records the same feature in WordCraft 0.14.0. The source for the WordCraft column is `docs/file-format-parity.md` in that project.

A row stays true only while `format_matrix` passes. The test is `crates/edgedoc/tests/format_coverage.rs`.

## Status words

| Word | Meaning |
|---|---|
| Modeled | The reader and the writer use the document model. |
| Kept | The save copies the bytes or the markup. EdgeDoc does not edit that feature. |
| Refused | The call returns an error and no document body. |
| Absent | EdgeDoc does not read the feature and does not keep it. |

## How to keep this table current

1. Add a case in `crates/edgedoc/tests/format_coverage.rs`.
2. Add a row here with the same id in the Test column.
3. Run `cargo test -p edgedoc --test format_coverage`.

When a case has no row, the test fails. When a Test cell has no case, the test fails. Use `none` only for a feature that has no implementation.

## Result

EdgeDoc models the Transitional package, the owned text, tables, sections, fields, pictures, content controls, notes, comments, and mail merge. It keeps macros, themes, charts, equations, metafiles, and conditional table formatting. It refuses binary `.doc`, Word 2003 XML, and encrypted packages.

WordCraft also reads binary `.doc`, decrypts a package, rewrites Word 2003 XML, writes Flat OPC, and lays out pages. EdgeDoc does not.

Owned character, paragraph, and table properties also round-trip in `owned_features_round_trip` in `crates/edgedoc/tests/e2e.rs`. Chapter [12-word-features.md](12-word-features.md) lists every modeled field with its own test.

## Package

| Area | Feature | EdgeDoc | WordCraft | Test |
|---|---|---|---|---|
| Package | DOCX Transitional package | Modeled | Modeled | `pkg-docx` |
| Package | DOCM flavor and macro bytes | Modeled flavor. Macros kept | Modeled. Macros never run | `pkg-docm` |
| Package | DOTX template | Modeled | Modeled | `pkg-dotx` |
| Package | DOTM flavor and macro bytes | Modeled flavor. Macros kept | Modeled. Macros never run | `pkg-dotm` |
| Package | Strict markup in, Transitional markup out | Modeled | Read modeled. Write absent | `pkg-strict` |
| Package | Flat OPC read | Modeled read. Save is a zip | Modeled read and write | `pkg-flat` |
| Package | Save with no edit | Modeled. The original bytes return | Absent | `pkg-clean` |
| Package | Part with no model | Kept | Kept | `pkg-unknown` |
| Package | Word 2003 XML | Refused `unsupported` | Read and write | `pkg-wordml2003` |
| Package | Binary `.doc` | Refused `unsupported` | Read-only | `pkg-doc-binary` |
| Package | Encrypted package, no password | Refused `package.password_required` | Modeled | `pkg-encrypted` |
| Package | Encrypted package, with a password | Refused `package.encrypted_unsupported` | Modeled | `pkg-encrypted-password` |

## Text, fields, and stories

| Area | Feature | EdgeDoc | WordCraft | Test |
|---|---|---|---|---|
| Text | Tab | Modeled as a tab character | Modeled | `text-tab` |
| Text | Line break | Modeled as a newline character | Modeled | `text-break` |
| Text | Footnote mark and footnotes part | Modeled | Modeled | `text-footnote-ref` |
| Text | Endnote mark and endnotes part | Modeled | Modeled | `text-endnote-ref` |
| Text | Comment mark and comments part | Modeled | Modeled | `text-comment` |
| Text | Tracked insertion | Kept. An edit asks for consent | Modeled | `text-revision` |
| Text | Equation markup | Kept. Markdown export is LaTeX. An edit asks for consent | Modeled | `text-equation` |
| Text | Bookmark | Modeled | Modeled | `text-bookmark` |
| Text | External hyperlink | Modeled | Modeled | `text-hyperlink` |
| Text | PAGE field | Kept. The cached result stays | Editor updates the result | `text-page-field` |
| Text | Simple field | Modeled | Modeled | `text-simple-field` |
| Text | Right-to-left run and run language | Modeled | Modeled | `text-rtl` |

## Sections, tables, drawings, and controls

| Area | Feature | EdgeDoc | WordCraft | Test |
|---|---|---|---|---|
| Section | Default, first, and even header, and first footer | Modeled | Modeled | `sect-headers` |
| Table | Column span above 63 | Modeled. The span becomes 63 | Modeled | `tbl-clamp` |
| Drawing | PNG picture | Modeled | Modeled | `pic-png` |
| Drawing | JPEG picture | Modeled | Modeled | `pic-jpeg` |
| Drawing | EMF metafile | Kept, with the same relationship id | Modeled | `pic-emf` |
| Control | Date, drop-down, and checkbox | Modeled | Modeled | `sdt-kinds` |

## Parts that stay in the package

| Area | Feature | EdgeDoc | WordCraft | Test |
|---|---|---|---|---|
| Part | Numbering part | Modeled. The writer appends to it | Modeled | `part-numbering` |
| Part | Theme part | Kept | Modeled | `part-theme` |
| Part | Chart part | Kept | Modeled | `part-chart` |
| Part | Font table | Kept | Kept | `part-fonts` |
| Part | Settings part | Kept, except the part that the model owns | Modeled | `part-settings` |

## Merge

| Area | Feature | EdgeDoc | WordCraft | Test |
|---|---|---|---|---|
| Merge | Letter field | Modeled | Modeled | `merge-letter` |

## Not in this version

These features are absent or refused. The Test cell is `none` because a success test would claim support that the code does not have. Refused rows in the package table already have a test.

| Area | Feature | EdgeDoc | WordCraft | Test |
|---|---|---|---|---|
| Package | Strict writer | Absent | Absent | `none` |
| Package | Flat OPC writer | Absent | Modeled | `none` |
| Package | Encrypt on save | Absent | Modeled | `none` |
| Package | Decrypt with a password | Absent. Phase 9 | Modeled | `none` |
| Package | Binary `.doc` writer | Absent | Absent | `none` |
| Layout | Pagination and PDF | Absent | Modeled | `none` |
| Field | PAGE, NUMPAGES, TOC, and REF evaluation | Absent. The cached result stays | Modeled | `none` |
| Field | Spelling | Absent | Modeled | `none` |
| Macro | VBA execution | Absent. The bytes stay | Absent | `none` |
| Drawing | Chart, SmartArt, VML, and WordArt editing | Absent. Chart bytes stay | Modeled | `none` |
| Math | OMML editing | Absent. The markup stays | Modeled | `none` |
| Area | Feature | EdgeDoc | WordCraft | Test |
|---|---|---|---|---|
| Story | Comment, footnote, and endnote text | Modeled. Each note and comment is a story part | Modeled | `text-footnote-ref` |
| Protect | Document protection | Kept. The settings part stays | Modeled | `part-settings` |
| Text | Caps, small caps, spacing, kern, and run borders | Modeled | Modeled | `text-rtl` |
| Paragraph | Tab stops, borders, and character indents | Modeled | Modeled | `text-tab` |
| Table | Floating table, table style, and conditional format | Modeled position and style. Conditional format kept | Modeled | `tbl-clamp` |
| Section | Page border, gutter, line number, and text direction | Modeled | Modeled | `sect-headers` |
| Data | Custom XML binding evaluation | Absent. The binding markup stays | Modeled | `none` |
| File | RTF, ODT, HTML, and Markdown | Absent | Modeled | `none` |
| Data | Formula evaluation in a sheet source | Absent. Cached cell values stay | Modeled | `none` |
