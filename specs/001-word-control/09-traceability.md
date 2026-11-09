# 09 — Traceability

Requirements are [01](01-intent.md). Edge cases are [08](08-edge-cases.md).

| Requirement | Code | Test |
| --- | --- | --- |
| WC-1 Open | `DisplayHandle::edit_open` | e2e `05-invoice` |
| WC-2 Blank | `Editor::blank` | e2e `01-blank`, `an_edit_session_types_and_keeps_the_read_only_open_path` |
| WC-3 Type and paragraphs | `Editor::insert_text`, `insert_newline`, `delete` | `typing_inserts_and_undo_restores`, `enter_splits_the_paragraph`, `backspace_joins_paragraphs`, e2e `02-typed` |
| WC-4 Caret | `caret::hit_caret`, `Editor::move_caret`, `slot_edge` | `backspace_joins_paragraphs`, e2e click and Home |
| WC-5 Formatting | `Editor::toggle_mark`, `set_font`, `set_font_size`, `set_color`, `set_highlight`, para spacing/shading/borders | `bold_marks_the_selection`, e2e `03-bold`, shading/border steps |
| WC-6 Align and lists | `Editor::set_align`, `toggle_list` | ribbon commands, covered by the same edit path as bold |
| WC-7 Merge field | `insert_field`, `isolate_objects` | `a_merge_field_round_trips_into_the_package`, e2e `03-field` |
| WC-8 Table | `insert_table` | e2e `08-table` |
| WC-9 Picture | `insert_picture` | guard in `Editor::insert_picture` |
| WC-10 Undo | `Editor::undo`, `redo`, cap 80 | `typing_inserts_and_undo_restores` |
| WC-11 Preview | `Editor::preview`, `end_preview` | `preview_fills_a_field_without_losing_the_template`, e2e `06-preview`, `07-template-restored` |
| WC-12 Download | `documentBytes` → `package` | handle test reads `text` after insert |
| WC-13 Read-only demo | `open` clears the editor. `chrome` defaults to true | `an_edit_session_types_and_keeps_the_read_only_open_path` |
| WC-14 Lock and raw XML | `model.locked`, `raw_xml = None` on mutation | model edit path |
| WC-15 Atomic field | `hit_caret` sets `select_end` | caret unit, click selects the marker |

## Commands a host calls

| Wasm | Handle | Editor |
| --- | --- | --- |
| `editBlank` | `edit_blank` | `blank` |
| `editOpen` | `edit_open` | `open` |
| `editInsert` | `edit_insert` | `insert_text` |
| `editDelete` | `edit_delete` | `delete` |
| `editEnter` | `edit_enter` | `insert_newline` |
| `editMergeField` | `edit_field` | `insert_field` |
| `editTable` | `edit_table` | `insert_table` |
| `editPicture` | `edit_picture` | `insert_picture` |
| `editMark` | `edit_mark` | `toggle_mark` |
| `editAlign` | `edit_align` | `set_align` |
| `editList` | `edit_list` | `toggle_list` |
| `editFont` | `edit_font` | `set_font` |
| `editFontSize` | `edit_font_size` | `set_font_size` |
| `editColor` | `edit_color` | `set_color` |
| `editHighlight` | `edit_highlight` | `set_highlight` |
| `editUndo` / `editRedo` | `edit_undo` / `edit_redo` | `undo` / `redo` |
| `editCaret` | `edit_click` | `click` |
| `editMove` | `edit_move` | `move_caret` |
| `editSelectAll` | `edit_select_all` | `select_all` |
| `editSelectSentence` | `edit_select_sentence` | `select_sentence` |
| `editParaSpacing` | `edit_para_spacing` | `para_spacing` |
| `editParaShading` | `edit_para_shading` | `set_para_shading` via editor |
| `editParaBorder` | `edit_para_border` | `set_para_border` via editor |
| `editPreview` | `edit_preview` | `preview` |
| `editEndPreview` | `edit_end_preview` | `end_preview` |
| `documentBytes` | `document_bytes` | `package` |
| `editState` | `edit_state` | `state` |
