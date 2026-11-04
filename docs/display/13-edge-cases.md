# Every boundary case and the test that locks it.

> Part of the EdgeDoc Display specification. Date: 2026-11-04. Requirement ids are
> normative. See [the index](README.md).

## 105. How to read this clause

Each row is a requirement. It names the case, the required behavior, the diagnostic code,
and the test name. The test name is the CI name. "Browser" means a test in a real browser
through the harness of [11-demo-site.md](11-demo-site.md). "Manual" means a person compares
a page with Word, and the result is recorded in the test note.

A row with no diagnostic code shows a visible result and no panel entry. A row that shows a
page must show the page. A viewer must not blank, and it must not shift silently.

`DP-X-01`. A row's test exists before the feature that satisfies it. A failing test for a
new row is a normal state in a phase.

## 106. Package and input

| Id | Case | Required behavior | Code | Test |
|---|---|---|---|---|
| `DP-X-01` | Zero-byte input | `display.not_zip`, no viewer | `display.not_zip` | `empty_bytes` |
| `DP-X-02` | A `.zip` that is not a Word package | `display.not_word`, names the next action | `package.not_word` | `zip_not_word` |
| `DP-X-03` | A `.doc` binary file renamed to `.docx` | The same, with a message that names the format | `package.not_word` | `binary_doc_renamed` |
| `DP-X-04` | Flat OPC package | `display.flat_opc_unsupported` | `display.flat_opc_unsupported` | `flat_opc_rejected` |
| `DP-X-05` | Encrypted package, no password | Prompt for a password. Never a blank page | `package.password_required` | `encrypted_no_password` |
| `DP-X-06` | Encrypted package, wrong password | One retry, then the same prompt | `package.wrong_password` | `encrypted_wrong_password` |
| `DP-X-07` | ZIP bomb, 1 GB declared | `package.limit` before it allocates | `package.limit` | `zip_bomb` |
| `DP-X-08` | Truncated ZIP | `package.not_zip` with no panic | `package.not_zip` | `truncated_zip` |
| `DP-X-09` | A part name that differs by case only | One part wins, as Word does | none | `part_case` |
| `DP-X-10` | No `word/document.xml` | `package.missing_part` | `package.missing_part` | `missing_main_part` |
| `DP-X-11` | Main part with an empty body | One blank page, as Word shows | none | `empty_body` |
| `DP-X-12` | Body with no `sectPr` | A4 portrait, 2.54 cm margins | `display.style_undefined` | `no_sectpr` |
| `DP-X-13` | Strict namespace | Read succeeds | none | `strict_namespace` |
| `DP-X-14` | `mc:AlternateContent` with two branches | Paint the selected branch once | none | `alternate_content_once` |
| `DP-X-15` | `mc:AlternateContent` with no supported branch | Paint a placeholder with the extent | `display.object_placeholder` | `alternate_content_unsupported` |
| `DP-X-16` | A part referenced but absent | Placeholder of the declared size | `display.resource_missing` | `missing_image_part` |
| `DP-X-17` | 20,001 pages declared | `display.page_limit` | `display.page_limit` | `page_limit` |
| `DP-X-18` | A package with 40,000 parts | `package.limit` | `package.limit` | `part_count_limit` |

## 107. Page geometry

