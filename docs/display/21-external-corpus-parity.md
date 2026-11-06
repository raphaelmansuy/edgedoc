# External DOCX corpus and rendered-page inspection

Updated: 2026-11-05. Acceptance reference: the unmodified Wordcraft 0.15.1 checkout at
`1c7b5013fab765f204cf435b9f8814b46b7c1278`.

This chapter records the earlier 143-case run. A subsequent expansion added 181 pinned upstream
fixtures and fixes for explicit table cell borders and border spacing. See the
[expanded corpus results](22-expanded-corpus-parity.md) for the completed 327-case run.
The [new table image inspection](table-border-image-inspection.json) records the rerendered
Wordcraft and EdgeDoc pages after these fixes. The previously failing `poi-31-table-indent`
case now passes its independent glyph, geometry and pixel comparison.

**Full Wordcraft rendering parity is not established.** The expanded gate contains 143 cases:
82 pass, 60 fail, and one is blocked by the XML depth limits of both engines. All 39 previously
passing cases still pass. Every earlier input and failure remains in the gate.

The [before/after snapshot](external-corpus-parity-result.json) records every case, input hash,
source URL, diagnostic, failure category, engine/adapter fingerprint, font profile and report hash.
The [manual image-inspection record](wordcraft-image-inspection.json) records observations and
SVG/PNG hashes for 11 page pairs, comprising 22 engine page images.

## Downloaded inputs

The [corpus manifest](../../tests/corpus/external-manifest.json) pins 75 original DOCX files from
Apache POI and docx4j to upstream commits. The files contain 303,227 text characters and total
6,726,848 compressed bytes. Each download is verified against SHA-256 and upstream Git blob SHA-1.
Original bytes stay under `target/external-docx/`; test inputs are not rewritten to improve scores.
The [inventory and download instructions](../../tests/corpus/EXTERNAL.md) describe provenance,
archive budgets and measured document structure.

This includes `Docx4j_GettingStarted.docx`, with 1,285 paragraphs, nine tables, four note references,
76 field-instruction elements and 601 tabs. Both engines produce 45 pages from it. It still fails:
the inspected pages show missing TOC content, different wrapping, spacing and page boundaries.
Its matching page count is not rendering-parity evidence.

## Measured comparison

| Scope | Before | After |
|---|---|---|
| Original 42 cases | 39 pass, 3 fail | 39 pass, 3 fail |
| Downloaded 75 files | 0 pass, 72 fail, 3 blocked | 20 pass, 54 fail, 1 blocked |
| Added regression inputs | Not present | 23 pass, 3 fail |
| Total | 117 cases | 143 cases, 141 distinct DOCX inputs |

The before state included spurious inactive-grid and metadata warnings as well as real geometry
and import failures. These are acceptance-gate outcomes, not percentages of features implemented.
Two valid BOM-prefixed files that previously failed to open now reach layout and retain their
remaining rendering failures. The deliberately deep-table file independently returns EdgeDoc
`xml.limit` and Wordcraft `Limit("XML nesting too deep")`; the 256-level limits remain unchanged.

The 82 accepted cases span 121 pages, including 34 pages from the 20 downloaded documents.
Maximum accepted glyph-origin error is about 0.000031 pt. No ink is unmatched outside the
one-pixel neighborhood; maximum common-ink color error is 0.001795. The gate allows 0.5 pt origins,
0.05 pt font sizes, 1% unmatched ink within that neighborhood, and 0.05 color error at 144 dpi.
Text, whitespace, line grouping, page counts/sizes, glyph IDs, actual font bytes and styles must
also match. Images receive no alignment, registration or cropping correction.

Nine failing cases expose reference-adapter limitations and 14 use reference font bytes outside
the controlled supplied profile. These categories can overlap with product mismatches. They
remain failures and must not be counted as proved renderer defects or accepted parity cases.
The adapter now exports raster pictures with crop/opacity and verifies one-to-one source mappings
for numbering labels. Unsupported shapes, transforms, strokes, image effects and unverified text
mappings still prevent acceptance.

