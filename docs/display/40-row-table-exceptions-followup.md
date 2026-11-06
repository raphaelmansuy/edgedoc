# Row table-property exceptions follow-up

Wordcraft applies `w:tblPrEx` borders to the row's cells and reserves the corresponding border
insets. EdgeDoc now reads those exceptions, applies their border precedence during measurement and
painting, and preserves the original exception markup during unrelated edits. Edited structured
border values are written back while unrelated exception properties are retained.

The [before comparison](row-exception-before-result.json) showed missing row-specific borders and
a one-point glyph-position error. The [updated comparison](row-exception-after-result.json)
passes, and its [page pair](row-exception-after-image-inspection.json) was inspected. Rust tests
cover layout, unrelated edited saves, and an edited border value.

The [POI form comparison](row-exception-poi-result.json) still fails. Page-one unmatched ink fell
from 64.45% to 61.04%, and the [inspected page](row-exception-poi-image-inspection.json) shows the
row-specific borders restored in the lower section. Drawing, text, merge, and pagination gaps
remain. A 17-case table comparison retained its earlier case and page statuses.
