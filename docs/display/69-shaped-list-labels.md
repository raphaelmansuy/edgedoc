# Shaped list labels

Wordcraft shapes list labels as text, including the font's kerning adjustments. EdgeDoc previously shaped the labels but then discarded the shaped advances when measuring and painting them. This shifted glyphs in labels such as `11.` and changed the starting position of right-aligned Roman labels.

EdgeDoc now uses the shaped advances consistently when computing the label width, placing following text, and painting each glyph. A regression uses a bundled font and an `AV` pair whose shaped advance differs from the unkerned glyph width, so it fails if the adjustments are discarded.

The pinned four-page Word-exported document `00b4bca293` now passes the strict Wordcraft comparison on every page with the installed Word font profile. Before the fix, its list labels differed by up to 1.19 points; after the fix the maximum measured error is 0.217 points and unmatched ink is zero on all pages. The third page was visually inspected in EdgeDoc, Wordcraft and the Word-exported PDF. This focused result is not a general parity claim. The input is identified by `tests/corpus/word-benchmark-manifest.json`.

After this change, all 209 selected earlier passing regression cases passed again. The full 48-document benchmark passed 9 and failed 39, with no blocked cases; the four-page label document is the additional pass. Results and fingerprints are summarized in `label-kerning-regression-209-summary.json` and `word-benchmark-48-label-kerning-summary.json`. Fresh comparison SVGs were discarded after use to conserve disk space; inspected images and inputs remain available.

The subsequent list-label change in `71-justified-space-shrinking.md` also respects the resolved kerning threshold: shaping retains its advances when kerning is enabled and uses unkerned advances when it is disabled.
