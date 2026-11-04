# Requirement index, test map, and the DRY and SOLID audit.

> Part of the EdgeDoc Display specification. Date: 2026-11-04. Requirement ids are
> normative. See [the index](README.md).

## 149. How to read this clause

`DP-N-12`. The index lists every requirement of this specification by class. The test column
names the test that locks it. A requirement with more than one test names the first one.

`DP-N-13`. The test name is a CI name. The suite runs with a single thread, and the command
`cargo test <name>` runs one test.

`DP-N-14`. A class row without a test is a defect of this document. The test
`traceability_complete` reads this file and [13-edge-cases.md](13-edge-cases.md), and it fails
on a requirement that names no test.

## 150. Class I: intent

| Id | Clause | Subject | Test |
|---|---|---|---|
| `DP-I-01` | 1.4 | Layout is Rust and WASM, no browser measurement | `layout_no_browser_measure` |
| `DP-I-02` | 2 | Paged document with Word page geometry | `page_geometry_a4` |
| `DP-I-03` | 2 | Zoom 10% to 800%, anchor fixed | `zoom_anchor_stable` |
| `DP-I-04` | 2 | One scroll strip, nearby pages only | `scroll_mounts_nearby` |
| `DP-I-05` | 2 | Selection by mouse and keyboard | `selection_mouse_and_key` |
| `DP-I-06` | 2 | Search and move between hits | `search_navigate` |
| `DP-I-07` | 2 | Print matches the screen | `print_200` |
| `DP-I-08` | 2 | One call loads and paints | `one_call_load` |
| `DP-I-09` | 2 | A failed page shows a placeholder, not a blank | `page_placeholder_not_blank` |
| `DP-I-10` | 3 | The library does not edit | `no_edit_api` |
| `DP-I-11` | 3 | No macro, script, or OLE execution | `no_active_content` |
| `DP-I-12` | 3 | No byte-identical promise against Word | `fidelity_tiers_stated` |
| `DP-I-13` | 3 | No text editor, comment editor, or form filler | `no_editor_api` |
| `DP-I-14` | 4 | Tier claim T0 to T2, plus the T3 placeholder | `tier_claim` |
| `DP-I-15` | 4 | Five gates with a measure and a target | `gate_report_present` |
| `DP-I-16` | 5 | Metric targets with a measurement | `perf_first_page` |
| `DP-I-17` | 6 | Reader roles | n/a (documentation) |
| `DP-I-18` | 7 | Pagination is computed, not read | `stale_page_break_hint` |

## 151. Class N: normative

| Id | Clause | Subject | Test |
|---|---|---|---|
| `DP-N-01` | 8 | Each requirement maps to a source | `traceability_complete` |
| `DP-N-02` | 9 | The text layer follows the pdf.js contract | `text_layer_alignment` |
| `DP-N-03` | 10 | One meaning per term | `glossary_terms` |
| `DP-N-04` | 11 | The claimed classes | `tier_claim` |
| `DP-N-05` | 12 | A page is correct when it passes the gates | `gate_report_present` |
| `DP-N-06` | 12 | The meaning of "perfect", in four rules | `fidelity_four_rules` |
| `DP-N-07` | 117 | A WordCraft-proven feature is portable | `wordcraft_map_current` |
| `DP-N-08` | 120 | A known gap is fixed or stated | `known_gaps_listed` |
| `DP-N-09` | 120 | The comparison harness is in phase 2 | `plan_phase_order` |
| `DP-N-10` | 122 | The port procedure | `ported_test_citation` |
| `DP-N-11` | 122 | A copied test cites its source | `ported_test_citation` |
| `DP-N-12` | 149 | The index lists every requirement | `traceability_complete` |
| `DP-N-13` | 149 | A test name is a CI name | `traceability_complete` |
| `DP-N-14` | 149 | A requirement without a test fails | `traceability_complete` |

## 152. Class A: architecture

| Id | Clause | Subject | Test |
|---|---|---|---|
| `DP-A-01` | 13 | A crate depends only on a lower layer | `layer_rules` |
| `DP-A-02` | 13 | No display crate reads XML or ZIP | `no_xml_in_display` |
| `DP-A-03` | 13 | Paint does not depend on flow | `paint_independent` |
| `DP-A-04` | 14 | The facade is the only named crate | `facade_surface` |
| `DP-A-05` | 15 | One trait, one reason | `trait_audit` |
| `DP-A-06` | 15 | Flow depends on traits only | `flow_trait_only` |
| `DP-A-07` | 16 | Load returns before layout ends | `load_before_layout` |
| `DP-A-08` | 16 | Range layout runs the required passes | `range_respects_passes` |
| `DP-A-09` | 17 | Cancel keeps finished pages | `cancel_keeps_pages` |
| `DP-A-10` | 17 | Relayout reasons | `relayout_reasons` |
| `DP-A-11` | 17 | A cached tile paints with no work | `cached_tile_no_work` |
| `DP-A-12` | 18 | One crate changes for one reason | `crate_ownership` |
| `DP-A-13` | 18 | No `match` on an object kind in flow | `object_renderer_open` |
| `DP-A-14` | 18 | A missing glyph is `.notdef`, not `Err` | `all_faces_missing` |
| `DP-A-15` | 18 | Traits stay separate | `trait_audit` |
| `DP-A-16` | 18 | Flow and paint depend on traits | `flow_trait_only` |
| `DP-A-17` | 19 | One geometry type | `single_rect_type` |
| `DP-A-18` | 19 | One unit bridge | `unit_conversion_once` |
| `DP-A-19` | 19 | One bidi resolver | `bidi_once` |
| `DP-A-20` | 19 | One line breaker | `line_break_once` |
| `DP-A-21` | 19 | One style resolution | `style_once` |
| `DP-A-22` | 19 | One codec | `codec_round_trip` |
| `DP-A-23` | 19 | One hit test | `hit_once` |
| `DP-A-24` | 19 | One source of page size | `page_size_from_worker` |
| `DP-A-25` | 20 | Layout is a pure function | `layout_deterministic` |
| `DP-A-26` | 20 | No clock read inside a page | `no_clock_in_page` |
| `DP-A-27` | 20 | `f32` glyphs, `f64` accumulated height | `height_accumulation` |
| `DP-A-28` | 118 | The layer table is a data table | `layer_rules` |
| `DP-A-29` | 118 | The one deliberate split from WordCraft | `layer_rules` |
| `DP-A-30` | 119.1 | Pixel budgets live in geometry | `pixel_budget_module` |

