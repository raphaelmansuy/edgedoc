# Expanded corpus: Wordcraft rendering results

Updated 2026-11-05. **Full Wordcraft parity remains unproven.** This run uses the unchanged
Wordcraft 0.15.1 checkout at `1c7b5013fab765f204cf435b9f8814b46b7c1278` as the reference.
It retains all earlier cases and adds more upstream documents and regression inputs.

The combined corpus now has 256 pinned upstream inputs: the original 75 plus 181 additional
Apache POI and docx4j fixtures. The additional set includes 164 bounded DOCX archives and 17
negative or unsupported archive inputs. See the [corpus manifest, sources and download
instructions](../../tests/corpus/EXTERNAL.md). Both SHA-256 and the upstream Git blob SHA-1
are checked. Files are not altered and remain under `target/`.

| Strict Wordcraft comparison | Pass | Fail | Blocked |
|---|---:|---:|---:|
| Earlier 143 cases, rechecked | 83 | 59 | 1 |
| 181 additional upstream inputs | 39 | 124 | 18 |
| Three new focused regression inputs | 1 | 2 | 0 |
| **Total: 327 cases** | **123** | **185** | **19** |

There are 311 distinct DOCX byte sequences. Across the 256 external inputs, 60 pass, 177 fail,
and 19 are blocked. All 82 earlier passes still pass; `poi-31-table-indent` changed from failed
to passed. The first new table fixture, `poi-extra-52104d1357d4` (`Bug66263-table.docx`),
also passes. The 123 passing cases cover 168 pages. The largest accepted glyph-origin error is
0.251 pt, unmatched ink is below 0.000363, and common-ink color error is below 0.0486. The gate
also compares source text and whitespace, line grouping, page size/count, glyph IDs, font bytes,
and styles. Its limits and 144 dpi raster method are in the [runner instructions](../../tools/wordcraft-parity/README.md).

Nineteen cases could not be compared: both engines reject many malformed or encrypted inputs;
the original deeply nested table exceeds both engines' XML depth limits; one corrupt ZIP is
rejected by EdgeDoc, and Wordcraft times out on one long document. These are recorded as blocked,
never as passes. Some failures are comparison limits: 45 cases report reference-adapter limitations
and 45 involve Wordcraft font data outside the supplied profile. Those categories overlap other
failures. They do not by themselves prove an EdgeDoc defect or a match.

The [machine-readable before/after snapshot](expanded-256-parity-result.json) retains every
case, upstream source, checksum, status, issue category and both engine fingerprints. The full
local report and page images are at `target/wordcraft-parity/expanded-256/`. Wordcraft output
for 259 cases was reused only after checking original input, reference and adapter hashes,
the supplied font bytes/styles and every SVG page. EdgeDoc and all page comparisons were rerun.

## Fixes driven by the new examples

- Explicit borders on adjacent table cells now paint shared edges even without table-level
  inside rules. Effective top and final bottom border widths reserve space in the cell, and
  horizontal rules are centered on their edges. The previously failing table-indent document
  now passes the strict comparison.
- Row-level content controls now expose their contained rows to layout and preserve their wrapper
  and properties through an edited save. A table with no rows completes rather than retrying on
  every page. Before this change the downloaded row-control case generated 20,000 blank pages;
  it now renders one page. A simple gridless auto-width table measures cell contents, which
  brings that case into strict Wordcraft parity. Complex autofit and border conflict resolution
  remain open.

The implementations follow inspected Wordcraft table layout and OOXML reader behavior. The
reference checkout was not modified. The new focused regression that combines a preceding edited
paragraph with a row control still fails its Wordcraft comparison due to an 8 pt position difference;
its passing Rust contract checks preservation and the absence of runaway pages, not full parity.
The new explicit-cell-border regression also remains in the gate as a failing comparison.

## Image inspection

The [inspection record](expanded-table-image-inspection.json) contains current-build hashes and
observations from both engines for three representative page pairs. The corrected
[table-indent Wordcraft page](../../target/corpus-inspection/visual-256-latest/poi-31-table-indent/wordcraft-page-001.png)
and [EdgeDoc page](../../target/corpus-inspection/visual-256-latest/poi-31-table-indent/edgedoc-page-001.png)
show the restored interior rules and aligned rows. The corrected
[row-control Wordcraft page](../../target/corpus-inspection/visual-256-latest/poi-extra-52104d1357d4/wordcraft-page-001.png)
and [EdgeDoc page](../../target/corpus-inspection/visual-256-latest/poi-extra-52104d1357d4/edgedoc-page-001.png)
show both adjacent cells on one page.

In the newly downloaded SmartArt guide, both engines display the first-page embedded image, but
EdgeDoc still differs in bullet marks, the title rule, paragraph spacing, hyperlink wrapping and
image position. Its [Wordcraft page](../../target/corpus-inspection/visual-256-latest/docx4j-extra-375d370aacec/wordcraft-page-001.png)
and [EdgeDoc page](../../target/corpus-inspection/visual-256-latest/docx4j-extra-375d370aacec/edgedoc-page-001.png)
were inspected directly. The earlier [11-page-pair review](21-external-corpus-parity.md) remains
historical evidence for the previous build.

## Validation and open work

The workspace suite passes 300 tests, including 49 focused rendering contracts. Clippy with
warnings denied and formatting pass. Thirteen parity-gate tests and six download-integrity tests
pass. The rebuilt browser viewer passes 20 checks and the original 33-document browser corpus
passes 128 checks. The original 75-file native corpus test opens and lays out 74 files and checks
the expected depth-limit rejection of the remaining input. These smoke checks are not parity tests.

The 185 strict failures remain in the report. They include TOC and nested fields, list positioning,
complex table sizing/borders/merges, notes, pictures and wrap, text boxes, pagination, and RTL or
complex-script shaping. Full support for all row-control nesting and unusual table autofit is not
established. The gate stays nonzero until every included rendering case passes; bounded negative
fixtures must be assessed as rejections rather than silently removed.
