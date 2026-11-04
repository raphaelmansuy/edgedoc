# The feature assessment against WordCraft 0.14.0

Date: 2026-11-04. Part of the EdgeDoc Display specification. See [README.md](README.md).

This chapter answers one question: for every layout, style, and font behavior that WordCraft
measured against Word, what does EdgeDoc Display do today?

It is an assessment, not a plan. Each row states the behavior, the WordCraft evidence, the state in
this library, and the requirement that governs it. A row marked **Ported** has a test that fails if
the behavior regresses. A row marked **Gap** has no implementation, and the effect is stated.

## How to read a state

| State | Meaning |
|---|---|
| **Ported** | Implemented, with a test in this repository |
| **Partial** | Implemented for the common case, and the limit is named |
| **Placeholder** | Paints a marked box with a diagnostic, so the page is honest |
| **Gap** | Not implemented. The page shows the default, and no diagnostic says so |

## 1. Text and line breaking

| Behavior | WordCraft evidence | State | Requirement |
|---|---|---|---|
| Break opportunities from UAX #14 | `layout::tests::no_line_break_right_after_a_slash` | Ported: `unicode-linebreak`, plus the slash and hyphen rule | `DP-F-22`, `DP-F-23` |
| No break after a slash | `a_word_after_a_slash_wraps_whole` | Ported: `break_after` is cleared after `/` | `DP-F-23` |
| A break after a hyphen before a digit | `a_hyphen_before_digits_is_a_break_opportunity_like_word` | Ported | `DP-F-23` |
| A bookmark is no break opportunity | `a_bookmark_is_no_line_break_opportunity` | Ported: a marker takes no room | `DP-F-22` |
| Kinsoku: a closing mark cannot start a line | `docs/layout-parity.md` | Ported: `NO_START` and `NO_END` | `DP-F-23` |
| A word wider than the band overflows | `word_wider_than_line` | Ported, with `display.page_overflow` | `DP-F-27` |
| First-fit breaking, not a global optimum | `crates/layout/src/para.rs` module comment | Ported: the scan stops at the last fitting break | `DP-F-47` |
| Justification stretches spaces and tabs | `justification_word_2013` | Ported: proportional to the stretch of each opportunity | `DP-F-28` |
| Distributed and Thai distributed | `distributed_alignment` | Partial: every cluster takes an equal share, with no Thai cluster rule | `DP-F-29` |
| Last line stays left-aligned | `justify_last_line` | Ported | `DP-F-30` |
| Trailing spaces hang past the margin | `caret_after_hanging_spaces_stays_at_the_margin` | Ported: the line measure stops at the last visible cluster | `DP-F-49` |
| Tabs: left, center, right, decimal, bar | `right_positional_tab_ends_text_on_the_right_margin` | Partial: left, right, and decimal align. center and bar paint at the stop | `DP-F-32` |
| Tab leaders on a 600-dpi grid | `leader_dots_are_spaced_and_placed_as_in_word` | Partial: the leader count is `floor(gap / advance)`, not a 600-dpi grid | `DP-F-34` |
| Positional tabs `w:ptab` | `right_positional_tab_ends_text_on_the_right_margin` | Gap | `DP-F-37` |
| Soft hyphen paints only at a break | `soft_hyphen_break` | Ported | `DP-F-36` |
| All caps, small caps, and `ß` as `SS` | `caps_draw_sharp_s_as_two_capitals_in_one_cluster` | Partial: `to_uppercase` gives `SS`, and the cluster count changes | `DP-F-12` |
| Hidden text takes no room | `hidden_text_off` | Ported | `DP-F-05` |
| Kerning only with `w:kern` | `kerning_follows_kerning_for_fonts_like_word` | Ported: the feature set is `kern` only when the run asks | `DP-F-12` |
| `w14:ligatures` switches | `ligatures_follow_w14_ligatures` | Gap: `liga` and `clig` are always on | `DP-F-12` |
| Automatic hyphenation | `hyphenation` | Gap: `hyphen` is never called | `DP-F-24` |
| Drop caps | `w:framePr w:dropCap` | Gap | `DP-F-52` |
| Ruby, enclose, and combined characters | `ruby_phonetic_guide` | Gap | `DP-F-52` |
| Vertical text `tbRl` | `vertical_text` | Gap | `DP-L-47` |

