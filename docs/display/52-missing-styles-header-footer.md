# Header and footer fallback without a styles part

When an imported package has no styles part, Wordcraft supplies built-in Header and Footer styles with single line spacing, no trailing paragraph space, and the usual center and right tabs. EdgeDoc now supplies the same fallback while leaving document-provided styles untouched.

The downloaded Apache POI `poi-extra-62a6f4519bd5` document has nested footer content controls and no styles part. Its footer previously appeared 10.2 pt too high; it now passes the strict comparison (`footer-fallback-focused-result.json`). The Wordcraft and EdgeDoc PNGs were visually inspected and are byte-identical (`footer-fallback-image-inspection.json`). The original test input is retained under `crates/edgedoc-display/tests/fixtures/footer-control`.

All 188 cases in the expanded passing set pass (`footer-fallback-188-result.json`). Workspace tests, Clippy and browser checks pass (18/18 viewer checks and 134/134 checks over 33 documents). The latest full 394-case baseline predates this and several subsequent improvements and still reports substantial failures and blocked inputs (`linebreak-394-streaming-result.json`).
