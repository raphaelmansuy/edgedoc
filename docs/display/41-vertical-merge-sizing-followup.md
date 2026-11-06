# Vertical-merge sizing follow-up

Wordcraft distributes a merged cell's required height across its row span and grows the final
continuation row when needed. EdgeDoc now measures merged cells that way, suppresses continuation
cell content and shading, and uses the full merged height for the restart cell's fill and alignment.

The [initial fixture](vmerge-height-before-result.json) placed neighboring row content too low;
the [before images](vmerge-height-before-image-inspection.json) show the difference. The
[updated comparisons](vmerge-final-result.json) pass page-level checks for the sizing fixture,
a table moved to a later page, and a forced split across pages. The
[inspected split-page images](vmerge-final-image-inspection.json) are pixel-identical on the
selected first page. All three documents remain failed overall because vertical merging still
has an unsupported diagnostic.

The [downloaded docx4j table samples](vmerge-real-result.json) remain failures. On page three of
the table sample, unmatched ink fell from 31.7% to 7.4%; the [page pair](vmerge-real-image-inspection.json)
also shows remaining row grid offsets and patterned-border differences. The 17-case table check
retained its earlier case and page statuses.
