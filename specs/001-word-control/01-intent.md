# 01 — Intent

Lens: product owner. See [README](README.md) for the five whys. Experience is [02](02-experience.md). The promise to a host is [05](05-merge-and-documents.md) and [06](06-growth.md).

## Job

A person in a SaaS app opens a template, fixes the wording, drops a merge field into a sentence or a table cell, previews it with one customer, and downloads a `.docx` Word can open.

The control is successful when that person does not have to leave the app, and when a file they did not edit comes back byte-for-byte.

## Users

- **Template author.** Writes the invoice, the letter, the packing list. Needs fields, tables, pictures, and ordinary formatting.
- **Operator.** Opens a template, checks a preview for one record, downloads or hands the bytes to the host.
- **Host developer.** Embeds one page, or the same module, and stores the package. They do not parse WordprocessingML.

## Requirements

| Id | Statement |
| --- | --- |
| WC-1 | Open a `.docx` package and show its pages. |
| WC-2 | Start from a blank A4 page with 25.4 mm margins and the title Untitled. |
| WC-3 | Type, delete, and split paragraphs, including a soft line break. |
| WC-4 | Place the caret from a click, and move it with the arrows, Home, and End. |
| WC-5 | Select a range and apply bold, italic, underline, font, size, color, and highlight. |
| WC-6 | Set alignment and toggle a bullet or numbered list on the paragraph. |
| WC-7 | Insert a merge field that round-trips through `write_bytes` and reads back as `MERGEFIELD`. |
| WC-8 | Insert a table of 1 to 32 rows and columns, and put the caret in the first cell. |
| WC-9 | Insert a PNG or JPEG, at most 8 MB. |
| WC-10 | Undo and redo, up to 80 snapshots of the package. |
| WC-11 | Preview one record. The template bytes are restored when preview ends. Edits during preview fail with `editor.preview`. |
| WC-12 | Download the current package. An unedited open returns the original bytes. |
| WC-13 | The read-only display demo keeps working. Editing is a separate entry on the same module. |
| WC-14 | A locked paragraph is refused. A paragraph whose `raw_xml` was kept is cleared when its text changes, so the edit is what gets written. |
| WC-15 | A click on a field selects the whole field. Typing replaces it. |

## Acceptance

The host can point a browser at `site/word/index.html?doc=invoice-template.docx`, see the invoice, insert or preview a field, and download a package Word opens. The checks live in `tools/word_control_e2e.mjs` and in the editor tests named from [09](09-traceability.md).