## 153. Class P: pipeline

| Id | Clause | Subject | Test |
|---|---|---|---|
| `DP-P-01` | 21 | A stage failure degrades at one level | `stage_failure_level` |
| `DP-P-02` | 21 | Each stage runs at its stated frequency | `stage_frequency` |
| `DP-P-03` | 22 | Four package flavors, Flat OPC rejected | `flat_opc_rejected` |
| `DP-P-04` | 22 | Only the parts that paint are read | `parts_read_list` |
| `DP-P-05` | 22 | Active content is not read | `no_active_content` |
| `DP-P-06` | 22 | The three display limits | `page_limit` |
| `DP-P-07` | 22 | `open` never panics | `hostile_corpus` |
| `DP-P-08` | 23 | The default policy denies external resources | `external_denied` |
| `DP-P-09` | 23 | A fetch has a timeout and a size limit | `external_timeout` |
| `DP-P-10` | 23 | A missing resource paints a marked placeholder | `missing_image_part` |
| `DP-P-11` | 23 | An SVG is sanitized | `svg_script` |
| `DP-P-12` | 23 | Font licence bits are honoured | `restricted_font` |
| `DP-P-13` | 23 | A crop is a clip, not a resample | `crop_out_of_bounds` |
| `DP-P-14` | 24 | One computed document | `computed_doc_once` |
| `DP-P-15` | 24 | Style resolution happens once | `style_once` |
| `DP-P-16` | 25 | The layout request fields | `layout_request_fields` |
| `DP-P-17` | 25 | Flow produces geometry, never a raster | `no_raster_in_flow` |
| `DP-P-18` | 25 | Layout is restartable | `layout_resume` |
| `DP-P-19` | 26 | The encoder reuses one buffer | `encoder_buffer_reuse` |
| `DP-P-20` | 26 | Glyph runs are deduplicated | `glyph_run_dedup` |
| `DP-P-21` | 27 | Two painters agree on a page | `painter_parity` |
| `DP-P-22` | 28 | A diagnostic is one record | `diagnostic_shape` |
| `DP-P-23` | 28 | The code set is closed and stable | `diagnostic_codes_frozen` |
| `DP-P-24` | 28 | A diagnostic is deduplicated | `glyph_dedup` |
| `DP-P-25` | 28 | Diagnostics reach the panel | `diagnostic_panel` |
| `DP-P-26` | 119.7 | Every limit has a counterpart and a reason | `limits_table` |

## 154. Class S: style

| Id | Clause | Subject | Test |
|---|---|---|---|
| `DP-S-01` | 29 | The cascade is resolved once | `style_once` |
| `DP-S-02` | 30 | The resolution order is fixed | `cascade_order` |
| `DP-S-03` | 30 | A missing `basedOn` ends the chain | `missing_based_on` |
| `DP-S-04` | 30 | An absent attribute means "not set" | `absent_is_not_default` |
| `DP-S-05` | 30 | The table conditional order | `conditional_table_style` |
| `DP-S-06` | 30 | Numbering precedes the paragraph style | `numbering_before_style` |
| `DP-S-07` | 30 | A change record is read, the final form paints | `revision_shown` |
| `DP-S-08` | 31 | One owner per property | `property_owner` |
| `DP-S-09` | 31 | A computed record is a plain `Copy` value | `computed_is_copy` |
| `DP-S-10` | 32 | Theme font attributes resolve | `theme_font` |
| `DP-S-11` | 32 | Tint then shade | `theme_tint_shade` |
| `DP-S-12` | 32 | A missing theme falls back | `missing_theme` |
| `DP-S-13` | 32 | Two default styles | `two_defaults` |
| `DP-S-14` | 33 | Counters are per `numId` | `separate_counters` |
| `DP-S-15` | 33 | `startOverride` resets deeper levels | `start_override` |
| `DP-S-16` | 33 | `isLgl` formats as decimal | `islgl` |
| `DP-S-17` | 33 | The legacy indent rule | `legacy_indent` |
| `DP-S-18` | 33 | A list in a cell shares the body series | `list_in_table` |
| `DP-S-19` | 33 | Bullet characters come from a table | `bullet_unmapped` |
| `DP-S-20` | 33 | A picture bullet is a resource | `picture_bullet` |
| `DP-S-21` | 33 | Each `numFmt` value has a case | `numfmt_table` |
| `DP-S-22` | 33 | An unknown format paints a decimal number | `unknown_numfmt` |
| `DP-S-23` | 34 | One section per `sectPr`, in order | `section_ranges` |
| `DP-S-24` | 34 | A `sectPr` in a paragraph ends the section | `sectpr_in_paragraph` |
| `DP-S-25` | 34 | The break type before the section | `section_break_types` |
| `DP-S-26` | 34 | A section with no size takes the previous | `no_sectpr` |
| `DP-S-27` | 35 | The compatibility set | `compat_table` |
| `DP-S-28` | 35 | An ignored setting is logged once | `compat_ignored` |
| `DP-S-29` | 119.4 | Resolved records are values | `computed_is_copy` |
| `DP-S-30` | 119.4 | The measured numbering rules | `separate_counters` |

## 155. Class F: fonts and text

