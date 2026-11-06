# Footer frame and trailing line-break follow-up

This is a historical comparison with clean local Wordcraft commit
`1c7b5013fab765f204cf435b9f8814b46b7c1278`, not a Microsoft Word test.

Paragraph-frame `xAlign` and `yAlign` are imported and saved. One bounded case—a footer with an
auto-sized, right-margin-aligned frame containing only a PAGE field—is positioned outside footer
text flow. General frames and their wrap remain unsupported and continue to produce a diagnostic.
A final hard line break now adds the empty last line before the paragraph mark.

The [338-case comparison](footer-break-parity-result.json) reports 135 passes, 184 failures and 19
blocked cases. No earlier case changed status. The added hard-break fixture passes; two footer-frame
fixtures fail the overall gate because frames remain diagnosed, and one of those also differs in
vertical positioning.

All four pages of `docx4j-32-toc` pass the report's page-level text, font, geometry and raster
checks (zero unmatched ink and at most 0.05 pt origin error). The document itself still fails:
its cached fields and general paragraph frames are diagnosed. Pages 1 and 3 were visually reviewed
against Wordcraft; [image hashes and observations](footer-break-image-inspection.json) are recorded.
This is evidence for those pages under the supplied font profile, not general frame, field or
Wordcraft parity.

The workspace suite passed 317 tests; strict Clippy, formatting, 18 browser viewer checks and 128
browser corpus checks across 33 documents passed.