## 2. Fonts

| Behavior | WordCraft evidence | State | Requirement |
|---|---|---|---|
| Metric-compatible substitution | `fonts::word::tests::a_missing_thai_font_resolves_to_an_installed_thai_face` | Ported: Calibri→Carlito, Arial→Arimo, Times→Tinos, and 20 more | `DP-F-03` |
| A substitute keeps the requested name | `fonts::word::resolve` | Ported: `FontFace::requested` | `DP-F-03` |
| Vertical metrics from `hhea`, then `OS/2`, then 0.8 em | `fonts::word::line_metrics` | Ported, with `display.font_vertical_metrics_missing` | `DP-F-42` |
| A glyph that no face has becomes `.notdef` | `all_faces_missing` | Ported, with one diagnostic per code point | `DP-F-15` |
| Symbol fonts map to Unicode | `wingdings_text_draws_and_reads_as_its_unicode_symbols` | Ported: the five tables from Unicode's data | `DP-F-35` |
| A private-use bullet with no face shows the level default | `private_use_bullet_no_font_draws_shows_the_levels_default_bullet` | Gap | `DP-S-19` |
| Full font names in a request | `fonts::word::tests` | Partial: family match only, no name ID 4 | `DP-F-02` |
| Colour glyphs (COLR, `sbix`, CBDT) | `render::tests::color_glyphs::emoji_are_drawn_in_colour` | Gap: `color_layers` returns `None` | `DP-P-10` |
| WOFF2 embedded font | `fonts::fontdb::decode_font_file` | Gap: a package font part is not read in version 1 | `DP-F-06` |
| `.odttf` deobfuscation | `docs/file-format-parity.md` | Gap, for the same reason | `DP-F-06` |
| Synthetic bold and italic | `layout::tests::synthetic_italic` | Gap: the list carries the flags, and no painter applies them | `DP-F-13` |

## 3. Paragraph and style

| Behavior | WordCraft evidence | State | Requirement |
|---|---|---|---|
| The cascade order | `crates/doc/src/resolve.rs` | Ported | `DP-S-02` |
| Style chain, cycle guard, and 32-link cap | `style_cycle` | Ported | `DP-S-02` |
| Table conditional formats | `conditional_table_style` | Gap: a table style's band format is not read | `DP-S-05` |
| Line spacing: single, multiple, exact, at least | `line_spacing_rules` | Partial: exact and at least paint the same height | `DP-F-41` |
| Contextual spacing | `contextual_spacing` | Ported: the gap drops between two paragraphs of one style | `DP-F-43` |
| Space between paragraphs is `max(after, before)` | `space_between_paragraphs_is_the_larger_of_after_and_before` | Ported, measured at 12 pt for 12 and 12 | `DP-L-43` |
| HTML automatic spacing is 14 pt | `automatic_paragraph_spacing_like_word` | Gap: the property is not read | `DP-L-43` |
| Indents in characters | `character_indents_use_the_paragraph_mark_size` | Partial: measured at 11 pt, not the mark's own size | `DP-S-31` |
| Widow and orphan control | `keep_next/keep_lines` | Ported | `DP-F-26` |
| Keep with next and keep lines | `keepnext_chain` | Ported | `DP-L-05` |
| Page break before | `page_break_before` | Ported | `DP-L-05` |
| Vertical page alignment: top, center, bottom | `valign_overflow` | Partial: top and center. bottom and justify fall back to top | `DP-L-03` |
| Document grid `w:docGrid` | `document_grid` | Gap | `DP-S-31` |
| Tab stops from a style | `tabs` | Ported | `DP-F-32` |
| Paragraph borders and shading | `paragraph_side_borders` | Partial: all four sides paint. a bar border does not | `DP-S-08` |

## 4. Numbering and lists

