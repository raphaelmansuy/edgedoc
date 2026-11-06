# Endnote layout follow-up

Wordcraft collects endnotes in reference order and places them after the last body block or at
a section boundary when `sectEnd` is selected. EdgeDoc now builds the referenced stories,
places their separator and note bodies at those boundaries, and allows note content to continue
through the normal page flow. An explicitly defined separator story is rendered in the active
column. The note diagnostic remains enabled: the focused tests do not prove all note formatting,
complex story contents, or interactions with every layout feature.

The focused fixtures cover two endnotes alongside footnotes, section-end placement, and a
four-page continuation. Their page-level text, glyph and raster comparisons pass. A custom
separator, including a two-column case, matches glyph positions and raster ink, but the strict
text comparison fails because the Wordcraft adapter supplies empty source text for the
separator's decorative story. That case is not counted as a strict page pass; the
[visually inspected pair](endnote-custom-image-inspection.json) records its rendered output.

Wordcraft's reference behavior is implemented in `crates/layout/src/notes.rs` and
`crates/layout/src/pass.rs` in the sibling checkout. The previous [endnote gap image](endnote-gap-image-inspection.json)
shows why this work was needed; the [section-end image pairs](endnote-section-image-inspection.json)
show the updated rendering.

The downloaded [Apache POI endnote sample](endnote-poi-image-inspection.json) was
visually inspected in both engines after this change.

A [second Apache POI sample](endnote-poi-extra-image-inspection.json) remains visibly
different: text and tables inside content controls are missing, including content within
note stories and the header. This is not a passing visual comparison.

The [363-case comparison](endnote-363-parity-result.json) reports 143 passed, 201 failed,
and 19 blocked; the previous 359 cases retain their statuses. The POI sample and the
focused default, section-end and continuation cases pass page checks but remain document
failures under the note diagnostic.
