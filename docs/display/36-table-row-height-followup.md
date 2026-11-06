# Table row-height follow-up

Wordcraft adds the row's largest cell margins and border insets to a minimum row height; an
exact row height includes the row's bottom cell margin. EdgeDoc now follows those rules and
uses the row's largest top and bottom margins when positioning cell contents. An explicit
`tblCellMar` set uses zero for omitted sides, matching Wordcraft's import behavior.

The [focused comparison](row-height-targeted-result.json) passes the full Wordcraft comparison.
A downloaded [POI table-alignment document](conditional-poi-targeted-result.json) now passes
its page-level glyph and raster checks, and the [page pair](row-height-poi-image-inspection.json)
was visually inspected. The document remains a failure under the conditional-style diagnostic.
The [whole-table region comparison](whole-table-targeted-result.json) also passes its page check
and remains diagnostic-blocked at document level. Other table features remain to be measured.

The [375-case comparison](row-height-375-parity-result.json) reports 154 passed, 203 failed,
and 18 blocked. All 373 previous cases retained their status; the new row-height case passed,
while the new whole-table fixture passed its page check and retained the table-style diagnostic.
