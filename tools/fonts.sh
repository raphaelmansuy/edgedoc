#!/bin/sh
# Cut the bundled faces to the characters that a business document uses, so the WebAssembly
# module stays inside its size budget. See docs/display/10-wasm-browser.md, DP-B-32.
#
# The range covers Latin-1 and Latin Extended-A, the punctuation that Word writes, the currency
# and legal signs, the arrows and box drawing that a list bullet uses, and the Greek and Cyrillic
# letters that a European invoice can hold. A document outside the range still paints: every
# missing glyph falls back to the reader's font and reports one diagnostic.
#
#     tools/fonts.sh
set -eu
here=$(cd "$(dirname "$0")/.." && pwd)
src="$here/assets/fonts"
out="$src/subset"
mkdir -p "$out"

UNICODES="U+0020-007E,U+00A0-024F,U+02B0-02FF,U+0300-036F,U+0370-03FF,U+0400-04FF,U+1E00-1EFF,U+2000-206F,U+20A0-20CF,U+2100-214F,U+2190-21FF,U+2200-22FF,U+25A0-25FF,U+2600-26FF,U+FB00-FB06"

for f in "$src"/*.ttf; do
  name=$(basename "$f")
  pyftsubset "$f" \
    --output-file="$out/$name" \
    --unicodes="$UNICODES" \
    --layout-features='kern,liga,clig,calt,locl,frac,onum,tnum' \
    --drop-tables+=DSIG \
    --name-IDs='*' \
    --no-hinting \
    >/dev/null 2>&1
  before=$(wc -c < "$f")
  after=$(wc -c < "$out/$name")
  printf '  %-24s %5d KB -> %4d KB\n' "$name" "$((before / 1024))" "$((after / 1024))"
done
