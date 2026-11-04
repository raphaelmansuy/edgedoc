# Phases, exit tests, estimates, and risks.

> Part of the EdgeDoc Display specification. Date: 2026-11-04. Requirement ids are
> normative. See [the index](README.md).

## 136. The rule of the plan

`DP-P-27`. Do the phases in order. Do not start a phase before the previous phase meets its
exit test. The rule exists because a layout engine fails late: a line breaker that is 0.3 pt
wrong looks correct on page 1 and moves page 40.

`DP-P-28`. Every phase has one exit test that a machine runs. The phase is done when the
test passes on the corpus, not on one document.

`DP-P-29`. The estimates are planning estimates for one engineer who knows Rust and knows
the WordCraft checkout. They are not measurements. Each phase names the WordCraft module that
reduces its risk, so a reader can check the size of the work.

## 137. Phase 0: workspace and layers

**Estimate: 1 week.**

1. Create the crates of [03-architecture.md](03-architecture.md) with empty public types.
2. Copy `xtask/src/layers.rs` from WordCraft and enter the display table of `DP-A-28`.
3. Set `rust-version` and the lints of `ED-S-06`.
4. Add the CI skeleton: fmt, clippy, layers, `cargo test`, and the WASM build.
5. Add `edgedoc-display-testkit` with the empty corpus builder.

**Exit test.** `cargo xtask layers` fails when `edgedoc-paint` is given a dependency on
`edgedoc-flow`, and passes otherwise.

**WordCraft reference.** `xtask/src/layers.rs`, `xtask/src/main.rs`.

## 138. Phase 1: geometry, fonts, and text

**Estimate: 5 weeks.** This is the longest phase, and the one that decides the project.

1. Port `edgedoc-geom` from `wordcraft-geom`, with the guards of `DP-A-30`.
2. Port `edgedoc-fonts` from `wordcraft-fonts`: `harfrust` shaping, `skrifa` metrics, the
   substitute table, `.odttf` decode, and the symbol tables.
3. Port `edgedoc-text` from `crates/layout/src/para`: the cluster model, the breaker, tabs,
   justification, bidi, and kinsoku.
4. Write `FixedShaper` and `FixedMeasurer` for the tests, so a line test needs no font.
5. Add the golden tests for 30 text cases.

**Exit test.** The text suite reproduces the WordCraft line boxes for the 30 synthetic
paragraph documents within 0.5 pt, and `layout_deterministic` passes twice.

**WordCraft reference.** `crates/fonts`, `crates/layout/src/para`, `crates/layout/src/kinsoku.rs`,
`crates/layout/src/para/bidi.rs`.

**Risk.** Shaping differences from a font substitute. Mitigation: `DP-F-04` keeps the metrics
of the reference face, and the test compares the line box, not the glyph.

## 139. Phase 2: reference harness and corpus

**Estimate: 2 weeks.** Do this before pagination, not after.

1. Build the corpus of [14-testing-and-quality.md](14-testing-and-quality.md): 20 synthetic
   documents, and the mutation function.
2. Port `xtask/src/visual_diff.rs` as `cargo xtask display-diff`.
3. Wire the WordCraft dev harness. The harness reads the same documents and prints the same
   measures.
4. Generate the fidelity report.

**Exit test.** `cargo xtask display-diff` prints a comparable report for the 20 documents, and
the report lists the WordCraft known gaps of clause 120 as warnings.

**WordCraft reference.** `xtask/src/visual_diff.rs`, `xtask/src/parity.rs`,
`crates/layout/src/work.rs`.

**Why so early.** A layout engine without a gate drifts in every phase after this one. The
WordCraft checkout lists its own page-by-page harness as missing and estimates 8–12 hours.
That cost at the start is lower than the cost of finding a 40-page drift at the end.

## 140. Phase 3: styles and numbering

**Estimate: 3 weeks.**

1. Port `edgedoc-style` from `wordcraft-doc::resolve` and `crates/doc/src/numbering.rs`.
2. Implement the cascade of `DP-S-02` and the conditional table formats of `DP-S-05`.
3. Implement the numbering formats, including the measured rules of `DP-S-30`.
4. Implement the theme resolution and the compatibility flags.

**Exit test.** The 12 numbering documents and the 8 style documents match the WordCraft
computed records, and `restart_each_page` and `separate_counters` pass.

**WordCraft reference.** `crates/doc/src/resolve.rs`, `crates/doc/src/numbering.rs`,
`crates/doc/src/cjk_numbers.rs`, `crates/doc/src/section.rs`.

## 141. Phase 4: flow and pagination

**Estimate: 6 weeks.** The second long phase.

1. Port the page builder and the flow loop from `crates/layout/src/page.rs`,
   `crates/layout/src/flow.rs`, and `crates/layout/src/boxes.rs`.
2. Port the multi-pass loop of `DP-L-52` with its three caps.
3. Port `section_side` and the header selection of `DP-L-17`.
4. Port the table fit and the row split rules.
5. Port the note area and the float placement.

**Exit test.** The 20 synthetic documents and 25 real documents pass G1 and G3 of the gate
table in `DP-I-15`. That is a page break within one line, and every character in order.

**WordCraft reference.** `crates/layout/src/page.rs`, `flow.rs`, `boxes.rs`, `table.rs`,
`table_flow.rs`, `notes.rs`, `floats.rs`, `headers.rs`.

**Risk.** The table and float interaction is where a port takes the longest. Mitigation: port
the WordCraft tests one by one, and add the document that each one needs to the corpus.

## 142. Phase 5: display list and painters

**Estimate: 3 weeks.**

