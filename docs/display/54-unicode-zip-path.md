# Unicode ZIP entry paths

Wordcraft honors the Info-ZIP Unicode Path extra field when reading DOCX packages. The Apache POI `unicode-path.docx` fixture assigns the effective `word/document.xml` name to an entry whose legacy ZIP name differs. Previously EdgeDoc selected the other entry and displayed `1`; with a valid version 1 extra field and matching CRC-32, it selects the same document part as Wordcraft and displays `2`. Malformed, truncated, invalid UTF-8, and checksum-mismatched fields do not override the legacy name.

The focused comparison is recorded in `unicode-path-focused-result.json`; paired page images and their checksums are recorded in `unicode-path-image-inspection.json` and were inspected visually. The original 190-case run has 189 passes and one disk-space-interrupted case in `unicode-path-190-result.json`; that case passed when rerun in `unicode-path-190-retry.json`. These results cover only the tested inputs, and the broader 394-case comparison remains necessary.

The workspace test suite passed after the ZIP change. Strict workspace clippy and formatting checks also passed. The rebuilt browser viewer passed 18/18 interaction checks and 134/134 checks across 33 corpus documents.