| Id | Case | Required behavior | Code | Test |
|---|---|---|---|---|
| `DP-X-19` | Landscape section | Width and height swap, content does not rotate | none | `landscape_section` |
| `DP-X-20` | Portrait section after a landscape section | New page, correct size | none | `portrait_after_landscape` |
| `DP-X-21` | Zero page size | A4 portrait, one diagnostic | `display.geometry_invalid` | `zero_page_size` |
| `DP-X-22` | Negative margin | Clamped to zero | `display.geometry_invalid` | `negative_margin` |
| `DP-X-23` | Margin sum past the page width | The page area is 1 pt wide, content overflows | `display.page_overflow` | `margin_exceeds_page` |
| `DP-X-24` | Mirror margins on an even page | Left and right margins swap | none | `mirror_margins` |
| `DP-X-25` | Gutter on the right in an RTL section | Gutter on the right, body shifts | none | `rtl_gutter` |
| `DP-X-26` | Continuous section break inside a column | Columns change mid-page from the current y | none | `continuous_break_columns` |
| `DP-X-27` | `oddPage` break from an odd page | One blank page, with header and footer | none | `odd_page_blank` |
| `DP-X-28` | `evenPage` break from an even page | The same | none | `even_page_blank` |
| `DP-X-29` | Header taller than the top margin | Body moves down, no overlap | `display.header_overflow` | `tall_header` |
| `DP-X-30` | Footer taller than the bottom margin | Body area shrinks, no overlap | `display.header_overflow` | `tall_footer` |
| `DP-X-31` | Header taller than half the page | The header clips and the body keeps 1 line | `display.header_overflow` | `huge_header` |
| `DP-X-32` | `vAlign="center"` with content taller than the area | The content starts at the top | none | `valign_overflow` |
| `DP-X-33` | 45 columns in one section | Paint 45 equal columns, 1 pt minimum each | `display.geometry_invalid` | `many_columns` |
| `DP-X-34` | Column gap wider than the page area | Zero-width columns, one diagnostic, text still visible | `display.geometry_invalid` | `column_gap_too_wide` |

## 108. Styles and numbering

| Id | Case | Required behavior | Code | Test |
|---|---|---|---|---|
| `DP-X-35` | Style `basedOn` cycle | Stop at the second visit, paint with the last style | `display.style_cycle` | `style_cycle` |
| `DP-X-36` | Style chain of 40 links | Stop at 32, paint with the last | `display.style_undefined` | `style_chain_too_long` |
| `DP-X-37` | Undefined style id | Default paragraph style, id kept | `display.style_undefined` | `undefined_style` |
| `DP-X-38` | Two default paragraph styles | The last one wins, one diagnostic | `display.style_undefined` | `two_defaults` |
| `DP-X-39` | Theme colour with both tint and shade | Tint then shade | none | `theme_tint_shade` |
| `DP-X-40` | Missing theme part | Office default scheme | `display.style_undefined` | `missing_theme` |
| `DP-X-41` | Numbering with no abstract numbering | Paint the level text as body text | `display.numbering_missing` | `no_abstract_num` |
| `DP-X-42` | Two `numId` on one `abstractNum` | Separate counters | none | `separate_counters` |
| `DP-X-43` | `startOverride` at level 2 | Levels 2 and deeper reset | none | `start_override` |
| `DP-X-44` | `numFmt` with no table entry | Decimal number, one diagnostic | `display.numbering_missing` | `unknown_numfmt` |
| `DP-X-45` | Counter past 4,000,000 | Roman and letter formats hold the last value | `display.numbering_missing` | `huge_counter` |
| `DP-X-46` | `numRestart="eachPage"` | Numbers restart per page | none | `restart_each_page` |
| `DP-X-47` | A list that starts in a table cell | Shares the body series | none | `list_in_table` |
| `DP-X-48` | A bullet with no symbol mapping | The code point from the run font | `display.glyph_missing` | `bullet_unmapped` |
| `DP-X-49` | `w:cnfStyle` on a banded row | The band format applies | none | `conditional_table_style` |

## 109. Text

