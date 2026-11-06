# Rendering parity: verified behavior and remaining work

Updated: 2026-11-05. This chapter supersedes the implementation status and counts in
[18-assessment.md](18-assessment.md) and [19-deep-audit.md](19-deep-audit.md).
Those chapters remain historical evidence, not a current parity claim.

The subsequent [external-corpus inspection](21-external-corpus-parity.md) expands the gate to
75 downloaded POI/docx4j files and records the current before/after result and manual page-image
inspection. The fixture-only counts near the end of this chapter are historical snapshots.

**The acceptance reference is Wordcraft. Complete rendering parity is not yet established.** Keeping a DOCX property
through a save does not prove that the display engine renders it. Passing the included corpus
does not prove that every Wordcraft feature works, either. No pixel comparison against Microsoft Word
was performed in this change.

## Reference and method

The reference checkout is `../wordcraft`, version 0.15.1, commit
`1c7b5013fab765f204cf435b9f8814b46b7c1278`. Relevant implementation examples are
`crates/layout/src/tests/pages.rs` (margins and story geometry), `crates/layout/src/headers.rs`
(header inheritance), `crates/layout/src/fields.rs` (section fields), and
`crates/layout/src/pass.rs` (bounded column balancing with state restoration).

The new [rendering_parity.rs](../../crates/edgedoc-display/tests/rendering_parity.rs) suite builds
small OOXML packages, reads them through the public facade, and checks geometry and painted
output. Its first ten cases all failed before the corresponding fixes. Additional checks exposed
truncated headers, misplaced footers, and paint positions that disagreed with line layout.

Four old gap tests were removed: their samples did not contain `SECTIONPAGES`, mirror margins,
footnotes, or an exercised column-balancing case. They could pass without the named behavior.
The first-page footer assertion in `fidelity.rs` now checks every numbered footer; the report's
cover correctly uses its separate unnumbered first-page footer.

## Implemented and exercised

The regression suite contains 46 passing tests, with no ignored cases. These are EdgeDoc
contracts; differential results are reported separately below.

| Area | Verified behavior |
|---|---|
| Section ranges | Paragraph `sectPr` describes the section ending there; the final body `sectPr` describes the final range. Landscape and portrait sizes reach the correct pages. Content-control wrappers do not change section boundaries. |
| Header and footer references | Resolve by model part ID, independently for default, first, and even variants. Missing references inherit from the preceding section. `titlePg` and document `evenAndOddHeaders` control selection. |
| Story geometry | Header distance is measured from the top edge and footer distance from the bottom edge. All paragraphs in a tall story are measured before body placement; the body reserves their height. |
| Mirror margins | Even pages place the outside margin on the left and the inside margin plus gutter on the right. Explicit `mirrorMargins=0` leaves margins unchanged. |
| Page and section fields | Exact command matching distinguishes `PAGE`, `NUMPAGES`, `SECTION`, and `SECTIONPAGES`. Section totals participate in bounded convergence, including sections sharing a page. `PAGE` uses the section number format. |
| Continuation | Line and row cursors survive a new page. A 90-line paragraph paints each line once. A multi-page table paints each body row once and reserves room for its repeated header. |
| Breaks | Inline page and column breaks end a flow fragment, while retaining subsequent text. An edited DOCX save retains a column break as `w:br type="column"`. |
| Paragraph geometry | Exact, at-least, and multiple line spacing reach pagination and paint. Bottom alignment reaches the body bottom. Keep-with-next moves a heading with its following paragraph. |
| Shape cache | Cache hits validate text, width, run ranges, complete run and paragraph properties, and object extents. Identical text with different alignment or line spacing receives distinct geometry. |
| Document defaults | Read document default character and paragraph properties through the existing WML readers. Omitted imports use Wordcraft’s Letter page, 12 pt Calibri-compatible font, 8 pt paragraph spacing and 1.15 line spacing; a present styles part uses its explicit defaults or a 10 pt fallback. Raw markup remains preserved. If an edited save creates a styles part, the import defaults become explicit so reopening retains the same formatting. |
| Contextual shaping | Shape adjacent text together, retaining shaped advances and clusters through paint. Kerning follows its font-size threshold. Ligatures and text joining survive grapheme boundaries. Advanced typography switches remain open. |
| Glyph export and viewer | SVG and Canvas paint the actual loaded-font outlines and glyph IDs. An invisible text layer keeps the text readable. The viewer caches glyph paths and clears them on each document load. |
| Table compatibility | Before compatibility mode 15, align the first cell’s text to the start margin; mode 15 aligns the table edge. Cell margins inherit the table defaults. Conditional styles and complex table layout remain open. |
| Paint positions | A baseline includes its line offset once. Glyph advances follow the layout positions, including justified spaces. Exact line spacing clips paragraph ink, with a distinct SVG clip ID for each paragraph. Fixed line baselines use the descent; extra multiple leading stays below the natural baseline. |
| Numbering input | Parse numbering elements and attributes through the XML reader, including concrete definitions, formats, starts, and level start overrides. A lower-Roman list starting at seven paints `vii.` and `viii.`. |
| Continuous sections | Compatible page sizes retain the current physical page. A changed column set begins below the preceding content. |
| Column balancing | Before a continuous section, reflow only the final page with a bounded height search and restored list/cursor state. Both short sections and a paragraph continued from an earlier page retain all their text. Explicit breaks retain their placement. |
| Public options | The page budget applies during a section and between sections. Showing hidden text changes shaping as well as painting. Relayout replaces diagnostics rather than accumulating copies. |
| Fidelity reporting | Known unsupported requests produce `display.unsupported_layout` warnings. Unknown diagnostic codes no longer claim that the document is complete. |