| Id | Clause | Subject | Test |
|---|---|---|---|
| `DP-F-01` | 36 | One measure function | `measure_once` |
| `DP-F-02` | 37 | One face resolution per triple | `face_resolution_once` |
| `DP-F-03` | 37 | The substitute table is data | `substitute_table` |
| `DP-F-04` | 37 | A missing face keeps its metrics | `complex_script_fallback` |
| `DP-F-05` | 37 | Faces load lazily | `lazy_faces` |
| `DP-F-06` | 37 | `.odttf` deobfuscation | `odttf` |
| `DP-F-07` | 37 | A built-in fallback face | `builtin_face` |
| `DP-F-08` | 38 | The `GlyphRun` shape | `glyph_run_shape` |
| `DP-F-09` | 38 | A shape call never splits a cluster | `combining_stack` |
| `DP-F-10` | 38 | A ligature maps several clusters | `ligature` |
| `DP-F-11` | 38 | Kerning comes from `GPOS` | `no_gpos` |
| `DP-F-12` | 38 | The feature set, and `w:kern` | `kerning_follows_w_kern` |
| `DP-F-13` | 38 | A variable face, and synthesized bold | `variable_weight` |
| `DP-F-14` | 38 | Synthetic italic is 12 degrees | `synthetic_italic` |
| `DP-F-15` | 38 | A missing glyph becomes `.notdef` | `all_faces_missing` |
| `DP-F-16` | 39 | Paragraph direction | `bidi_paragraph_direction` |
| `DP-F-17` | 39 | Bidi levels resolve before breaking | `bidi_once` |
| `DP-F-18` | 39 | A break keeps logical order | `rtl_latin_word` |
| `DP-F-19` | 39 | Digits in Arabic text | `arabic_digits` |
| `DP-F-20` | 39 | Mirrored characters | `mirror_pairs` |
| `DP-F-21` | 39 | Bracket pairs | `bracket_pairs` |
| `DP-F-22` | 40 | Break opportunities from UAX #14 | `break_opportunity` |
| `DP-F-23` | 40 | The Word rules on top of UAX #14 | `kinsoku` |
| `DP-F-24` | 40 | Hyphenation is opt-in | `hyphenation_off_by_default` |
| `DP-F-25` | 40 | First-fit breaking, not a global optimum | `first_fit_break` |
| `DP-F-26` | 40 | A paragraph splits at a line boundary | `huge_paragraph` |
| `DP-F-27` | 40 | A word wider than the line overflows | `word_wider_than_line` |
| `DP-F-28` | 41 | Justification distributes free space | `justify_space_distribution` |
| `DP-F-29` | 41 | Distribute and Thai distribute | `distribute_alignment` |
| `DP-F-30` | 41 | The last line stays left-aligned | `justify_last_line` |
| `DP-F-31` | 41 | The clamp at 3 pt | `justify_clamp` |
| `DP-F-32` | 42 | Five tab alignments | `tab_alignments` |
| `DP-F-33` | 42 | A tab past the last stop | `tab_past_last` |
| `DP-F-34` | 42 | Tab leaders | `leader_dots` |
| `DP-F-35` | 43 | `w:sym` mapping | `symbol_run` |
| `DP-F-36` | 43 | A soft hyphen paints at a break only | `soft_hyphen_break` |
| `DP-F-37` | 43 | A positional tab | `positional_tab` |
| `DP-F-38` | 44 | The fragment fields | `fragment_fields` |
| `DP-F-39` | 44 | The first-line indent applies once | `split_paragraph_indent` |
| `DP-F-40` | 44 | `keepLines` past a page | `keeplines_too_tall` |
| `DP-F-41` | 45 | Line height from four numbers | `line_height_rules` |
| `DP-F-42` | 45 | A face with no vertical metrics | `no_hhea` |
| `DP-F-43` | 45 | Contextual spacing | `contextual_spacing` |
| `DP-F-44` | 45 | Superscript and subscript | `nested_superscript` |
| `DP-F-45` | 119.2 | The `harfrust` call shape | `shape_rtl_runs` |
| `DP-F-46` | 119.2 | Kerning follows `w:kern` | `kerning_follows_w_kern` |
| `DP-F-47` | 119.3 | First-fit, as measured | `first_fit_break` |
| `DP-F-48` | 119.3 | The earlier clause is withdrawn | `no_knuth_plass` |
| `DP-F-49` | 119.3 | `xs`, `beside`, and `hyphen` | `line_field_roles` |
| `DP-F-50` | 119.3 | The breaker is a state machine | `breaker_states` |
| `DP-F-51` | 119.3 | The minimum line width | `zero_width` |
| `DP-F-52` | 119.3 | Exclusions mirror once for RTL | `rtl_exclusions` |

## 156. Class L: layout

| Id | Clause | Subject | Test |
|---|---|---|---|
| `DP-L-01` | 46 | Points, origin top left | `page_coordinates` |
| `DP-L-02` | 46 | The page box | `page_geometry_a4` |
| `DP-L-03` | 46 | Vertical page alignment | `valign_overflow` |
| `DP-L-04` | 46 | Gutter and mirror margins | `mirror_margins` |
| `DP-L-05` | 47 | `keepNext` and `keepLines` | `keepnext_chain` |
| `DP-L-06` | 47 | The open block stack limit | `block_stack_limit` |
| `DP-L-07` | 47 | A tall header moves the body | `tall_header` |
| `DP-L-08` | 48 | A blank page has a header | `odd_page_blank` |
| `DP-L-09` | 48 | A continuous break mid-page | `continuous_break_columns` |
| `DP-L-10` | 48 | Page numbering across sections | `page_numbering` |
| `DP-L-11` | 48 | Four page fields are evaluated | `numpages_two_pass` |
| `DP-L-12` | 48 | The two-pass rule | `numpages_two_pass` |
| `DP-L-13` | 49 | Column count and width | `columns_unequal` |
| `DP-L-14` | 49 | A column break | `column_break` |
| `DP-L-15` | 49 | The note area spans the page | `note_area_width` |
| `DP-L-16` | 49 | A float wraps inside its column | `float_in_cell` |
| `DP-L-17` | 50 | The header selection rule | `header_variants` |
| `DP-L-18` | 50 | Header inheritance | `header_inherit` |
| `DP-L-19` | 50 | The header area height | `tall_header` |
| `DP-L-20` | 50 | An empty header can hold a float | `header_float` |
| `DP-L-21` | 50 | A page field lays out per page | `page_field_1000` |
| `DP-L-22` | 50 | `linkToPrevious` off | `header_link_off` |
| `DP-L-23` | 51 | The note area reservation | `note_at_page_bottom` |
| `DP-L-24` | 51 | A note that continues | `note_longer_than_page` |
| `DP-L-25` | 51 | Note numbering and restart | `note_restart_after_move` |
| `DP-L-26` | 51 | Endnote position | `endnote_section_end` |
| `DP-L-27` | 51 | A nested note | `note_in_note` |
| `DP-L-28` | 52 | The anchor reference frames | `anchor_frames` |
| `DP-L-29` | 52 | Wrap reduces the line width | `tight_wrap_jpeg` |
| `DP-L-30` | 52 | `behindDoc` paint order | `behind_doc` |
| `DP-L-31` | 52 | A displaced float | `float_unplaced` |
| `DP-L-32` | 52 | A text box is a story | `textbox_overflow` |
| `DP-L-33` | 52 | An inline object takes a line box | `inline_object_line` |
| `DP-L-34` | 53 | Fixed and automatic table layout | `table_autofit` |
| `DP-L-35` | 53 | Cell width, `gridSpan`, `vMerge` | `vmerge_orphan` |
| `DP-L-36` | 53 | Row height rules | `row_height_rules` |
| `DP-L-37` | 53 | A repeating header row | `table_many_pages` |
| `DP-L-38` | 53 | `cantSplit` | `cantsplit_too_tall` |
| `DP-L-39` | 53 | Border collapse | `border_collapse` |
| `DP-L-40` | 53 | Cell margins | `cell_margins` |
| `DP-L-41` | 53 | Nested table depth | `table_depth_25` |
| `DP-L-42` | 53 | A table indent | `table_ind_clamp` |
| `DP-L-43` | 53 | A table in a header or a note | `header_table` |
| `DP-L-44` | 54 | The fragment set | `fragment_fields` |
| `DP-L-45` | 54 | Paint order is list order | `behind_doc` |
| `DP-L-46` | 54 | A fragment names its block path | `fragment_path` |
| `DP-L-47` | 54 | The page record fields | `page_record_fields` |
| `DP-L-48` | 55 | The line layout contract | `line_layout_contract` |
| `DP-L-49` | 55 | The run advances sum to the line width | `advance_sum` |
| `DP-L-50` | 55 | A narrow band under a float | `float_band_line_width` |
| `DP-L-51` | 119.5 | A whole-document pass | `layout_all_passes` |
| `DP-L-52` | 119.5 | The multi-pass loop and its caps | `layout_pass_caps` |
| `DP-L-53` | 119.5 | Each loop terminates | `layout_budget` |
| `DP-L-54` | 119.5 | The first guess is low | `totals_first_guess` |
| `DP-L-55` | 119.5 | A fragment shares one `ParaLayout` | `fragment_shares_layout` |
| `DP-L-56` | 119.5 | `Cell` and `Object` variants | `fragment_fields` |
| `DP-L-57` | 119.5 | Blank page selection | `odd_page_blank` |
| `DP-L-58` | 119.5 | The ported `section_side` | `section_side_cases` |
| `DP-L-59` | 119.5 | The table fit function | `autofit_max_below_min` |
| `DP-L-60` | 119.5 | Clean, clamp, decide | `zero_page_size` |
| `DP-L-61` | 119.5 | Text box depth | `textbox_depth` |

