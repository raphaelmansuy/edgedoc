# EdgeDoc Display specification

Date: 2026-11-04. Status: design. Version: 0.1.

EdgeDoc Display reads a Word package and paints it in a browser. It computes the page
layout that Word computes, and it paints that layout on a canvas. The reader gets a
paged document with zoom, scroll, search, selection, and print, in the manner of
[pdf.js](https://mozilla.github.io/pdf.js/).

The document library in [../README.md](../README.md) owns the package, the model, and the
merge engine. This specification owns everything from a parsed model to a painted page.

Start at [01-intent.md](01-intent.md). It begins with the five whys.

## Requirement ids

`DP-<class>-<nn>` is normative for this library. `ED-<class>-<nn>` stays normative for the
document library. An implementation conforms when it meets every requirement that it
claims in [16-traceability.md](16-traceability.md).

| Class | Prefix | Subject |
|---|---|---|
| I | `DP-I` | Intent, goals, and fidelity tiers |
| N | `DP-N` | Normative references and terms |
| A | `DP-A` | Architecture, SOLID, and DRY |
| P | `DP-P` | Open pipeline and resource resolution |
| S | `DP-S` | Style cascade and computed properties |
| F | `DP-F` | Text shaping, fonts, and line breaking |
| L | `DP-L` | Layout and pagination |
| G | `DP-G` | Display list and painters |
| V | `DP-V` | Viewer model and public API |
| B | `DP-B` | WebAssembly and browser integration |
| U | `DP-U` | Demo site and viewer user interface |
| E | `DP-E` | Performance budgets |
| X | `DP-X` | Edge cases |
| Q | `DP-Q` | Quality, fidelity gates, and tests |

## How to read these files

| File | What it specifies |
|---|---|
| [01-intent.md](01-intent.md) | Five whys, goals, non-goals, fidelity tiers, success metrics |
| [02-references.md](02-references.md) | Standards, prior art, terms, conformance classes |
| [03-architecture.md](03-architecture.md) | Crates, layers, traits, call sequence, SOLID and DRY rules |
| [04-pipeline.md](04-pipeline.md) | From bytes to a painted page, resource resolution, diagnostics |
| [05-styles.md](05-styles.md) | The style cascade, numbering, themes, and computed properties |
| [06-text-and-fonts.md](06-text-and-fonts.md) | Font database, shaping, bidi, line breaking, justification, tabs |
| [07-layout-and-pagination.md](07-layout-and-pagination.md) | Page filling, sections, columns, headers, footnotes, floats, tables |
| [08-display-list-and-paint.md](08-display-list-and-paint.md) | Display list IR, binary encoding, painters, tiles, hit testing, text layer |
| [09-viewer-api.md](09-viewer-api.md) | Rust and JavaScript API, zoom, paging, search, selection, print |
| [10-wasm-browser.md](10-wasm-browser.md) | Worker protocol, memory, cancellation, budgets, hosting |
| [11-demo-site.md](11-demo-site.md) | Demo user interface, wireframes, keyboard map, accessibility |
| [12-performance.md](12-performance.md) | Budgets, virtualization, caching, profiling |
| [13-edge-cases.md](13-edge-cases.md) | Every boundary case and the test that locks it |
| [14-testing-and-quality.md](14-testing-and-quality.md) | Corpus, golden images, fidelity metrics, CI gates |
| [15-plan.md](15-plan.md) | Phases, exit tests, estimates, risks |
| [16-traceability.md](16-traceability.md) | Requirement index, cross-reference matrix, DRY and SOLID audit |
| [17-wordcraft-map.md](17-wordcraft-map.md) | The WordCraft reference: crate map, ported code, known gaps |
| [18-assessment.md](18-assessment.md) | Every WordCraft-measured behavior, its state here, and the gap list |
| [19-deep-audit.md](19-deep-audit.md) | The deeper audit: property diff, test-name map, defects found, and the 29 gaps |
| [20-rendering-parity.md](20-rendering-parity.md) | Current verified rendering behavior, regression evidence, and remaining Word parity work |
| [21-external-corpus-parity.md](21-external-corpus-parity.md) | Pinned external DOCX corpus, measured Wordcraft comparisons, and manual rendered-page inspection |
| [22-expanded-corpus-parity.md](22-expanded-corpus-parity.md) | 256 upstream inputs, retained failures, table fixes, and current Wordcraft measurements |
| [23-wordcraft-code-parity-audit.md](23-wordcraft-code-parity-audit.md) | Detailed code-to-code rendering assessment, evidence limits, and prioritized gaps |
| [24-field-range-followup.md](24-field-range-followup.md) | Nested fields, TOC rendering fixes, image inspection, and current verification |
| [25-tabs-and-borders-followup.md](25-tabs-and-borders-followup.md) | Tab coordinates, inherited borders, page inspection, and updated comparison |
| [26-style-and-spacing-followup.md](26-style-and-spacing-followup.md) | Explicit formatting off values, automatic spacing, and line-break comparison |
| [27-footer-and-break-followup.md](27-footer-and-break-followup.md) | Bounded footer frame and trailing line-break behavior with page evidence |
| [28-bullet-followup.md](28-bullet-followup.md) | Symbol mappings, hanging bullet geometry, and updated comparison |
| [29-leading-and-notes-followup.md](29-leading-and-notes-followup.md) | Bottom-margin leading, automatic page breaks, and footnote image inspection |
| [30-hyphen-followup.md](30-hyphen-followup.md) | Special hyphen elements, edited saves, and remaining font fallback differences |
| [31-footnote-layout-followup.md](31-footnote-layout-followup.md) | Footnote reservation, continuation, image evidence, and remaining note gaps |
| [../../tests/corpus/README.md](../../tests/corpus/README.md) | The 33 real Word documents, and the checks that run over them |

## Quick start

The public call is one line. It takes the bytes of a `.docx` file and paints pages.

```ts
import { createViewer } from "edgedoc-viewer";

const viewer = await createViewer({
  container: "#viewer",
  workerUrl: new URL("./edgedoc-viewer.worker.js", import.meta.url),
  zoom: "fit-width",
});

await viewer.load(arrayBuffer);
```

The same call in Rust, without a browser:

```rust
use edgedoc_display::{DisplayDocument, OpenOptions};

let doc = DisplayDocument::open(&bytes, &fonts, &OpenOptions::default())?;
let report = doc.layout(&mut sink)?;
assert!(report.pages > 0);
```

## Status of this specification

Every requirement is a design decision, not a measurement. The phase plan in
[15-plan.md](15-plan.md) turns each one into a test. Where this specification estimates a
number, [12-performance.md](12-performance.md) names the measurement that replaces it.

## Reference implementation

The layout, shaping, and rendering behavior comes from the WordCraft checkout at version
`0.14.0`. [17-wordcraft-map.md](17-wordcraft-map.md) maps each display crate to the module
that proves the behavior. Read that chapter before you write a layout module. It also lists
the WordCraft gaps that this specification either fixes or states as a known limit. It also
corrects two decisions that an earlier draft made against the measured behavior.

- [Endnote layout follow-up](32-endnote-layout-followup.md)
- [Content-control layout follow-up](33-control-layout-followup.md)
- [Inherited table-style follow-up](34-table-style-followup.md)
- [Conditional table-style follow-up](35-conditional-table-style-followup.md)
- [Table row-height follow-up](36-table-row-height-followup.md)
