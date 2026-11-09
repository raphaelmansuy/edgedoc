# 04 — Editing

Lens: full stack. The screen is [02](02-experience.md). The module map is [03](03-architecture.md). Edge cases are [08](08-edge-cases.md).

## Source map

Layout expands a field before paint. `SourceSpan` records `{ expanded_start, len, source_start, atomic }` on the flow paragraph and on each fragment.

- Plain text is one-to-one. Adjacent spans merge.
- A field, a picture, a note, and a positional tab are atomic. `source_start` is the marker, and the source length is 3 (U+FFFC).
- `with_fields` replaces a page-number marker with one or more digits and rebuilds the spans so a later click still lands on the source marker.
- An empty `source` list means identity. Tests and callers that do not edit keep that.

A click uses `hit_caret`. The caret's page point is `fragment.rect` plus the line's cluster x and the line's top. `line.xs` has one entry per boundary, including the end of the line.

## Runs

The writer emits a field only when a run's text is exactly U+FFFC. `isolate_objects` runs after insert, delete, split, join, and append, and it refuses to merge a marker run with its neighbors.

`raw_xml` is cleared on a text edit. If it stayed, the writer would emit the old XML and drop the change. A locked paragraph returns `model.locked` and the edit does not commit.

## Session

`Editor` owns the `DisplayDocument`, the caret, the anchor, pending character properties, and the undo stack.

- No selection: bold, italic, underline, font, size, color, and highlight set `pending` and apply to the next insert. Moving the caret clears `pending`.
- A selection toggles a mark on when any character in the range lacks it, and applies font, size, color, and highlight to the range.
- Enter splits the paragraph. Shift+Enter inserts `\n`, which layout already treats as a line break.
- Backspace at offset 0 joins the previous paragraph when one exists in the same container. Top-level body paragraphs are one container. Cells in one row's parent are one container. A selection that crosses cells is `model.bad_path`.
- Undo stores the package bytes and the caret, capped at 80. Redo is the other stack.
- `info_at` reads paragraph properties through a clone that always returns an error, so the live document is not dirtied by a peek.

## Paths

Block paths the painter emits:

- `body[N]`
- `body[N]/row[R]/cell[C]/block[B]` nested the same way
- `part[id]/block[N]`

`BlockPath` in the model is one table deep, so the editor parses the path and walks nested tables itself.

## Limits

| Limit | Code |
| --- | --- |
| Paste above 100,000 characters | `editor.limit` |
| Font size outside 1..=1638 | `editor.limit` |
| Field name empty, longer than 64, or containing a control character | `editor.field` |
| Table outside 1..=32 | `model.limit` |
| Picture over 8 MB, or not PNG or JPEG | `editor.limit` / `editor.image` |
| Edit while preview is showing | `editor.preview` |
| Command before `editOpen` or `editBlank` | `editor.closed` |
