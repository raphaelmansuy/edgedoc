# Header paragraph border flow

Wordcraft includes a paragraph's visible bottom border and its distance from text when positioning the next paragraph in a header or footer. EdgeDoc previously painted the border but omitted that space from story flow. It now adds the border room before the following paragraph, while preserving the existing handling of joined borders and trailing story space.

The controlled `target/wordcraft-parity/header-border-flow` fixture now passes the strict Wordcraft comparison, with matching glyph positions and raster output. A regression test checks that a one-point border moves the next header paragraph by one point.

In downloaded `wordbench-056bfeba7c`, the half-point discrepancy after a header border had shifted most content on pages three and four. Both pages now pass the glyph, raster and color checks. Pages one and two still differ, and the document remains a strict failure because floating-picture, floating-table and paragraph-frame support is incomplete.
