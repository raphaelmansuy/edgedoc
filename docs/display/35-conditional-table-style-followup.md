# Conditional table-style follow-up

Table-style look switches and conditional regions are now read, saved, and applied in the
same precedence order used by Wordcraft. The implementation handles region text formatting,
shading, vertical and paragraph alignment, cell borders, row and column bands, and corner
precedence. Inherited table-wide shading and `wholeTable` borders are also applied. Style and
look settings survive an edited save in the regression tests.

The [conditional header image pair](conditional-header-image-inspection.json) was visually
inspected in both engines. Five focused fixtures pass their page-level comparisons, but remain
document-level failures under the conservative table-style diagnostic. In a real [Apache POI
sample](conditional-poi-image-inspection.json), the conditional appearance is present but row
heights differed at that point; the [row-height follow-up](36-table-row-height-followup.md) fixes the measured issue. Conditional styles involving unsupported properties, themes, merged
cells, or complex table geometry are not yet proven at parity.

The [373-case comparison](conditional-373-parity-result.json) reports 153 passed, 202 failed,
and 18 blocked. The four status changes from the previous full run are the cases that had
been blocked by temporary resource exhaustion and were already resolved by individual retries;
no previously resolved case changed status. All five new fixtures remain failed at document
level because of the diagnostic.
