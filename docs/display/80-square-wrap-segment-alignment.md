# Alignment in clear bands around a square float

For the bounded margin-anchored square-picture case, EdgeDoc now lets left-, center-, and right-aligned paragraphs use both clear bands on a row. Center and right alignment are applied to each band separately, following Wordcraft. General floating-picture support remains diagnosed; justified and bidirectional text, other anchor types, and other wrap modes need additional work.

Controlled centered and right-aligned paragraph variants were rendered using the same font files in EdgeDoc and Wordcraft. Both page images match with zero unmatched ink and no glyph-position error; the centered pair was inspected. A focused geometry test verifies that each band is independently centered or right-aligned. The fixtures are generated in the ignored `target/wordcraft-parity/square-wrap-alignment` directory from the project's inline-picture test document.

The 48-document benchmark recorded 12 passes, 34 failures, and 2 blocked Wordcraft reference timeouts. All 209 selected previous passes passed again. Exact-source summaries are in `word-benchmark-48-float-align-summary.json` and `float-align-regression-209-summary.json`. The workspace passed 415 tests and strict Clippy; the rebuilt browser passed 18 interaction checks and 134 checks across 33 documents.
