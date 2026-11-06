# Square-wrap side selection

For the supported object-only, margin-anchored floating-picture subset, EdgeDoc now applies `wrapText="left"`, `"right"`, and `"largest"` as well as `"bothSides"`. The side is retained with the exclusion through early-float reflow, and the available paragraph width determines the wider side. As in Wordcraft, a tie chooses the left side.

Controlled versions of the inline-picture fixture in `target/wordcraft-parity/square-wrap-other-modes` exercise all three side modes with justified text and an earlier paragraph intersected by the float. Their rendered pages match the pinned Wordcraft reference in glyphs, positions, and raster ink, and the right-side Wordcraft and EdgeDoc page images were inspected. The comparisons still fail at document level because floating-picture support remains partial and is diagnosed as unsupported.

The implementation does not claim support for all anchor bases, floats embedded in text runs, bidirectional paragraphs, contours, floating tables, or other wrap types. Largest-side selection uses the paragraph's available width; its interaction with a distinct first-line indent needs further verification.

Wordcraft also requires a clear span of at least 20 points next to a float. A controlled 10-point span previously split words into very narrow fragments in EdgeDoc; both renderers now move the text below the float and produce matching page images. Exclusions that do not overlap the paragraph horizontally do not impose this minimum.

Additional `largest` fixtures at positions near the left and right margins both match Wordcraft at the page level. These exercise selection of each side, complementing the centered tie case.
