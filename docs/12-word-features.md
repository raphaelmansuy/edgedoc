# Word feature implementation

Date: 2026-11-04.

This chapter lists the Word features that EdgeDoc models, and the test that proves each one. It extends [11-format-coverage.md](11-format-coverage.md). That chapter says what EdgeDoc supports. This chapter says what the model carries, field by field.

The test is `crates/edgedoc/tests/word_features.rs`. A row without a case fails the test. A case without a row fails the test.

## Character formatting

| Area | Feature | EdgeDoc | Test |
|---|---|---|---|
| Character | Caps, small caps, emboss, imprint, outline, shadow | Modeled | `char-extended` |
| Character | Letter spacing, kerning, vertical position, width scale | Modeled | `char-extended` |
| Character | Complex-script bold and italic, snap to grid, no proof | Modeled | `char-extended` |
| Character | Underline style and colour, run border, run style | Modeled | `char-extended` |
| Character | Unknown run property elements | Kept | `char-unknown-kept` |

## Paragraph formatting

| Area | Feature | EdgeDoc | Test |
|---|---|---|---|
| Paragraph | Paragraph borders | Modeled | `paragraph-extended` |
| Paragraph | Tab stops with alignment and leader | Modeled | `paragraph-extended` |
| Paragraph | Character-unit indents and hanging indent | Modeled | `paragraph-extended` |
| Paragraph | Contextual spacing, suppressed line numbers | Modeled | `paragraph-extended` |
| Paragraph | Frame and drop cap | Modeled | `paragraph-extended` |
| Paragraph | Generated bullet and number list | Modeled | `list-generated` |
| Paragraph | A generated list keeps the existing numbering part | Modeled | `list-generated-keeps-existing` |
| Paragraph | An existing numbering definition | Kept | `list-kept-numbering` |

## Sections

| Area | Feature | EdgeDoc | Test |
|---|---|---|---|
| Section | Header and footer distance, gutter | Modeled | `section-extended` |
| Section | Multiple columns with a separator | Modeled | `section-extended` |
| Section | Title page, right-to-left, right-to-left gutter | Modeled | `section-extended` |
| Section | Text direction | Modeled | `section-extended` |
| Section | Line numbering | Modeled | `section-extended` |
| Section | Page borders | Modeled | `section-extended` |
| Section | Document grid | Modeled | `section-extended` |
| Section | Footnote and endnote numbering | Modeled | `section-extended` |
| Section | Builder: landscape, columns, header distance, gutter | Modeled | `section-builder` |
| Section | Section break in the middle of the document | Modeled | `section-mid-document` |

## Tables

| Area | Feature | EdgeDoc | Test |
|---|---|---|---|
| Table | Table style, alignment, indent, layout, cell spacing | Modeled | `table-extended` |
| Table | Floating table position and overlap | Modeled | `table-extended` |
| Table | Row header, exact height, cannot split | Modeled | `table-extended` |
| Table | Cell span, width, vertical and horizontal merge | Modeled | `table-extended` |
| Table | Cell shading, borders, margins, text direction, wrap | Modeled | `table-extended` |
| Table | Builder: style, header row, alignment, floating position | Modeled | `table-builder` |
| Table | Builder: shaded cell with borders and vertical alignment | Modeled | `cell-builder` |

## Drawings

| Area | Feature | EdgeDoc | Test |
|---|---|---|---|
| Drawing | Inline picture with size and alternative text | Modeled | `picture-inline` |
| Drawing | Floating picture: behind text, alignment, wrap | Modeled | `picture-floating` |
| Drawing | Floating picture offset | Modeled | `picture-offset` |
| Drawing | Picture size and alternative text read from a file | Modeled | `picture-alt-round-trip` |

## Stories

| Area | Feature | EdgeDoc | Test |
|---|---|---|---|
| Story | Footnote text as a story, with the note id and type | Modeled | `footnote-story` |
| Story | Add a paragraph to a footnote | Modeled | `footnote-edit` |
| Story | Picture in a footnote, with the note relationship part | Modeled | `footnote-picture` |
| Story | Comment text as a story, with the author and the date | Modeled | `comment-story` |
| Story | Add a paragraph to a header | Modeled | `header-edit` |
| Story | Picture in a header, with its own relationship part | Modeled | `header-picture` |
| Drawing | VML picture, kept with its relationship and bytes | Kept | `vml-picture-kept` |

## Controls and fields

| Area | Feature | EdgeDoc | Test |
|---|---|---|---|
| Control | Drop-down items and check-box state | Modeled | `control-items` |
| Control | Builder for text, date, drop-down, and check box | Modeled | `control-builder` |
| Field | Simple field with a cached result | Modeled | `simple-field-kept` |
| Field | PAGE field cache | Kept | `page-field-kept` |

## Properties and styles

| Area | Feature | EdgeDoc | Test |
|---|---|---|---|
| Property | Title, subject, keywords, description, author | Modeled | `core-props-round-trip` |
| Property | Last modified by, modified time, category, status, language | Modeled | `core-props-round-trip` |
| Style | Table style and conditional formatting | Kept | `style-table-preserved` |
| Style | Document defaults and latent styles | Kept | `style-table-preserved` |
| Style | Based-on, next, and quick-format flags | Modeled | `style-table-preserved` |
| Style | Builder for paragraph, character, and table styles | Modeled | `style-builder` |
| Run | Builder: small caps, superscript, spacing, caps | Modeled | `run-builder-extended` |
| Paragraph | Builder: alignment, hanging indent, tab stop, border | Modeled | `paragraph-builder-extended` |
| Paragraph | A paragraph built from a full property set | Modeled | `builder-with-props` |
| Character | Default property set is empty | Modeled | `char-props-default-is-empty` |
| Package | Content types cover pictures, numbering, notes, comments | Modeled | `content-types-cover-media` |
| Model | Block access by index | Modeled | `block-helpers` |

## Not modeled

These features stay as kept markup or are absent. A success test would claim support that the code does not have. The package rows in [11-format-coverage.md](11-format-coverage.md) hold the refused cases.

| Area | Feature | EdgeDoc | Test |
|---|---|---|---|
| Layout | Pagination, line breaking, and PDF | Absent | `none` |
| Field | PAGE, NUMPAGES, TOC, and REF evaluation | Absent | `none` |
| Math | OMML editing | Kept | `none` |
| Drawing | Chart, SmartArt, VML, and WordArt editing | Kept | `none` |
| Revision | Tracked-change model | Kept | `none` |
| Protect | Document protection | Kept | `none` |
| Data | Custom XML binding evaluation | Kept | `none` |
| File | Strict writer, encryption writer, Flat OPC writer | Absent | `none` |