These are focused regression contracts, not assertions about every combination of these features.
For example, a field's explicit formatting switches and locked/cached-result rules still need
separate coverage; section formatting alone does not establish those rules.

## Remaining inventory

The following rows retain all 29 gaps from the earlier audit. "Open" means full behavior has not
been established. A warning exposes a limitation; it does not close it.

| Audit # | Feature | Current state and next work |
|---|---|---|
| 1 | Floating pictures and square wrap | Open. Connect model anchors to page placement and exclusions; cover anchor movement and long words beside floats. |
| 2 | Note continuation | Open. Body references are not connected to fully measured note stories; implement reservation, splitting, and continuation separators. |
| 3 | Note numbering | Open. Implement continuous, per-section, and per-page numbering after note placement. |
| 4 | Continuous sections with column changes | Implemented for compatible page sizes; regression cases cover shared pages and changed column widths. Different page sizes start a new page. Complex margin changes still need coverage. |
| 5 | Conditional table formats | Open. Read and cascade conditional styles, band sizes, and edge/corner rules. |
| 6 | CJK numbering | Open. Port counting systems and verify glyph fallback and mixed-format multi-level labels. |
| 7 | Circled/enclosed numbers | Open. Implement the format tables and out-of-range fallback. |
| 8 | Picture bullets | Open. Resolve numbering picture resources and their metrics. |
| 9 | Running-title and reference fields | Open. `STYLEREF`, `PAGEREF`, `REF`, `SEQ`, and `TOC` use cached results. |
| 10 | Section page totals | Implemented. Each section counts the physical pages it occupies, including shared pages. |
| 11 | Text boxes | Open. Connect text-box stories, nested containers, drawing placement, and linked overflow. |
| 12 | Drop caps | Open. Frame markup survives the package but does not determine text layout. |
| 13 | Ruby text | Open. Implement annotation metrics and placement above the base. |
| 14 | Vertical text | Open. Implement vertical section, paragraph, cell, and run placement. |
| 15 | Ligature switches | Open. Contextual run shaping now retains ligatures and kerning. Explicit ligature switches, script-specific features, fallback segmentation and broader complex-script parity still need coverage. |
| 16 | Automatic paragraph spacing | Open. Read and cascade before/after automatic spacing. |
| 17 | Document grid | Open. Apply line pitch, character pitch, and per-paragraph/run snapping rules. |
| 18 | CJK/Latin automatic spacing | Open. Resolve per-paragraph settings and script boundaries. |
| 19 | Mirror margins | Implemented for the body and header/footer bands; inside/outside floating anchors remain part of row 1. |
| 20 | Final-column balancing | Implemented with bounded final-page retries. Broader float/note combinations depend on rows 1–3. |
| 21 | Page borders and color | Page borders paint from `w:pgBorders` on each edge. Page background colour is still not a section property. |
| 22 | Line numbering | Implemented. Body lines count with `countBy`, `start`, `distance`, `suppressLineNumbers`, and `newPage` / `newSection` / `continuous` restart. |
| 23 | Revision author colors | Open. Implement author-specific insert/delete markup and display choices. |
| 24 | Color glyphs | Open. Render COLR, sbix, and bitmap color glyphs consistently in each painter. |
| 25 | Obfuscated embedded fonts | Open. Deobfuscate `.odttf`, enforce font embedding rules, and use the embedded face consistently. |
| 26 | Automatic hyphenation | Open. Dictionary hyphenation is not connected; requesting the public option produces a diagnostic. |
| 27 | Thai distributed alignment | Open. Stretch at valid Thai cluster boundaries. |
| 28 | Missing private-use bullets | Open. Use a level-appropriate visible fallback when no font has the bullet. |
| 29 | Positional tabs | Implemented for exercised margin/indent-relative positions and leaders. Ordinary right/center stops and leading tabs have regression coverage. Decimal-stop behavior still differs from this Wordcraft revision; broader wrap/bidi combinations remain open. |

