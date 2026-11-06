# Additional underline patterns

EdgeDoc now carries the resolved underline pattern through its style cascade and paints Wordcraft-compatible dotted, dashed, thick, and word-only underlines. Word-only underlines leave spaces clear. Wordcraft maps OOXML's heavy and compound dash variants to the same dotted or dashed geometry; EdgeDoc follows that mapping. Other unsupported variants, including wave underlines, continue to report a diagnostic.

Four controlled DOCX variants were generated from the pinned `2559791938` application form, changing its double-underlined title to one of these four patterns. Each was rendered with the same installed font files by EdgeDoc and Wordcraft. All four passed the strict glyph and raster comparison with zero unmatched ink. Paired close-up images were also inspected at `target/wordcraft-parity/underline-patterns-reference/underline-inspection.png`. This validates those fixtures, not every document or underline combination.

Focused rendering tests verify the dot and dash spacing and that a word-only underline does not paint across a space. The original document is identified by `tests/corpus/word-benchmark-manifest.json`; the generated variants and reference images remain in the ignored `target` directory.

The 48-document benchmark recorded 12 passes, 34 failures, and 2 blocked reference timeouts; the 209 selected previous passes all passed again. Exact-source summaries are in `word-benchmark-48-underline-pattern-summary.json` and `underline-pattern-regression-209-summary.json`. The workspace passed 408 tests and strict Clippy; the rebuilt browser passed 18 interaction checks and 134 checks across 33 documents.

The later change in `74-wavy-underlines.md` adds the wavy variants described above as unsupported at the time of this assessment.
