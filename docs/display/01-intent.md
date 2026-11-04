# Why EdgeDoc shows Word documents in the browser.

> Part of the EdgeDoc Display specification. Date: 2026-11-04. Requirement ids are
> normative. See [the index](README.md).

## 1. Five whys

### 1.1 Why a Word document must appear on a web page

A business sends invoices, statements, contracts, and reports as `.docx` files. A reader
who opens the web page must see the same page that Word shows. A download link is not a
display. A converted copy is a second document that the sender did not approve.

### 1.2 Why a server conversion is not enough

A PDF conversion fixes the page. The reader then loses the live link, the heading
structure, and the text that the page carries. An HTML conversion keeps the text and
loses the page: margins, header, numbering, and page breaks move. Both routes also upload
a confidential file to a service.

### 1.3 Why the layout must run in the browser

The file stays on the device. There is no upload, no server cost, and no trust boundary.
The page works offline after the first load. A server cannot do this for a private
document without a promise that the business may not accept.

### 1.4 Why Rust and WebAssembly rather than JavaScript

A Word page needs text shaping, bidirectional text, Unicode line breaking, and page
filling. JavaScript libraries approximate these steps, and each one drifts. Rust gives
one implementation with unit tests, native speed, and reuse in a server, a command-line
tool, and the browser. A Rust core also gives a deterministic layout, so a golden image
test can compare two runs.

`DP-I-01`. The layout engine is written in Rust and compiled to `wasm32-unknown-unknown`.
It does not call the browser to measure or to break text.

### 1.5 Why a display list and a painter trait rather than HTML and CSS

A browser that lays out HTML keeps control of the line breaks and the page breaks. Word
breaks lines with its own rules: its hyphenation, its justification, its table fit. A DOM
cannot be told to break a line at the position Word chose. The DOM also gives no stable
coordinate for a glyph.

A display list is a list of paint operations in page coordinates. It is measurable, so a
test can compare it with a reference. It paints on a canvas, an SVG tree, or a PDF file
through one interface. It also gives exact hit testing, exact selection rectangles, and a
text layer that matches the paint.

## 2. Goals

`DP-I-02`. The reader sees a paged document: page size, margins, columns, header, footer,
and page breaks as Word draws them.

`DP-I-03`. The reader zooms from 10% to 800% with no loss of sharpness at the new scale.
The point under the pointer stays under the pointer.

`DP-I-04`. The reader scrolls one continuous strip of pages. The viewer paints only the
pages near the viewport.

`DP-I-05`. The reader selects text with the mouse and with the keyboard. The selection
copies as text. A screen reader reads the page in document order.

`DP-I-06`. The reader searches the document and moves between hits.

`DP-I-07`. The reader prints the document, and the paper matches the screen.

`DP-I-08`. One call loads a document and paints it. The call takes bytes, and it returns a
value that the caller can control.

`DP-I-09`. A document that the library cannot lay out completely still shows every page.
The page that fails shows a placeholder and a diagnostic. The viewer does not blank.

## 3. Non-goals

`DP-I-10`. The library does not edit. It reads a package and paints pages. The editing
library is in [../05-model-and-api.md](../05-model-and-api.md).

`DP-I-11`. The library does not run macros, scripts, or OLE objects. It does not fetch a
remote resource without an explicit policy.

`DP-I-12`. The library does not promise a byte-identical page against Word. It promises
the fidelity tiers in section 4 and the gates in [14-testing-and-quality.md](14-testing-and-quality.md).

`DP-I-13`. The library does not implement a text editor, a comment editor, or a form
filler. It shows the content that the file holds.

## 4. Fidelity tiers

A tier is a set of features and a gate. A build states the tiers that it claims. A
feature outside every claimed tier paints a placeholder with its extent and a diagnostic.

| Tier | Name | Content |
|---|---|---|
| T0 | Page frame | Page size, margins, orientation, one column, body text, page breaks |
| T1 | Office document | Character and paragraph formatting, styles, lists, tables, images, links, sections, headers, footers |
| T2 | Full flow | Footnotes, endnotes, columns, floating objects, text boxes, fields with a cached result, tracked changes, content controls |
| T3 | Rich objects | Charts, equations, SmartArt, drawings, embedded objects, ink |

`DP-I-14`. Version 1 claims T0, T1, and T2 for read. It claims T3 only for the placeholder
path, and for a registered `ObjectRenderer`.

`DP-I-15`. A gate is a number, and a test measures it:

| Gate | Measure | Target |
|---|---|---|
| G1 | Page-break parity against a reference render | 95% of page breaks at the same block |
| G2 | Glyph position error | 0.5 pt for 99% of glyphs |
| G3 | Text order and content | 100% of characters, in order, on every page |
| G4 | Image position and size | 1 pt |
| G5 | Table cell rectangle | 1 pt |

G1 and G2 need a reference. [14-testing-and-quality.md](14-testing-and-quality.md) states
how the reference is made, and why Word is not the only reference.

## 5. Success metrics

`DP-I-16`. The metrics below are targets. A test in [12-performance.md](12-performance.md)
replaces each one with a measurement.

| Metric | Target | Condition |
|---|---|---|
| First page painted | 300 ms | 10-page invoice, 4-core laptop, cold cache |
| Last page painted | 2 s | 100-page report, warm cache |
| Zoom response | 16 ms per frame | CSS transform only, no repaint |
| Repaint after zoom | 120 ms | One A4 page at 200% |
| Document bytes to layout | 4x file size | Display list and glyph runs |
| Worker module, gzip | 1.6 MiB | Default features |
| Viewer script, gzip | 60 KiB | Without the worker module |

## 6. Readers of this specification

`DP-I-17`. The primary reader writes the layout engine and checks a requirement against a
test. The secondary reader builds the demo viewer and needs the contract of the API only.
Chapters 01 to 08 and 13 to 16 are descriptive. Chapter 15 is a procedure.

## 7. Why the page count is not in the file

A Word package stores no page positions. It stores content and formatting. Word computes
the page when it opens the file. It may cache two hints, and neither one is trusted:

- `w:lastRenderedPageBreak` marks where a page break fell when Word last saved. It can be
  stale after an edit on another machine.
- `docProps/app.xml` holds `Pages`. It is a number from the last save, and it can be wrong.

`DP-I-18`. The library computes pagination from content and fonts. It reads the two hints
only to size the scrollbar before layout completes ([12-performance.md](12-performance.md)).

This fact drives the whole design. A page exists only after layout, so the viewer streams
pages as the layout produces them.
