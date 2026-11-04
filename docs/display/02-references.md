# Normative sources, prior art, terms, and conformance classes.

> Part of the EdgeDoc Display specification. Date: 2026-11-04. Requirement ids are
> normative. See [the index](README.md).

## 8. Normative references

A later edition applies only when this clause names it. Microsoft implementation notes
override ECMA-376 where Word and the standard disagree.

| Id | Document | Role |
|---|---|---|
| D1 | [ECMA-376](https://ecma-international.org/publications-and-standards/standards/ecma-376/) Part 1, 5th edition | WordprocessingML markup |
| D2 | [ECMA-376](https://ecma-international.org/publications-and-standards/standards/ecma-376/) Part 2 | Package, and the `mc:AlternateContent` rule |
| D3 | [ECMA-376](https://ecma-international.org/publications-and-standards/standards/ecma-376/) Part 3, Annex L | Markup compatibility |
| D4 | [ECMA-376](https://ecma-international.org/publications-and-standards/standards/ecma-376/) Part 4, `w:drawing` | DrawingML geometry, pictures, and text bodies |
| D5 | [MS-OI29500](https://learn.microsoft.com/en-us/openspecs/office_standards/ms-oi29500/1fd4a662-8623-49c0-82f0-18fa91b413b8) 25.0 | Where Office differs from the standard |
| D6 | [MS-DOCX](https://learn.microsoft.com/en-us/openspecs/office_standards/ms-docx/b839fe1f-e1ca-4fa6-8c26-5954d0abbccd) 23.0 | Word extensions |
| D7 | [ISO/IEC 29500-1:2016](https://www.iso.org/standard/71691.html) | Public markup reference |
| D8 | [UAX #9](https://www.unicode.org/reports/tr9/) | Bidirectional algorithm |
| D9 | [UAX #14](https://www.unicode.org/reports/tr14/) | Line breaking |
| D10 | [UAX #29](https://www.unicode.org/reports/tr29/) | Grapheme clusters and word boundaries |
| D11 | [UAX #11](https://www.unicode.org/reports/tr11/) | East Asian width |
| D12 | [UAX #50](https://www.unicode.org/reports/tr50/) | Vertical text orientation |
| D13 | [Unicode 16.0](https://www.unicode.org/versions/16.0.0/) | Character data, case, and normalization |
| D14 | [OpenType 1.9](https://learn.microsoft.com/en-us/typography/opentype/spec/) | Font tables, GPOS, GSUB, and variations |
| D15 | [WOFF2](https://www.w3.org/TR/WOFF2/) | Web font packaging |
| D16 | [ICC.1:2010](https://www.color.org/specification/ICC1v43_2010-12.pdf) | Color profiles |
| D17 | [CSS Color 4](https://www.w3.org/TR/css-color-4/) | Color spaces and conversion |
| D18 | [HTML Living Standard](https://html.spec.whatwg.org/multipage/) | `contenteditable`, `Selection`, `Clipboard`, and `PointerEvent` |
| D19 | [WAI-ARIA 1.2](https://www.w3.org/TR/wai-aria-1.2/) and [WCAG 2.2](https://www.w3.org/TR/WCAG22/) | Accessible viewer |
| D20 | [RFC 9457](https://www.rfc-editor.org/rfc/rfc9457) | Problem details in the error object |
| D21 | [CSP Level 3](https://www.w3.org/TR/CSP3/) | Content security policy for the demo |
| D22 | [pdf.js](https://mozilla.github.io/pdf.js/) | Prior art for the viewer and the text layer |
| D23 | [HarfBuzz](https://harfbuzz.github.io/) | Shaping behavior reference |
| D24 | [harfrust](https://crates.io/crates/harfrust) 0.13 | Rust port of HarfBuzz. The shaping engine |
| D25 | [skrifa](https://crates.io/crates/skrifa) 0.47 | Font parsing, metrics, outlines, and variations |
| D26 | [Open XML SDK](https://learn.microsoft.com/en-us/office/open-xml/open-xml-sdk) | Behavior reference, not a dependency |
| D27 | [WCAG 2.2 target size](https://www.w3.org/WAI/WCAG22/Understanding/target-size-minimum.html) | Viewer control size |
| D28 | [WordCraft 0.14.0](https://github.com/storytold/wordcraft) | Reference implementation. See [17-wordcraft-map.md](17-wordcraft-map.md) |

`DP-N-01`. Clause 16 maps each requirement to one of these sources, or marks it as an
EdgeDoc Display rule.

## 9. Prior art and what this design takes from it

| Source | What we take | What we do not take |
|---|---|---|
| pdf.js | Paged viewer, continuous scroll, text layer, tile strategy, worker split | PDF parsing |
| LibreOffice Writer layout | Table fit, footnote flow, section handling | UNO, the whole editor |
| Chromium LayoutNG | Fragment model: one block, many fragments | CSS cascade and style resolution |
| TeX line breaking | WordCraft's measured first-fit behavior, and the Word exceptions to [UAX #14](https://www.unicode.org/reports/tr14/) | Knuth–Plass optimum, which moves a page break |
| HarfBuzz | Shaping through a stable interface | A C dependency in the WASM module |
| Google Docs viewer | Progressive paint while parsing | Server rendering |

`DP-N-02`. The text layer follows the pdf.js contract: a transparent DOM text node sits at
the painted glyph position. A hit test then uses the browser selection engine, and the
screen reader reads real text.

## 10. Terms

Each term has one meaning in this specification and in the code.

| Term | Meaning |
|---|---|
| package | The ZIP container defined by ECMA-376 Part 2 |
| model | The document model from `edgedoc-model` |
| display document | A model plus resolved resources, styles, and fonts |
| resource | A part that paint needs: image, font, color profile, or external target |
| computed style | The property set after the cascade, with no inheritance left |
| shaping | Map a character string and a font to positioned glyphs |
| glyph run | Shaped glyphs with one font, one size, one colour, and one transform |
| fragment | A part of a block on one page, in one column |
| flow | The layout pass that fills fragments into a page area |
| page area | The rectangle inside the margins, in page coordinates |
| display list | An ordered list of paint operations for one page |
| painter | A trait that consumes a display list |
| tile | A raster region of one page at one scale |
| text layer | Transparent DOM content at painted glyph positions |
| fidelity tier | A named set of features and gates (T0 to T3) |
| viewer | The user interface that holds pages, zoom, and navigation |
| worker | The Web Worker that runs layout off the main thread |
| document order | Reading order of blocks as the file lists them |
| logical order | Character order before bidi resolution |
| visual order | Character order after bidi resolution |

`DP-N-03`. The specification uses "page" only for a laid-out page. It uses "part" for a
package item. It uses "block" for a paragraph or a table, and never for a page.

## 11. Conformance classes

| Class | Id prefix | Claim |
|---|---|---|
| I intent | `DP-I` | Goals, tiers, and metrics |
| N normative | `DP-N` | Sources and terms |
| A architecture | `DP-A` | Layers, traits, SOLID, and DRY |
| P pipeline | `DP-P` | Open, resolve, and diagnose |
| S style | `DP-S` | Cascade and computed properties |
| F font | `DP-F` | Shaping, bidi, and line breaking |
| L layout | `DP-L` | Pagination and flow |
| G paint | `DP-G` | Display list and painters |
| V viewer | `DP-V` | API and viewer model |
| B browser | `DP-B` | WASM, worker, and memory |
| U demo | `DP-U` | Demo site and controls |
| E performance | `DP-E` | Budgets |
| X edge | `DP-X` | Boundary behavior |
| Q quality | `DP-Q` | Corpus, gates, and CI |

`DP-N-04`. Version 1 claims classes I, N, A, P, S, F, L, G, V, B, U, E, X, and Q. It claims
T3 paint only through the placeholder path and a registered `ObjectRenderer`.

## 12. What "looks like Word" means

`DP-N-05`. A page is correct when it passes the gates of clause 4 in
[01-intent.md](01-intent.md). The gates compare a page against a reference page. A test
does not compare pixels with Word output, because Word is not available in CI and its
output changes with the printer driver.

`DP-N-06`. The word "perfect" in the project goal means the following, and nothing more:

1. Every character of the document appears on some page, once, in document order
   (`DP-Q-02`).
2. Every page break falls within one line of the reference break (`DP-Q-03`).
3. Every glyph is within 0.5 pt of the reference position (`DP-Q-04`).
4. A feature that the library does not model paints a marked placeholder. It never
   disappears, and it never shifts another block without a diagnostic (`DP-Q-05`).

A reader who wants a guarantee beyond these four rules does not have one from this
specification. They have the fidelity tier statement and the test report.
