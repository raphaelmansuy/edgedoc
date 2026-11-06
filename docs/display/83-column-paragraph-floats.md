# Column- and paragraph-anchored square pictures

EdgeDoc now places a single square-wrapped picture anchored to its column horizontally and its paragraph vertically, including when the paragraph also contains text or non-picture inline objects. The float is recorded for a bounded reflow so its own text and later paragraphs can flow beside it. The vertical anchor includes any effective gap inherited from the preceding paragraph while excluding the anchored paragraph's own space-before. Other anchor bases and paragraphs with multiple pictures remain outside this subset and continue to be diagnosed.

A controlled picture-and-text fixture at `target/wordcraft-parity/column-paragraph-float` matches the Wordcraft page in glyphs, origins and raster ink. The downloaded `wordbench-115c721277` document exposed a second mismatch: a two-line paragraph split across pages despite widow/orphan control. After correcting that behavior, both pages match Wordcraft in the same metrics, with zero unmatched ink. The first-page Wordcraft and EdgeDoc images were inspected. The strict document comparison still fails because floating-picture support remains partial.

Wordcraft's `flow::settled_end` also moves a three-line paragraph whole when the only possible splits would leave an orphan or widow; EdgeDoc follows the same rule for this case. These results do not establish general floating-object or document parity.

Three additional fixtures vary the anchored paragraph's before-spacing, the preceding paragraph's after-spacing, and both together. All three match the Wordcraft page. A regression test checks that the prior paragraph's after-spacing determines the anchor origin when both values are present.