| Id | Case | Required behavior | Code | Test |
|---|---|---|---|---|
| `DP-X-50` | A grapheme with 5 combining marks | One cluster, correct advance | none | `combining_stack` |
| `DP-X-51` | An emoji with a ZWJ sequence | One cluster, one glyph sequence | none | `zwj_sequence` |
| `DP-X-52` | A flag emoji (two regional indicators) | One cluster | none | `flag_cluster` |
| `DP-X-53` | A lone surrogate | Replacement character, one diagnostic | `display.glyph_missing` | `lone_surrogate` |
| `DP-X-54` | A right-to-left paragraph with a Latin word | The Latin word paints left to right inside the line | none | `rtl_latin_word` |
| `DP-X-55` | Arabic digits in Arabic text | Digits in logical order, painted left to right | none | `arabic_digits` |
| `DP-X-56` | Mirrored parentheses in RTL | Mirror mapping applies | none | `mirror_pairs` |
| `DP-X-57` | A word wider than the line | Overflows, no break, one diagnostic past the page area | `display.page_overflow` | `word_wider_than_line` |
| `DP-X-58` | A CJK line with no break opportunity | Breaks between characters | none | `cjk_break_anywhere` |
| `DP-X-59` | A closing bracket at the line start | Kinsoku moves it back | none | `kinsoku` |
| `DP-X-60` | `autoSpaceDE` between CJK and Latin | 1/4 em inserted | none | `auto_space` |
| `DP-X-61` | A soft hyphen at a break | A hyphen paints | none | `soft_hyphen_break` |
| `DP-X-62` | A soft hyphen away from a break | Nothing paints | none | `soft_hyphen_no_break` |
| `DP-X-63` | A tab past the last stop | Next default stop | none | `tab_past_last` |
| `DP-X-64` | A decimal tab with no separator | Text right-aligned at the stop | none | `decimal_tab_no_sep` |
| `DP-X-65` | A tab leader across a table cell edge | Stops at the cell edge | none | `tab_at_cell_edge` |
| `DP-X-66` | Justification that needs 12 pt of stretch | Left-align the line, one diagnostic | `display.justify_clamped` | `justify_clamp` |
| `DP-X-67` | A paragraph of 300,000 characters | Splits per page, no quadratic time | none | `huge_paragraph` |
| `DP-X-68` | A `keepLines` paragraph taller than the page | Splits, one diagnostic | `display.page_overflow` | `keeplines_too_tall` |
| `DP-X-69` | `keepNext` on the last paragraph of a page | Moves to the next page with the next block | none | `keepnext_chain` |
| `DP-X-70` | A chain of 200 `keepNext` blocks | The first must move, and the chain breaks with one diagnostic | `display.keep_chain_broken` | `keepnext_chain_long` |
| `DP-X-71` | Hidden text with `showHiddenText` off | Absent from paint and from search | none | `hidden_text_off` |
| `DP-X-72` | Hidden text with `showHiddenText` on | Painted with a dotted underline | none | `hidden_text_on` |
| `DP-X-73` | A run with `w:position` of 200 | Baseline moves 100 pt, clipped by the line box | none | `position_run` |
| `DP-X-74` | Superscript inside superscript | 65% of 65%, baseline offsets add | none | `nested_superscript` |
| `DP-X-75` | A face with no `hhea` table | `OS/2` metrics, then 0.8 em, one diagnostic | `display.font_vertical_metrics_missing` | `no_hhea` |
| `DP-X-76` | A complex script with no face | Fallback face, advances from the substitute | `display.font_substituted` | `complex_script_fallback` |

## 110. Fonts

| Id | Case | Required behavior | Code | Test |
|---|---|---|---|---|
| `DP-X-77` | A font with `fstype` 0x0002 | Metrics only, fallback paint | `display.font_not_embeddable` | `restricted_font` |
| `DP-X-78` | An `.odttf` obfuscated font | Deobfuscates and paints | none | `odttf` |
| `DP-X-79` | An `.odttf` with a wrong key | Fallback, one diagnostic | `display.font_corrupt` | `odttf_bad_key` |
| `DP-X-80` | A TTC with 4 faces | The named face loads | none | `ttc_named_face` |
| `DP-X-81` | A variable font with `wght` 350 | Paints at 350, no synthesis | none | `variable_weight` |
| `DP-X-82` | A face with no `GPOS` table | No kerning | none | `no_gpos` |
| `DP-X-83` | A missing glyph in every face | `.notdef` with a space advance, one diagnostic | `display.glyph_missing` | `all_faces_missing` |
| `DP-X-84` | 10,000 missing glyphs of one code point | One diagnostic | `display.glyph_missing` | `glyph_dedup` |
| `DP-X-85` | A WOFF2 face | Loads and paints | none | `woff2` |
| `DP-X-86` | A WOFF2 face with a corrupt `brotli` stream | Fallback, one diagnostic | `display.font_corrupt` | `woff2_corrupt` |

