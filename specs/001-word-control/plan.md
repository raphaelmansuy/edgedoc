# Implementation plan

Related: [requirements](README.md), [architecture](architecture.md), [edge cases](editing.md).

The plan is the order the work landed, and the check that each requirement has code and a test. Principles for the code: one writer (`Editor`), one painter (the viewer), no second document model. See [architecture](architecture.md).

## Order

1. Source spans on the flow paragraph, so a painted byte can become a source caret (WC-02, WC-03).
2. Mutations on the model that keep an object in its own run and clear `raw_xml` (WC-10).
3. The session: caret, undo, preview, layout after each edit (WC-04, WC-05, WC-06).
4. Wasm methods beside the existing viewer, without changing `open` for the read-only demo.
5. The Word page on that viewer (WC-07, WC-08, WC-16, WC-17).
6. End-to-end screenshots, each one looked at (WC-11).

## Traceability

| Id | Code | Test |
| --- | --- | --- |
| WC-01 | `Document::write_bytes`, `documentBytes` | e2e save path via open; display open tests |
| WC-02 | `SourceSpan`, `caret.rs` | `backspace_joins_paragraphs` (Home), e2e caret |
| WC-03 | atomic spans, `insert_field` | `a_merge_field_round_trips_into_the_package`, e2e field |
| WC-04 | `Editor::transact` lays out | editor tests, `revision` bumps only then |
| WC-05 | undo stack cap 80 | `typing_inserts_and_undo_restores`, e2e undo |
| WC-06 | `preview` / `end_preview` | `preview_fills_a_field_without_losing_the_template`, e2e preview |
| WC-07 | `site/word/index.html` tabs | e2e drives Home, Insert, Mailings |
| WC-08 | side column, samples | e2e invoice template |
| WC-09 | `delete_span` same container | model path tests, join on backspace |
| WC-10 | `Paragraph::isolate_objects` | merge-field round trip |
| WC-11 | `tools/word_control_e2e.mjs` | screenshots in `e2e/screenshots` |
| WC-12 | `paragraph_mut` locked | model locked path |
| WC-13 | `insert_text` cap | `limits_refuse_a_huge_paste_a_bad_picture_and_an_empty_field` |
| WC-14 | `insert_picture` | same limit test, `editor.image` |
| WC-15 | `insert_field` | same limit test, `editor.field` |
| WC-16 | caret div times scale | e2e `10-zoom` |
| WC-17 | `Editor::blank` | e2e `01-blank` |
| WC-18 | `move_caret` | `word_motion_lands_on_the_next_word`, e2e typing and Enter |

## How to run

```text
cargo test -p edgedoc-model -p edgedoc-flow -p edgedoc-display -p edgedoc-display-wasm --offline --lib
make word-e2e
```

`make word-e2e` builds the wasm module and runs `tools/word_control_e2e.mjs`. The suite has **41** checks (editing, merge, navigation, layout chrome, and file-size gates on **14** screenshots). Screenshots land in `specs/001-word-control/e2e/screenshots`. A failure still leaves the shots taken so far.

## Review of the shots

Each file is a step, looked at as a picture, not only as a passing check.

| Shot | What has to be true on the pixels |
| --- | --- |
| 01-blank | Ribbon, blank page, caret, ruler, empty fields |
| 02-typed | The sentence on the page |
| 03-bold | The sentence selected or bold |
| 04-field | «Customer» and the field in the side column |
| 05-table | A grid on the page |
| 06-preview | The banner and the sample name |
| 07-template | The placeholder back |
| 08-undo | The grid gone |
| 09-invoice | The invoice sample, with ink |
| 10-zoom | A larger page, caret still on it |
| 11-narrow | The page still readable, fields under it |
| 12-layout | Layout tab / page setup chrome visible |
| 13-view | View ribbon and navigation pane with document headings |
| 14-highlight | Field highlight state in Mailings |

If a shot is wrong, the page changes and the script runs again. The check is the picture.