## 157. Class G: display list and paint

| Id | Clause | Subject | Test |
|---|---|---|---|
| `DP-G-01` | 56 | Flow never touches a raster | `no_raster_in_flow` |
| `DP-G-02` | 57 | The closed operation set | `op_set_frozen` |
| `DP-G-03` | 57 | No block-level operation | `op_set_frozen` |
| `DP-G-04` | 57 | A run carries glyph ids | `glyph_run_shape` |
| `DP-G-05` | 57 | A link is an operation | `link_rect_exact` |
| `DP-G-06` | 58 | The paint kinds | `paint_kinds` |
| `DP-G-07` | 58 | Colours are sRGB at encode time | `color_srgb` |
| `DP-G-08` | 58 | The tint and shade formula | `theme_tint_shade` |
| `DP-G-09` | 58 | A CMYK image converts | `cmyk_jpeg` |
| `DP-G-10` | 59 | The page header layout | `codec_header` |
| `DP-G-11` | 59 | A coordinate is 1/100 pt | `codec_round_trip` |
| `DP-G-12` | 59 | Strings and resource ids | `codec_round_trip` |
| `DP-G-13` | 59 | The text index | `text_index_shape` |
| `DP-G-14` | 59 | The text layer reads the index | `text_layer_alignment` |
| `DP-G-15` | 59 | The CRC check | `page_corrupt` |
| `DP-G-16` | 59 | The format version check | `protocol_mismatch` |
| `DP-G-17` | 59 | The page budget | `display_list_budget` |
| `DP-G-18` | 60 | The canvas painter and its backend | `canvas_native_parity` |
| `DP-G-19` | 60 | The SVG painter | `svg_export` |
| `DP-G-20` | 60 | The PDF painter | `pdf_page_count` |
| `DP-G-21` | 60 | The recorder painter | `painter_parity` |
| `DP-G-22` | 60 | A skipped operation is logged | `painter_parity` |
| `DP-G-23` | 61 | The tile grid | `tile_grid` |
| `DP-G-24` | 61 | The device ratio cap | `zoom_800` |
| `DP-G-25` | 61 | Tile eviction | `tile_eviction` |
| `DP-G-26` | 61 | Zoom response path | `zoom_transform_first` |
| `DP-G-27` | 62 | The hit result | `hit_shape` |
| `DP-G-28` | 62 | A hit returns a character boundary | `hit_char_boundary` |
| `DP-G-29` | 62 | An invisible run captures no click | `hidden_text_off` |
| `DP-G-30` | 62 | A table cell hit | `hit_in_cell` |
| `DP-G-31` | 62 | The spatial index | `hit_spatial_index` |
| `DP-G-32` | 62 | One hit path | `hit_once` |
| `DP-G-33` | 63 | A run becomes one span | `text_layer_alignment` |
| `DP-G-34` | 63 | A run break adds no space | `selection_across_pages` |
| `DP-G-35` | 63 | The layer copies and does not edit | `copy_table` |
| `DP-G-36` | 63 | An RTL run in the layer | `rtl_text_layer` |
| `DP-G-37` | 63 | Hidden text in the layer | `hidden_text_off` |
| `DP-G-38` | 63 | Tabs, breaks, and soft hyphens | `text_layer_breaks` |
| `DP-G-39` | 63 | Paragraph and table structure | `text_layer_roles` |
| `DP-G-40` | 63 | The character cap | `text_layer_dropped` |
| `DP-G-41` | 63 | Selection across pages | `selection_across_pages` |
| `DP-G-42` | 64 | Page landmarks | `axe_no_critical` |
| `DP-G-43` | 64 | Heading navigation | `outline_navigation` |
| `DP-G-44` | 64 | An undescribed image | `image_alt_fallback` |
| `DP-G-45` | 64 | A revision in the layer | `revision_layer_state` |
| `DP-G-46` | 64 | High contrast | `high_contrast` |
| `DP-G-47` | 64 | Reduced motion | `reduced_motion` |
| `DP-G-48` | 64 | Forced colours | `forced_colors` |
| `DP-G-49` | 119.6 | One list, three targets | `painter_parity` |
| `DP-G-50` | 119.6 | Turned and rotated groups | `rotate_flip` |
| `DP-G-51` | 119.6 | The raster clamps | `zoom_800` |
| `DP-G-52` | 119.6 | The browser painter is the new target | `canvas_native_parity` |

