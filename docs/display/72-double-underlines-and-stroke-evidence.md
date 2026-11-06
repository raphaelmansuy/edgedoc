# Double underlines and patterned-stroke evidence

The pinned Greek application form `2559791938` uses a double underline on its title. The first Wordcraft SVG adapter rendered all patterned lines as solid, so a side-by-side inspection of EdgeDoc, Wordcraft, and the supplied Word-exported PDF exposed an evidence gap. The adapter now draws Wordcraft's double, triple, and dashed line patterns using the geometry in Wordcraft's renderer. Wave strokes still report an explicit adapter limitation.

EdgeDoc previously reduced every underline to a single stroke. The resolved run style now preserves a double underline and paints the two lines with Wordcraft's width and offsets. A regression asserts that a double-underlined run paints distinct strokes on both sides of the reference baseline. The form's table style also uses a zero-width `nil` indent, which is now recognized as a supported zero indent rather than an unsupported conditional table style; a nonzero `nil` indent remains conservatively diagnosed.

The form now passes the strict Wordcraft comparison with newly rendered references and the installed font profile. Both resulting page images were inspected, as was the Word PDF. This is a focused result, not a claim that every underline, table style, drawing, or document is supported. The input is pinned in `tests/corpus/word-benchmark-manifest.json`.

With freshly rendered references, the 48-document benchmark recorded 12 passes, 34 failures, and 2 blocked reference-render timeouts. All 209 selected previous passes passed again. Results and fingerprints are recorded in `word-benchmark-48-stroke-adapter-summary.json` and `stroke-adapter-regression-209-summary.json`; separate retries of both timed-out Wordcraft reference renders also exceeded a 600-second limit. The workspace passed 405 tests and strict Clippy, and the rebuilt browser passed 18 interaction checks and 134 checks on 33 documents.

After that run, a diagnostic was added for unsupported underline patterns, with a focused regression. Comparisons using the stricter diagnostic are in progress.

With the stricter underline diagnostic, the 48-document result remained 12 passes, 34 failures, and 2 blocked reference timeouts. The 209 selected prior passes all passed again. See `word-benchmark-48-underline-diagnostic-summary.json` and `underline-diagnostic-regression-209-summary.json` for exact-source fingerprints. The workspace passed 406 tests and strict Clippy; the rebuilt browser passed 18 interaction and 134 corpus checks.
