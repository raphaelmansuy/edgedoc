# Wordcraft rendering comparison

This independent Cargo workspace calls the reader, layout engine and page display list of the
sibling `../../../wordcraft` checkout. It does not patch the reference or import EdgeDoc layout
code. The reference source and commit, adapter source/lockfile, input DOCX and actual font bytes
are identified in the report. The local checkout tested on 2026-11-05 is Wordcraft 0.15.1,
commit `1c7b5013fab765f204cf435b9f8814b46b7c1278`.

The adapter exports glyph origins and outlines from Wordcraft's `Draw` list. Solid fills, solid
lines, clips, figure children and PNG/JPEG/GIF/WebP pictures with crop/opacity are supported.
Numbering labels without source ranges are mapped only when every character's glyph ID matches.
Decorative empty-text glyphs paint without inventing selectable text.
An unsupported draw, image effect/format or synthetic font face fails
the comparison instead of disappearing silently. Extending this adapter needs independent
validation; it is not a complete replacement for Wordcraft's native display exporters.

The earlier [result snapshot](../../docs/display/wordcraft-parity-result.json) contains 39 passing
and three failing cases (42 cases, 40 distinct DOCX inputs). The passing cases span 61 pages with
zero glyph-origin error and zero unmatched ink outside the one-pixel tolerance. Notes/drop caps
and a body PAGE-format difference keep the full gate red. Other untested feature combinations
remain outside this bounded result.

The expanded corpus adds [75 pinned external documents](../../tests/corpus/EXTERNAL.md), including
long guides and complex tables. That corpus is kept in the gate even when it exposes failures.
The [current result and page inspection](../../docs/display/21-external-corpus-parity.md) records
82 passes, 60 failures and one bounded rejection across 143 cases, including 20 accepted external
documents. Full parity remains unproven.

## Run

Requirements: a Rust toolchain supporting Wordcraft's edition 2024, Python with NumPy and Pillow,
and `rsvg-convert` on PATH. The production EdgeDoc workspace keeps its separate toolchain floor.
No system font files belong in this repository.

From the EdgeDoc root, export the synthetic regression inputs and create a new run directory:

```sh
EDGEDOC_PARITY_FIXTURES="$PWD/target/wordcraft-parity/fixtures" \
  CARGO_TARGET_DIR=target cargo test -p edgedoc-display --test rendering_parity
python3 tools/wordcraft_parity.py prepare target/wordcraft-parity/run \
  --fixtures target/wordcraft-parity/fixtures
```

Supply identical real font files to both renderers, with explicit styles. Include every face the
inputs use and the bundled Carlito fallback (regular, bold, italic and bold-italic). This example
shows only regular and bold; add the other faces as needed:

```sh
python3 tools/wordcraft_parity.py run target/wordcraft-parity/run \
  --font regular="$PWD/assets/fonts/subset/Carlito-Regular.ttf" \
  --font bold="$PWD/assets/fonts/subset/Carlito-Bold.ttf" \
  --font regular="/absolute/path/Arial.ttf" \
  --font bold="/absolute/path/Arial Bold.ttf"
```

The checked run supplied all 12 bundled subset files and the regular, bold, italic and bold italic
Arial and Times New Roman files already installed on the test machine. Both engines must use the
same bytes, including when both substitute a compatible family. A substitution warning is retained
in the report; actual font bytes and glyph IDs must still match.

`run` builds the reference with its lockfile and records binary provenance. `render DIRECTORY CASE`
and `reference DIRECTORY CASE` refresh one side. `compare DIRECTORY` reruns the gate. A missing,
changed or unrenderable case is blocked and makes the process exit nonzero. Known unsupported
features remain failed cases. No baseline is generated from EdgeDoc itself.

For the expanded corpus, run `python3 tools/fetch_external_corpus.py`, then add
`--corpus-manifest tests/corpus/external-manifest.json` to `prepare`. A JSON list of font
arguments can be passed with `--font-config /absolute/path/to/font-profile.json`.
To carry an earlier run's complete input set into a fresh directory, use
`prepare NEW_DIRECTORY --retain-manifest OLD_DIRECTORY/manifest.json --fixtures FIXTURE_DIRECTORY`.
This validates retained source hashes and appends distinct new regression inputs. It never
filters cases according to their earlier results. Both engines run independently when one fails.
For a change confined to EdgeDoc, `run --reuse-reference` can retain the independent Wordcraft
outputs. Reuse requires identical input bytes, Wordcraft/adapter fingerprints, the complete supplied
font profile (paths, bytes and styles), and every reference SVG page. Otherwise Wordcraft renders
again. The report lists reused case identities. Geometry and pixels are always compared again.
`--jobs 3` renders up to three independent cases concurrently; its default is one, and the maximum
is four. Each document still records both renderer outcomes separately.

## What a pass means

Each case requires the same page dimensions/count, source text including whitespace, line grouping,
font bytes, glyph IDs, styles and font sizes (0.05 pt tolerance). Glyph origins may differ by at most
0.5 pt. SVGs are rasterized at 144 dpi with no registration, scaling correction, cropping or shifting.
No more than 1% of ink may be unmatched within a one-pixel neighborhood, and normalized color error
on common ink must be at most 0.05. Both geometry and pixels must pass; empty output is not evidence.
Matching blank pages within a document with text are allowed, but an entirely empty text
rendering cannot pass. The report states these tolerances. This is bounded fixture evidence,
not exhaustive parity.

Outputs stay under the selected directory: original inputs, each engine's glyph JSON and SVG/PNG
pages, red/blue diff PNGs, and `comparison.json`. The negative gate tests are separate from reference
evidence:

```sh
python3 -m unittest discover -s tools -p test_wordcraft_parity.py
```

The optional browser gate verifies that Canvas paints the same outlines as the public SVG API for
the exported `ligatures_and_kerning_survive_layout_and_paint-*.docx` fixture:

```sh
DISPLAY_PARITY_FIXTURE=/absolute/path/to/ligatures_fixture.docx \
DISPLAY_E2E_OUT=target/wordcraft-parity/browser node tools/display_e2e.mjs
```

The browser module must first be rebuilt with `make display-wasm`. `PUPPETEER_PATH` and `CHROME_PATH`
select local browser dependencies. This Canvas check validates the viewer's connection to layout;
the independent Wordcraft comparisons establish the external reference evidence.

For manual inspection, prepare selected page pairs from current evidence:

```sh
python3 tools/prepare_wordcraft_inspection.py target/wordcraft-parity/expanded-final \
  target/corpus-inspection/visual-final poi-06-fancyfoot:1 \
  docx4j-01-docx4j_gettingstarted:22
```

The command checks source/build identity and writes the two images with SVG/PNG hashes.
It does not supply a manual visual judgment. The checked inspection record covers 11 page pairs
from both engines, including visible failures. A compact before/after report can be generated
with `tools/summarize_wordcraft_parity.py BEFORE_DIRECTORY AFTER_DIRECTORY OUTPUT.json`.
