# Product

Lens: product owner. Related: [why this exists](README.md), [the screen](ux.md), [what ships](plan.md).

## The job

A person in the assembly product has to produce a Word document without leaving the app.

| Job | Done when |
| --- | --- |
| Write a letter or a note | Text, Enter, bold, and undo behave like Word |
| Prepare an invoice or a shipping note | Merge fields sit in the sentence, and a table can hold them |
| Check the template against one customer | Preview fills the fields and the template is still there afterwards |
| Hand the file to someone who uses Word | Save downloads the package, and Word opens it |

The reference editor in `wordcraft` shows the kind of surface people expect. This control does not port that application. It reuses the EdgeDoc package and the display painter. See [architecture](architecture.md).

## The document they start from

| Action | Result | Id |
| --- | --- | --- |
| New | A4, 1 inch margins, title Untitled, caret in the first paragraph | WC-17 |
| Open | The package they choose, caret in the first paragraph | WC-01 |
| Sample | Invoice, letter, price list, or report from `site/assets/documents` | WC-08 |
| Save | The current package, including edits | WC-01 |

## What “as good as Word” means here

The first hour of work, not the whole of Word.

- The caret is where the click landed, including after zoom (WC-16).
- Typing, Enter, Shift+Enter, Backspace, and Delete change the package.
- Bold, italic, underline, font, size, color, and highlight apply to the selection, or to the next characters when nothing is selected.
- Ctrl+Left and Ctrl+Right move by word. Home and End move on the line. Ctrl+Home and Ctrl+End move through the body (WC-18).
- A merge field is one chip. Clicking it selects it. The painted result is «Name» (WC-03).
- Undo and redo walk the package, up to 80 steps (WC-05).

Later Word features (comments, tracked changes as a review UI, section dialogs, headers as a story of their own) stay in the package when they were already there. The control does not yet offer a ribbon for them. Opening such a file still paints, and an edit drops retained markup only with the consent flags the session already sets (`EditOptions` all true). See [editing](editing.md).

## Out of the first release

- Real-time co-authoring.
- Desktop Word automation.
- A template gallery beyond the four samples.
- Mail merge of many records in one click. Preview is one record (WC-06). The host can merge the saved package with its own record set.

## Success

A person can open the invoice sample, change a sentence, insert a field, preview one customer, and save a `.docx` that still contains the field. The end-to-end script in [the plan](plan.md) is that path.
