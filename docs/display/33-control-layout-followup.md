# Content-control layout follow-up

Inline content controls now expose their runs to layout while preserving the control's opening
and closing markup in the model and on save. Cell-level controls are parsed into table cells,
with their wrappers retained in the same manner as row controls. Regression tests cover edits
inside an inline control and edited saves of both control types.

The [inspected page pairs](control-fixture-image-inspection.json) show an inline control in a
header and body that passes the strict Wordcraft comparison. The cell-control fixture initially painted its content but failed because its auto-sized table columns had different widths. A subsequent bounded auto-fit change makes that fixture pass; see the [targeted result](table-autofit-targeted-result.json) and [inspected page pair](table-autofit-image-inspection.json).
The real [Apache POI document](endnote-poi-extra-image-inspection.json) exposed both omissions;
its [current output](control-poi-after-image-inspection.json) restores the missing control text but still differs in table layout and page
count. The corpus remains a work in progress, not proof of complete parity.

The [365-case run](control-365-parity-result.json) recorded 150 passes, 195 failures and
20 blocked. Six existing cases improved from failure to pass, and the new inline-control case
passed. A drawing-heavy case timed out during the parallel run; a [standalone retry](control-timeout-retry.json)
completed and still failed on the previously known drawing gap. The other new cell-control case
failed on automatic table widths.

The [subsequent 365-case auto-fit run](autofit-365-parity-result.json) reports 151 passes,
195 failures and 19 blocked. The cell-control fixture improved from failure to pass, and the
previously timed-out drawing case completed as a failure. No previously passing case regressed.

The later [table-style follow-up](34-table-style-followup.md) addresses the base style,
preferred grid width and paragraph spacing differences in that POI example; conditional
style regions and other complex table behavior still need work.