## 111. Images and objects

| Id | Case | Required behavior | Code | Test |
|---|---|---|---|---|
| `DP-X-87` | An EMF image | Placeholder with the declared size | `display.image_unsupported` | `emf_placeholder` |
| `DP-X-88` | A JPEG with a CMYK profile | Converts through the profile | none | `cmyk_jpeg` |
| `DP-X-89` | A JPEG with no profile | Default conversion, one diagnostic | `display.image_no_profile` | `cmyk_no_profile` |
| `DP-X-90` | A 65,535 x 65,535 PNG header | Rejected before allocation | `display.image_too_large` | `png_bomb` |
| `DP-X-91` | A GIF with 400 frames | First frame only | none | `gif_first_frame` |
| `DP-X-92` | An animated WEBP | First frame only | none | `webp_first_frame` |
| `DP-X-93` | An SVG with a `<script>` | Sanitized, no script runs | `display.svg_sanitized` | `svg_script` |
| `DP-X-94` | An SVG with an external `<image href>` | The reference is removed | `display.svg_sanitized` | `svg_external_ref` |
| `DP-X-95` | An SVG with a billion-laughs entity | Rejected, no expansion | `display.svg_sanitized` | `svg_entity_bomb` |
| `DP-X-96` | A crop rectangle past the image edge | Clamped to the edge | `display.image_crop_invalid` | `crop_out_of_bounds` |
| `DP-X-97` | A rotated image with a flip | Transform order: rotate then flip | none | `rotate_flip` |
| `DP-X-98` | A linked image with a denied host | Placeholder, one diagnostic | `display.image_denied` | `external_denied` |
| `DP-X-99` | A linked image that times out | Placeholder after 5 s | `display.image_timeout` | `external_timeout` |
| `DP-X-100` | A chart object | Placeholder with the chart extent | `display.object_placeholder` | `chart_placeholder` |
| `DP-X-101` | An `OMML` equation | Placeholder unless a renderer is registered | `display.object_placeholder` | `equation_placeholder` |
| `DP-X-102` | An OLE object with an icon | The icon image paints | none | `ole_icon` |
| `DP-X-103` | A drawing canvas with 200 shapes | Every shape paints in order | none | `drawing_canvas` |
| `DP-X-104` | An ink annotation | Placeholder with the ink extent | `display.object_placeholder` | `ink_placeholder` |

## 112. Tables

| Id | Case | Required behavior | Code | Test |
|---|---|---|---|---|
| `DP-X-105` | A table with no `tblGrid` | Equal columns, one diagnostic | `display.table_no_grid` | `table_no_grid` |
| `DP-X-106` | A table wider than the page area | Clamped, one diagnostic | `display.table_too_wide` | `table_too_wide` |
| `DP-X-107` | `growAutofit` past `max_table_overflow` | Clamped at the option | `display.table_too_wide` | `grow_autofit_clamp` |
| `DP-X-108` | 63 columns | Paints, no clamp | none | `table_63_cols` |
| `DP-X-109` | 64 columns | Clamped to 63, one diagnostic | `display.table_too_wide` | `table_64_cols` |
| `DP-X-110` | A cell with `gridSpan` past the last column | Clamped to the last column | `display.table_span_overflow` | `span_overflow` |
| `DP-X-111` | `vMerge` with no `restart` above | The cell starts a merge of one | `display.table_merge_orphan` | `vmerge_orphan` |
| `DP-X-112` | A row taller than the page with `cantSplit` | Moves to the next page, splits there, one diagnostic | `display.page_overflow` | `cantsplit_too_tall` |
| `DP-X-113` | A header row taller than half the page | Repeats once, one diagnostic | `display.table_header_too_tall` | `table_header_tall` |
| `DP-X-114` | A table split across 40 pages | Header repeats on each page | none | `table_many_pages` |
| `DP-X-115` | Nested table at depth 24 | Paints | none | `table_depth_24` |
| `DP-X-116` | Nested table at depth 25 | Placeholder in the cell | `display.limit` | `table_depth_25` |
| `DP-X-117` | A cell with a 200,000 character paragraph | Splits across pages inside the cell | none | `cell_huge_paragraph` |
| `DP-X-118` | Mixed borders on a shared edge | Wider border wins, painted once | none | `border_collapse` |
| `DP-X-119` | A float inside a cell | Wraps inside the cell | none | `float_in_cell` |
| `DP-X-120` | A table with `tblInd` outside the page | Clamped, one diagnostic | `display.table_too_wide` | `table_ind_clamp` |

