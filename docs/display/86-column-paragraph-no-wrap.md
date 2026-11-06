# No-wrap column/paragraph anchors

A single picture anchored horizontally to its column and vertically to its paragraph can now use `wrapNone`, in body or header text. It takes no line width, and foreground and background pictures paint on the appropriate side of the paragraph text. Header float fragments do not reserve body-flow height.

Four controlled fixtures compare foreground and background pictures in the body and header against the pinned Wordcraft reference. Their glyphs, positions and raster ink match; the foreground header pages were inspected visually. Their strict comparisons still fail because floating-picture support is partial. A regression test checks paint order for all four cases.

Downloaded document `wordbench-07ffc3f5af` contains a no-wrap header picture. Its first-page unmatched ink fell from approximately 70% to 7%; the document still fails because of other layout differences and unsupported conditional table styles. `wordbench-056bfeba7c` also improved but retains large differences and unsupported floating tables. Both pages of `wordbench-0800162a66` continue to pass the page-level comparison.
