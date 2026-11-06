# Detailed rendering parity audit against Wordcraft

> Historical baseline: this audit predates the field-range reader and TOC-link display changes
> in [the field follow-up](24-field-range-followup.md). Its counts and source descriptions
> describe that earlier build; consult the follow-up for current verification and remaining gaps.

Audit date: 2026-11-05. This audit compares the current EdgeDoc source with the local,
unmodified Wordcraft 0.15.1 checkout at commit `1c7b5013fab765f204cf435b9f8814b46b7c1278`.
The measured report's EdgeDoc, Wordcraft, adapter and comparison-tool fingerprints match the
current source. Wordcraft is the agreed reference; this is not a direct Microsoft Word test.

**Conclusion: EdgeDoc has bounded parity on a substantial simple-document subset, but it does
not have general Wordcraft rendering parity.** The missing paths affect ordinary documents with
notes, complex tables, text boxes, TOCs, floating pictures and mixed-direction text. Passing
a case means its actual text, glyphs, geometry and pixels met the gate's limits; preserving an
OOXML element, producing a page, or passing an internal contract does not establish parity.

This is a rendering audit: it does not claim parity for Wordcraft's editor commands, ribbon,
editing, proofing, save/export formats or platform integrations.

## Evidence and how to read it

The independent [327-case report](expanded-256-parity-result.json) contains **123 passes,
185 failures and 19 blocked cases**. Among **256 external inputs**, **60 pass, 177 fail and 19
are blocked**. The suite contains 311 distinct byte sequences; the pass counts are not a
percentage of Wordcraft's features, document population, pages or users. The 123 passing cases
cover 168 pages. Existing successful evidence includes multi-page table continuation, different
headers/footers, page geometry, a contextual ligature fixture, and two downloaded table examples.

The gate checks page dimensions/count, source text and whitespace, line grouping, glyph IDs,
actual font bytes and styles, origins within 0.5 pt, font sizes within 0.05 pt, and 144 dpi
pixels with a one-pixel neighborhood and 1% unmatched-ink ceiling. It checks the original
coordinates without registering or scaling one image to the other. This is a bounded tolerance,
not pixel identity. See [the comparison](../../tools/wordcraft_parity.py) and
[method and provenance](22-expanded-corpus-parity.md).

The text gate compares the two engines' exported glyph text; it does not independently prove
that every printable character in the original DOCX was emitted. A shared omission could therefore
escape this check. The Wordcraft adapter covers only part of Wordcraft's display list, and
Wordcraft itself is the chosen reference rather than an independently verified Word oracle.

The failure categories overlap and must not be added together: 120 cases have a line/text
difference, 33 have an origin error, 23 have different page counts, and 119 carry an EdgeDoc
unsupported-layout diagnostic. Forty-five cases have reference-adapter limitations and 45 use
reference font bytes outside the controlled profile; those counts overlap both each other and
other causes. Thirty cases are marked as having empty text output, which the gate intentionally
does not accept as proof even when the fixture may be structural or image-only. Nineteen are
blocked rather than marked passed; malformed, encrypted, depth-limited and timed-out inputs need
to be interpreted separately. See the individual records, not just these totals.

Manual inspection has covered page pairs from both engines, including a long guide, tables,
headers, pictures, a row control and Arabic text. [Current image inspection](expanded-table-image-inspection.json)
and the [earlier review](wordcraft-image-inspection.json) give source/build and PNG hashes. The
earlier images belong to an earlier EdgeDoc build. A five-page SmartArt guide has equal page
counts but visibly different bullets, image placement, text wrapping and title rule.

Statuses below mean **exercised** for a bounded passing sample, **partial** for a working but
incomplete path, and **open** when the required connection or feature is absent. They are not
an exhaustive feature checklist.

## Code-by-code assessment

