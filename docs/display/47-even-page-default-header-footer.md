# Default header and footer on even pages

When odd/even headers are enabled and a section has no even variant, Wordcraft uses the default header or footer for even pages. EdgeDoc now does the same, while retaining the distinct first-page behavior.

The downloaded docx4j `odd_even` and `odd_even_different_first_page` documents previously omitted their default footer on page two. Both now pass the strict Wordcraft comparison (`even-footer-focused-result.json`). The page-two Wordcraft and EdgeDoc PNGs were visually inspected and are byte-identical (`even-footer-image-inspection.json`).

The expanded `even-footer-183-result.json` includes the 181 previously passing cases and both newly fixed documents; all 183 pass. Workspace tests, Clippy and the rebuilt viewer checks pass (18/18 viewer checks and 134/134 checks over 33 corpus documents). The latest full 394-case report remains `linebreak-394-streaming-result.json`, from before this and the font/shading fixes, and records 196 failures and 18 blocked cases; it is not proof of complete parity.
