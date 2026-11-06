# Table grid and width follow-up

Rows now honor `gridBefore` and `gridAfter` when positioning cells, selecting conditional styles,
measuring merged cells, and drawing row edges. The original grid properties remain preserved
through edited saves. Explicit table widths now use their point or percentage value, including
literal percentages and widths beyond the text column; nested explicit widths are relative to the
containing cell.

The [before grid-offset comparison](grid-offset-before-result.json) had a 90-point glyph-position
error. The [focused comparison](grid-width-final-result.json) passes six grid and width fixtures,
including 50% and 150% widths, a literal percentage, and a nested percentage-width table. The
[page pairs](grid-width-final-image-inspection.json) were visually inspected. Rust tests cover
row offsets, edited-save preservation, point and percentage widths, and nested widths.

The [downloaded docx4j table comparison](grid-width-real-result.json) still fails. Page-three
unmatched ink fell from 7.4% to 2.5%; the [inspected page pair](grid-width-real-image-inspection.json)
shows the grid offsets restored, with patterned-border differences still visible. A 17-case table
check retained its earlier case and page statuses.

The full 390-case comparison is recorded in `grid-width-390-parity-result.json`: 172 passed, 199 failed, and 19 were blocked. One previously passing ligature case timed out in the full run; its isolated retry passed, as recorded in `grid-width-390-timeout-retry.json`. Accounting for that retry gives 173 passing and 18 blocked, with no status regressions among the previous 384 cases. The six added grid and width fixtures passed.