## 158. Class V: viewer

| Id | Clause | Subject | Test |
|---|---|---|---|
| `DP-V-01` | 65 | The Rust entry point | `one_call_load` |
| `DP-V-02` | 65 | The JavaScript entry point | `one_call_load` |
| `DP-V-03` | 65 | The accepted sources | `load_sources` |
| `DP-V-04` | 65 | `load` resolves at the first page | `load_before_layout` |
| `DP-V-05` | 66 | The range layout report | `range_respects_passes` |
| `DP-V-06` | 66 | A page clone is cheap | `fragment_shares_layout` |
| `DP-V-07` | 66 | Send and Sync on native | `send_sync` |
| `DP-V-08` | 66 | The error shape | `error_shape` |
| `DP-V-09` | 67 | The viewer composition | `viewer_composition` |
| `DP-V-10` | 67 | A call before load | `zoom_before_load` |
| `DP-V-11` | 67 | `destroy` releases everything | `no_leak` |
| `DP-V-12` | 68 | The zoom value set | `zoom_values` |
| `DP-V-13` | 68 | The zoom range | `zoom_clamp` |
| `DP-V-14` | 68 | Zoom does not relayout | `zoom_no_relayout` |
| `DP-V-15` | 68 | Fit zoom on resize | `resize_during_layout` |
| `DP-V-16` | 68 | Rotation | `hit_after_rotate` |
| `DP-V-17` | 68 | Pinch zoom | `pinch` |
| `DP-V-18` | 69 | Scroll and spread modes | `scroll_modes` |
| `DP-V-19` | 69 | Scroll restore | `scroll_restore` |
| `DP-V-20` | 69 | A placeholder holds the height | `placeholder_height` |
| `DP-V-21` | 69 | `goToPage` | `goto_clamp` |
| `DP-V-22` | 69 | `pagechange` fires once | `pagechange_once` |
| `DP-V-23` | 69 | Keyboard navigation | `keyboard_nav` |
| `DP-V-24` | 69 | Lazy layout | `lazy_layout` |
| `DP-V-25` | 69 | Scroll bounds | `scroll_bounds` |
| `DP-V-26` | 70 | The search result | `search_navigate` |
| `DP-V-27` | 70 | A hit across a break or a page | `search_across_lines` |
| `DP-V-28` | 70 | Incremental search | `search_incremental` |
| `DP-V-29` | 70 | A bad query and the step budget | `regex_budget` |
| `DP-V-30` | 70 | The highlight layer | `highlight_layer` |
| `DP-V-31` | 71 | A drag becomes a text range | `selection_mouse_and_key` |
| `DP-V-32` | 71 | Copy text uses paragraph separators | `selection_across_pages` |
| `DP-V-33` | 71 | Copy a table | `copy_table` |
| `DP-V-34` | 71 | A selection that crosses a float | `selection_float_crossed` |
| `DP-V-35` | 71 | Keyboard caret | `keyboard_nav` |
| `DP-V-36` | 72 | The link overlay | `link_rect_exact` |
| `DP-V-37` | 72 | The outline | `outline_navigation` |
| `DP-V-38` | 72 | Print through PDF | `print_200` |
| `DP-V-39` | 72 | A placeholder prints | `print_placeholder` |
| `DP-V-40` | 73 | The event set | `event_set` |
| `DP-V-41` | 73 | A handler that throws | `handler_throws` |
| `DP-V-42` | 74 | A missing option is a `TypeError` | `missing_option` |
| `DP-V-43` | 74 | Two options are enough | `two_options_enough` |
| `DP-V-44` | 119.8 | The cache key | `cache_key_complete` |
| `DP-V-45` | 119.9 | Print without links | `print_no_links` |

## 159. Class B: browser

| Id | Clause | Subject | Test |
|---|---|---|---|
| `DP-B-01` | 75 | The module runs in a worker | `worker_only` |
| `DP-B-02` | 75 | No `SharedArrayBuffer` | `no_isolation_headers` |
| `DP-B-03` | 75 | One shaping thread | `single_thread_shape` |
| `DP-B-04` | 76 | One exported class | `module_surface` |
| `DP-B-05` | 76 | Result shapes | `module_surface` |
| `DP-B-06` | 76 | Errors are objects | `error_shape` |
| `DP-B-07` | 77 | The message fields | `protocol_shape` |
| `DP-B-08` | 77 | Bytes are transferred | `transfer_bytes` |
| `DP-B-09` | 77 | Pages arrive in order | `page_order` |
| `DP-B-10` | 77 | Cancel keeps pages | `cancel_keeps_pages` |
| `DP-B-11` | 77 | One document per worker | `one_doc_per_worker` |
| `DP-B-12` | 77 | The message size guard | `message_too_large` |
| `DP-B-13` | 77 | The protocol version | `protocol_mismatch` |
| `DP-B-14` | 78 | The heap ceiling | `memory_limit` |
| `DP-B-15` | 78 | The input bytes are freed | `bytes_freed` |
| `DP-B-16` | 78 | The page cache limit | `page_cache_limit` |
| `DP-B-17` | 78 | The resident budget | `perf_memory` |
| `DP-B-18` | 78 | No leak across documents | `no_leak` |
| `DP-B-19` | 79 | The worker holds the fonts | `font_in_worker` |
| `DP-B-20` | 79 | The font source order | `font_source_order` |
| `DP-B-21` | 79 | The font fetch limits | `font_fetch_limits` |
| `DP-B-22` | 79 | The built-in face is offline | `builtin_face` |
| `DP-B-23` | 79 | An embedded font needs no fetch | `offline_embedded_font` |
| `DP-B-24` | 79 | The text layer face | `text_layer_font` |
| `DP-B-25` | 80 | The capability table | `capabilities` |
| `DP-B-26` | 80 | A missing `OffscreenCanvas` | `offscreen_unavailable` |
| `DP-B-27` | 80 | No asm.js fallback | `no_wasm` |
| `DP-B-28` | 81 | The demo CSP | `csp_demo` |
| `DP-B-29` | 81 | `wasm-unsafe-eval` only | `csp_demo` |
| `DP-B-30` | 81 | The service worker cache | `sw_cache` |
| `DP-B-31` | 81 | The MIME type | `wasm_mime` |
| `DP-B-32` | 81 | The artifact budgets | `size_budgets` |
| `DP-B-33` | 81 | CI fails past a budget | `size_budgets` |
| `DP-B-34` | 82 | No active content | `no_active_content` |
| `DP-B-35` | 82 | A blocked link scheme | `unsafe_link` |
| `DP-B-36` | 82 | SVG sanitizing | `svg_script` |
| `DP-B-37` | 82 | No second XML parser | `no_dtd` |
| `DP-B-38` | 82 | The image size guard | `png_bomb` |
| `DP-B-39` | 82 | The network allowlist | `external_denied` |
| `DP-B-40` | 82 | The password is cleared | `no_secret_in_error` |
| `DP-B-41` | 83 | Print through a hidden frame | `print_200` |
| `DP-B-42` | 83 | A blocked print | `print_blocked` |
| `DP-B-43` | 119.10 | The size budget is measured | `size_budgets` |

