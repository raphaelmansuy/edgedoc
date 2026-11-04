# EdgeDoc specification

Date: 2026-11-04. Status: design.

EdgeDoc is a standalone Rust library that reads, edits, merges, and saves Word documents. The same facade ships as a crate, a PyPI wheel, and a WebAssembly module.

A requirement id such as `ED-M-04` is normative. An implementation conforms when it meets every requirement it claims in [the index](10-traceability.md).

## Five whys

The full text is in [01-intent.md](01-intent.md). The chain is:

1. Invoices and letters still move as `.docx` files. A server job needs a library, not an editor.
2. `python-docx` and `docx-rs` drop the parts they do not model. The saved file is no longer the approved template.
3. A `.docx` file is a package of parts. A paragraph list cannot represent that package. [ECMA-376](https://ecma-international.org/publications-and-standards/standards/ecma-376/) defines the package and the markup.
4. One Rust core must feed Python and WebAssembly. Three merge engines would drift.
5. Creation, editing, and mail merge are different jobs. WordCraft already splits the document from merge state. EdgeDoc keeps that split and does not write preview text into the template.

## How to read these files

| File | What it specifies |
|---|---|
| [01-intent.md](01-intent.md) | Five whys, scope, and version 1 limits |
| [02-references.md](02-references.md) | Standards, terms, and conformance classes |
| [03-architecture.md](03-architecture.md) | Crates, call sequence, public types, SOLID, DRY, limits |
| [04-package.md](04-package.md) | OPC, flavors, namespaces, encryption |
| [05-model-and-api.md](05-model-and-api.md) | Document model and fluent API |
| [06-merge.md](06-merge.md) | Field merge, filters, labels, and letters |
| [07-adapters.md](07-adapters.md) | Error codes, PyPI, and WebAssembly |
| [08-edge-cases.md](08-edge-cases.md) | Boundary behavior and test names |
| [09-plan.md](09-plan.md) | Implementation plan |
| [10-traceability.md](10-traceability.md) | WordCraft map, requirement index, acceptance scenario |
| [11-format-coverage.md](11-format-coverage.md) | Word format coverage table and the test that locks each row |
| [12-word-features.md](12-word-features.md) | Every modeled Word field and its test |
| [13-word-validation.md](13-word-validation.md) | The rules Word enforces, and how to check a file |
| [14-round-trip.md](14-round-trip.md) | Every feature that must survive an edit, and its test |
| [15-adapters-audit.md](15-adapters-audit.md) | The Python and WebAssembly surface, and the audit against the spec |

The WordCraft checkout used for this design is `/Users/raphaelmansuy/Github/03-working/wordcraft`, package version `0.14.0`.

## Pictures

Crate layers, the call sequence, and the public types are in [03-architecture.md](03-architecture.md). The merge decision flow is in [06-merge.md](06-merge.md). The three packages over one core are in [07-adapters.md](07-adapters.md).
