# Expanded external rendering corpus

On 2026-11-05 a [second pinned manifest](external-expanded-manifest.json) added 181 more
upstream fixtures: 90 from Apache POI and 91 from docx4j, totaling 11,482,731 bytes. It
includes 164 bounded DOCX ZIP archives and 17 upstream negative or unsupported archive cases.
Together the two manifests contain 256 independently sourced input cases. The
[second structural inventory](external-expanded-inventory.json) records 637,637 extracted text
characters where XML is readable and reports malformed/encrypted fixtures individually.

Reproduce and verify the additional inputs without changing the original 75-file corpus:

```sh
python3 tools/fetch_external_corpus.py --manifest tests/corpus/external-expanded-manifest.json \
  --directory target/external-docx-expanded
python3 tools/fetch_external_corpus.py --verify-only --manifest tests/corpus/external-expanded-manifest.json \
  --directory target/external-docx-expanded
python3 tools/wordcraft_parity.py prepare target/wordcraft-parity/expanded-256 \
  --retain-manifest target/wordcraft-parity/expanded-final/manifest.json \
  --fixtures target/wordcraft-parity/cell-border-fixtures \
  --corpus-manifest tests/corpus/external-expanded-manifest.json \
  --corpus-directory target/external-docx-expanded
python3 tools/wordcraft_parity.py run target/wordcraft-parity/expanded-256 \
  --font-config /absolute/path/to/font-profile.json --reuse-reference
```

The additional manifest includes fuzz, malformed, encrypted and otherwise unsupported cases;
all are retained in the report and independently attempted by both engines. A bounded rejection
is a rejection, not a passing rendering comparison.

Selected on 2026-11-05: 75 unmodified DOCX files, 6.7 MB compressed, with 303,227 text characters.
The [manifest](external-manifest.json) pins the upstream repository, commit, original path,
Git blob identity, SHA-256, byte size and feature tags for every file. Downloads stay in the
ignored `target/external-docx/` directory. They are never rewritten to make a comparison pass.

Sources are [Apache POI's document fixtures](https://github.com/apache/poi/tree/7f4d36854109efa17152b27a5fb1b2c7d842c314/test-data/document)
and [docx4j](https://github.com/plutext/docx4j/tree/0eec2587ab38db5265ce66c12423849c1bea2c60).
Both upstream projects publish their repositories under Apache License 2.0; original files and
notices remain upstream. The manifest records the original source of each downloaded file.
No proprietary font files are distributed with this corpus.

The selection includes technical guides and invoices as well as focused rendering regressions:

- `Docx4j_GettingStarted.docx`: about 70,000 text characters, 1,285 paragraph elements, nine tables,
  four footnote references, 76 field-instruction elements and 601 tabs; its stored page hint is 45.
- `headers_footers.docx`: about 41,000 characters, multi-section content and a stored 31-page hint.
- `rFonts.docx`: about 42,000 characters exercising font resolution and a stored 20-page hint.
- A Russian guide, Arabic alignment/numbering, invoices, multilevel lists and start overrides,
  first/even/linked headers, table alignment and repeated headers, nested tables, notes,
  tracked changes, images, bookmarks, fields and text boxes.
- A deeply nested table file deliberately exceeds the default XML depth budget. A bounded
  rejection is recorded separately from a successful rendering; limits are not relaxed to
  inflate the pass count.

The [structural inventory](external-inventory.json) is measured from the original ZIP parts.
Stored page counts are hints supplied by the authoring application and may be stale. Wordcraft's
actual layout and display list provide the comparison reference.

## Reproduce

```sh
python3 tools/fetch_external_corpus.py
python3 tools/fetch_external_corpus.py --verify-only
python3 tools/wordcraft_parity.py prepare target/wordcraft-parity/expanded \
  --fixtures target/wordcraft-parity/current-fixtures \
  --corpus-manifest tests/corpus/external-manifest.json
python3 tools/wordcraft_parity.py run target/wordcraft-parity/expanded \
  --font-config /absolute/path/to/font-profile.json
```

First export the focused fixtures as described in
[the parity runner instructions](../../tools/wordcraft-parity/README.md). The font profile is a JSON
list of `regular=/absolute/font.ttf`, `bold=/absolute/bold.ttf`, `italic=...` and `bold-italic=...`
entries. The same actual font data must be used by both engines. A reference-adapter limitation
or missing comparison font does not establish a renderer defect or a parity pass.

The downloader rejects altered, truncated or oversized archives and verifies both SHA-256 and
upstream Git blob identity. `--verify-only` never accesses the network. Renderer subprocesses and
SVG rasterization have timeouts, and failures remain in the report. The 42 earlier synthetic
cases, including the existing note/drop-cap failures, stay in the combined gate.

To retain every input from an earlier run while appending newly exported regression inputs:

```sh
python3 tools/wordcraft_parity.py prepare target/wordcraft-parity/expanded-after \
  --retain-manifest target/wordcraft-parity/expanded-before/manifest.json \
  --fixtures target/wordcraft-parity/expanded-fixtures
```

Retained inputs are hash-checked before copying. Only newly appended duplicates are deduplicated;
earlier case identities and failures remain. This first expanded run contains 141 cases. The
current [measured result and image inspection](../../docs/display/21-external-corpus-parity.md)
also includes two defects found in rendered images: footer borders and leading tabs. It retains
all 117 earlier inputs plus 26 distinct regression inputs, for 143 cases in
`target/wordcraft-parity/expanded-final/`.

The subsequent [327-case measurement](../../docs/display/22-expanded-corpus-parity.md) includes
all 256 upstream inputs and the retained regression cases. The additional examples exposed and
helped fix a 20,000-page empty-table loop and row-content-control rendering.

`make corpus-external-check` verifies all downloads and runs the opt-in native smoke test.
That test requires 74 documents to open and lay out, and the deep-table document to return the
expected depth-limit diagnostic. Successful bounded layout is separate from parity acceptance.
