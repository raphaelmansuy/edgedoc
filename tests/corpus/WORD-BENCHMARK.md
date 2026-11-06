# Word-exported benchmark corpus

This local test set selects 48 clean DOCX/PDF pairs from [neurotic_docx_bench-fixtures](https://huggingface.co/datasets/arthrod/neurotic_docx_bench-fixtures), pinned at revision `984e64d869803abea841e66364f7b82bb30b3a70`. The dataset describes its PDFs as exports from Microsoft Word for Mac. The selection spans 25 languages, 10 document types and reference documents of one to six pages. The manifest records source URLs, metadata and both SHA-256 checksums; the checksums were checked against the dataset's pinned checksum manifest.

```sh
python3 tools/fetch_word_benchmark.py
CARGO_TARGET_DIR=target CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 \
  python3 tools/compare_word_benchmark.py --font-config target/corpus-inspection/fonts.json
```

The second command requires the local Wordcraft checkout, the configured font profile, `pdfinfo`, `rsvg-convert`, Pillow and NumPy. It checks each input hash, compares EdgeDoc with Wordcraft, and records PDF page counts. The font-profile file contains local font paths and must be created for the machine running the test. Results are in `target/wordcraft-parity/word-benchmark-48/comparison.json`.

Downloaded documents and reference PDFs remain in ignored `target/word-benchmark-corpus/`. The dataset says its database license covers the collection, while individual documents originate from public websites and may carry their own rights; do not assume the originals are licensed for redistribution. Page-count agreement alone is not a visual comparison with Word. Wordcraft's reference adapter also does not emit every drawing type, which must be considered when interpreting comparisons.

The reference adapter records every placed paragraph style's font bytes and collection index, including paragraph marks that produce no glyph. The comparator fails a case if a reference face is not supplied to EdgeDoc. Wordcraft may discover system fonts that are not in the explicit profile; add matching font files to the local profile or treat the case as a font-evidence failure. To supply a nonzero collection face, use a profile entry such as `bold:1=/path/to/Helvetica.ttc`. The comparator verifies that the face is available; it does not prove that every nonpainting mark chose that face. Summaries from before this check did not verify fonts used only for nonpainting paragraph marks.
