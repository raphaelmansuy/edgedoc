# Changelog

All notable changes to this project are recorded here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
This project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- The document model now carries the full character, paragraph, section, and table
  property set, so a save keeps formatting that the reader finds.
- Numbering definitions for generated bullet and number lists. A new definition
  never reuses the id of a definition that the file already holds.
- Footnotes, endnotes, and comments as story parts that you can read and extend.
- Styles of every kind, with the original style element kept for table styles and
  conditional formatting.
- Inline and floating pictures, with size, alternative text, and wrap settings.
- `DocumentBuilder::header`, `footer`, `header_first`, and `footer_first`.
- `BodyCursor::table_at` to change a table in place.
- Twelve example documents: invoices, a quarterly report, letters, and a directory.
- `docs/11-format-coverage.md` and `docs/12-word-features.md`, each locked by a test.
- A `Makefile` with one target for each task.

### Fixed

- Every drawing now carries `wp:docPr`, which ECMA-376 makes mandatory. A file
  without it failed in Word with "Word experienced an error trying to open the file",
  while WordCraft and python-docx opened it without complaint. One counter keeps the
  drawing ids unique across the package, and a floating drawing gains the
  `wp:effectExtent` that Word writes.
- The build now validates every generated document against `wml.xsd`, so a package
  that Word would refuse fails the tests.

- A merge field result longer than its placeholder truncated the saved text, because
  the run length was not updated.
- A header picture wrote its media part twice, which made the package invalid.
- An unknown element inside a run was dropped on save.

## [0.1.0] - 2026-11-04

First release.

- Read, create, edit, merge, and save Word packages.
- WordprocessingML reader and writer with a Transitional output.
- Mail merge with CSV records, filters, sort keys, and value maps.
- Python and WebAssembly adapters over the same core.