## 160. Class U: demo

| Id | Clause | Subject | Test |
|---|---|---|---|
| `DP-U-01` | 84 | A static site | `demo_static` |
| `DP-U-02` | 85 | Four regions | `demo_layout` |
| `DP-U-03` | 85 | The left rail | `demo_thumbs` |
| `DP-U-04` | 85 | The top bar | `demo_toolbar` |
| `DP-U-05` | 85 | The status bar | `demo_status` |
| `DP-U-06` | 86 | The narrow layout | `demo_narrow` |
| `DP-U-07` | 86 | Touch defaults | `demo_touch` |
| `DP-U-08` | 87 | The skeleton and the progress bar | `demo_loading` |
| `DP-U-09` | 87 | A drop loads a file | `demo_drop` |
| `DP-U-10` | 87 | The failure panel | `demo_failure` |
| `DP-U-11` | 88 | Every control is reachable | `axe_no_critical` |
| `DP-U-12` | 88 | The target size | `axe_no_critical` |
| `DP-U-13` | 88 | The zoom menu | `demo_zoom_menu` |
| `DP-U-14` | 88 | The page field | `demo_page_field` |
| `DP-U-15` | 89 | The shortcut guard | `keyboard_nav` |
| `DP-U-16` | 90 | The highlight colours | `highlight_layer` |
| `DP-U-17` | 90 | The hit counter | `demo_hit_counter` |
| `DP-U-18` | 91 | The diagnostic badge | `diagnostic_panel` |
| `DP-U-19` | 91 | The fidelity sentence | `diagnostic_panel` |
| `DP-U-20` | 92 | WCAG level AA on the shell | `axe_no_critical` |
| `DP-U-21` | 92 | The landmarks | `demo_landmarks` |
| `DP-U-22` | 92 | Keyboard only | `keyboard_nav` |
| `DP-U-23` | 92 | Theme and motion | `reduced_motion` |
| `DP-U-24` | 93 | The five sample documents | `demo_samples` |
| `DP-U-25` | 93 | Compare mode | `demo_compare` |
| `DP-U-26` | 93 | The gate values on screen | `demo_compare` |
| `DP-U-27` | 94 | The test handle | `demo_handle` |
| `DP-U-28` | 94 | URL parameters | `demo_url_state` |
| `DP-U-29` | 94 | The site documents the measurement | `demo_docs` |
| `DP-U-30` | 119.10 | The embed mode | `demo_embed` |

## 161. Class E: performance

| Id | Clause | Subject | Test |
|---|---|---|---|
| `DP-E-01` | 95 | A budget is a p95 | `perf_first_page` |
| `DP-E-02` | 95 | A budget CI cannot measure is a warning | `perf_report_shape` |
| `DP-E-03` | 96 | Encoding is never the cost | `perf_encode` |
| `DP-E-04` | 96 | The first page never waits for all pages | `load_before_layout` |
| `DP-E-05` | 97 | Three page sets | `scroll_mounts_nearby` |
| `DP-E-06` | 97 | A returned page paints from a bitmap | `cached_tile_no_work` |
| `DP-E-07` | 97 | The mounted limit | `scroll_burst` |
| `DP-E-08` | 97 | The scroll handler does no layout | `scroll_burst` |
| `DP-E-09` | 98 | Four caches, four owners | `cache_ownership` |
| `DP-E-10` | 98 | One key function | `cache_key_complete` |
| `DP-E-11` | 98 | A cache counts bytes | `cache_bytes` |
| `DP-E-12` | 98 | The tile cache adapts | `cache_size_low_memory` |
| `DP-E-13` | 98 | One image decode per document | `image_decode_once` |
| `DP-E-14` | 98 | A font stays for the session | `font_cache_persist` |
| `DP-E-15` | 99 | The progressive schedule | `progressive_layout` |
| `DP-E-16` | 99 | A chunk yields | `chunk_yields` |
| `DP-E-17` | 99 | A jump has priority | `jump_priority` |
| `DP-E-18` | 99 | The progress line | `progressive_layout` |
| `DP-E-19` | 100 | The cost classes | `layout_budget` |
| `DP-E-20` | 100 | The shape cache | `shape_cache` |
| `DP-E-21` | 100 | A paragraph keeps its lines | `paragraph_cache_hit` |
| `DP-E-22` | 101 | Zoom does not lay out | `zoom_no_relayout` |
| `DP-E-23` | 101 | The transform path | `zoom_transform_first` |
| `DP-E-24` | 102 | The search index | `search_incremental` |
| `DP-E-25` | 102 | The search budget | `search_10k` |
| `DP-E-26` | 102 | A hit needs no second shape call | `hit_once` |
| `DP-E-27` | 103 | The harness writes a report | `perf_report_shape` |
| `DP-E-28` | 103 | The 15% regression rule | `perf_regression` |
| `DP-E-29` | 103 | The report line format | `perf_report_shape` |
| `DP-E-30` | 103 | An unverified machine | `perf_unverified` |
| `DP-E-31` | 104 | A slow page is reported | `slow_page` |
| `DP-E-32` | 104 | The page limit offers two actions | `page_limit` |

## 162. Class X: edge cases

Every row of [13-edge-cases.md](13-edge-cases.md) is a requirement with a test name. The
suite `edge_cases_complete` reads that file and fails on a row that names no test. The suite
`edge_case_count` fails when the count changes without a change to that file.