## 113. Sections, notes, and floats

| Id | Case | Required behavior | Code | Test |
|---|---|---|---|---|
| `DP-X-121` | A footnote reference on the last line of a page | The paragraph moves, or the note continues | none | `note_at_page_bottom` |
| `DP-X-122` | A footnote longer than the page | Splits across pages | none | `note_longer_than_page` |
| `DP-X-123` | 500 footnotes on one page | Notes fill the page, the body moves | `display.note_area_full` | `many_notes` |
| `DP-X-124` | A footnote inside a footnote | Flattened, one diagnostic | `display.note_nested` | `note_in_note` |
| `DP-X-125` | A note in a header | Inline text, one diagnostic | `display.note_in_header` | `note_in_header` |
| `DP-X-126` | `numRestart="eachPage"` with a note that moved | The number follows the page it lands on | none | `note_restart_after_move` |
| `DP-X-127` | An endnote at the section end | Paints at the end of that section | none | `endnote_section_end` |
| `DP-X-128` | A float with no room in the column | Moves to the next page | `display.float_unplaced` | `float_next_page` |
| `DP-X-129` | 20 floats chasing each other | First 19 place, the 20th goes to the page top | `display.float_unplaced` | `float_stampede` |
| `DP-X-130` | A text box taller than the page | Clips, one diagnostic | `display.page_overflow` | `textbox_overflow` |
| `DP-X-131` | A text box with `autoFit` and 10,000 characters | Grows to a limit, then clips | `display.page_overflow` | `textbox_autofit` |
| `DP-X-132` | A float anchored in a footnote | Placed inside the note area | none | `float_in_note` |
| `DP-X-133` | A float with `wrapTight` on a JPEG | Approximated box, one diagnostic | `display.tight_wrap_approximated` | `tight_wrap_jpeg` |
| `DP-X-134` | A `behindDoc` float over text | Paints under the text | none | `behind_doc` |
| `DP-X-135` | A field with no cached result | Paints nothing visible, one diagnostic | `display.field_no_cached_result` | `field_no_result` |
| `DP-X-136` | `NUMPAGES` in a footer | The real page count, two passes | none | `numpages_two_pass` |
| `DP-X-137` | `PAGE` in a header on 1000 pages | The right number on each page | none | `page_field_1000` |
| `DP-X-138` | A field result that holds a nested field | The inner cached result paints | none | `nested_field_result` |

## 114. Viewer and browser

