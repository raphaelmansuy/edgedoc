# Underline value and color

Wordcraft treats `w:u` without a `w:val` as no underline. EdgeDoc now does the same and preserves the original element for an edited save. An explicit underline color also travels through layout to paint instead of being replaced by the text color.

The downloaded Apache POI document `poi-extra-fcd5027c3c76` contains color-only underline elements, explicit black underlines, and a white underline. It now passes the strict comparison with zero unmatched ink and a maximum glyph-position difference below 0.001 pt (`underline-color-focused-result.json`). Both page images were visually inspected (`underline-color-image-inspection.json`); their PNG bytes differ because of minor color differences.

All 185 documents in the expanded passing set pass (`underline-185-result.json`). The regression tests cover no-value underlines, edited-save preservation, and independent underline color. Workspace tests, Clippy and browser checks pass (18/18 viewer checks and 134/134 checks over 33 documents). The most recent complete 394-case baseline, `linebreak-394-streaming-result.json`, predates this and several other subsequent fixes; it still records many unmatched or blocked cases and does not prove overall parity.
