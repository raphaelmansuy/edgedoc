# The deep audit: what WordCraft has, and what this library does with it

Date: 2026-11-04. Part of the EdgeDoc Display specification. See [README.md](README.md).

This chapter is the result of a systematic audit of the WordCraft checkout at
`/Users/raphaelmansuy/Github/03-working/wordcraft` (version 0.14.0). It is deeper than
[18-assessment.md](18-assessment.md), because it reads the source and the 531 layout tests, not
only the parity document.

## 1. Method

The audit ran four steps, in this order:

1. **Compare the property structs.** `CharProps`, `ParaProps`, and `SectionProps` were read from
   both trees, and each field was matched by meaning, not by name.
2. **Compare the layout tests.** The 531 test names under `crates/layout/src/tests` were grouped by
   capability, and each group was checked against the code here.
3. **Probe the behavior.** A probe program asked the code a specific question per behavior. A
   string that appears in the source is not proof: three probes first answered `yes` and the
   behavior was absent.
4. **Write the failing test first.** Each gap became a test in
   `crates/edgedoc-display/tests/gaps.rs`. A test that fails names work. A test that passes closes
   a gap.

A probe that reads the source can lie. The table in section 3 is the honest count. A row counts as
`Ported` only when a test in this repository fails if the behavior regresses.

## 2. The property structs

| Struct | WordCraft | EdgeDoc model | Difference |
|---|---|---|---|
| `CharProps` | 46 fields | 44 fields | The model keeps `sym_font` as `font`, `size` as `size_pt`, `cs` as `complex_script`, `hidden` as `vanish`, `engrave` as `imprint`. `ligatures` and `text_effects` are absent |
| `ParaProps` | 41 fields | 29 fields | The model keeps spacing as `spacing_before`, `spacing_after`, and `line`, and numbering as `num_id` and `ilvl`. 12 behaviors are absent: `beforeAutospacing`, `drop_cap`, `kinsoku`, `word_wrap`, `overflowPunct`, `topLinePunct`, `autoSpaceDE`, `autoSpaceDN`, `suppressHyphens`, `mirrorIndents`, `snapToGrid`, `outlineLevel` |
| `SectionProps` | 29 fields | 29 fields | Same count. The names differ, and the model carries every value that layout reads |

`DP-S-32`. A property that the model does not name arrives in `ParaProps::extra` or
`CharProps::extra` as raw XML. The display library may read it there. It reads none today, so a
`beforeAutospacing` paragraph takes the plain spacing rule.

## 3. The state after this audit

| State | Count | Meaning |
|---|---|---|
| Ported, with a test | 53 | A test in this repository fails if it regresses |
| Partial | 22 | The common case holds, and the limit is named |
| Placeholder | 4 | The page shows a marked box and a diagnostic |
| Gap, test written and failing | 1 | The test names work that is planned |
| Gap, no implementation | 28 | The default paints, and nothing says so |

## 4. What this audit found and closed

Four behaviors were wrong, and the screenshots did not show them. Each one now has a test.

| Defect | How it looked | Why the screenshot missed it | Fix | Test |
|---|---|---|---|---|
| Every character was a break opportunity | A word split mid-word: `smallestba` then `se.` | The text was correct. only the line end was wrong | A break is a point that equals a cluster's end, read from a set, not from a one-way scan | `justification_stretches_spaces_only` |
| A bold run painted with the regular face | Every heading looked regular | The layout was right, and the weight was plausible at 8 pt | The face map is keyed by family **and** style, so a bold run finds the bold face | `a_bold_run_paints_bold` |
| A character style resolved as a paragraph style | A character style's font and colour were ignored | A sample document uses a character style only for a link | The character chain accepts a character style, and a paragraph style still contributes its character properties | `a_bold_run_paints_bold` |
| The painter ignored the face weight | A bold run drew at weight 400 | A canvas at 1x hides a 700 against a 400 on a substituted face | The face table carries `bold` and `italic`, and the SVG writes `font-weight` and `font-style` | `a_bold_run_paints_bold` |

