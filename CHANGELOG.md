# Changelog

All notable changes to this project are recorded here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
This project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **EdgeDoc Display: a Word document, painted in the browser.** A new set of crates
  reads a package. It produces page geometry, a display list, and SVG or PDF pages. One
  call loads a document and paints it. The viewer gives zoom, paged scroll, text
  selection, search, and print, in the manner of pdf.js.
  - `edgedoc-geom`: one length type, one rectangle, one colour.
  - `edgedoc-fonts`: the font database, `harfrust` shaping, and the metric-compatible
    substitutes that keep a line break where Word puts it.
  - `edgedoc-text`: grapheme clusters, UAX #9 bidi, UAX #14 breaking with the Word
    exceptions, first-fit lines, justification, and tabs.
  - `edgedoc-style`: the cascade, numbering counters, and computed properties.
  - `edgedoc-flow`: sections, columns, tables, headers, footers, notes, and the pass loop
    that resolves a page total.
  - `edgedoc-paint`: the display list, its binary codec, and four painters (canvas, SVG,
    PDF, recorder).
  - `edgedoc-display`: the facade. `DisplayDocument::open` then `layout` then `page`.
  - `edgedoc-display-wasm`: the browser adapter, with one exported class.
  - A demo at `site/display/` with a headless browser test at `tools/display_e2e.mjs`.
- `assets/fonts/` carries Carlito, Arimo, and Tinos under the SIL Open Font License, so a
  document paints offline with no font fetch.
- The display specification in `docs/display/`: 661 requirements, 22 sequence and state
  diagrams, 13 wireframes, 170 edge cases, and a port map against WordCraft 0.14.0.
- Symbol fonts (Symbol, Wingdings 2 and 3, Webdings) map their 8-bit codes to Unicode, so a
  bullet or a tick paints from an installed face.
- Paragraph spacing is the larger of the space after and the space before, which is the Word
  measurement. A 12 pt after and a 12 pt before leave 12 pt, not 24 pt.
- An indent in characters (`w:leftChars`, `w:firstLineChars`) is read.
- No line break follows a slash, and a break follows a hyphen before a digit.
- `assets/fonts/subset/` holds the bundled faces cut to the characters a business document uses:
  6.2 MB becomes 1.7 MB, and the module is 4.11 MB raw and 1.40 MB gzip.
- `docs/display/18-assessment.md` states every WordCraft-measured behavior and its state here:
  50 ported, 22 partial, 4 placeholder, 33 gap.
- `docs/display/19-deep-audit.md` states the deeper audit against WordCraft 0.14.0: a property
  comparison, a map of its 531 layout tests, and the four defects the audit found.

### Fixed

- **A Word document with whitespace after the root element did not open.** Word writes a newline
  after the last close tag of most parts. The package parser read that newline as a second root.
  12 of the 33 corpus documents failed to open, and every synthetic document passed. The parser
  now ignores text outside the root element.

- A line broke inside a word, so `smallest base.` painted as `smallest ba` and `se.`. Every
  character was a break opportunity, because a break point was read as "after any earlier point".
  The test `a_line_ends_at_a_word_boundary` fails if this returns.
- Every bold run painted with the regular face. The face table was keyed by family alone, so the
  last face of a family won. It is now keyed by family and style.
- A character style resolved as a paragraph style, so its font and colour were ignored.
- A painter ignored the weight of the face, so a bold run drew at weight 400.
- A corpus of 33 real Word documents from the python-docx project, in `tests/corpus/downloaded`,
  with `tools/fetch_corpus.sh`, `make corpus`, and `make corpus-e2e`. It is the only proof that
  the reader understands a file this library did not write.
- `make display-wasm`, `make display-e2e`, `make display-serve`, `make corpus`,
  `make corpus-report`, and `make corpus-e2e`.

### Changed

- `Document::part_blocks` returns the blocks of one story part, which a header needs.
- `edgedoc_model::Twips` is re-exported, so a caller needs one import.
- `clippy.toml` allows `unwrap` and `expect` in a test, and library code still denies them.

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
