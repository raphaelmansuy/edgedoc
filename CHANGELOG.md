# Changelog

All notable changes to this project are recorded here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
This project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.3.1] - 2026-11-14

### Added

- Explicit paragraph selection and document-wide Select All, effective/mixed font state,
  per-run Grow/Shrink and direct character-format reset through the display WASM API.
- Browser viewer export in the npm package (`edgedoc-wasm/viewer`) alongside its matching WASM.
- Page setup and multi-paragraph selection prerequisites previously present only in the
  EdgeBase integration checkout, now covered by the engine release.

### Fixed

- Paragraph alignment, spacing, indentation, lists, shading and borders cover selected paragraphs
  in one Undo transaction and retain the chosen character format for subsequent typing.
- Selected character edits preserve run-size differences, semantic properties and opaque package content.
- Reading-order uniqueness and selected range length inspection avoid repeated full scans/copies.

### Qualification

- Display/WASM engine regression: 256 tests passed in the integration checkout.
- EdgeBase integration: 93 live-stack browser cases passed without retries.
- Native Word review/layout fidelity remains unqualified. This release does not implement Word's
  complete review, reference or advanced layout feature set.

## [0.2.0] - 2026-11-09

### Added

- **Word control in the browser** at `site/word/`: a Word-like ribbon (Home, Insert,
  Layout, Mailings), caret editing on the display stack, merge fields with a side pane,
  record preview without touching the template, navigation headings, find/select menus, and
  download of edited package bytes. Specification: `specs/001-word-control/`.
- Editor session APIs on the display WebAssembly module: `editParaSpacing`,
  `editParaShading`, `editParaBorder`, `editSelectSentence`, extended list and paragraph
  formatting in `edgedoc-display`, backed by `set_space_before`, `set_para_shading`, and
  `set_para_border` on the model.
- `make word-e2e` and `tools/word_control_e2e.mjs`: **41** automated checks and **14**
  golden screenshots under `specs/001-word-control/e2e/screenshots/`.

### Notes

This release also publishes the EdgeDoc Display crates, the Markdown round-trip, the
expanded rendering corpus, and the changes listed under **[Unreleased]** below that
shipped since v0.1.1. Tag `v0.2.0` updates the seven core crates on crates.io, the PyPI
wheel, `edgedoc-wasm` on npm, and GitHub Pages (`display/` and `word/`).

## [Unreleased]

### Added

- Markdown read and write. `Document::to_markdown` exports the body, and
  `Document::from_markdown` reads it back. `open_markdown` reads a UTF-8 file.
  Office Math is LaTeX in the text and OMML in the saved package.
- Python `markdown_to_docx` and `open_markdown`. WebAssembly
  `markdownToDocument` and `markdownToDocumentWithImages`.
- The npm package name is `edgedoc-wasm`. The name `edgedoc` on npm is a
  different project.
- A release workflow publishes the seven core crates, the PyPI wheel, and
  `edgedoc-wasm` from a `v*` tag.

- A rendering parity ledger at `docs/display/20-rendering-parity.md`, with 49
  exercised OOXML regression tests and an explicit inventory of remaining Wordcraft parity work.
- An independent Wordcraft 0.15.1 differential gate with source/font fingerprints, glyph geometry,
  144 dpi pixel comparisons, retained failing cases, and negative tests for the acceptance gate.
- Warnings for known unsupported rendering features, without claiming complete fidelity.
- A corpus of 75 original DOCX documents pinned to Apache POI and docx4j commits, with dual
  checksum verification, structural inventory, bounded layout checks and independent Wordcraft
  geometry/pixel comparisons. Rendered pages from both engines receive manual image inspection.
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
  6.2 MB becomes 1.7 MB. The current display module is approximately 3.54 MB raw and 1.61 MB gzip.
- `docs/display/18-assessment.md` states every WordCraft-measured behavior and its state here:
  50 ported, 22 partial, 4 placeholder, 33 gap.
- `docs/display/19-deep-audit.md` states the deeper audit against WordCraft 0.14.0: a property
  comparison, a map of its 531 layout tests, and the four defects the audit found.

### Fixed

- UTF-8 BOM offsets in preserved XML, style-part property slices, omitted/localized default
  styles, Latin theme fonts, field-result fonts and nonpainting proofing/bookmark markers.
- Positional tabs, measured right/center/decimal tab segments, leading-tab glyph origins,
  decorative leader text, and document-start/explicit-break paragraph spacing.
- All 12 bundled Carlito/Arimo/Tinos faces now load. Supplied compatible faces take precedence
  over built-in substitutes; an unknown family still respects the caller's explicit fallback.
- Paragraph-border painting honors width and distance from text, and a footer's bottom rule
  participates in its bottom placement. Border grouping and full body-flow reservation remain open.
- Documents requesting the observed incomplete Arabic/bidirectional layout now surface a fidelity
  diagnostic; the expanded corpus requires this warning.
- Shared table-cell borders now paint without table-level inside rules. Cell layout reserves the
  effective top and final bottom border widths, and horizontal rules are centered on their edges.
  The inspected Apache POI table-indent document now passes the independent Wordcraft geometry
  and pixel comparison; complex border conflict resolution remains open.
- Row-level content controls in tables now render their rows and retain the wrapper on an edited
  save. An empty table completes without generating pages repeatedly. A simple gridless auto-width
  table sizes its columns from cell content; more complex table autofit remains open.
- Imported page/style defaults, contextual kerning and ligatures, footer and continuous-section spacing,
  and table alignment across compatibility modes now follow inspected Wordcraft behavior.
- SVG and browser Canvas paint loaded-font glyph outlines and layout advances instead of reshaping
  text with a viewer-selected font. Each clipped paragraph receives a unique SVG clip ID.
- Display section ranges, header/footer references and inheritance, story distances and height
  reservation, mirrored margins, section page totals, and section page-number formats.
- Multi-page paragraph and table continuation, inline page and column breaks, line spacing,
  keep-with-next, bottom alignment, and final-page column balancing before continuous sections.
- Layout cache validation, painted baselines and justified glyph positions, numbering XML
  parsing and start overrides, page-budget enforcement, and showing hidden text.
- Column breaks survive an edited DOCX save. Four ineffective gap checks were replaced by
  fixtures that contain and exercise the relevant features.

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
