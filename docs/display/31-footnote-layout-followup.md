# Footnote layout follow-up

Footnote stories are now laid out from their reference lines. Their measured height and
separator are reserved during page filling; long notes split at line boundaries and continue
on subsequent pages. Document-defined separator, continuation separator and continuation
notice stories are retained. Existing note diagnostics remain enabled because other note
features, including some complex note contents, are not yet at parity. Endnote placement has since been improved in the [endnote follow-up](32-endnote-layout-followup.md).
Per-page footnote numbering is resolved through bounded pagination passes; failure to
converge emits an unsupported-layout diagnostic. A [two-page comparison](per-page-footnote-parity-result.json)
passes every page-level text, glyph and raster check, while the document remains failed
under the conservative note diagnostic. Both page pairs were also [visually inspected](per-page-footnote-image-inspection.json).

The [Apache POI footnote sample's page](footnote-poi-image-inspection.json) meets the strict
page-level text, glyph and raster thresholds. The docx4j guide's fourth page does likewise when both renderers use the same
Symbol font bytes; the full guide still fails and has different page counts. The
[guide comparison](footnote-guide-parity-result.json) records the document-level result and
its per-page measurements, and the [image inspection](footnote-guide-image-inspection.json)
records the rendered pages used for visual review.

The [long-note comparison](long-footnote-parity-result.json) covers a 16-page document with
continuation pages: every page passes the text, glyph and raster checks, but the overall case
remains failed because of the conservative note diagnostic. The stress input can be recreated
from the exported crowded-note fixture with [the generator](../../tools/generate_note_stress.py).
The run also exposed a shaping dependency overflow in debug builds; long runs are now split at
grapheme-safe boundaries and their source offsets retained.

A four-page fixture with a custom continuation notice has matching rendered notice glyphs and
positions, but the strict text comparison fails because the Wordcraft adapter receives empty
source text for that decorative story. Its [image inspection](note-notice-image-inspection.json)
is visual evidence only, not a strict document-level pass.

The [358-case comparison](footnote-layout-parity-result.json) reports 143 passed, 196 failed,
and 19 blocked. The previous 354 cases keep their statuses. Three added footnote cases pass
every page-level check and remain document-level failures under the note diagnostic; the
continuation-notice case also hits the reference adapter's text limitation.

The [359-case comparison](per-page-359-parity-result.json) reports 143 passed, 197 failed,
and 19 blocked. All 358 earlier cases retain their statuses; the added per-page fixture
passes both page checks but remains failed by the note diagnostic.

The earlier [endnote image comparison](endnote-gap-image-inspection.json) exposed an
omission: EdgeDoc then painted only the reference marks. The [subsequent follow-up](32-endnote-layout-followup.md)
adds endnote bodies at section and document boundaries, including page continuation.
