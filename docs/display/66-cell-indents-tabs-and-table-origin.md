# Cell indents, tabs and table origins

The Word-exported `b3ff2c3238` benchmark is a five-page table. Its original Wordcraft comparison failed: centered cell text differed by up to 6.5 points and the common-ink color error exceeded the 0.05 threshold. Three independent corrections were necessary:

* Ordinary cell paragraphs now use their left, right and first-line indents when calculating the line width and origin. Earlier, only list paragraphs used those indents.
* Center and right alignment leave the part of a line through its last tab anchored, following Wordcraft's `crates/layout/src/para/lines.rs`. A synthetic centered cell with a trailing tab moved from a 76.6-point mismatch to identical glyph origins.
* For modern start-aligned tables, when the first cell has no left border, the origin accounts for half of the table's left border width. Wordcraft uses that same fallback in `crates/layout/src/table.rs`. This resolved a 0.25-point shift across almost every glyph in the sample.

With the installed Word font profile, the new rendering passes the strict Wordcraft comparison on all five pages: maximum glyph-origin error is zero, unmatched ink stays below 0.3%, and mean common-ink color error stays below 0.038. The second page of the Word-exported PDF, Wordcraft render and EdgeDoc render was visually inspected. This supports parity for this example; it is not a general Word parity claim.

Five previously passing Word benchmark cases and two independent table-border regressions also pass with these changes. These seven cases and the newly passing table sample were rerun individually; the remaining benchmark cases have not been rerun after the last table-origin change, so the preceding 5/48 aggregate must not be reported as a current complete-run result. The corpus itself and its hashes are documented in `tests/corpus/WORD-BENCHMARK.md`.
