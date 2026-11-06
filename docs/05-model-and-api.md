# The document model and the fluent API.

> Part of the EdgeDoc specification. Date: 2026-11-04. Requirement ids are normative. See [the index](README.md).

## 9. Document model

### 9.1 Stories and positions

`ED-T-05`. A position is a story id, a path, and a byte offset. The offset is a character boundary. A bad offset returns `model.bad_offset`. WordCraft uses `StoryRef`, `Path`, and `Pos` (`crates/doc/src/lib.rs`, `crates/doc/src/position.rs`).

`ED-T-06`. The body story cannot be deleted. `ensure_nonempty` inserts one empty paragraph when a story has no blocks. WordCraft does this in `Document::ensure_nonempty`.

`ED-T-07`. Paragraph text is one `String`. A run length is a byte length. The sum of run lengths equals the text length. The object character is U+FFFC. The k-th U+FFFC is `objects[k]`. WordCraft defines this in `crates/doc/src/para.rs`.

### 9.2 Owned set

`ED-T-08`. Version 1 owns these elements. The catalog lists them. Any other element in a story is opaque markup.

Owned character properties:

- Bold, italic, underline, strike, and double strike.
- Color, size, highlight, and shading.
- ASCII, high-ANSI, complex-script, and East Asian fonts.
- Vertical align, language, right-to-left, the complex-script flag, vanish, and web-hidden.

Owned paragraph properties:

- Style, alignment, indent, spacing, and numbering.
- Paragraph-mark properties and section properties.
- Keep-with-next, keep-lines, page-break-before, and widow control.
- Outline level, shading, and bidi.

Owned table properties:

- Widths, grid, borders, cell margins, and look.
- Visual bidi, row height, and header row.
- Grid span, vertical merge, and vertical align.

The owned set also includes:

- Paragraph styles, character styles, and numbering.
- Page size, margins, orientation, and column count.
- Header references, footer references, and page-number start and format.
- Hyperlinks, fields, and bookmarks.
- PNG, JPEG, and GIF inline pictures.
- The content controls in `ED-T-12`.

