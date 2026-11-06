# Justified text and list-label kerning

The pinned Ukrainian examination document `30dde38655` exposed a Wordcraft line-breaking rule in compatibility mode 15: a justified line can fit a little more text by shrinking its inter-word spaces. EdgeDoc now permits the same bounded overrun and distributes the reduction among the spaces; it does not apply that rule in older compatibility modes or on tabbed lines. Focused tests exercise all three cases.

The same document exposed a separate label difference: Wordcraft shapes a list label using the resolved paragraph-mark kerning setting, while EdgeDoc had enabled pair kerning for every label. EdgeDoc now carries that setting into label measurement and painting. Tests cover both sides of the configured size threshold.

The document passes the strict three-page Wordcraft comparison with the installed font profile: zero unmatched ink, and glyph origins within 0.001 points. Its first page was inspected in EdgeDoc, Wordcraft, and the supplied Word-exported PDF. The Word PDF still has differences in list indentation and wrapping, so this is not a Word parity claim. The pinned input is identified in `tests/corpus/word-benchmark-manifest.json`.

The complete 48-document benchmark passed 11 and failed 37, with no blocked cases or regressions among earlier passes. All 209 selected previously passing cases passed again. Per-case results and fingerprints are recorded in `word-benchmark-48-justification-shrink-summary.json` and `justification-shrink-regression-209-summary.json`. The Rust workspace passed 404 tests, strict Clippy passed, and the rebuilt browser passed 18 interaction checks and 134 checks across 33 documents.
