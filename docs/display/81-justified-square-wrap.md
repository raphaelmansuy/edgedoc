# Justified text around a square-wrapped picture

Wordcraft lays out each clear segment of a justified line separately. EdgeDoc now distributes spaces within each segment, without stretching text across the picture. The final segment of a paragraph remains unjustified, as in Wordcraft.

The line breaker also selects floating-picture exclusion bands at the final line-spaced vertical positions. Previously it used natural font line heights before paragraph spacing was applied, so a picture could incorrectly obstruct a later line after the line had moved below the picture.

The controlled short and three-line fixtures under `target/wordcraft-parity/square-wrap-justify` and `target/wordcraft-parity/square-wrap-justify-long` were compared against the pinned Wordcraft reference with the same Carlito font. Both pages match glyph text, glyph origins, and raster ink; their whole-document comparisons remain failures because EdgeDoc still reports floating-picture layout as unsupported. The longer Wordcraft and EdgeDoc page images were inspected side by side.

This covers the supported object-only, margin-anchored square-wrap subset. It does not establish general square-wrap or Wordcraft parity. See `77-two-sided-square-wrap.md` and `78-early-square-reflow.md` for the scope and remaining limitations.
