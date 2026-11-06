# Indexed and large font collections

Font registration can select a collection face explicitly, and shaping and outlines use that same face index. The font size limit is 96 MiB, allowing the locally installed 22 MiB Arial Unicode font and 75 MiB PingFang collection to load. A generated two-face collection regression test checks face selection, glyph lookup, outlines and shaping.

Controlled comparisons using Helvetica bold face 1 and PingFang HK face 8 matched Wordcraft's glyphs and page images. The PingFang page PNGs were byte-identical and were inspected visually. Font style metadata in the comparison uses the actual face weight and italic status, rather than the registration alias.

Registration still selects faces by regular/bold and upright/italic aliases; it does not automatically enumerate collections or implement arbitrary-weight matching. Reference-face availability checks do not establish which face each nonpainting paragraph mark selected. These controlled comparisons do not establish document-wide parity.

With the indexed-face adapter and the recorded local font profiles, a fresh run of the 209-case regression passed all 209. The 48-document Word benchmark passed 12, failed 34 and retained two blocked Wordcraft reference timeouts. The comparison reports and the input and tool fingerprints are summarized in `indexed-font-regression-209-summary.json` and `word-benchmark-48-indexed-font-summary.json`. The Rust workspace, strict Clippy, and the rebuilt browser viewer checks also passed (18 viewer checks and 134 checks across 33 corpus documents).