The first row is the largest. A wrong break point moves every line after it. That moves a page
break, which changes a page number, which changes `NUMPAGES`. The sample report showed 15 pages
before the fix and 15 after it. Only a check of the character at a line end found the defect.

## 5. The gaps, in the order a business document hits them

Each row states the behavior, the WordCraft test that measures it, the effect in this library, and
the work. A gap is not a defect: `DP-N-06` rule 4 allows a marked placeholder. A gap that moves
another block without a diagnostic breaks that rule, and those rows are marked **moves**.

| # | Gap | WordCraft test | Effect here | Moves | Work |
|---|---|---|---|---|---|
| 1 | Text wrap square beside a float | `a_word_too_wide_for_the_strip_beside_a_float_moves_on_whole` | Text runs over the picture | **moves** | 3 weeks |
| 2 | A note continues on the next page | `long_note_continues` | A long note paints its first lines and stops | **moves** | 2 weeks |
| 3 | Note numbering restarts per page or section | `note_restart_after_move` | Every note is numbered from 1 | no | 3 days |
| 4 | A continuous break that changes the column count | `continuous_sections_one_after_another` | A new page starts, with a diagnostic | **moves** | 1 week |
| 5 | Table conditional formats | `row_bands_shade_only_with_a_band_size` | A banded table paints with no band | no | 1 week |
| 6 | CJK counting formats | `chinese_counting_list_labels_and_their_font_runs` | A CJK list paints a decimal number | no | 1 week |
| 7 | Circled and enclosed numbers | `numfmt_table` | A decimal number paints instead | no | 2 days |
| 8 | Picture bullets | `picture_bullet` | The level default bullet paints | no | 3 days |
| 9 | `STYLEREF` in a header | `styleref_in_headers` | A running title stays blank | no | 1 week |
| 10 | `SECTIONPAGES` counts its own section | `sectionpages` | It counts the whole document | no | 2 days |
| 11 | Text boxes | `text_boxes_show_in_headers_footers_cells_and_notes` | A callout paints a placeholder | no | 2 weeks |
| 12 | Drop caps | `drop_caps` | The first letter is body size | no | 1 week |
| 13 | Ruby text | `ruby_text_sits_over_its_base` | The base text paints, the ruby does not | no | 1 week |
| 14 | Vertical text `tbRl` | `vertical_text` | The text paints upright | no | 2 weeks |
| 15 | Ligature switches `w14:ligatures` | `ligatures_follow_w14_ligatures` | `liga` and `clig` are always on | no | 3 days |
| 16 | Automatic paragraph spacing of 14 pt | `automatic_paragraph_spacing_like_word` | The literal space paints | no | 2 days |
| 17 | A document grid `w:docGrid` | `line_grid_snaps_line_heights` | Lines take their natural height | no | 2 weeks |
| 18 | Space between CJK and Latin text | `autospace` | The characters touch | no | 3 days |
| 19 | Mirror margins on an even page | `mirror_margins` | Both pages take the same left margin | no | 3 days |
| 20 | Columns balance on a last page | `columns_before_a_continuous_break_are_balanced` | The first column takes all the text | no | 4 days |
| 21 | Page borders and page colour | `page_borders` | Neither paints | no | 3 days |
| 22 | Line numbers | `line_numbers` | None paint | no | 3 days |
| 23 | An author colour for a revision | `tracked_changes_colour_by_author` | One colour paints for every author | no | 2 days |
| 24 | Colour glyphs (COLR, `sbix`) | `emoji_are_drawn_in_colour` | The outline glyph paints | no | 1 week |
| 25 | An `.odttf` font in the package | `odttf` | The substitute face paints | no | 4 days |
| 26 | Hyphenation dictionaries | `hyphenation` | No automatic hyphen | no | 2 weeks |
| 27 | Distributed Thai alignment | `thai_distributed` | Every cluster takes an equal share | no | 3 days |
| 28 | A private-use bullet with no face | `private_use_bullet_no_font_draws` | The code point paints from the fallback | no | 1 day |
| 29 | Positional tabs `w:ptab` | `right_positional_tab_ends_text_on_the_right_margin` | The tab goes to the next stop | no | 2 days |

