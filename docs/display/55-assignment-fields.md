# Assignment fields

Wordcraft renders `ASK` and `SET` fields as assignments and suppresses their cached result text. EdgeDoc previously displayed that text, changing both the content and the position of following text. It now omits the result for these field commands, case-insensitively, in simple and complex field forms. Other unsupported field commands continue to be reported.

`field-assignment-before-result.json` and `field-assignment-after-result.json` capture a paired differential case. The former fails and the latter passes; `field-assignment-image-inspection.json` records the inspected Wordcraft and EdgeDoc page images. The workspace tests, strict clippy and formatting checks passed, as did the rebuilt browser viewer's 18/18 interaction checks and 134/134 checks on 33 corpus documents. This focused result does not establish parity for all fields or documents.
