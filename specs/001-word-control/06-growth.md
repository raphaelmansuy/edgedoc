# 06 — Growth

Lens: marketing and growth. The job is [01](01-intent.md). The embed is [05](05-merge-and-documents.md).

## The sentence a host can repeat

Compose the invoice in the app. Preview it with one customer. Download a Word file.

That sentence is true when three things are visible without a tour:

1. A template opens as pages, not as XML.
2. A merge field is a named chip the author inserts, and a form the operator fills.
3. Download is a `.docx`.

## Adoption

| Step | What the host does | What we hand them |
| --- | --- | --- |
| Try | Open `site/word/?doc=invoice-template.docx` | The invoice template and the ribbon |
| Embed | Frame the page, or call the wasm `Viewer` | `editOpen`, `documentBytes`, `editPreview` |
| Trust | Save an unedited file and diff it | Original bytes when nothing changed |
| Expand | Add their own template | The same field panel, because names come from the package |

## What we do not claim

We do not claim pixel parity with Word's pagination, tracked changes that survive an edit, or a mail-merge of a thousand records. Preview is one record, on purpose: an operator checks the letter before the host's own job sends the batch. The batch stays in the host's pipeline, using the same `preview_with` the control uses for that one record.

## Proof a buyer can see

The screenshots in [e2e/screenshots](e2e/screenshots) are the blank page, typing, a field, bold, the invoice, preview, the restored template, and a table. They are produced by `tools/word_control_e2e.mjs`, not drawn by hand.