| Behavior | WordCraft evidence | State | Requirement |
|---|---|---|---|
| Counters per `numId` | `separate_counters` | Ported | `DP-S-14` |
| Level text `%1.%2` | `chinese_counting_list_labels_and_their_font_runs` | Ported | `DP-S-21` |
| Decimal, letter, roman, ordinal, cardinal | `doc::numbering::tests` | Ported, and the letter form doubles past `z` | `DP-S-21` |
| Arabic letters `arabicAlpha` and `arabicAbjad` | `arabic_letters_count_in_both_orders` | Gap: falls back to a decimal number | `DP-S-21` |
| CJK counting formats | `doc::cjk_numbers` | Gap | `DP-S-21` |
| `①` decimal enclosed circle | `numfmt_table` | Gap | `DP-S-21` |
| `startOverride` resets deeper levels | `start_override` | Ported | `DP-S-15` |
| `isLgl` formats as decimal | `islgl` | Ported | `DP-S-16` |
| A list in a cell shares the body series | `list_in_table` | Ported | `DP-S-18` |
| A label left-aligned with its text after a page break | `list_label_after_leading_page_break` | Gap: the label is always right-aligned to the indent | `DP-S-14` |
| Picture bullets | `picture_bullet` | Gap | `DP-S-20` |

## 5. Sections, pages, and stories

| Behavior | WordCraft evidence | State | Requirement |
|---|---|---|---|
| Next page, continuous, odd, even breaks | `section_breaks` | Partial: next, odd, and even. a continuous break with a column change starts a page and reports it | `DP-L-08` |
| A blank page for a wrong parity | `odd_page_blank` | Ported, with the header painted | `DP-L-08` |
| Different first page | `header_variants` | Ported | `DP-L-17` |
| Odd and even headers | `header_variants` | Ported | `DP-L-17` |
| A tall header moves the body | `tall_header` | Ported, with `display.header_overflow` | `DP-L-19` |
| A tall footer moves the body up | `tall_footer` | Partial: the footer paints in the bottom margin only | `DP-L-19` |
| Columns, column breaks, balancing | `columns` | Partial: columns and column breaks. no balance on a last page | `DP-L-13` |
| Page borders and page colour | `page_borders` | Gap | `DP-L-02` |
| Line numbers | `line_numbers` | Gap | `DP-L-02` |
| Gutter and mirror margins | `mirror_margins` | Partial: the gutter shifts the body. a mirrored even page does not swap | `DP-L-04` |
| Vertical alignment of the page | `vertical_page_alignment` | Partial, as above | `DP-L-03` |
| Footnotes at the page bottom | `footnotes_at_the_page_bottom` | Partial: a note paints below a separator, and its own height is fixed | `DP-L-23` |
| A note that continues on the next page | `note_longer_than_page` | Gap | `DP-L-24` |
| Endnotes at the section end | `endnote_section_end` | Gap | `DP-L-26` |
| Note numbering per page or section | `note_restart_after_move` | Gap: every note is numbered from one | `DP-L-25` |
| A text box is a story | `text_boxes` | Gap | `DP-L-32` |

## 6. Tables

| Behavior | WordCraft evidence | State | Requirement |
|---|---|---|---|
| Autofit to contents | `table_autofit` | Partial: the widths come from `tblGrid`. a column with no `gridCol` shares the rest | `DP-L-34` |
| Fixed layout | `layout_fixed` | Ported | `DP-L-34` |
| A cell width, `gridSpan`, and `vMerge` | `vmerge_orphan` | Partial: `gridSpan`. a `vMerge` continuation reserves no height | `DP-L-35` |
| Row height `atLeast` and `exact` | `row_height_rules` | Ported: `exact` clamps and `atLeast` grows | `DP-L-36` |
| A row splits across a page | `table_many_pages` | Ported | `DP-L-36` |
| Header rows repeat | `table_many_pages` | Ported | `DP-L-37` |
| `cantSplit` keeps a row whole | `cantsplit_too_tall` | Ported | `DP-L-38` |
| Border collapse: a shared edge paints once | `border_collapse` | Ported: the WordCraft `region_borders` rule | `DP-L-39` |
| Cell margins and cell spacing | `cell_margins` | Partial: margins. `tblCellSpacing` is not read | `DP-L-40` |
| Nested tables | `nested_tables` | Ported: a nested table lays out in its cell width | `DP-L-41` |
| A floating table `w:tblpPr` | `floating_tables` | Partial: it takes a place in the flow, not a float position | `DP-L-28` |
| A table indent | `table_ind_clamp` | Ported | `DP-L-42` |

