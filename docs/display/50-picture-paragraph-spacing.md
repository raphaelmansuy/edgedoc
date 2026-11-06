# Spacing before picture-only paragraphs

A picture-only paragraph is represented as a picture block. The block now consumes the preceding paragraph's trailing space before placement and respects page and column boundaries, instead of dropping that space.

In the downloaded eight-page docx4j section/header example `docx4j-extra-f749038ff223`, the earlier even-page header fix restored the missing headers, but a picture on page two remained 10 pt too high. Both fixes together make all eight pages pass the strict Wordcraft comparison with zero unmatched ink and zero glyph-position error (`picture-spacing-focused-result.json`). Page two was visually inspected in both renderers (`picture-spacing-image-inspection.json`); the PNG files are not byte-identical.

The 186-case passing set (`picture-spacing-186-result.json`) includes the 185 preceding passing cases and this document; all pass. Workspace tests, Clippy and browser checks pass (18/18 viewer checks, 134/134 checks over 33 documents). The latest complete 394-case baseline predates this and other later fixes and is retained in `linebreak-394-streaming-result.json`; it records substantial remaining failures and blocked inputs.