`ED-T-09`. A paragraph whose style name is missing from the style sheet displays as Normal and still saves the name it had. Word and WordCraft both do this (`docs/file-format-parity.md`, issue note #5077).

`ED-T-10`. Built-in heading styles `heading 1` through `heading 9` have outline levels when `w:outlineLvl` is absent. WordCraft records this in the same parity doc (#4696).

`ED-T-11`. A table has at most 63 columns. A declared span above 63 becomes 63. WordCraft clamps in `crates/doc/src/table.rs`.

### 9.3 Content controls

`ED-T-12`. Version 1 models plain text, rich text, date, drop-down, and checkbox content controls. The model stores the tag, the title, the lock, and the placeholder.

`ED-T-13`. A data binding is opaque markup. The library does not evaluate the XPath. WordCraft keeps the binding and does not evaluate it (`docs/file-format-parity.md`, custom XML section).

### 9.4 What stays opaque

Charts, SmartArt, OLE objects, VML, equations, comments, revisions, themes, embedded fonts, glossary parts, custom XML, VBA, and signatures are passthrough parts or opaque markup.

`ED-R-03`. An unedited document writes the original package bytes. The facade stores the original bytes and a dirty flag. WordCraft uses a fingerprint for an unedited glossary (`glossary_fingerprint` in `crates/docx/src/package.rs`). EdgeDoc uses a hash of the original package for the whole file.

`ED-R-04`. A passthrough part is copied by part name. The writer does not parse it.

`ED-R-05`. Settings that the model does not own stay in the original `settings.xml`. The writer merges owned settings into that XML. WordCraft documents this merge in `docs/file-format-parity.md`.

`ED-R-06`. An edit that would drop revision markup, comment anchors, or an equation returns `model.needs_drop_consent` until `EditOptions.allow_drop` contains that kind. After consent, `EditReport.dropped` lists each loss.

## 10. Fluent API

The API follows the Rust API guidelines. Names are `snake_case`. Types are `PascalCase`. Constructors are `new`, `open`, or `builder`. Getters have no `get_` prefix. Fallible functions return `Result`. Public types implement `Debug`. Builders take `self` by value.

### 10.1 Create

`ED-F-01`. `Document::builder` returns a builder. `build` returns `Result<Document, Error>`. A builder that has no paragraph still builds a document with one empty paragraph (`ED-T-06`).

```rust
let mut doc = Document::builder()
    .core(CoreProps::new().title("Invoice"))
    .page(Page::a4().margins(Mm(20.0)))
    .style(Style::paragraph("Heading1").size(Pt(16.0)).bold())
    .paragraph(
        Paragraph::new()
            .style("Heading1")
            .text("Invoice ")
            .merge_field("InvoiceNumber"),
    )
    .table(
        Table::new()
            .widths([Mm(90.0), Mm(30.0), Mm(40.0)])
            .header(["Description", "Qty", "Amount"])
            .row(["Widget", "2", "40.00"]),
    )
    .build()?;
```

`ED-F-02`. Length values use `Pt`, `Mm`, `Cm`, `Inch`, `Twips`, or `Emu`. The model stores twips for paragraph and table metrics and EMU for drawings. Callers do not pass raw integers.

`ED-F-03`. `merge_field("Total")` writes the instruction `MERGEFIELD Total` and the cached result `«Total»`. A name that contains a space is quoted. WordCraft quotes in `quote` (`crates/engine/src/cmd/mailings.rs`) and shows `«{name}»`.

### 10.2 Edit

`ED-F-04`. Edits go through `Document::edit`. The closure receives `DocumentEdit`. Direct field mutation of stories is crate-private.

```rust
let report = doc.edit(EditOptions::default(), |d| {
    d.body().paragraph_after_last(|p| {
        p.text("Total ")?;
        p.merge_field("Total")?;
        Ok(())
    })?;
    Ok(())
})?;
```

`ED-F-05`. `paragraph_after_last`, `paragraph_at`, and `table_at` are the only ways to add or change blocks. They return `Result`. The closure uses `?`.

`ED-F-06`. Character formatting methods on a run builder are `bold`, `italic`, `size`, `font`, `color`, and `style`. They return the builder. They do not return `Result` when the value is in range. An out-of-range size returns `model.limit`. Font size is from 1 point to 1638 points. WordCraft clamps to that range in `crates/doc/src/resolve.rs`.

### 10.3 Open and save

```rust
let doc = Document::read_bytes(&bytes, &ReadOptions::default())?;
let out = doc.write_bytes(&WriteOptions::default())?;
// feature "fs"
let doc = Document::open("template.docx", &ReadOptions::default())?;
doc.save("out.docx", &WriteOptions::default())?;
```

`ED-F-07`. `read_bytes` and `write_bytes` exist on every target. `open` and `save` exist only with the `fs` feature.

`ED-F-08`. `save` writes a temporary sibling, syncs it, and replaces the destination. On failure the destination bytes stay as they were. WordCraft specifies this path in `docs/file-format-parity.md` under "Native save-path guarantees".

`ED-F-09`. `save` refuses to follow a symbolic link at the destination. It refuses to replace a directory. It does not truncate the destination in place.

`ED-P-12`. `ReadOptions` carries the password borrow, the budget, and `UnknownPart::Retain`. `UnknownPart::Drop` exists for tests. The default is `Retain`.

`ED-P-13`. `WriteOptions` carries the flavor, the budget, and `pretty: false`. Version 1 does not pretty-print XML. Pretty-printing changes diffs and hides semantic changes.

### 10.4 Inspection

`ED-F-10`. These methods are read-only and do not mark the document dirty.

| Method | Returns |
|---|---|
| `plain_text` | Owned text of one story. Fields contribute their cached result |
| `merge_field_names` | Field names in body, headers, and footers, in order, without duplicates |
| `stories` | Story ids |
| `part_names` | Package part names, including passthrough parts |
| `flavor` | Package flavor |
| `is_dirty` | Whether a save would rewrite the package |

`merge_field_names` follows WordCraft `merge_fields` (`crates/engine/src/cmd/mailings/merge.rs`). A keyword is `MERGEFIELD` only when the next character is whitespace or a quote. `MERGEFIELDX` is a different field.

### 10.5 Markdown

`ED-F-14`. `Document::to_markdown` and `to_markdown_with` export the body. `Document::from_markdown` and `from_markdown_with` read that Markdown. `open_markdown` and `open_markdown_with` read a UTF-8 file and exist only with the `fs` feature.

The reader accepts the Markdown the exporter writes:

- Headings `#` through nine hashes, and a setext underline on the next line.
- `**bold**`, `*italic*`, `~~strike~~`, `<sub>`, and `<sup>`.
- Links, footnote `[^id]`, and endnote `[^end-id]`, with the definitions at the end.
- Bullet and ordered lists, including a nested item indented by two spaces.
- A GFM table. The first row is the header. `<br>` is a new paragraph in the cell.
- A thematic break `---` becomes a page break.
- Inline `$...$` and display `$$` become Office Math. The LaTeX is kept, and the package stores OMML so a saved `.docx` still has the formula.
- `![alt](path)` becomes a picture. The bytes come from `MarkdownImage` or from `image_dir`.

`ED-F-15`. Image lookup uses the caller-supplied bytes first, matched by the path in the Markdown. A directory read refuses an absolute path, a `..` segment, and a symbolic link (`markdown.image_dir`). A missing file, or an `http` or `https` URL with no bytes, returns `markdown.image_missing`. The library does not download a URL. Text larger than 32 MiB returns `markdown.limit`. A file that is not UTF-8 returns `markdown.utf8`.

A fenced code block becomes one paragraph. A nested table that the exporter flattened stays cell text. Headers, footers, comments, and text boxes are outside the Markdown body in both directions. After a save and a read, a formula can come back with the braces the OMML converter writes, such as `{x}^{2}` for `x^{2}`. The in-memory export, before that save, keeps the LaTeX the caller wrote.
