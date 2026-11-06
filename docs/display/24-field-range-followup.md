# Field-range follow-up to the Wordcraft parity audit

> This is a historical field baseline. See [the tab and border follow-up](25-tabs-and-borders-followup.md) for later results.

This follow-up compares EdgeDoc with the clean local Wordcraft checkout at commit
`1c7b5013fab765f204cf435b9f8814b46b7c1278`. The earlier
[code audit](23-wordcraft-code-parity-audit.md) is a historical baseline, not a claim about
current source. Microsoft Word itself was not the comparison engine.

## Changes and limits

The WML reader now tracks bounded nested fields across paragraphs. TOC and ADDIN results use
explicit start and end objects, leaving their cached result runs and nested page-reference
fields visible. Complex fields inside another ordinary field contribute their cached result to
its parent; tabs and line breaks retain their order. The writer emits range boundaries and
serializes cached result tabs and breaks as Word elements. Edited-save tests cover a TOC spanning
paragraphs, nested fields and cached tab/line-break content.

For display, EdgeDoc suppresses the built-in Hyperlink character style on linked runs inside
TOC ranges, following Wordcraft's `crates/docx/src/read/toc.rs`. This adjustment runs on a copy
of the document; it retains targets and direct formatting and does not rewrite the stored style.
The test covers a localized Hyperlink style ID, a range spanning paragraphs, and links outside
that range.

These changes do not implement general field evaluation, preserve all nested field codes, or
prove arbitrary malformed/multi-story field structures. They do not fix TOC tab-stop wrapping,
heading formatting, title borders or general pagination. The field diagnostic remains active.
Other known open categories include notes, floats, complex tables, drawing effects, fonts and
mixed-direction text. See the historical audit for detailed source comparisons.

## Visual inspection

The [image inspection record](field-range-image-inspection.json) identifies the exact input,
reference and build hashes and both 144-dpi images. On the first page of the docx4j getting
started guide the previously missing “What is docx4j?” entry is present and TOC links are black
as in Wordcraft. The indented tab leaders and page numbers still wrap differently, the title rule
is missing, and spacing differs. On the first page of the declaration TOC, link colors now match,
but the title rule, Contents heading, spacing and body flow do not. Neither is a visual pass.

## Verification

The Rust workspace suite passed 306 tests; strict workspace Clippy, formatting, the 13 parity-driver
tests and six corpus-downloader tests also passed. The browser viewer passed 18 checks, and the browser corpus passed 128
checks across 33 documents. Those tests exercise behavior; they do not prove Wordcraft parity.

The [final 327-case differential report](field-final-parity-result.json) records **123 passed,
185 failed and 19 blocked**. The statuses are unchanged from the previous baseline. It retains
all 256 external inputs, with 60 passed, 177 failed and 19 blocked. The harness reused reference
artifacts only after checking input, reference and font hashes; EdgeDoc was rerendered. A pass is
bounded by the report's stated glyph, geometry, font and raster tolerances, and the corpus is not
exhaustive. Adapter limitations, unsupported diagnostics, textless fixtures and failed renders
remain explicit rather than being counted as parity.