| Range | Group | Count |
|---|---|---|
| `DP-X-01` to `DP-X-18` | Package and input | 18 |
| `DP-X-19` to `DP-X-34` | Page geometry | 16 |
| `DP-X-35` to `DP-X-49` | Styles and numbering | 15 |
| `DP-X-50` to `DP-X-76` | Text | 27 |
| `DP-X-77` to `DP-X-86` | Fonts | 10 |
| `DP-X-87` to `DP-X-104` | Images and objects | 18 |
| `DP-X-105` to `DP-X-120` | Tables | 16 |
| `DP-X-121` to `DP-X-138` | Sections, notes, and floats | 18 |
| `DP-X-139` to `DP-X-160` | Viewer and browser | 22 |
| `DP-X-161` to `DP-X-168` | Determinism and parity | 8 |
| `DP-X-169` to `DP-X-170` | Hostile input | 2 |
| Sum | | 170 |

## 163. Class Q: quality

| Id | Clause | Subject | Test |
|---|---|---|---|
| `DP-Q-01` | 123 | Three test levels | `test_levels` |
| `DP-Q-02` | 123 | Each level answers one question | `test_levels` |
| `DP-Q-03` | 124 | The four corpus sets | `corpus_sets` |
| `DP-Q-04` | 124 | A synthetic document is code | `corpus_synthetic_built` |
| `DP-Q-05` | 124 | A real document states its origin | `corpus_real_provenance` |
| `DP-Q-06` | 124 | The hostile set is generated | `hostile_corpus` |
| `DP-Q-07` | 124 | The stress set is not a fidelity set | `corpus_sets` |
| `DP-Q-08` | 124 | The manifest and the coverage report | `feature_coverage` |
| `DP-Q-09` | 125 | A golden test stores a display list | `golden_text_form` |
| `DP-Q-10` | 125 | The PNG is not the gate | `golden_text_form` |
| `DP-Q-11` | 125 | The accept tool | `golden_accept` |
| `DP-Q-12` | 126 | Three references | `reference_set` |
| `DP-Q-13` | 126 | WordCraft is a dev harness | `reference_harness` |
| `DP-Q-14` | 126 | A known gap is a warning | `known_gaps_listed` |
| `DP-Q-15` | 126 | The LibreOffice reference is coarse | `reference_lo` |
| `DP-Q-16` | 126 | The tolerance values | `tolerance_values` |
| `DP-Q-17` | 126 | The report is generated | `fidelity_report` |
| `DP-Q-18` | 127 | The diff measures | `display_diff_shape` |
| `DP-Q-19` | 127 | The summary line | `display_diff_shape` |
| `DP-Q-20` | 127 | A text order failure is hard | `text_order_hard` |
| `DP-Q-21` | 128 | The report columns | `fidelity_report` |
| `DP-Q-22` | 128 | The report is committed | `fidelity_report` |
| `DP-Q-23` | 129 | The test plan counts | `test_plan_counts` |
| `DP-Q-24` | 129 | No silent ignore | `no_ignored_test` |
| `DP-Q-25` | 129 | Every requirement names a test | `traceability_complete` |
| `DP-Q-26` | 130 | The native test path | `native_test_map` |
| `DP-Q-27` | 130 | The two rasters agree within 2% | `canvas_native_parity` |
| `DP-Q-28` | 131 | Two runs are equal | `layout_deterministic` |
| `DP-Q-29` | 131 | An ordered map where order reaches output | `ordered_map` |
| `DP-Q-30` | 131 | Native and WASM agree | `native_wasm_parity` |
| `DP-Q-31` | 132 | Four required jobs | `ci_jobs` |
| `DP-Q-32` | 132 | The job order by cost | `ci_jobs` |
| `DP-Q-33` | 132 | Artifacts carry the numbers | `ci_jobs` |
| `DP-Q-34` | 132 | A dependency update runs the goldens | `ci_jobs` |
| `DP-Q-35` | 133 | A failure names page, block, and cause | `failure_message` |
| `DP-Q-36` | 133 | The cause precedes the symptom | `failure_message` |
| `DP-Q-37` | 133 | A font failure names the face | `font_failure_message` |
| `DP-Q-38` | 134 | Four manual checks | `manual_report` |
| `DP-Q-39` | 134 | A manual result is recorded | `manual_report` |
| `DP-Q-40` | 135 | A tier needs coverage | `feature_coverage` |
| `DP-Q-41` | 135 | A placeholder is named | `tier_claim` |
| `DP-Q-42` | 119.7 | The step counter | `layout_budget` |

## 164. Cross-reference matrix

`DP-N-15`. A requirement is checked against one other file. The matrix below names the file
that holds the behavior and the file that checks it. A reviewer can then read a clause in one
place.

| Requirement class | Behavior in | Enforced by | Measured in |
|---|---|---|---|
| `DP-I` | 01-intent | 14-testing-and-quality | 12-performance |
| `DP-N` | 02-references | 16-traceability | 14-testing-and-quality |
| `DP-A` | 03-architecture | 03-architecture | 14-testing-and-quality |
| `DP-P` | 04-pipeline | 13-edge-cases | 14-testing-and-quality |
| `DP-S` | 05-styles | 13-edge-cases | 14-testing-and-quality |
| `DP-F` | 06-text-and-fonts | 13-edge-cases | 14-testing-and-quality |
| `DP-L` | 07-layout-and-pagination | 13-edge-cases | 14-testing-and-quality |
| `DP-G` | 08-display-list-and-paint | 09-viewer-api | 14-testing-and-quality |
| `DP-V` | 09-viewer-api | 11-demo-site | 12-performance |
| `DP-B` | 10-wasm-browser | 13-edge-cases | 12-performance |
| `DP-U` | 11-demo-site | 14-testing-and-quality | 12-performance |
| `DP-E` | 12-performance | 12-performance | 12-performance |
| `DP-X` | 13-edge-cases | 13-edge-cases | 14-testing-and-quality |
| `DP-Q` | 14-testing-and-quality | 14-testing-and-quality | 14-testing-and-quality |

## 165. The DRY audit

`DP-A-31`. A behavior has one home. The table below lists each repeated concept, its one
home, and the callers that read it. A second home is a defect, and the test
`single_home` reads the module list and fails on a duplicate.