| Area | Status and observed consequence | EdgeDoc implementation | Wordcraft reference |
|---|---|---|---|
| Package and story import | Partial. UTF-8 BOM spans, standard body/section stories, first/even/default headers and localized default styles have exercised coverage. Block, drawing and complex content-control forms remain incomplete. | [WML reader](../../crates/edgedoc-wml/src/read.rs#L233) reads table rows, story blocks and preserved markup. A direct row inside a row-level content control is parsed and its wrapper retained; other nesting is not proven. | [Story block reader](../../../wordcraft/crates/docx/src/read/story/blocks.rs) handles a broader set of wrapped blocks, nested stories and tables. |
| Style cascade and themes | Partial. Default, paragraph and character style chains and Latin major/minor theme fonts have passing fixtures. Conditional table styles are explicitly diagnosed, and non-Latin/theme and compatibility combinations remain open. | [Cascade](../../crates/edgedoc-style/src/lib.rs#L420) and [theme setup](../../crates/edgedoc-display/src/build.rs#L257). | [Wordcraft table layout](../../../wordcraft/crates/layout/src/table.rs#L486) resolves table-style regions and their text, fills, borders and row exceptions. |
| Font selection and shaping | Partial. Bundled Carlito/Arimo/Tinos families, supplied font faces, contextual shaping and a ligature/kerning case work with controlled bytes. Embedded fonts are not loaded; script-specific fallback and synthetic styles are not generally equivalent. | [Built-in faces](../../crates/edgedoc-display/src/fonts.rs#L63), [embedded-font stub](../../crates/edgedoc-display/src/fonts.rs#L99), [fallback candidates](../../crates/edgedoc-fonts/src/lib.rs#L373) and [contextual shaping](../../crates/edgedoc-text/src/lib.rs#L439). | [Wordcraft family substitution](../../../wordcraft/crates/fonts/src/word.rs#L39) and [run-aware fallback](../../../wordcraft/crates/fonts/src/fontdb/fallback.rs#L146) cover a wider range of scripts and font availability. |
| Mixed RTL and complex scripts | Open for parity. Arabic fixture `docx4j-26-arabic_numbering` differs in paragraph alignment, ordering and glyph presentation; EdgeDoc emits a limitation diagnostic. | [Bidi levels and shaping](../../crates/edgedoc-text/src/lib.rs#L368) exist, but [the display preparation](../../crates/edgedoc-display/src/build.rs#L107) explicitly flags complex-script/bidi layout. | Wordcraft has script-aware [fallback](../../../wordcraft/crates/fonts/src/fontdb/fallback.rs#L146), paragraph layout and bidi handling. Presence of those paths is not proof of parity for every script. |
| Paragraphs, lines and pagination | Partial. Simple alignment, exact/at-least/multiple spacing, continuation and selected page/column breaks pass. Different first-line indents, line wraps, keep behavior, compatibility combinations and accumulated vertical spacing still fail real files. | [Line breaker](../../crates/edgedoc-text/src/lib.rs#L746), [paragraph flow](../../crates/edgedoc-flow/src/lib.rs#L1663) and [section pass](../../crates/edgedoc-flow/src/lib.rs#L725). | [Wordcraft paragraph/box layout](../../../wordcraft/crates/layout/src/boxes.rs#L188) and [section pass](../../../wordcraft/crates/layout/src/pass.rs#L474) apply more interacting rules. |
| Tabs and leaders | Partial. Margin/indent positional tabs and focused right/center cases pass. The retained decimal-tab regression fails; long TOC lines and mixed formatting still wrap differently. | [Aligned tabs](../../crates/edgedoc-text/src/lib.rs#L1120) measure the following segment; [leader painting](../../crates/edgedoc-paint/src/lower.rs#L361) keeps decorative text out of selection. | Wordcraft [resolves pending tabs](../../../wordcraft/crates/layout/src/para/tabs.rs#L48) as content is placed and has explicit positional-tab handling. |
| Sections, columns, headers and footers | Exercised for selected combinations, partial overall. The header inheritance fixture passes three pages, and a short continuous-column balancing fixture passes. Complex stories, floats, notes and mixed section geometry remain unproven. | [Section construction](../../crates/edgedoc-display/src/build.rs#L44), [balancing](../../crates/edgedoc-flow/src/lib.rs#L975) and [story bands](../../crates/edgedoc-flow/src/lib.rs#L1367). | Wordcraft [headers](../../../wordcraft/crates/layout/src/headers.rs) and [section passes](../../../wordcraft/crates/layout/src/pass.rs#L474) cover more story and page interactions. |
| Tables | Partial. Repeated headers and row continuation pass a four-page fixture. Explicit shared-cell borders and a simple gridless auto-width case pass selected downloaded files. Conditional formats, merged cells, cell spacing, row splitting, border conflicts and general autofit remain open. Two new focused table cases fail the independent comparison even though their internal contracts pass. | [Table preparation](../../crates/edgedoc-display/src/build.rs#L1026), [cell bands and row height](../../crates/edgedoc-flow/src/lib.rs#L2419), and [fragment construction](../../crates/edgedoc-flow/src/lib.rs#L2511). The gridless measurement is restricted to simple left-aligned, unspanned text cells. | [Wordcraft table layout](../../../wordcraft/crates/layout/src/table.rs#L196) measures min/max content widths, spans and styles; [effective borders and merge handling](../../../wordcraft/crates/layout/src/table.rs#L535) resolve further interactions. |
| Notes | Open. A note reference is currently expanded to the literal `1`, and the prepared paragraph's note list is empty. Consequently the existing placeholder note-placement code is not connected to full imported note stories, numbering or continuation. `notes-dropcap` fails. | [Note expansion](../../crates/edgedoc-display/src/build.rs#L796), [empty note list](../../crates/edgedoc-display/src/build.rs#L844), and [placeholder note placement](../../crates/edgedoc-flow/src/lib.rs#L2109). | [Wordcraft note layout](../../../wordcraft/crates/layout/src/notes.rs) measures note stories, separators, splitting and continuation. |
| Fields and TOC | Partial for PAGE/NUMPAGES/SECTION/SECTIONPAGES; open for general fields and ranges. The long guide's first TOC entry disappears, and `docx4j-32-toc` fails on all four compared pages. A body PAGE upper-Roman fixture also differs from Wordcraft. | [Field reader](../../crates/edgedoc-wml/src/read.rs#L726) has a single optional field state per paragraph, and [field results](../../crates/edgedoc-display/src/build.rs#L859) use cached values for other commands. A nested begin replaces the existing state. | Wordcraft [field reader](../../../wordcraft/crates/docx/src/read/story/fields.rs#L23) uses bounded nested state and range fields across paragraphs; [field resolution](../../../wordcraft/crates/layout/src/fields.rs#L283) handles more instructions and switches. |
| Pictures, shapes and floating layout | Partial for simple raster output; open for Wordcraft placement and drawing parity. A picture in a text paragraph is currently emitted as a later block; a floating table is placed in ordinary flow. The inspected picture and SmartArt documents differ. | [Block construction](../../crates/edgedoc-display/src/build.rs#L605), [floating-table path](../../crates/edgedoc-flow/src/lib.rs#L1956), and [SVG images](../../crates/edgedoc-paint/src/svg.rs#L274). Unsupported shapes/text boxes are diagnosed. | Wordcraft [float anchoring and wrap](../../../wordcraft/crates/layout/src/floats.rs#L340) handles anchor positions and exclusions; its [display list](../../../wordcraft/crates/layout/src/display.rs#L36) includes shapes, rotation, turned content and art effects. |
| Glyph and page painting | Exercised for selected text cases, partial overall. EdgeDoc's SVG and Canvas paint loaded glyph outlines and the existing browser fixture matches; the independent gate also checks rasterized output. This does not establish equivalence for color glyphs, drawing effects or all border patterns. | [Glyph lowering](../../crates/edgedoc-paint/src/lower.rs#L330) and [SVG outlines](../../crates/edgedoc-paint/src/svg.rs#L342). | [Wordcraft display list](../../../wordcraft/crates/layout/src/display.rs#L36) and its page exporter provide the reference drawing commands. |
| Advanced layout and effects | Open. Drop caps/frames, page borders, line numbering, vertical text, ruby, automatic hyphenation, color glyphs and many character effects are either diagnosed, absent or not connected end to end. | [Fidelity diagnostics](../../crates/edgedoc-display/src/build.rs#L107); [gap inventory](20-rendering-parity.md). | Wordcraft has dedicated modules and display commands for several of these areas; each still needs a controlled differential fixture. |

## Highest-impact next implementation sequence

1. **Reader fidelity:** represent nested and multi-paragraph field ranges, preserving cached
   result runs and controls on edited saves. Start with the failing TOC and hyperlink/PAGEREF
   fixtures, and check both text and images. Preserve exact source order and bound nesting.
2. **Notes and floats:** connect note references to their actual stories, measure and reserve
   space, split continuations, and place anchored images with exclusions. Test a note or picture
   crossing a page boundary and interactions with sections and columns.
3. **Tables and list geometry:** port content-driven min/max sizing, border conflict precedence,
   conditional styles, spans and row splitting, then list-level indents, restarts and label
   placement. Retain the two failing table regressions and the large external examples.
4. **Fonts and complex scripts:** load supported embedded faces and implement styled,
   script-aware run fallback, bidi ordering and shaping. Compare Arabic, Hebrew and CJK cases
   using the same font bytes and verified source-to-glyph mappings.
5. **Drawings and paint:** connect inline and floating object positions, shapes/text boxes,
   transforms and effects to layout and painters. Extend the Wordcraft adapter in parallel;
   validate each new adapter primitive independently before crediting a visual match.
6. **Proof quality:** add a suitable acceptance path for genuinely textless structural/image
   documents; keep unsupported draws, missing font bytes, malformed input and timeouts distinct.
   Broaden real-world coverage and image inspection without relaxing existing tolerances or
   converting expected rejection into a rendering pass.

Each row above needs focused imported DOCX regression inputs, a matching Wordcraft display-list
comparison, image inspection where relevant, and tests for changed pagination/selection and
round-trip preservation. Even if this 327-case gate turns green, untested combinations would
remain outside the proof.