The inventory above is not exhaustive Wordcraft feature coverage. Additional open work includes
split table rows, cell spacing, table autofit to measured content, inline picture
placement within text, drawings/charts/equations, full shaping and bidi behavior, and compatibility
settings. Character shadow, outline, emboss, imprint, and run borders paint with the run. Vertical
page justification (`w:vAlign` `both`) spreads body blocks. Comment margin stories are still open. The older assessment has further partial behaviors in these areas.

The remaining implementation areas include floating placement and wrap, measured note stories and
continuation, conditional table styles and merges, text boxes and advanced shaping. Each needs an
exercised regression fixture and independent Wordcraft comparison before it can close.

## Validation

The browser viewer suite passes 20/20 checks, including a Canvas/SVG pixel comparison for
contextual kerning and ligatures. The browser corpus suite passes 128/128 checks over
33 external DOCX files (24 with visible text and nine structural fixtures). The rebuilt WebAssembly
module compiles. Browser screenshots were inspected for page layout, tables, typography, and
diagnostics. These suites check the available fixtures. Independent Wordcraft geometry and pixel results use
the separate gate described below.

Use the repository's normal Rust gates and the browser suites:

```sh
CARGO_TARGET_DIR=target cargo test --workspace
CARGO_TARGET_DIR=target cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
make display-wasm
DISPLAY_E2E_OUT=target/parity-review/browser node tools/display_e2e.mjs
DISPLAY_E2E_OUT=target/parity-review/corpus node tools/display_corpus_e2e.mjs
```

`PUPPETEER_PATH` and `CHROME_PATH` select local browser dependencies. `DISPLAY_E2E_OUT` keeps
generated evidence outside the committed historical screenshots. Test reports and inspected
screenshots from this run are in `target/parity-review/` and the `target/parity-*.log` files.


## Independent Wordcraft gate

The [runner instructions](../../tools/wordcraft-parity/README.md) describe the reference adapter,
font controls and reproducible commands. `tools/wordcraft_parity.py` runs the unmodified Wordcraft
reader, layout and actual page display list independently of EdgeDoc. It compares glyph origins,
source text and whitespace, glyph IDs, actual font bytes, styles, page counts and page dimensions.
Both SVGs are rasterized at 144 dpi without registration or alignment correction. A case must pass
both geometry and pixel gates. Missing/stale evidence, unsupported rendering and failures return a
nonzero status. Thirteen gate tests exercise missing/changed evidence, shifted glyphs, stale
build inputs. Metric-compatible font substitution is allowed only when both engines use the same
actual bytes and glyph IDs; the warning remains in the report.

The earlier [result snapshot](wordcraft-parity-result.json) records **39 passing and three failing
cases**, from 42 fixture cases containing 40 distinct DOCX inputs. Across the 61 passing pages,
maximum glyph-origin error is **0 pt**, and no ink is unmatched outside the one-pixel neighborhood.
These are historical fixture results, not a percentage of Wordcraft features implemented. In that run, all 280 Rust tests,
Clippy with warnings denied, formatting, eight negative gate tests, 20 browser checks and 128 corpus
checks passed. Canvas and SVG also had zero unmatched ink and zero color error on the ligature fixture.

The remaining differential failures are:

- `notes-dropcap`: note references and note stories are not placed as Wordcraft places them, and
  the paragraph's drop cap is not laid out.
- `known_fidelity_gaps...`: the standalone drop-cap fixture remains unsupported and diagnosed.
- `section_fields...`: EdgeDoc applies the section's upper-Roman format to the body PAGE field;
  this Wordcraft revision paints a decimal body field. Wordcraft assigns its section page format
  in its header context. The difference remains a failed case, rather than being hidden or
  changing the existing EdgeDoc body-field contract solely to make this fixture pass.

The full local run is `target/wordcraft-parity/run-03/comparison.json`. It includes successful and failing
cases; it is not a complete parity certificate. The known note/drop-cap fixtures remain in the gate
so unsupported features cannot disappear from the acceptance result. Detailed source fingerprints,
thresholds and per-case results are preserved with the rendered pages and diff images.
