# Signed page margins

Wordcraft's DOCX reader converts signed page-margin, header, footer and gutter distances to absolute values, bounded by the larger page dimension. EdgeDoc now applies the same normalization in the display conversion while preserving the source document model for editing and saving.

The downloaded docx4j `simple_hf_negative` document previously placed body text 64.52 points too high. It now passes the strict Wordcraft comparison, and the inspected reference and EdgeDoc PNGs are byte-identical; see `negative-margin-focused-result.json` and `negative-margin-image-inspection.json`. The focused run also passes five related numbered-story and normal-margin cases. In the 40-case header, footer and section regression set (`negative-margin-regressions-result.json`), this document changed from failed to passed; the other 39 cases retained their statuses (28 passed, 12 failed).

Workspace tests and Clippy passed. The rebuilt viewer passed 18/18 browser checks and 134/134 checks on 33 corpus documents. These checks do not establish overall Wordcraft parity; the 394-case baseline and its disk-full recovery are recorded in `header-394-combined-status.json`.