1. Define the operation set of `DP-G-02`, and the encoder of `DP-G-10`.
2. Port the `Draw` lowering from `crates/layout/src/display`.
3. Write `RecordPainter`, `SvgPainter`, and `PdfPainter`.
4. Write `CanvasPainter` over a `Canvas2d` trait, with a `tiny-skia` native backend.
5. Add the golden display lists and the 60 golden PNGs.

**Exit test.** `painter_parity` passes on 20 pages, and the display list round-trip holds a
coordinate within 0.005 pt.

**WordCraft reference.** `crates/layout/src/display`, `crates/render`,
`crates/pdf/src/exporter.rs`.

## 143. Phase 6: viewer and demo

**Estimate: 4 weeks.**

1. Build `edgedoc-display-wasm` with the exports of [10-wasm-browser.md](10-wasm-browser.md)
   and the message protocol of `DP-B-07`.
2. Build the viewer of [09-viewer-api.md](09-viewer-api.md): scroll, zoom, tiles, text layer,
   search, selection, print.
3. Build the demo of [11-demo-site.md](11-demo-site.md), including the wireframes and the
   keyboard map.
4. Add the five sample documents.

**Exit test.** One call loads a `.docx` from disk and paints it. Zoom works from 10% to 800%,
page navigation works, and the first page appears within 300 ms on the reference laptop.

**WordCraft reference.** `apps/wordcraft-web`, `packaging/web/README.md`.

## 144. Phase 7: budgets, accessibility, and hardening

**Estimate: 3 weeks.**

1. Meet the budgets of [12-performance.md](12-performance.md), or record the real value and
   move the budget (`DP-B-43`).
2. Run `axe-core` and fix the critical issues.
3. Run the fuzz targets for one week, and fix every find.
4. Complete the edge-case suite of [13-edge-cases.md](13-edge-cases.md).

**Exit test.** Two conditions hold for the size table and the performance table. Each number
sits inside its budget, or it carries a measured value. The fuzz run also finds nothing in
24 hours.

## 145. Phase 8: version 1.0

**Estimate: 2 weeks.**

1. Publish the npm package and the crate with one version.
2. Generate the documentation and the fidelity report.
3. Run the four manual checks of `DP-Q-38`.
4. Write the release note with the tier claim and the known gaps.

**Exit test.** A person who has never seen the project loads a `.docx` on the demo site,
reads it, zooms, and prints, with no instruction.

## 146. The table

| Phase | Work | Estimate | Cumulative |
|---|---|---|---|
| 0 | Workspace, layers, CI | 1 week | 1 |
| 1 | Geometry, fonts, text | 5 weeks | 6 |
| 2 | Reference harness and corpus | 2 weeks | 8 |
| 3 | Styles and numbering | 3 weeks | 11 |
| 4 | Flow and pagination | 6 weeks | 17 |
| 5 | Display list and painters | 3 weeks | 20 |
| 6 | Viewer and demo | 4 weeks | 24 |
| 7 | Budgets, accessibility, hardening | 3 weeks | 27 |
| 8 | Version 1.0 | 2 weeks | 29 |

`DP-P-30`. The sum is 29 weeks for one engineer. A second engineer can shorten phase 6 and
phase 7. That engineer cannot shorten phase 1 or phase 4, because each of those phases is one
sequence of measurements.

`DP-P-31`. The WordCraft checkout estimates 90–145 hours to a full layout parity. That
estimate covers the remaining work on top of an engine that already exists. This plan builds
the engine for a narrower target: a read-only viewer with no editor, no ribbon, and no
command registry. The two numbers do not compare, and this plan states its own.

## 147. Risks

`DP-P-32`. Each risk has a mitigation, and a mitigation that is not a test is not a
mitigation.

| Risk | Likelihood | Effect | Mitigation |
|---|---|---|---|
| Font substitution moves a page break | High | The core promise fails on a document with a missing font | `DP-F-04` keeps the metrics, and the corpus holds the missing-font documents |
| Table and float placement takes longer than phase 4 | High | The plan slips 3 weeks | Port the WordCraft tests in the first week of the phase, so the gap is visible early |
| The WASM module passes the 1.6 MiB budget | Medium | First load is slow | `DP-B-43` measures in phase 6, and a smaller built-in font is the first cut |
| The canvas and the native raster disagree | Medium | A golden PNG is not a gate | `DP-Q-27` compares the two within 2% of pixels |
| A hostile package hangs the worker | Low | A tab freezes | The caps of `DP-L-53` and the fuzz target of `DP-X-170` |
| WordCraft has a behavior we copy that is wrong | Medium | A wrong page that a test approves | Clause 120 lists the known gaps, and the manual check of `DP-Q-38` covers the rest |
| The text layer is too heavy on a dense page | Medium | A scroll stutters | `DP-G-40` drops the layer past 40,000 characters, and `DP-E-07` mounts 8 pages |
| A browser lacks a feature the viewer needs | Low | The viewer shows a download link | `DP-B-25` detects each feature, and `DP-B-27` states the fallback |

`DP-P-33`. A risk that becomes true stops the phase. The plan does not carry a risk forward
into the next phase, because a layout defect compounds.

## 148. Definition of done for version 1

`DP-P-34`. Version 1 is done when all of these hold:

1. Every requirement in [16-traceability.md](16-traceability.md) names a test, and the test
   passes.
2. Tier T0, T1, and the stated part of T2 are covered by corpus documents.
3. The fidelity report is generated and committed.
4. The five gates of `DP-I-15` hold on the `synthetic` and `real` sets.
5. The size table and the performance table are inside budget, or carry a measured value.
6. The four manual checks of `DP-Q-38` pass, with the Word version recorded.
7. The demo loads a file, zooms, navigates, searches, selects, and prints, with no
   instruction.
8. `cargo xtask layers` passes, and `unsafe_code` is forbidden in every crate.