| Id | Case | Required behavior | Code | Test |
|---|---|---|---|---|
| `DP-X-139` | `load` called twice | The first document unloads, one worker, no leak | none | `load_twice` |
| `DP-X-140` | `load` with a 60 MB file | Loads, memory within budget | none | `large_file` |
| `DP-X-141` | `setZoom` before `load` | Stored, applied after load | none | `zoom_before_load` |
| `DP-X-142` | `goToPage` past the end | Clamps to the last page | none | `goto_clamp` |
| `DP-X-143` | `destroy` then any call | Rejects with `display.bad_state` | `display.bad_state` | `destroy_then_call` |
| `DP-X-144` | 100 create and destroy cycles | Heap returns to baseline plus 10% | none | `no_leak` |
| `DP-X-145` | Container width of 0 | One diagnostic, no divide by zero, paints at 100% | `display.container_zero` | `zero_width` |
| `DP-X-146` | Container resized during layout | Relayout of the visible pages, no jump | none | `resize_during_layout` |
| `DP-X-147` | Zoom to 800% on a 4K display | Sharp tiles, memory within budget | none | `zoom_800` |
| `DP-X-148` | Pinch zoom on a touch device | Scale follows the fingers, no scroll hijack | none | `pinch` |
| `DP-X-149` | Rotate 90 then click a word | The correct word is hit | none | `hit_after_rotate` |
| `DP-X-150` | Selection across a page break | One string, no duplicate line | none | `selection_across_pages` |
| `DP-X-151` | Copy a table | `text/html` holds a `<table>` | none | `copy_table` |
| `DP-X-152` | Search for a term split by a line break | Found, two rectangles | none | `search_across_lines` |
| `DP-X-153` | A regex with catastrophic backtracking | Stops at the step budget | `display.search_budget` | `regex_budget` |
| `DP-X-154` | A `javascript:` link | Blocked by the default handler | `display.link_blocked` | `unsafe_link` |
| `DP-X-155` | Print a 200-page document | 200 pages in the PDF | none | `print_200` |
| `DP-X-156` | A worker script from an older cache | Clear message that names the cache | `display.protocol_mismatch` | `stale_worker` |
| `DP-X-157` | A browser with no WebAssembly | A download link and a message | `display.no_wasm` | `no_wasm` |
| `DP-X-158` | A scroll of 10,000 px in one frame | At most 8 pages mount | none | `scroll_burst` |
| `DP-X-159` | A page that arrives after the reader left it | Cached, not painted | none | `late_page` |
| `DP-X-160` | A diagnostic handler that throws | The viewer continues, one console entry | none | `handler_throws` |

## 115. Determinism and parity

| Id | Case | Required behavior | Code | Test |
|---|---|---|---|---|
| `DP-X-161` | Two layout runs of one document | Equal page geometry, byte for byte | none | `layout_deterministic` |
| `DP-X-162` | Native layout and WASM layout | Equal page geometry within 0.01 pt | none | `native_wasm_parity` |
| `DP-X-163` | `CanvasPainter` and `RecordPainter` on one page | Same visible operation set | none | `painter_parity` |
| `DP-X-164` | Display list encode and decode | Equal geometry within 0.005 pt | none | `codec_round_trip` |
| `DP-X-165` | `lastRenderedPageBreak` hint that is stale | Ignored for paint | none | `stale_page_break_hint` |
| `DP-X-166` | `docProps/app.xml` page count that is wrong | Used only for the scrollbar hint | none | `wrong_page_hint` |
| `DP-X-167` | A document with 20 sections and 4 header variants | Each page takes the right variant | none | `header_variants` |
| `DP-X-168` | A layout with a `ManualClock` that jumps 1 s | The budget stops the run at a page boundary | `display.layout_budget` | `layout_budget` |

## 116. Hostile input

`DP-X-169`. A crafted package must not panic the module, must not allocate without a
budget, and must not read a file. The test `hostile_corpus` runs 200 mutated packages
through `open` and `layout`.

| Mutation | Required result |
|---|---|
| Truncate the ZIP at 200 offsets | `package.not_zip`, no panic |
| Rewrite a declared part size to 2 GB | `package.limit` |
| Insert a 300-level nested table | `display.limit` |
| Insert a table with 500 columns | Clamped, one diagnostic |
| Insert `w:ilvl` of 255 | Clamped to 9, one diagnostic |
| Insert a `numId` of 0 | No numbering, one diagnostic |
| Insert a negative `w:sz` | Clamped to 1 pt |
| Insert a `w:jc` that is not a value | Default alignment, one diagnostic |
| Insert 1,000,000 `w:t` elements in one paragraph | `xml.limit` |
| Insert a part with a DTD | `xml.malformed` |
| Insert a 4 GB declared image | `display.image_too_large` |
| Insert a `w:color` with 12 hex digits | Default colour, one diagnostic |

`DP-X-170`. A fuzz target runs 24 hours against `open` and `encode_page`, and it must find
no panic and no unbounded allocation. The target ships with the corpus of
[14-testing-and-quality.md](14-testing-and-quality.md).