`DP-P-35`. Two gaps break rule 4, and they are rows 1, 2, and 4. They are the first three units of
work for the next version. Every other gap paints a visible default, and the page around it is
correct.

## 6. What a probe cannot prove

`DP-Q-43`. The audit found four defects by probing, and the screenshots found none of them. The
lesson is in the order the checks run:

| Check | Finds | Misses |
|---|---|---|
| A compile | A broken call | Every wrong pixel |
| A unit test | A wrong rule | A rule never called |
| A golden display list | A changed page | A page that was always wrong |
| A screenshot | A missing colour, a blank area | A plausible value, such as weight 400 against 700 |
| A byte-level assertion | A line that ends inside a word | Why the line ends there |

`DP-Q-44`. A fidelity claim needs a check that names the value. The test
`a_line_ends_at_a_word_boundary` reads the character at the break point and fails when the break
falls between two letters. The screenshot showed the same page and passed.

`DP-Q-46`. A test that guards a defect must fail when the defect returns. The first two versions of
that test passed against a deliberately broken line breaker. One read the wrong field, and the
other read the wrong side of the break. The procedure is:

1. Write the test.
2. Break the code on purpose, in the way the defect broke it.
3. Run the test. It must fail, and the message must name the defect.
4. Restore the code, and run it again. It must pass.

A test that survives step 3 is not a guard. It is a comment that costs a compile.

## 7. The downloaded corpus

`DP-Q-47`. A document that this library wrote cannot prove that the reader understands a document
that Word wrote. The corpus in [../../tests/corpus/README.md](../../tests/corpus/README.md) holds
33 real Word files from the python-docx project.

Its first run found a defect that every synthetic document missed. 12 files did not open, because
Word writes a newline after the last close tag and the parser read it as a second root.

| Check | File | Result |
|---|---|---|
| The package opens | `crates/edgedoc-display/tests/corpus.rs` | 33 of 33 |
| A page exists | the same file | 33 of 33 |
| A document with text paints it | the same file | 24 of 24 |
| A fragment stays inside its page | the same file | 33 of 33 |
| A table draws a table fragment | the same file | more than 10 |
| The browser paints the page | `tools/display_corpus_e2e.mjs` | 128 of 128 |

## 8. The tests this audit added

| File | Tests | Purpose |
|---|---|---|
| `crates/edgedoc-display/tests/gaps.rs` | 6 | One test per behavior the audit probed, named after the WordCraft test |
| `crates/edgedoc-display/tests/fidelity.rs` | 16 | The behavior of the whole pipeline on a real document |
| `crates/edgedoc-text/src/lib.rs` | 30 | The line breaker, including the break-point set |

The workspace holds 249 tests, and every one passes.

`DP-Q-45`. A gap that this chapter lists and that has no test is a defect in this chapter. The
count in section 3 comes from the tables in [18-assessment.md](18-assessment.md) and this file,
and a reader can check each row.

## 9. How to close a row

The procedure is the one in [17-wordcraft-map.md](17-wordcraft-map.md), clause 122, with one
addition:

1. Read the WordCraft test for the behavior, and the code that it covers.
2. Write the test here first, in `tests/gaps.rs`, and watch it fail.
3. Port the behavior, and move the test to `tests/fidelity.rs`.
4. Run `node tools/display_e2e.mjs`, and inspect the screenshot for the page the behavior changes.
5. Update the row in this chapter, and the count in section 3.

A row that moves another block needs a sixth step: a check that the diagnostic names the cause.
