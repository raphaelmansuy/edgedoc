# Whitespace line metrics

Wordcraft sizes a line from its visible content: spaces and tabs do not enlarge it, and a line containing only those characters uses the paragraph mark's font metrics. EdgeDoc now passes the mark style through to the line breaker and follows that rule. The layout cache includes the mark style so edits to it cannot reuse stale metrics.

In the downloaded Apache POI `IllustrativeCases.docx` (`poi-11-illustrativecases`), a whitespace-only paragraph with a larger run font previously pushed content onto an extra blank fifth page. EdgeDoc and Wordcraft now both render four pages, with the full focused comparison passing (`whitespace-focused-result.json`). Their final pages were visually inspected and are retained in `whitespace-image-inspection/`. A regression test covers whitespace-only lines and large spaces/tabs alongside smaller text.

The current-source selected regression run passed 199/199 cases (`whitespace-199-result.json`). Workspace tests, strict clippy and formatting passed; browser checks passed 18/18 interactions and 134/134 checks across 33 corpus documents. The most recent complete 394-case baseline before this change was 195 passed, 181 failed, 18 blocked (`picture-final-394-result.json`); it does not establish complete parity.

A separate font-profile check supplies EdgeDoc with `/System/Library/Fonts/Symbol.ttf`, matching the exact font hash used by the preserved Wordcraft render. With that font, the downloaded POI `numberingWithoutOutOfOrderID.docx` also passes (`symbol-real-focused-result.json`), and the page pair was inspected (`symbol-real-image-inspection.json`). This result is conditional on that system font and is not folded into the common-font baseline.
