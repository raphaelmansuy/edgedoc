# 07 — Implementation plan

The plan follows the principles in [README](README.md) and the shape in [03](03-architecture.md). Each step is in the tree.

| Step | Done when | Where |
| --- | --- | --- |
| 1. Model edits | Insert, delete, split, join, format, table, field, picture. Object runs stay isolated. | `edgedoc-model` `para.rs`, `document.rs` |
| 2. Source map | Spans recorded at expansion and remapped for page numbers. | `edgedoc-flow`, `edgedoc-display` `build.rs` |
| 3. Session | `Editor` lays out after each committed edit. Undo is package bytes. | `edgedoc-display` `edit.rs`, `caret.rs` |
| 4. Module surface | `DisplayHandle` edit commands return state JSON and page bytes. `open` stays read-only. | `edgedoc-display-wasm` |
| 5. Viewer reuse | `chrome: false`, `repaint({ keepView })`. The display demo is unchanged when `chrome` is omitted. | `site/display/edgedoc-viewer.js` |
| 6. Word page | Ribbon, caret, fields, preview, download. | `site/word/` |
| 7. Proof | Editor tests, handle test, `make word-e2e`, screenshots inspected. | `tools/word_control_e2e.mjs` |

## Order of a change

A new editing command lands in `Editor` first, with a native test. Then one method on `DisplayHandle`, then one wasm name, then one ribbon control. The page does not grow a second way to mutate a paragraph.

## Tests that must stay green

- `cargo test -p edgedoc-display --lib edit`
- `cargo test -p edgedoc-display-wasm --lib an_edit_session`
- `node tools/word_control_e2e.mjs` after `make display-wasm`

The display demo's e2e stays the check that `chrome` defaults to on.
