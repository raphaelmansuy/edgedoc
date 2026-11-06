# Double underlines and patterned-stroke evidence

The pinned Greek application form `2559791938` uses a double underline on its title. The first Wordcraft SVG adapter rendered all patterned lines as solid, so a side-by-side inspection of EdgeDoc, Wordcraft, and the supplied Word-exported PDF exposed an evidence gap. The adapter now draws Wordcraft's double, triple, and dashed line patterns using the geometry in Wordcraft's renderer. Wave strokes still report an explicit adapter limitation.

EdgeDoc previously reduced every underline to a single stroke. The resolved run style now preserves a double underline and paints the two lines with Wordcraft's width and offsets. A regression asserts that a double-underlined run paints distinct strokes on both sides of the reference baseline. The form's table style also uses a zero-width `nil` indent, which is now recognized as a supported zero indent rather than an unsupported conditional table style; a nonzero `nil` indent remains conservatively diagnosed.

The form now passes the strict Wordcraft comparison with newly rendered references and the installed font profile. Both resulting page images were inspected, as was the Word PDF. This is a focused result, not a claim that every underline, table style, drawing, or document is supported. The input is pinned in `tests/corpus/word-benchmark-manifest.json`.
