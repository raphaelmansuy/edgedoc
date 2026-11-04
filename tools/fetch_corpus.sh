#!/bin/sh
# Download the corpus, so a reader can confirm where each file came from.
#
# The documents are MIT licensed fixtures from python-docx. They stay unmodified. See
# tests/corpus/README.md.
#
#     tools/fetch_corpus.sh
set -eu
here=$(cd "$(dirname "$0")/.." && pwd)
out="$here/tests/corpus/downloaded"
base="https://raw.githubusercontent.com/python-openxml/python-docx/master/features/steps/test_files"
mkdir -p "$out"

files="
blk-containing-table.docx
blk-paras-and-tables.docx
comments-rich-para.docx
doc-access-sections.docx
doc-add-section.docx
doc-coreprops.docx
doc-default.docx
doc-no-coreprops.docx
doc-odd-even-hdrs.docx
doc-word-default-blank.docx
fnt-color.docx
hdr-header-footer.docx
num-having-numbering-part.docx
par-alignment.docx
par-hyperlinks.docx
par-known-paragraphs.docx
par-known-styles.docx
par-rendered-page-breaks.docx
run-char-style.docx
run-enumerated-props.docx
sct-first-page-hdrftr.docx
sct-section-props.docx
set-no-settings-part.docx
shp-inline-shape-access.docx
sty-behav-props.docx
sty-having-no-styles-part.docx
sty-having-styles-part.docx
sty-known-styles.docx
tab-stops.docx
tbl-2x2-table.docx
tbl-cell-props.docx
tbl-having-applied-style.docx
tbl-having-tables.docx
tbl-props.docx
txt-font-highlight-color.docx
txt-font-props.docx
"

for f in $files; do
  if [ -f "$out/$f" ]; then
    printf '  %-34s kept\n' "$f"
    continue
  fi
  curl -sL -o "$out/$f" "$base/$f"
  printf '  %-34s %s\n' "$f" "$(wc -c < "$out/$f" | tr -d ' ') bytes"
done

printf '\n%d documents in %s\n' "$(ls "$out"/*.docx | wc -l | tr -d ' ')" "$out"
