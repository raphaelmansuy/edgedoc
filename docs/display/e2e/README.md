# The end-to-end run, and what each screenshot shows

Date: 2026-11-04. Part of the EdgeDoc Display specification. See [../README.md](../README.md).

`node tools/display_e2e.mjs` serves `site/`, opens the demo in headless Chrome, loads
`site/assets/documents/report.docx`, and drives the viewer through every control. It writes one
screenshot per step here and a machine-readable result to [report.json](report.json).

A person inspects the screenshots. This folder exists to catch a defect that a check cannot state.
Four examples: a missing colour, a doubled rule, a word broken mid-word, and a page number that
does not change.

## The run

```
EdgeDoc Display: end-to-end

  ok    the viewer module loads
  ok    every page mounts  15 pages
  ok    the first page paints ink  210426 dark pixels
  ok    the text layer holds one span per run  52 spans
  ok    the first paint is under 300 ms  290 ms
  ok    a missing font is reported  2 diagnostics
  ok    fit width sets a scale  1.861
  ok    zoom 200 reaches 2.0  2.000
  ok    a zoom change moves the scroll position
  ok    goToPage reaches page 2  Page 2 of 15
  ok    the page field follows
  ok    a page field shows the page number
  ok    a total field shows the page count  of 15
  ok    a click finds a character
  ok    search finds a term  21 hits
  ok    the outline holds the headings  30 entries
  ok    a bad file is refused with a code  package.not_zip
  ok    the browser reported no error

18/18 checks passed
```

## The screenshots

| File | What it shows | What to look for |
|---|---|---|
| `01-empty.png` | The viewer before a document | The toolbar holds the page field, the zoom, and Print. No page area is blank without a message |
| `02-loaded.png` | Page 1 at fit width | The logo, the chart image, the caption, the footer. The title is large and bold |
| `03-fit-width.png` | Fit width | The page fills the viewport, and the side margin stays even |
| `04-zoom-200.png` | Zoom at 200% | Text stays sharp, and the page does not blur |
| `05-page-2.png` | Page 2 | The KPI strip, the two-column table, and the numbered list |
| `06-refused.png` | A file that is not a package | The panel names the code and one next action |
| `07-page-2-full.png` | Page 2 at a readable zoom | The blue headings, the grey labels, the rules drawn once |
| `08-narrow.png` | A 430 px viewport | The toolbar wraps, and the page stays usable |

## What the screenshots caught

Each row is a defect that the checks passed and a person found. The left column is what the page
showed, and the right column is the fix.

| Defect | Cause | Fix |
|---|---|---|
| Every page element mounted, but the page area was blank | `loadBytes` rewrote `container.innerHTML`, which detached every element the viewer held | A load no longer rewrites the container. A failure panel replaces it and the next load builds a new viewer |
| The chart and the logo were missing | The viewer had no `image` case, and the module did not expose the image bytes | `DP-G-12` gains an image index, and the viewer decodes each image once |
| The chart caption sat 100 pt too low | A picture-only paragraph reserved the picture height as a line, and the picture took it again | A picture-only paragraph emits the picture block and no paragraph |
| Every heading was black | `RunStyle` carried no colour, so the lowerer painted black for every run | `RunStyle` carries the colour, the underline, the strike, and the highlight, which `DP-S-08` already required |
| A table edge was drawn twice, at double width | Each cell painted all four of its borders | A cell paints its outer edges only, and the table's inside rules paint the shared edges once (`DP-L-39`) |
| A word broke in the middle: `ba` and `se.` | The breaker cut at the first cluster that did not fit | The breaker backs up to the last break opportunity, so a word moves whole (`DP-F-23`) |
| The footer said `Page 2 of 1` | The total field read a guess, because the flow never scanned the headers for a total | The flow scans every section part for `NUMPAGES` and takes a second pass (`DP-L-12`) |
| Characters looked uneven inside a word | Each glyph was placed on its own pen, which lost the browser's kerning | A run with no offset paints as one string, and the browser kerns it (`DP-G-04`) |

## How to run it

```bash
make display-wasm          # build the module and the wasm-bindgen glue into site/display
node tools/display_e2e.mjs # run the browser test and write this folder
```

The script needs Chrome at `/Applications/Google Chrome.app/Contents/MacOS/Google Chrome` and
puppeteer. Set `CHROME_PATH` and `PUPPETEER_PATH` to override both.

## A sample of each document

| Document | Pages | What it exercises |
|---|---|---|
| `report.docx` | 15 | Sections, columns, a chart, a KPI table, a numbered list, `PAGE` and `NUMPAGES` fields |
| `invoice-template.docx` | 2 | A shaded table header, merge fields, right-aligned totals, a header address block |
| `letter-template.docx` | 1 | Merge fields, a signature block, one page with a tall header |
| `price-list-template.docx` | 1 | A wide table, a missing style, a table that fits the page area |