Raw evidence is in `target/wordcraft-parity/expanded-before/` and
`target/wordcraft-parity/expanded-final/`: original DOCX, independent glyph JSON, SVG/PNG pages,
red/blue difference images and `comparison.json`. The final run reuses unchanged Wordcraft outputs
only after checking input bytes, reference/adapter source, all supplied font paths/bytes/styles
and every reference SVG. It rerenders EdgeDoc and reruns geometry and pixels for every case.

## Implemented fixes

Wordcraft implementation references included `docx/src/read/styles.rs`, `fonts/src/word.rs`,
`layout/src/para/tabs.rs`, `layout/src/boxes.rs` and `layout/src/display/page.rs`. Paths are relative
to its `crates/` directory. The reference checkout is unchanged.

| Area | Change exercised by the downloaded files and focused regressions |
|---|---|
| XML preservation | UTF-8 BOM offsets retain correct raw spans. Style extensions use the styles part's bytes. |
| Defaults and themes | Apply omitted/localized default paragraph styles and resolve Latin major/minor theme fonts; edited saves retain theme attributes. |
| Font loading | Load all 12 bundled Carlito, Arimo and Tinos faces. Supplied compatible fonts take precedence over built-in substitutes; explicit fallback selection remains respected. |
| Nonpainting markup | Proofing, permission and bookmark markers retain their model data without inserting visible object widths. |
| Document grid | Omitted/default grid types no longer produce a false unsupported warning; active grid requests remain diagnosed and failed. |
| Tabs | Read/write positional tabs, measure following segments for right/center/decimal stops, honor margin/indent-relative targets, and preserve leading-tab glyph origins across style changes. The decimal case still differs from this Wordcraft revision. |
| Fields | Use actual result text and its first result-run formatting; XML indentation no longer enters simple field results. |
| Paragraph spacing | Retain document-start spacing and distinguish explicit break behavior across compatibility modes. |
| Leaders | Paint decorative leaders without adding dot strings to selectable text. |
| Borders | Paint paragraph rules with their requested width and distance outside the text. Footer bottom rules participate in bottom placement. Full body-flow reservation, inherited/grouped borders and complex patterns remain open. |
| Fidelity reporting | The visibly incorrect Arabic/bidirectional layout now produces an explicit unsupported-layout diagnostic, with a corpus regression assertion. |

## Manual image inspection

The rendered Wordcraft display-list pages and EdgeDoc public SVG pages were inspected directly,
with selected difference images. The corrected `FancyFoot` rule matches visually on both pages;
its complete case also passes the strict gate. First/default header/footer variants match, and
their inspected PNGs are byte-identical. The guide's final code line now stays contiguous after
the leading-tab correction. These observations do not waive remaining gate failures.

