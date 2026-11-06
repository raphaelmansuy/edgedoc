# Cell spacing and shared numbering follow-up

Wordcraft collapses adjacent paragraph spacing inside table cells, suppresses automatic spacing
at the cell edges, and treats a zero exact row height as unspecified. EdgeDoc now applies these
rules in both cell measurement and placement. Negative row heights are read as their absolute
value, as in Wordcraft.

The [focused cell comparison](cell-spacing-targeted-result.json) passes, and the
[rendered page pair](cell-spacing-image-inspection.json) was inspected: the two PNGs are
identical. The [376-case comparison](cell-spacing-376-parity-result.json) reports 155 passed,
203 failed, and 18 blocked. All 375 prior cases retained their status. This result predates the
numbering change below and does not establish complete parity.

Wordcraft shares numbering counters across instances of an abstract list, applying each
instance's restart once. EdgeDoc now follows that behavior for its implemented levels, including
a zero start. In the downloaded `NumberingWOverrides.docx`, the final label now matches
Wordcraft. The [376-case comparison after the counter change](shared-numbering-376-parity-result.json)
retained the previous counts and statuses. Its [initial page pair](shared-numbering-poi-image-inspection.json)
showed a remaining indentation and tab-placement gap.

Numeric labels now use their first-line indent and list-level tab. In a
[focused comparison](numeric-list-position-focused-result.json), two downloaded POI numbered-list
documents pass all checks; both [page pairs](numeric-list-position-image-inspection.json) are
pixel-identical. The [376-case run](numeric-position-376-parity-result.json) reports 159 passed,
199 failed, and 18 blocked: four documents improved and no previous passing case regressed.

An implicit tab at the hanging indent and numbering inherited from paragraph styles now follow
Wordcraft. Multilevel legal labels use the format of each referenced level, preserving zero-padded
decimals; labels also move with centered text and retain paragraph spacing. A
[six-document focused comparison](style-numbering-focused-result.json) passes, including two
previously failing docx4j cases and two previously passing bullet cases. Both
[docx4j page pairs](style-numbering-image-inspection.json) were inspected: the restart page is
pixel-identical, and the heading page has matching glyph positions with a small color difference.
Other list behavior, including multilevel restart variants, still needs broader coverage.

The [subsequent 376-case comparison](style-numbering-376-parity-result.json) reports 162 passed,
194 failed, and 20 blocked. Two formerly passing cases timed out in the batch and
[passed when retried individually](style-numbering-timeout-retries.json), giving 164 passed,
194 failed, and 18 blocked when the retries are accounted for. Five cases improved and no
previously passing case regressed. This remains far short of complete parity.

The [table-cell numbering fixture](table-numbering-before-result.json) identifies another gap:
Wordcraft numbers the text before, inside, and after a table in one sequence, while EdgeDoc
omits cell labels and skips their counters. Its [page pair](table-numbering-before-image-inspection.json)
was inspected.
