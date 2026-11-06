# Top-and-bottom floating-picture wrap

For the supported margin-anchored and column/paragraph-anchored picture subsets, EdgeDoc now handles `wrapTopAndBottom` by excluding the full paragraph width while the picture overlaps a line. Pictures outside the paragraph's horizontal extent do not exclude it. The existing bounded float reflow handles pictures that reach earlier text and pictures anchored within their own paragraph.

A float may begin partway down a line. Wordcraft tests the row against an estimated line height based on the paragraph mark, whereas EdgeDoc previously tested a one-point strip at the line top. EdgeDoc now uses the mark's estimated height and line-spacing rule when finding exclusion bands. A regression test covers a float beginning eight points into a line.

Controlled fixtures in `target/wordcraft-parity/topbottom-wrap`, `topbottom-paragraph`, and `topbottom-contact` match the Wordcraft pages in glyphs, origins, and raster ink. The margin-anchored pages were visually inspected. The earlier justified square-wrap example and both pages of downloaded `wordbench-115c721277` still match in targeted reruns. All these documents retain the partial-floating-support diagnostic, so their strict comparisons remain failures.

A matching square-wrap contact fixture at `target/wordcraft-parity/square-wrap-contact` verifies the same estimated-height rule for two-sided wrapping.
