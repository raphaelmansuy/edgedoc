# Table-cell numbering follow-up

Numbered paragraphs in table cells now share the document's list counters. Labels are assigned
once, measured in the cell, and reused on repeated headers. Cell block paths are unique across
rows, nested tables, headers, footers, and note stories, so cached labels do not collide.

The [initial focused comparison](table-numbering-before-result.json) showed Wordcraft counting
1–4 across a table while EdgeDoc omitted both cell labels and resumed at 2. The
[final comparison](table-numbering-final-result.json) passes the continuity fixture and every
page of a four-page table with a repeated numbered header. The
[inspected page pairs](table-numbering-final-image-inspection.json) are pixel-identical for the
selected pages. A Rust regression test checks all row labels and the following paragraph.

The [downloaded-document comparison](real-table-numbering-result.json) still fails overall.
On page 15 of the docx4j guide, the three table bullets now appear, but the document's pagination
still differs. The inspected [POI form and guide page pairs](real-table-numbering-image-inspection.json)
also show unresolved merged-cell borders, drawings, and layout differences. These results do not
establish complete Wordcraft parity.

The separate [header-table fixture](header-table-numbering-result.json) still fails: Wordcraft's
header label reflects its later header rendering pass, while EdgeDoc retains the earlier label.
The body labels match in that fixture. Its [page pair](header-table-numbering-image-inspection.json)
is retained for follow-up.
