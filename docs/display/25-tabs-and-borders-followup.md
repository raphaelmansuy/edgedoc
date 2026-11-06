# Tab and paragraph-border follow-up

This is a rendering comparison with the clean Wordcraft checkout at commit
`1c7b5013fab765f204cf435b9f8814b46b7c1278`, not a direct test against Microsoft Word.

EdgeDoc now resolves explicit and default tab positions against the column margin when text is
indented. The paragraph style cascade preserves inherited border sets when later properties omit
them, and replaces the inherited set when a border set is explicitly supplied, including an empty
set. Explicit empty sets survive edited saves. Top and bottom border width and distance also occupy
space in body flow. This does not yet cover every border grouping, table, header or pagination case.

The [333-case result](tab-border-parity-result.json) has 130 passed, 184 failed and 19 blocked.
All six added focused fixtures pass the independent comparison. Against the retained 327-case
baseline, `docx4j-extra-20ef77f77b6b` changed from failed to passed; no earlier pass changed to a
failure. These counts are a bounded corpus result, not a feature-completeness percentage.

The [image inspection](tab-border-image-inspection.json) records input and image hashes. On the
first page of the docx4j getting-started guide, all 31 lines match the reference text and glyph
origins, and raster unmatched ink is zero at 144 dpi. This is a page-level result: later pages
still have differences, and unsupported fields, notes and reference-adapter primitives prevent a
document-level pass. In the declaration sample, the title rule appears, but the TOC heading is
still italic and paragraph flow differs.

The Rust workspace suite passed 310 tests, strict Clippy and formatting passed, and browser tests
passed 18 viewer checks and 128 corpus checks over 33 documents.
