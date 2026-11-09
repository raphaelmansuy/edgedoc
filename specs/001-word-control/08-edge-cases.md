# 08 — Edge cases

Every row is a behavior the code enforces. The test column points at [09](09-traceability.md).

| Case | What happens | Test |
| --- | --- | --- |
| Type and undo | The package returns to the previous bytes and caret | `typing_inserts_and_undo_restores` |
| Enter in the middle of a word | Two paragraphs, caret on the second | `enter_splits_the_paragraph` |
| Backspace at the start of a paragraph | Joins with the previous body paragraph | `backspace_joins_paragraphs` |
| Bold on a selection | The run is bold after a round trip | `bold_marks_the_selection` |
| Merge field | The written package has `MERGEFIELD`, and plain text shows `«Name»` | `a_merge_field_round_trips_into_the_package` |
| Preview | The merged text shows the record. Typing fails. Ending preview restores the field names | `preview_fills_a_field_without_losing_the_template` |
| `open` after an edit session | Further edit commands return `editor.closed` | `an_edit_session_types_and_keeps_the_read_only_open_path` |
| Home on a line | Offset 0 of that line, not the end | `backspace_joins_paragraphs` (Home, then join) |
| Object run coalesced with text | `isolate_objects` splits the marker out before write | merge-field round trip |
| Click on a field | The whole marker is selected | caret `select_end` |
| Selection across cells | `model.bad_path`, no commit | `delete_span` path check |
| Locked paragraph | `model.locked` | model edit |
| `raw_xml` present | Cleared on the edited paragraph so the model is what is written | `insert_text` |
| Paste over 100,000 characters | `editor.limit` | `insert_text` guard |
| Field name empty or with a control character | `editor.field` | `insert_field` guard |
| Table 0 or 33 | `model.limit` | `insert_table` guard |
| GIF or a 9 MB image | `editor.image` / `editor.limit` | `insert_picture` guard |
| Edit during preview | `editor.preview`, template bytes kept | preview test and e2e `07-template-restored` |
| Page field grows from 9 to 10 | Spans remap in `with_fields` | flow source remap |
| Empty paragraph | A caret still has a slot | blank document e2e |
| Unedited save | Original bytes | existing `write_bytes` when not dirty |
| Encrypted or `.doc` | Opener error, toast, no session | existing package errors |
| IME composition | The composed string is inserted on `compositionend` via the paste and key paths the browser delivers as text | keyboard handler skips `isComposing` |
| Zoom after an edit | `repaint({ keepView })` keeps scale and scroll | viewer `repaint` |
| Read-only demo | `chrome` defaults to true. `open` does not start an editor | display e2e, handle test |

## Mitigations that are structural

- Commit-on-Ok means a refused edit does not need a compensating undo.
- Preview stashes bytes before it replaces the laid-out document, so a failed merge can still end on the template.
- Full layout per keystroke is the mitigation for a stale caret. Measured cost on the invoice template is a few milliseconds.