| Concept | One home | Callers |
|---|---|---|
| Length conversion | `edgedoc-geom::units` | style, text, flow, paint |
| Rectangle and transform | `edgedoc-geom::Rect`, `Affine` | every display crate |
| Pixel budget | `edgedoc-geom::pixels` | image decode, tile raster |
| Shaping | `edgedoc-fonts::shape` | text only |
| Face resolution | `edgedoc-fonts::resolve` | style, text |
| Glyph coverage and fallback | `edgedoc-fonts::fallback_chain` | text |
| Bidi levels | `edgedoc-text::bidi` | text, hit test |
| Break opportunity | `edgedoc-text::breaks` | text |
| Line fit and justify | `edgedoc-text::lines` | flow, table fit, notes, headers |
| Tab stop arithmetic | `edgedoc-text::tabs` | text only |
| Hyphenation | `edgedoc-text::hyphen` | text |
| Cascade | `edgedoc-style::cascade` | flow, header layout |
| Numbering counters | `edgedoc-style::numbering` | flow, header layout |
| Theme colour and font | `edgedoc-style::theme` | cascade |
| Symbol and bullet tables | `edgedoc-style::tables` | cascade, text |
| Section geometry | `edgedoc-style::sections` | flow |
| Table fit | `edgedoc-flow::table_fit` | flow only |
| Note numbering | `edgedoc-flow::notes` | flow only |
| Float placement | `edgedoc-flow::floats` | flow only |
| Page box | `edgedoc-flow::page_box` | flow, viewer hint |
| Display list codec | `edgedoc-paint::codec` | flow, worker, viewer |
| Colour conversion | `edgedoc-paint::color` | every painter |
| Hit test | `edgedoc-display::hit` | viewer, search, selection |
| Text index | `edgedoc-display::index` | viewer, text layer, clipboard |
| Diagnostic registry | `edgedoc-display::diagnostics` | every stage, viewer |
| Cache key | one per cache, in that cache's module | the cache |
| Message protocol | `edgedoc-display-wasm::protocol` | worker, viewer |

`DP-A-32`. Three concepts appear twice on purpose, and each has a stated reason:

| Concept | Two homes | Reason |
|---|---|---|
| Line layout | `edgedoc-text` and the WordCraft `para` module | One is the port, and the other is the source. The port keeps a test that compares the two while the port lands, and the test is removed in phase 4 |
| Page size | `edgedoc-style::sections` and the viewer placeholder | The viewer paints a guessed size before the worker answers. `DP-A-24` makes the worker the only source once it answers |
| Colour | `edgedoc-paint::color` and the canvas | The canvas converts nothing. It receives sRGB bytes. This is not a second home |

## 166. The SOLID audit

`DP-A-33`. Each principle is checked by a rule and a test. A principle with no test is a
slogan.

| Principle | Rule | Test | Failure mode it prevents |
|---|---|---|---|
| Single responsibility | A crate changes for one reason (`DP-A-12`) | `crate_ownership` | A style change breaks pagination |
| Open/closed | A new painter or object is a new trait object (`DP-A-13`) | `object_renderer_open` | A `match` grows in the flow crate |
| Liskov | Every `Shaper` returns visual-order glyphs with one `.notdef` contract (`DP-A-14`) | `all_faces_missing` | A test shaper hides a real defect |
| Interface segregation | `ResourceStore`, `FontSource`, `Painter`, and `LayoutSink` stay separate (`DP-A-15`) | `trait_audit` | A test must implement 30 methods |
| Dependency inversion | Flow and paint depend on traits, not crates (`DP-A-16`) | `flow_trait_only` | A headless test needs a browser |

`DP-A-34`. A trait with one implementation and no test double is a struct. The audit reads
the trait list of [03-architecture.md](03-architecture.md). It fails on a trait that has one
implementation and no test double, because such a trait adds a layer and buys nothing.

`DP-A-35`. A module that names a concrete type from a higher layer is a dependency inversion
defect. The test `no_upward_type` reads the `use` statements and fails on an upward name.

`DP-A-36`. Openness has a cost, and the audit checks both sides:

| Extension point | Cost of a new implementation | Check |
|---|---|---|
| `Painter` | One file, no change to flow | `painter_parity` runs a new painter with no change |
| `ObjectRenderer` | One file, no change to flow | `object_renderer_open` |
| `ResourceStore` | One file, no change to flow | `external_denied` |
| `FontSource` | One file, no change to text | `font_source_order` |
| `Shaper` | One file, no change to flow | `shape_rtl_runs` |
| `LayoutSink` | One file, no change to flow | `cancel_keeps_pages` |

## 167. The acceptance scenario

`DP-N-16`. A reviewer reads this one scenario to check that the specification hangs together.

A reader opens the demo. They drop `report.docx` on the page. The viewer shows a skeleton
page with the hinted size, then page 1 within 300 ms. Page 1 holds a two-column section, a
table with a repeating header, a footer with `PAGE` and `NUMPAGES`, and a logo in the header.

The worker reports three diagnostics. The document asks for `Aptos`, which no face provides,
so `display.font_substituted` names Carlito. Page 7 holds a chart, so
`display.object_placeholder` names the chart extent. The document sets
`doNotUseHTMLParagraphAutoSpacing`, which version 1 ignores, so `display.compat_ignored`
appears once.

The reader zooms to 200% with `Ctrl` and the wheel. The page under the pointer stays under
the pointer, and the text is sharp within one frame after the gesture ends. The reader
presses `N` to reach page 3, then `Ctrl+F` and types `revenue`. The viewer paints 12 hits and
moves to the first. The reader selects three lines across the page-2 to page-3 break and
copies them. The clipboard holds one string with one paragraph separator and no duplicate
line.

The reader prints. The hidden frame produces a PDF of 18 pages, and the paper matches the
screen. The placeholder chart prints with its label, so the reader knows that page 7 is
incomplete.

A developer runs `cargo xtask display-diff report.docx`. The tool prints 18 pages, no break
difference, 99.4% of lines matched, and a text order of `ok`. The fidelity report lists the
Aptos substitution as the only known gap for this document.

This scenario touches `DP-I-08`, `DP-I-03`, `DP-I-04`, `DP-I-05`, `DP-I-06`, `DP-I-07`,
`DP-P-24`, `DP-P-10`, `DP-S-28`, `DP-V-13`, `DP-V-14`, `DP-V-32`, `DP-V-38`, `DP-B-41`,
`DP-Q-19`, and `DP-Q-22`. A change to any of them changes this scenario, and the scenario is
the acceptance test of [15-plan.md](15-plan.md).
