# Round-trip fidelity

Date: 2026-11-04.

A user opens a document that they did not write. The library must edit one paragraph
and save the file without losing anything it did not understand. That is the promise of
this library, and this chapter tests it feature by feature.

The test is `crates/edgedoc-demos/tests/round_trip.rs`. Each case builds a package with
one feature, edits an unrelated paragraph, saves, and then checks the stated part. A row
without a case fails the test. A case without a row fails the test.

## The rule

A feature is either modeled or kept.

- **Modeled**: the reader builds a typed value and the writer emits it. An edit can change it.
- **Kept**: the reader does not understand the feature. It keeps the markup verbatim, and
  the save copies it.

A feature that is neither loses data, and that is a defect. The cases below cover the
features that a template can hold, in the order that the reader meets them.

## Settings and styles

| Area | Feature | EdgeDoc | Test |
|---|---|---|---|
| Settings | Document protection and enforcement | Kept | `document-protection` |
| Settings | Hyphenation and consecutive-hyphen limit | Kept | `hyphenation` |
| Style | Paragraph and run properties in a style | Modeled | `style-properties` |
| Style | Document defaults for runs and paragraphs | Kept | `document-defaults` |
| Part | Theme part | Kept | `theme-part` |

## Runs and paragraphs

| Area | Feature | EdgeDoc | Test |
|---|---|---|---|
| Paragraph | Positional tab (`w:ptab`) | Kept | `positional-tab` |
| Run | Ruby text (`w:ruby`) | Kept | `ruby-text` |
| Run | East Asian layout (`w:eastAsianLayout`) | Modeled | `east-asian-layout` |
| Run | Fit text width (`w:fitText`) | Modeled | `fit-text` |
| Run | Page break (`w:br w:type="page"`) | Modeled | `br-page-break` |

## Fields and forms

| Area | Feature | EdgeDoc | Test |
|---|---|---|---|
| Field | `w:dirty` and `w:fldLock` on a complex field | Modeled | `field-dirty-and-locked` |
| Field | Legacy form field (`w:ffData`): text, check box, drop-down | Modeled | `legacy-form-field` |
| Field | Simple field (`w:fldSimple`) with a cached result | Modeled | `simple-field` |

A legacy form field needs its full field structure. A begin marker on its own becomes an
orphan, and the writer drops it.

## Sections

| Area | Feature | EdgeDoc | Test |
|---|---|---|---|
| Section | Vertical alignment (`w:vAlign`) | Modeled | `section-vertical-align` |
| Section | Text direction (`w:textDirection`) | Modeled | `section-text-direction` |
| Section | Section type (`w:type`) | Modeled | `section-type` |
| Section | Editing permissions (`w:permStart`, `w:permEnd`) | Kept | `section-permissions` |

## Tables

| Area | Feature | EdgeDoc | Test |
|---|---|---|---|
| Table | Caption and description attributes | Kept | `table-alt-text` |
| Table | Table style reference | Modeled | `table-style` |
| Table | Row properties: cannot split, height, header row | Modeled | `table-row-properties` |
| Table | Cell margins and text direction | Modeled | `table-cell-properties` |
| Table | Nested table | Modeled | `nested-table` |
| Table | Content control inside a cell | Modeled | `control-in-table` |
| Table | List inside a cell | Modeled | `list-in-table` |
| Numbering | Level override and start override | Kept | `numbering-override` |

## Stories

| Area | Feature | EdgeDoc | Test |
|---|---|---|---|
| Story | Two footnotes on one run | Modeled | `two-footnotes` |
| Story | Footnote separator and its type | Modeled | `note-separator` |
| Story | Endnote and comment together | Modeled | `endnote-and-comment` |
| Story | Header bookmark and hyperlink | Modeled | `header-bookmark-and-link` |

## Unknown markup

| Area | Feature | EdgeDoc | Test |
|---|---|---|---|
| Revision | Move tracking (`w:moveFrom`, `w:moveTo`) | Kept | `tracked-changes` |
| Revision | Formatting revision (`w:rPrChange`) | Kept | `formatting-revision` |
| Math | Equation (`m:oMath`) | Kept | `equation` |
| Drawing | Shape with a text box (`wps:wsp`) | Kept | `shape-text-box` |
| Part | `w:altChunk` | Kept | `alt-chunk` |

## Defects found and fixed

Seven features lost data before this chapter existed. Each one now has a case above.

| Feature | What was lost | Cause |
|---|---|---|
| `w:vAlign` in `w:sectPr` | The vertical alignment | The name was owned for table cells, so the section reader skipped it |
| `w:fitText` | The fit width | The name was owned for table cells |
| `w:eastAsianLayout` | The layout | The name was owned but no reader or writer handled it |
| `w:dirty`, `w:fldLock` | The recalculation flags | The model had no place for them |
| `w:ffData` | The whole legacy form field | The reader ignored the children of `w:fldChar` |
| `wps:wsp` with a text box | The shape and its text | A shape has no image relationship, so the reader treated it as an empty picture |
| `mc:AlternateContent` inside a run | The whole shape | The name was owned, so it fell through the run reader with no handler |

Two of these share a cause. **A name in the owned catalog needs a handler in every
position where it can appear.** Without one, the value falls through the match silently.
A later save loses it.

Chapter [13-word-validation.md](13-word-validation.md) covers the rules that a reader
enforces on the output. This chapter covers the content that must survive the input.
