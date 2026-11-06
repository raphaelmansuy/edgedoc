# Fonts in the EdgeDoc display library

The display library paints a page with the faces that a document names. Those faces are
proprietary, so the library substitutes a metric-compatible open face and tells the reader.

| File | Face | Substitutes | Licence |
|---|---|---|---|
| `Carlito-Regular.ttf`, `Carlito-Bold.ttf`, `Carlito-Italic.ttf`, `Carlito-BoldItalic.ttf` | Carlito | Calibri, Aptos, Calibri Light | SIL Open Font License 1.1 |
| `Arimo-*.ttf` | Arimo | Arial, Helvetica, Segoe UI, Tahoma, Verdana, Trebuchet MS | SIL Open Font License 1.1 |
| `Tinos-*.ttf` | Tinos | Times New Roman, Times, Georgia, Garamond, Palatino | SIL Open Font License 1.1 |
| `SourceSans3-Regular.ttf` | Source Sans 3 | Nonbreaking-hyphen fallback | SIL Open Font License 1.1 (`OFL-SourceSans3.txt`) |

`Carlito-LICENSE.txt` is the licence for Carlito. Arimo and Tinos use the same SIL Open Font
License 1.1.

## Why these faces

A substituted face moves every line. Carlito, Arimo, and Tinos are metric-compatible with
Calibri, Arial, and Times New Roman, so a paragraph set in the substitute breaks at the same
word as the paragraph set in the original. The test `substitute_keeps_line_breaks` locks this.

Aptos, Word's default face since 2024, has no metric-compatible open substitute. The library
maps it to Carlito and reports `display.font_substituted` at `warning` level, so a reader knows
that the page break can differ. `DP-N-08` records this limit.

## Where the fonts came from

The faces came from the TeX Live 2024 distribution:

```
/usr/local/texlive/2024/texmf-dist/fonts/truetype/google/carlito/
/usr/local/texlive/2024/texmf-dist/fonts/truetype/google/arimo/
/usr/local/texlive/2024/texmf-dist/fonts/truetype/google/tinos/
```

Each family is a Google Font. The upstream project is
[google/fonts](https://github.com/google/fonts), under the SIL Open Font License 1.1.
Source Sans 3 and its licence were copied from the Wordcraft checkout for the same fallback
face used by its renderer.

## Two builds

`edgedoc-fonts` reads the faces that a test needs with `include_bytes!`, which keeps a test
offline. A released module carries one subset face, cut to Latin, Greek, Cyrillic, and the
common punctuation, so the module stays inside its size budget (`DP-B-32`). The subset tool is
`pyftsubset` from `fonttools`, and `tools/fonts.sh` records the exact command.

## Adding a face

1. Confirm the licence allows redistribution. A font with a restricted licence does not go in
   this folder.
2. Add the file and its licence text.
3. Add a row to the table above.
4. Add the family to `SUBSTITUTES` in `crates/edgedoc-fonts/src/lib.rs` when it substitutes a
   Word face.
5. Run `cargo test -p edgedoc-fonts`.
