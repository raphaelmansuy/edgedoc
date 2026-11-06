# Inherited table-style follow-up

The table-style cascade now uses inherited table-style paragraph and run defaults between
document defaults and paragraph styles. Table borders and cell margins inherit from the table
style chain and yield to properties set directly on the table or cell. A style's decoded table
properties are retained when that style is edited and saved.

The focused inherited-style and direct-override fixtures pass page-level Wordcraft comparisons,
and the wide-grid fixture passes the full comparison. A previously two-page Apache POI sample
now matches its single Wordcraft page at the text, glyph and raster thresholds. It remains a
document-level failure because the conservative diagnostics flag conditional table styles and
note stories. Conditional style regions, additional table-style properties, and complex table
layout remain open; these fixtures do not establish general table parity.

The [focused inherited-style comparison](table-style-targeted-result.json),
[wide-grid comparison](table-wide-targeted-result.json), and [POI comparison](table-style-poi-result.json)
record the evidence. The [POI page pair](table-style-poi-image-inspection.json) was also
inspected visually in both engines.

The [368-case comparison](table-style-368-parity-result.json) reports 151 passes, 195 failures,
and 22 blocked. Four cases were blocked by disk exhaustion or a killed renderer during the
parallel run. Their [individual retries](table-style-blocked-retries.json) produced two passes
and two failures, retaining their prior statuses. Accounting for those retries gives 153 passes,
197 failures and 18 blocked. One earlier row-control fixture improved from failure to pass;
the new wide-grid fixture passed, while the two table-style fixtures passed their page checks
but retained the conditional-style diagnostic.
