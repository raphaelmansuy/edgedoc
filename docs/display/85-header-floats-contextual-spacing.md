# Header floats, contextual spacing, and tabbed justification

A header paragraph can now place one square- or top-and-bottom-wrapped picture anchored to its column and paragraph. The picture is positioned independently of header text, and its exclusion applies to subsequent header paragraphs. It does not add its image height to the body-flow reservation. Other header anchor forms and multiple-picture paragraphs remain outside this subset and are diagnosed.

Wordcraft applies contextual spacing to each paragraph's own side of a gap. EdgeDoc now suppresses the previous paragraph's after-spacing only when that paragraph requests contextual spacing, and suppresses the next paragraph's before-spacing only when the next one requests it. Wordcraft also leaves text through the last tab anchored when justifying a wrapped line; EdgeDoc now stretches only spaces after that tab.

The downloaded `wordbench-0800162a66` document previously rendered as three pages instead of Wordcraft's two. With these fixes it renders two pages, and both pass the glyph and raster checks; the first-page images were inspected. Its strict document status remains failed because the floating-picture diagnostic is retained. Focused tests cover the header image's position and body reservation, all four contextual-spacing combinations, and tabbed justification.
