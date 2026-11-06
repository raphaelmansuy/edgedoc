# Word-exported corpus and page anchors

The pinned 48-document benchmark in `tests/corpus/WORD-BENCHMARK.md` was rendered before the page-anchor change. The strict EdgeDoc/Wordcraft gate passed 4 and failed 44; no cases were blocked. All 48 supplied PDF page counts agree with the dataset metadata. A pass measures EdgeDoc against Wordcraft with the recorded font profile, not full Word parity. Results and exact build identity are in `word-benchmark-48-summary.json`.

Inspected at 144 dpi:

* `132172576b`: one-page Word PDF, Wordcraft and EdgeDoc render. The Wordcraft/EdgeDoc pair passed the strict gate. The visual comparison has the same content and layout; it does not establish general parity.
* `036dd6afba`: Word PDF and Wordcraft have one page, EdgeDoc had two. A page-relative behind-text logo was incorrectly inserted into text flow. The reference adapter omits some drawing shapes, so the Wordcraft image is incomplete compared with the Word PDF. The document also contains tight-wrapped pictures, conditional table styles, unsupported drawings and missing fonts.

Wordcraft computes a page-relative floating object's rectangle from page extent and axis alignment in `crates/layout/src/floats.rs`. EdgeDoc now parses each axis's `relativeFrom`, preserves it when saving edits, and lays out the narrow body-picture subset with behind-text, `wrapNone`, and page-relative axes without consuming line space. Its page-aligned picture is painted before body text. The inspected re-render of `036dd6afba` moves its lower-right logo into the correct page corner, but remains two pages because its second logo uses `wrapTight`. EdgeDoc continues to diagnose all floating pictures as unsupported until the complete positioning and wrapping family is covered.

A synthetic fixture derived from the existing docx4j inline-picture fixture was rendered in both engines and visually inspected: the non-wrapping image is at the lower-right page corner in both. The integration regression checks page coordinates, text flow and edited-save preservation. The general Wordcraft comparison still rejects this fixture because floating pictures and the substituted font are diagnosed; visual agreement for this subset must not be treated as a full gate pass.

After the anchor change, the same 48-case profile again had four passes and 44 failures. One Wordcraft render timed out during concurrent execution; its failed result was verified by comparing the new EdgeDoc render against the earlier reference output after checking the input, reference identity and font profile. That document has seven EdgeDoc pages versus four Wordcraft pages. The initial run and rechecked rerun are identified separately in the two summary JSON files.