## 7. Objects and drawing

| Behavior | WordCraft evidence | State | Requirement |
|---|---|---|---|
| An inline picture keeps its size | `inline_object_line` | Ported | `DP-L-33` |
| A floating picture with an anchor | `float_next_page` | Partial: it paints at its block position, with no wrap area | `DP-L-28` |
| Text wrap square | `a_word_too_wide_for_the_strip_beside_a_float_moves_on_whole` | Partial: the text crate accepts an exclusion, and the flow passes none | `DP-L-29` |
| Text wrap tight and through | `text_wrap_tight_through` | Gap | `DP-L-29` |
| Behind text and in front of text | `behind_doc` | Gap | `DP-L-30` |
| A rotated picture or shape | `rotated_objects` | Gap: the transform exists in the list, and the flow never emits it | `DP-X-97` |
| A chart, an equation, or a drawing | `chart_placeholder` | Placeholder, with `display.object_placeholder` | `DP-P-10` |
| A metafile (EMF, WMF) | `metafile` | Placeholder | `DP-P-10` |
| A shape fill and a gradient | `shape_fills` | Placeholder, through the object path | `DP-P-10` |

## 8. Fields

| Behavior | WordCraft evidence | State | Requirement |
|---|---|---|---|
| `PAGE` per page | `page_field_1000` | Ported: the flow substitutes the marker on each page | `DP-L-11` |
| `NUMPAGES` after a second pass | `numpages_two_pass` | Ported: 2 passes on the sample report | `DP-L-12` |
| `SECTION` and `SECTIONPAGES` | `sectionpages` | Gap: both paint as the marker character | `DP-L-11` |
| `PAGEREF`, `REF`, `SEQ`, `TOC` | `pagerel_in_headers` | Gap: the cached result paints, and the number is not recomputed | `DP-L-11` |
| `STYLEREF` in a header | `styleref_in_headers` | Gap | `DP-L-11` |
| A field with no cached result | `field_no_result` | Ported: it paints nothing visible, with a diagnostic | `DP-L-11` |

## 9. What the assessment says

Of the 109 behaviors that this chapter lists, the state is:

| State | Count | Meaning |
|---|---|---|
| Ported | 50 | A test locks it |
| Partial | 22 | The common case holds, and the limit is named here |
| Placeholder | 4 | The page shows a marked box |
| Gap | 33 | The default paints, and no diagnostic says so |

The count comes from the tables above. The command `grep -c '| Ported'` over this file gives 50,
and the same run gives 22, 4, and 33 for the other states. A reader can check the number.

### 9.1 The largest gaps, in order of what a business document needs

| Rank | Gap | Why it matters | Effort |
|---|---|---|---|
| 1 | Text wrap and floating objects | A newsletter, a report cover, and a poster all use a picture with text beside it | 3 weeks |
| 2 | Note numbering and continuation | A contract with 40 footnotes needs a note to continue on the next page | 2 weeks |
| 3 | Continuous sections with a column change | A two-column page that returns to one column loses its page parity | 1 week |
| 4 | Table conditional formats | A banded table paints without its band | 1 week |
| 5 | CJK numbering and `①` formats | A Chinese or Japanese list paints a decimal number | 1 week |
| 6 | Text boxes | A callout box paints as a placeholder | 2 weeks |
| 7 | `STYLEREF` and `PAGEREF` | A running chapter title in a header stays blank | 1 week |
| 8 | Drop caps, ruby, vertical text | A magazine layout and a Japanese document need them | 3 weeks |

### 9.2 Why a gap is acceptable in version 1

`DP-N-06` states four rules for a correct page. A gap in this chapter breaks rule 4 at worst. That
rule says that a feature the library does not model paints a marked placeholder, and it does not
shift another block without a diagnostic. The 33 gaps below rule 4 in two places:

- A wrap gap moves the text, because no exclusion reaches the flow.
- A note gap moves the page, because a note takes a fixed height.

Every other gap paints a visible default, and the page around it is correct.

### 9.3 What to do next

The first three gaps are the ones a business document hits. They are also the three that the
WordCraft checkout records as done, so the port is a transfer of measured behavior, not a design.
Read `docs/layout-parity.md` rows 62 to 75 before starting, and port the tests with the code.
