# Merged-cell border follow-up

EdgeDoc previously drew table-wide grid lines through horizontally and vertically merged cells.
It now resolves visible edges per cell for merged tables, suppressing the horizontal edge at a
vertical continuation and omitting vertical edges inside a spanning cell.

The [before comparison](merged-border-before-result.json) and [page pair](merged-border-before-image-inspection.json)
show the extra lines. The [updated page comparison](merged-border-final-result.json) passes glyph
and raster thresholds, and its [page pair](merged-border-final-image-inspection.json) was visually
inspected. The document remains failed overall because vertical-merge layout is still reported as
unsupported; this change does not cover all merge behavior.

In the downloaded POI form, the [updated comparison](merged-real-after-result.json) remains a
failure, though page-one unmatched ink falls from 66.38% to 64.45%. The
[updated image](merged-real-after-image-inspection.json) shows fewer extraneous grid lines along
with substantial remaining layout, border, and drawing differences. The
[17-case table regression check](merged-border-regressions-result.json) retained all prior case
and page statuses, including diagnostic-blocked conditional styles.
