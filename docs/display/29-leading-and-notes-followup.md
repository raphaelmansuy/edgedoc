# Bottom-margin leading and remaining note gaps

Wordcraft allows the extra leading of a multiple-spaced line to extend into the normal bottom
margin when the text itself fits. EdgeDoc now applies the same rule when the page has no notes
and its available body reaches that margin. Exact and at-least spacing keep their full line box.

Both focused spacing fixtures match Wordcraft in the strict text, glyph and image comparison.
The exact-spacing fixture also exposed a separate gap: EdgeDoc repeated space before after an
automatic page break. The page transition now consumes that gap once, while explicit breaks
keep their existing handling.

The [343-case comparison](leading-parity-result.json) reports 140 passed, 185 failed and 18
blocked. Both new cases pass, with no status changes among the preceding 341 cases.

At this stage the [inspected guide images](leading-image-inspection.json) show page 2 matching pixel
for pixel. Page 4 differed because EdgeDoc did not yet lay out its footnote, leaving room for
additional body text. The [Apache POI footnote fixture](footnote-image-inspection.json) isolates the same omission:
the body reference is present, but the separator and note story are absent in EdgeDoc.
The note story's own `footnoteRef` and `endnoteRef` markers are now preserved through parsing
and edited saves, as verified by an end-to-end round-trip test. See the later
[footnote layout follow-up](31-footnote-layout-followup.md) for rendering and continuation work.
Body references now use the separate footnote/endnote numbering sequences, document settings,
section restarts and superscript metrics. The [reference-mark evidence](note-reference-evidence.json)
compares the body glyphs only; per-page numbering and endnote stories remained unsupported. Separate
[superscript comparisons](superscript-parity-result.json) cover mixed and superscript-only lines.
The [354-case comparison](note-reference-parity-result.json) records 143 passed, 192 failed,
and 19 blocked. All 346 previously measured cases kept their status; the new note documents
remain failures because their note stories are not rendered.
At the time of that inspection the guide rendered as 47 pages in EdgeDoc and 45 in Wordcraft
under this font profile. These observations do not establish document-level parity.
