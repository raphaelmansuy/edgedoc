# 001 — Word control

EdgeDoc as a Word-like document control for a SaaS assembly surface. A person composes a document, or edits a merge-field template (invoice, shipping note, letter, a table of fields), in the browser. The page should feel as direct as Word.

This folder is the contract. The running page is [`site/word`](../../site/word/index.html). The editing session is [`crates/edgedoc-display/src/edit.rs`](../../crates/edgedoc-display/src/edit.rs).

## Five whys

1. Why a control, and not a file export? People change an invoice after they see it. A download they must reopen in Word breaks the assembly flow.
2. Why Word-like, and not a form? The documents already exist as Word packages: letters, price lists, invoices with tables. The control has to edit those packages, not a parallel format.
3. Why merge fields inside the prose? The field is part of the sentence. A side form cannot show that «Customer» sits after “Bill to”.
4. Why the browser? The assembly product is a web app. The control has to load in that page, keep the package bytes, and hand them back on save.
5. Why build it on the display stack? The display library already paints the package faithfully. A second layout engine would drift from the file the customer downloads.

## First principles

- The package is the document. An edit that is not written into the package did not happen. An unedited save returns the original bytes (`Document::write_bytes` when the package is clean).
- What the person sees is what the layout produced. The caret and the selection use the same page geometry as the paint, mapped back to source bytes by [`SourceSpan`](../../crates/edgedoc-flow/src/lib.rs).
- A merge field is one object. The caret never sits inside the marker. A click selects the whole field.
- One session owns the package, the caret, and the undo stack. The page sends commands. It does not keep a second model.
- Layout of a business template is a few milliseconds, so each edit lays the document out again. There is no second, incremental geometry.
- Preview shows a record. It does not change the template.

## How to read this set

| Document | Lens | What it settles |
| --- | --- | --- |
| [Product](product.md) | Product owner | Who it is for, and the jobs |
| [Experience](ux.md) | UX, UI, front | Screens, navigation, visual rules |
| [Architecture](architecture.md) | Full stack | Modules, SOLID, DRY, the command sequence |
| [Editing](editing.md) | Full stack | Caret, mutations, edge cases, state |
| [Merge](merge.md) | Data | Fields, records, the template/preview split |
| [SaaS](saas.md) | SaaS, marketing, growth | How a host embeds it, and why a team adopts it |
| [Plan](plan.md) | All | Traceability from each requirement to code and tests |

## Requirement index

| Id | Statement | Detail |
| --- | --- | --- |
| WC-01 | The package is the document | [Architecture](architecture.md) |
| WC-02 | The caret uses source spans | [Editing](editing.md) |
| WC-03 | A field is atomic | [Editing](editing.md), [Merge](merge.md) |
| WC-04 | Each edit lays the document out again | [Architecture](architecture.md) |
| WC-05 | Undo stores packages, capped at 80 | [Editing](editing.md) |
| WC-06 | Preview does not change the template | [Merge](merge.md) |
| WC-07 | Home, Insert, Layout, Mailings | [Experience](ux.md) |
| WC-08 | Fields stay visible beside the page | [Experience](ux.md) |
| WC-09 | A selection stays in one block list | [Editing](editing.md) |
| WC-10 | An object marker stays in its own run | [Editing](editing.md) |
| WC-11 | End-to-end screenshots are kept and reviewed | [Plan](plan.md) |
| WC-12 | Locked content is refused | [Editing](editing.md) |
| WC-13 | A paste is capped at 100,000 characters | [Editing](editing.md) |
| WC-14 | A picture is PNG or JPEG, and at most 8 MB | [Editing](editing.md) |
| WC-15 | A field name is 1–64 characters, with no controls | [Merge](merge.md) |
| WC-16 | Zoom keeps the caret on the same source offset | [Experience](ux.md) |
| WC-17 | A new document is A4 with a 1 inch margin | [Product](product.md) |
| WC-18 | Arrows, words, Home, and End move the caret | [Editing](editing.md) |
