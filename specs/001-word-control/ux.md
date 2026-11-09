# Experience

Lenses: UX, UI, and front. Related: [jobs](product.md), [commands](architecture.md), [fields](merge.md).

The page is `site/word/index.html`. The viewer paints the pages. The control paints the chrome, the caret, and the selection. Brand color `#1f4e79`.

## Screen

```
+------------------------------------------------------------------+
| [Word]  Untitled                         New  Open  Save  Sample |
+------------------------------------------------------------------+
| Home   Insert   Layout   Mailings                                 |
| Undo Redo | Font Size B I U A ab | align  lists                   |
+------------------------------------------------------------------+
| Preview banner, only while a record is on screen                 |
+----------------------------------------------+-------------------+
| 0    1    2    3          ruler              | FIELDS            |
| +------------------------------------------+ | Customer          |
| |                                          | | Address           |
| |   Invoice 1042                           | |                   |
| |   Bill to «Customer»                     | | PREVIEW RECORD    |
| |   |                                      | | Customer [ Ada ]  |
| +------------------------------------------+ |                   |
+----------------------------------------------+-------------------+
| Page 1 of 1   3 words   Field Customer          Fit width  100%  |
+------------------------------------------------------------------+
```

On a narrow window the field column moves under the page, and the sample menu hides. The ribbon scrolls sideways instead of wrapping into a second broken row.

## Navigation

```
App bar
  New -------- blank A4
  Open ------- file package
  Save ------- download .docx
  Sample ----- invoice | letter | price list | report

Tabs
  Home ------- history, font, paragraph
  Insert ----- table, picture, page break
  Layout ----- page facts, alignment
  Mailings --- insert field, preview, return

Page
  click ------ place caret
  drag ------- extend selection
  type ------- insert
  keys ------- move, delete, marks, undo

Side column
  field name - fills the Mailings box
  record ----- values used by Preview
```

## Ribbon behavior

| Control | With a selection | With only a caret |
| --- | --- | --- |
| Bold, italic, underline | Toggles the range | Arms the next characters |
| Font, size, color, highlight | Applies to the range | Arms the next characters |
| Alignment, list | Applies to the paragraph | Applies to the paragraph |
| Undo, redo | Disabled when that stack is empty | Same |

A click on a ribbon button does not steal the caret. The button's `mousedown` is cancelled so the page keeps focus.

While preview is on, Home, Insert, and Layout ignore clicks. Mailings stays available so the person can return to the template. The banner repeats that action (WC-06).

## Caret and selection

The caret is a 2 px bar in `#1f4e79`, at least 12 px tall, blinking. A selection is a translucent rectangle on each line the range covers. The caret hides while a selection is showing.

Coordinates are page points times the current scale, measured from the page element. Zoom changes the scale and the caret is drawn again (WC-16). The source offset does not change.

## Visual rules

- The workspace is `#e7e9ee`. The page is white with a soft shadow.
- The ruler is 22 px, centered on the page width. Inch ticks, with the 1 inch margins shaded. That matches the blank document's 25.4 mm margins.
- Type is Segoe UI. The document itself is painted from font outlines (Carlito, Arimo, Tinos), not from CSS text.
- Focus rings are 2 px `#1f4e79`.
- Errors sit in the status bar, in `#a4262c`, and name the next action (“Leave preview before editing”).

## Keyboard

| Key | Action | Id |
| --- | --- | --- |
| Characters | Insert | WC-13 |
| Enter / Shift+Enter | New paragraph / line break | WC-18 |
| Backspace / Delete | Delete back / forward, joining paragraphs at the edges | WC-09 |
| Arrows | Character or line | WC-18 |
| Ctrl+Left / Right | Word | WC-18 |
| Home / End | Line edges | WC-18 |
| Ctrl+Home / End | Body edges | WC-18 |
| Ctrl+A | Select the paragraph | WC-09 |
| Ctrl+B/I/U | Marks | |
| Ctrl+Z / Ctrl+Y | Undo / redo | WC-05 |
| Ctrl+S | Save | WC-01 |

Keys typed into a field box, a menu, or a size box stay in that box.

## Empty, busy, and wrong

| State | What the person sees |
| --- | --- |
| New document | A blank page and a caret at the first line |
| No fields | “No merge fields yet.” and a hint in the side column |
| Preview | A banner, and the record's values in the prose |
| Refused edit | The status line, the page unchanged |
| File that is not a package | The status line from the package error |
