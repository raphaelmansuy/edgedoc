# Wavy underlines

EdgeDoc and the Wordcraft comparison adapter now draw wavy underlines using Wordcraft's alternating quadratic curves, stroke width, and amplitude. Wordcraft maps `wave`, `wavyHeavy`, and `wavyDouble` to the same wave stroke; EdgeDoc follows that behavior for Wordcraft parity. Other unknown patterns continue to be diagnosed.

Two controlled variants of the pinned `2559791938` form, with `wave` and `wavyDouble` on its title, were rendered by both engines using the same installed fonts. Both passed the strict comparison with zero unmatched ink. The paired close-ups at `target/wordcraft-parity/wave-underline-reference/wave-inspection.png` were inspected. A focused test also verifies that a wavy underline paints alternating quadratic curves. These results apply to the controlled fixtures and do not establish complete document parity.

With fresh references for the updated adapter, the 48-document benchmark recorded 12 passes, 34 failures, and 2 previously timed-out reference renders that remain blocked. All 209 selected previous passes passed again. Exact-source results are in `word-benchmark-48-wave-underline-summary.json` and `wave-underline-regression-209-summary.json`. The workspace passed 409 tests and strict Clippy; the rebuilt browser passed 18 interaction checks and 134 checks across 33 documents.
