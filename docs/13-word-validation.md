# Validating a Word document

Date: 2026-11-04.

A package can hold well-formed XML and still fail in Word. The XML parser accepts what
Word rejects. This chapter records the rules that a writer must meet, the defects that
broke them, and how to check a file.

## Why a ZIP check is not enough

`unzip -t` reports that the archive is intact. It says nothing about whether Word opens
the file. Three classes of defect pass every archive check and still fail:

| Class | What Word does |
|---|---|
| A child element in the wrong order | Reports the file as corrupt, or opens it with the formatting lost |
| A missing or wrong relationship or content type | Reports the file as corrupt |
| A reference to a part that does not exist | Reports the file as corrupt |

A fourth class is silent. The file opens, and the content is wrong:

| Class | What Word shows |
|---|---|
| A paragraph style with no definition | Plain body text |
| A list with no numbering part | Literal text instead of bullets |
| A repeated part name | The last copy wins, so a picture changes |
| An undeclared namespace prefix | A corrupt-file error |

## The rules this library enforces

### Child order

`w:pPr`, `w:sectPr`, `w:tblPr`, `w:trPr`, `w:tcPr`, `w:rPr`, `w:style`, `w:styles`,
`w:tbl`, and `w:tr` each have a fixed sequence in ECMA-376. A writer that emits the
elements in another order produces a file that Word can refuse.

The order of `w:sectPr` is the one that catches writers most often:

```
headerReference*, footerReference*, footnotePr?, endnotePr?, type, pgSz, pgMar,
pgBorders?, lnNumType?, pgNumType?, cols?, formProt?, vAlign?, noEndnote?,
titlePg?, textDirection?, bidi?, rtlGutter?, docGrid?
```

`w:type` comes before `w:pgSz`, not after it.

### Style definitions

A paragraph that names a style must have that style in `styles.xml`. Word reports no error for a missing style: the paragraph shows as body text. The document looks wrong with
no warning, so a test must check it.

### Numbering

A paragraph with `w:numPr` refers to `w:numId`. That id must exist in the numbering
part, and the numbering part must have a relationship from `document.xml.rels`. Word
shows the text with no bullet when the part is missing.

The writer keeps an existing numbering part, even when it builds no new definition.
A paragraph can refer to an id that the part already holds.

### One part, one name

A package holds each part name once. A merge that copies a header into several
sections must write that header part once and point every section at it. The same rule
applies to a picture: the writer joins identical bytes into one part.

### Namespaces

A header, a footer, a note, or a comment can hold a drawing. Each of those part types
needs the `w`, `r`, `wp`, `a`, and `pic` namespaces on its root element. Without them,
the file is not well-formed XML once a drawing is present.

### Field results

A merge replaces each field with its result. The run that held the field must take the
length of the result. A writer that slices runs by length then truncates or repeats the
text.

The merged text must hold no `MERGEFIELD` instruction and no `«` placeholder.

## Defects found with the WordCraft reader

These defects were present before this chapter existed. Each one is now locked by a
test in `crates/edgedoc-demos/tests/word_rules.rs`.

| Defect | Effect | Rule |
|---|---|---|
| `w:sectPr` wrote `pgSz` before `w:type` | Word may refuse the file | Child order |
| `w:tblPr` wrote `tblW` before `tblStyle` | Word may refuse the file | Child order |
| `w:trPr` wrote `tblHeader` before `trHeight` | Word may refuse the file | Child order |
| The merged output lost the numbering part | Bullets show as plain text | Numbering |
| A header picture wrote its media part twice | The package holds one name twice | One part, one name |
| A header, footer, note, or comment lacked `wp`, `a`, and `pic` | A corrupt-file error | Namespaces |
| A section break in a paragraph lost its headers | A region page shows no header | One part, one name |
| The demos named styles they never defined | Every heading shows as body text | Style definitions |
| A field result longer than its placeholder truncated | The saved text is wrong | Field results |
| Text after a field joined the field run | `Page 3 of 9` became `Page 3` | Field results |

## How to check a file

### With the WordCraft reader

WordCraft reads DOCX with its own parser and lays out pages. Build its command line
tool once:

```bash
cd ../wordcraft
cargo +1.95.0 build -p wordcraft-cli
```

Then run the check from this repository:

```bash
make verify
```

The script `tools/verify_with_wordcraft.py` reads each document. It fails when the
reader rejects the file, finds no page, or gives different text after a save by that
reader. A round trip through a second writer is the strongest evidence that the
format is correct.

Set `WORDCRAFT_CLI` when the binary lives elsewhere. The script exits with status 2
when it cannot find the reader, so "not checked" is not reported as "passed".

### With the tests

```bash
cargo test -p edgedoc-demos --test word_rules
```

The test reads the bytes and applies each rule above. It needs no Word installation and
no network, so it runs in continuous integration.

### By eye

WordCraft renders a page to a PNG, which shows the layout a reader computes:

```bash
wordcraft-cli render target/examples/report.docx page.png --page 1
```

A cover page should show the title in the size and colour that the style sets. Flat
black body text means the style is missing.

## What the check does not cover

Word itself is the final authority. The checks here catch the defects that a second
reader can detect. They do not prove that Word opens a file without a repair dialog. Open a new document in Word once for each layout that the library changes, and
record the result in the release notes.