| Document / page | Visual assessment | Rendered images |
|---|---|---|
| `poi-06-fancyfoot`, p. 1 | visually matched after fix | [Wordcraft](../../target/corpus-inspection/visual-final/poi-06-fancyfoot/wordcraft-page-001.png), [EdgeDoc](../../target/corpus-inspection/visual-final/poi-06-fancyfoot/edgedoc-page-001.png) |
| `poi-06-fancyfoot`, p. 2 | visually matched after fix | [Wordcraft](../../target/corpus-inspection/visual-final/poi-06-fancyfoot/wordcraft-page-002.png), [EdgeDoc](../../target/corpus-inspection/visual-final/poi-06-fancyfoot/edgedoc-page-002.png) |
| `poi-05-difffirstpageheadfoot`, p. 1 | visually matched | [Wordcraft](../../target/corpus-inspection/visual-final/poi-05-difffirstpageheadfoot/wordcraft-page-001.png), [EdgeDoc](../../target/corpus-inspection/visual-final/poi-05-difffirstpageheadfoot/edgedoc-page-001.png) |
| `poi-05-difffirstpageheadfoot`, p. 2 | visually matched | [Wordcraft](../../target/corpus-inspection/visual-final/poi-05-difffirstpageheadfoot/wordcraft-page-002.png), [EdgeDoc](../../target/corpus-inspection/visual-final/poi-05-difffirstpageheadfoot/edgedoc-page-002.png) |
| `poi-31-table-indent`, p. 1 | visible differences remain | [Wordcraft](../../target/corpus-inspection/visual-final/poi-31-table-indent/wordcraft-page-001.png), [EdgeDoc](../../target/corpus-inspection/visual-final/poi-31-table-indent/edgedoc-page-001.png) |
| `poi-39-variouspictures`, p. 1 | visible differences remain | [Wordcraft](../../target/corpus-inspection/visual-final/poi-39-variouspictures/wordcraft-page-001.png), [EdgeDoc](../../target/corpus-inspection/visual-final/poi-39-variouspictures/edgedoc-page-001.png) |
| `poi-01-complexnumberedlists`, p. 1 | visible differences remain | [Wordcraft](../../target/corpus-inspection/visual-final/poi-01-complexnumberedlists/wordcraft-page-001.png), [EdgeDoc](../../target/corpus-inspection/visual-final/poi-01-complexnumberedlists/edgedoc-page-001.png) |
| `docx4j-01-docx4j_gettingstarted`, p. 1 | visible differences remain | [Wordcraft](../../target/corpus-inspection/visual-final/docx4j-01-docx4j_gettingstarted/wordcraft-page-001.png), [EdgeDoc](../../target/corpus-inspection/visual-final/docx4j-01-docx4j_gettingstarted/edgedoc-page-001.png) |
| `docx4j-01-docx4j_gettingstarted`, p. 22 | visible differences remain | [Wordcraft](../../target/corpus-inspection/visual-final/docx4j-01-docx4j_gettingstarted/wordcraft-page-022.png), [EdgeDoc](../../target/corpus-inspection/visual-final/docx4j-01-docx4j_gettingstarted/edgedoc-page-022.png) |
| `docx4j-01-docx4j_gettingstarted`, p. 45 | visible differences remain | [Wordcraft](../../target/corpus-inspection/visual-final/docx4j-01-docx4j_gettingstarted/wordcraft-page-045.png), [EdgeDoc](../../target/corpus-inspection/visual-final/docx4j-01-docx4j_gettingstarted/edgedoc-page-045.png) |
| `docx4j-26-arabic_numbering`, p. 1 | visible differences remain | [Wordcraft](../../target/corpus-inspection/visual-final/docx4j-26-arabic_numbering/wordcraft-page-001.png), [EdgeDoc](../../target/corpus-inspection/visual-final/docx4j-26-arabic_numbering/edgedoc-page-001.png) |

The other inspected cases still show material differences: table cell rules and row geometry,
list-label/text indents, picture placement, TOC content/formatting, paragraph spacing and Arabic
alignment/order. The guide's first, middle and final pages demonstrate why a 45-page count alone
would conceal substantial differences.

To regenerate selected page pairs from current, source-checked evidence:

```sh
python3 tools/prepare_wordcraft_inspection.py target/wordcraft-parity/expanded-final \
  target/corpus-inspection/visual-final \
  poi-06-fancyfoot:1 poi-06-fancyfoot:2 \
  docx4j-01-docx4j_gettingstarted:1 docx4j-01-docx4j_gettingstarted:22 \
  docx4j-01-docx4j_gettingstarted:45 docx4j-26-arabic_numbering:1
```

This command prepares images and hashes. It does not make the manual visual judgment.

## Validation and remaining work

The 46 focused rendering contracts pass. All 297 workspace tests pass, Clippy denies warnings,
formatting and diff whitespace checks pass, and 13 gate tests plus five downloader integrity tests
pass. The opt-in external test opens/layouts 74 files and requires the expected bounded rejection
of the deep-table file. The rebuilt WebAssembly viewer passes 20 checks, including Canvas/SVG
outline comparison, and 128 checks over the earlier 33-file browser corpus. Those smoke checks
are separate from independent rendering parity.

The full differential gate intentionally exits nonzero. Next implementation work remains TOC and
nested field content/style handling; paragraph/list/tab geometry; table border conflict resolution,
row metrics, styles and merges; picture placement/wrap; notes and text boxes; complex-script fonts,
shaping and bidi placement. The retained synthetic failures also cover notes/drop caps, active
document grids, Roman list-label placement, decimal stops and a body PAGE-format difference.
The [earlier inventory](20-rendering-parity.md) remains applicable to other untested feature
combinations. Neither these fixes nor the passing subset establishes complete Wordcraft parity.
