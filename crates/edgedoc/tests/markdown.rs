//! DOCX to Markdown, including images written to a chosen directory.

use std::path::{Path, PathBuf};

use edgedoc::{
    CharProps, Document, ListKind, MarkdownOptions, Paragraph, PartKind, Picture, RunBuilder,
    Style, Table,
};
use edgedoc_opc::write_zip;

type Test = Result<(), String>;

fn err(err: impl std::fmt::Display) -> String {
    err.to_string()
}

fn sample() -> Result<Document, String> {
    let mut linked = Paragraph::new();
    let mut props = CharProps::default();
    props.link = Some("https://example.com/a".into());
    linked.push_text("Example", &props);
    Document::builder()
        .style(Style::paragraph("Heading1").named("heading 1"))
        .style(Style::paragraph("Heading2").named("heading 2"))
        .paragraph(Paragraph::new().style("Heading1").text("Overview"))
        .paragraph(Paragraph::new().style("Heading2").text("Detail"))
        .paragraph(
            Paragraph::new()
                .run(RunBuilder::new().text("Hello "))
                .run(RunBuilder::new().bold().text("strong"))
                .run(RunBuilder::new().text(" and "))
                .run(RunBuilder::new().italic().text("lean"))
                .run(RunBuilder::new().text(" "))
                .run(RunBuilder::new().strike().text("old")),
        )
        .paragraph(linked)
        .paragraph(Paragraph::new().list(ListKind::Bullet, 0).text("Apples"))
        .paragraph(Paragraph::new().list(ListKind::Bullet, 0).text("Pears"))
        .paragraph(Paragraph::new().list(ListKind::Decimal, 0).text("First"))
        .paragraph(Paragraph::new().list(ListKind::Decimal, 1).text("Nested"))
        .table(
            Table::new()
                .header(["Name", "Qty"])
                .row(["Pipe | here", "2"]),
        )
        .paragraph(
            Paragraph::new()
                .text("Before ")
                .picture(Picture::png(b"PNG-BYTES".to_vec()).alt("Chart"))
                .text(" after"),
        )
        .paragraph(Paragraph::new().picture(Picture::jpeg(b"JPEG-BYTES".to_vec()).alt("Photo")))
        .paragraph(
            Paragraph::new()
                .run(RunBuilder::new().text("shown"))
                .run(RunBuilder::new().vanish().text("hidden")),
        )
        .build()
        .map_err(err)
}

fn reopen(doc: &Document) -> Result<Document, String> {
    let bytes = doc.write_bytes().map_err(err)?;
    Document::read_bytes(&bytes).map_err(err)
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("edgedoc-md-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn image_target<'a>(markdown: &'a str, alt: &str) -> Result<&'a str, String> {
    let marker = format!("![{alt}](");
    let start = markdown
        .find(&marker)
        .ok_or_else(|| format!("missing image {alt} in {markdown}"))?
        + marker.len();
    let rest = &markdown[start..];
    let end = rest
        .find(')')
        .ok_or_else(|| format!("unclosed image {alt}"))?;
    Ok(&rest[..end])
}

fn assert_body(markdown: &str, chart: &str, photo: &str) -> Test {
    let expected = format!(
        r#"# Overview

## Detail

Hello **strong** and *lean* ~~old~~

[Example](https://example.com/a)

- Apples
- Pears

1. First
  1. Nested

| Name | Qty |
| --- | --- |
| Pipe \| here | 2 |

Before ![{chart_alt}]({chart}) after

![{photo_alt}]({photo})

shown
"#,
        chart_alt = "Chart",
        photo_alt = "Photo",
    );
    if markdown != expected {
        return Err(format!("markdown\n{markdown}\n---\n{expected}"));
    }
    Ok(())
}

#[test]
fn a_docx_becomes_markdown_and_writes_images() -> Test {
    let doc = reopen(&sample()?)?;
    let dir = scratch("export").join("media");
    let exported = doc
        .to_markdown_with(&MarkdownOptions::new().image_dir(&dir))
        .map_err(err)?;
    let chart = image_target(&exported.markdown, "Chart")?;
    let photo = image_target(&exported.markdown, "Photo")?;
    assert_body(&exported.markdown, chart, photo)?;
    if !chart.starts_with("media/") || !photo.starts_with("media/") {
        return Err(format!("prefixes {chart} {photo}"));
    }
    if exported.images.len() != 2 {
        return Err(format!("{} images", exported.images.len()));
    }
    for image in &exported.images {
        let stored = image
            .stored_at
            .as_ref()
            .ok_or_else(|| format!("{} was not stored", image.file_name))?;
        if stored != &dir.join(&image.file_name) {
            return Err(format!("stored at {}", stored.display()));
        }
        if !exported.markdown.contains(&image.markdown_path) {
            return Err(format!("markdown misses {}", image.markdown_path));
        }
        let on_disk = std::fs::read(stored).map_err(err)?;
        if on_disk != image.bytes {
            return Err(format!("{} bytes differ", image.file_name));
        }
    }
    let png = exported
        .images
        .iter()
        .find(|image| image.alt == "Chart")
        .ok_or("missing chart")?;
    let jpeg = exported
        .images
        .iter()
        .find(|image| image.alt == "Photo")
        .ok_or("missing photo")?;
    if png.bytes != b"PNG-BYTES" || jpeg.bytes != b"JPEG-BYTES" {
        return Err("the image bytes changed".into());
    }
    let _ = std::fs::remove_dir_all(dir.parent().unwrap_or(Path::new(".")));
    Ok(())
}

#[test]
fn the_link_prefix_is_independent_of_the_directory() -> Test {
    let doc = reopen(&sample()?)?;
    let dir = scratch("prefix");
    let exported = doc
        .to_markdown_with(
            &MarkdownOptions::new()
                .image_dir(&dir)
                .link_prefix("https://cdn.example/assets"),
        )
        .map_err(err)?;
    let chart = image_target(&exported.markdown, "Chart")?;
    if !chart.starts_with("https://cdn.example/assets/") {
        return Err(chart.into());
    }
    let file = chart.rsplit('/').next().unwrap_or("");
    let stored = dir.join(file);
    if std::fs::read(&stored).map_err(err)? != b"PNG-BYTES" {
        return Err("the chart file was not written beside the other prefix".into());
    }
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

#[test]
fn images_remain_in_memory_when_no_directory_is_set() -> Test {
    let doc = sample()?;
    let exported = doc.to_markdown().map_err(err)?;
    if exported.images.len() != 2 {
        return Err(format!("{} images", exported.images.len()));
    }
    for image in &exported.images {
        if image.stored_at.is_some() {
            return Err(format!("{} was written", image.file_name));
        }
        if !image.markdown_path.starts_with("images/") {
            return Err(image.markdown_path.clone());
        }
    }
    let chart = image_target(&exported.markdown, "Chart")?;
    if chart != "images/image.png" {
        return Err(chart.into());
    }
    let photo = image_target(&exported.markdown, "Photo")?;
    if photo != "images/image.jpg" {
        return Err(photo.into());
    }
    if exported.images[0].bytes != b"PNG-BYTES" || exported.images[1].bytes != b"JPEG-BYTES" {
        return Err("bytes were not kept".into());
    }
    Ok(())
}

#[test]
fn an_empty_link_prefix_uses_the_file_name() -> Test {
    let doc = sample()?;
    let dir = scratch("beside");
    let exported = doc
        .to_markdown_with(&MarkdownOptions::new().image_dir(&dir).link_prefix(""))
        .map_err(err)?;
    let chart = image_target(&exported.markdown, "Chart")?;
    if chart.contains('/') {
        return Err(chart.into());
    }
    if std::fs::read(dir.join(chart)).map_err(err)? != b"PNG-BYTES" {
        return Err("the chart was not stored beside the markdown".into());
    }
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

#[test]
fn a_symbolic_link_is_refused_as_the_image_directory() -> Test {
    let doc = sample()?;
    let real = scratch("real");
    std::fs::create_dir_all(&real).map_err(err)?;
    let link = std::env::temp_dir().join(format!("edgedoc-md-link-{}", std::process::id()));
    let _ = std::fs::remove_file(&link);
    std::os::unix::fs::symlink(&real, &link).map_err(err)?;
    match doc.to_markdown_with(&MarkdownOptions::new().image_dir(&link)) {
        Ok(_) => return Err("a symlink should be refused".into()),
        Err(failure) if failure.code == "markdown.image_dir" => {}
        Err(failure) => return Err(format!("{} {}", failure.code, failure.message)),
    }
    let _ = std::fs::remove_file(&link);
    let _ = std::fs::remove_dir_all(&real);
    Ok(())
}

#[test]
fn a_file_is_refused_as_the_image_directory() -> Test {
    let doc = sample()?;
    let dir = scratch("file");
    std::fs::create_dir_all(&dir).map_err(err)?;
    let file = dir.join("not-a-dir");
    std::fs::write(&file, b"x").map_err(err)?;
    match doc.to_markdown_with(&MarkdownOptions::new().image_dir(&file)) {
        Ok(_) => return Err("a file should be refused".into()),
        Err(failure) if failure.code == "markdown.image_dir" => {}
        Err(failure) => return Err(failure.message),
    }
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

#[test]
fn repeated_image_names_get_a_suffix() -> Test {
    let doc = Document::builder()
        .paragraph(Paragraph::new().picture(Picture::png(b"one".to_vec()).alt("One")))
        .paragraph(Paragraph::new().picture(Picture::png(b"two".to_vec()).alt("Two")))
        .build()
        .map_err(err)?;
    let exported = doc.to_markdown().map_err(err)?;
    if image_target(&exported.markdown, "One")? != "images/image.png" {
        return Err(exported.markdown);
    }
    if image_target(&exported.markdown, "Two")? != "images/image-2.png" {
        return Err(exported.markdown);
    }
    if exported.images[1].bytes != b"two" {
        return Err("the second image lost its bytes".into());
    }
    Ok(())
}

#[test]
fn an_empty_document_exports_no_markdown() -> Test {
    let doc = Document::builder().build().map_err(err)?;
    let exported = doc.to_markdown().map_err(err)?;
    if !exported.markdown.is_empty() || !exported.images.is_empty() {
        return Err(format!("{:?}", exported.markdown));
    }
    Ok(())
}

#[test]
fn a_footnote_in_a_docx_is_referenced() -> Test {
    let footnotes = br#"<?xml version="1.0"?><w:footnotes xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:footnote w:type="separator" w:id="-1"><w:p><w:r><w:separator/></w:r></w:p></w:footnote><w:footnote w:id="2"><w:p><w:r><w:t>Source note</w:t></w:r></w:p></w:footnote></w:footnotes>"#;
    let document = r#"<?xml version="1.0"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>Text</w:t><w:footnoteReference w:id="2"/></w:r></w:p></w:body></w:document>"#;
    let bytes = package(
        document,
        r#"<Relationship Id="rIdF" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/footnotes" Target="footnotes.xml"/>"#,
        &[("word/footnotes.xml", footnotes)],
    )?;
    let doc = Document::read_bytes(&bytes).map_err(err)?;
    let exported = doc.to_markdown().map_err(err)?;
    if exported.markdown != "Text[^2]\n\n[^2]: Source note\n" {
        return Err(exported.markdown);
    }
    Ok(())
}

fn package(document: &str, rels: &str, extra: &[(&str, &[u8])]) -> Result<Vec<u8>, String> {
    let mut types = String::from(
        r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Default Extension="png" ContentType="image/png"/><Default Extension="gif" ContentType="image/gif"/><Default Extension="jpeg" ContentType="image/jpeg"/><Default Extension="emf" ContentType="image/x-emf"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>"#,
    );
    for (name, _) in extra {
        if name.ends_with(".xml") {
            types.push_str(&format!(
                r#"<Override PartName="/{name}" ContentType="application/xml"/>"#
            ));
        }
    }
    types.push_str("</Types>");
    let mut parts = vec![
        ("[Content_Types].xml".into(), types.into_bytes()),
        (
            "_rels/.rels".into(),
            br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#.to_vec(),
        ),
        ("word/document.xml".into(), document.as_bytes().to_vec()),
        (
            "word/_rels/document.xml.rels".into(),
            format!(
                r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">{rels}</Relationships>"#
            )
            .into_bytes(),
        ),
    ];
    for (name, bytes) in extra {
        parts.push(((*name).into(), bytes.to_vec()));
    }
    write_zip(&parts).map_err(err)
}

#[test]
fn a_word_outline_level_is_a_heading() -> Test {
    let document = r#"<?xml version="1.0"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:pPr><w:outlineLvl w:val="0"/></w:pPr><w:r><w:t>Top</w:t></w:r></w:p><w:p><w:pPr><w:outlineLvl w:val="2"/></w:pPr><w:r><w:t>Third</w:t></w:r></w:p></w:body></w:document>"#;
    let bytes = package(document, "", &[])?;
    let doc = Document::read_bytes(&bytes).map_err(err)?;
    let exported = doc.to_markdown().map_err(err)?;
    if exported.markdown != "# Top\n\n### Third\n" {
        return Err(exported.markdown);
    }
    Ok(())
}

/// A hand-written DOCX, not one this crate just saved. The reader and the exporter
/// run together: styles, numbering, fields, notes, tables, and image files.
#[test]
fn a_hand_written_docx_exports_markdown_and_its_images() -> Test {
    let drawing = |rel: &str, descr: &str| {
        format!(
            r#"<w:r><w:drawing><wp:inline><wp:docPr id="1" name="pic" descr="{descr}"/><a:graphic><a:graphicData uri="pic"><pic:pic><pic:blipFill><a:blip r:embed="{rel}"/></pic:blipFill></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r>"#
        )
    };
    let document = format!(
        r#"<?xml version="1.0"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
<w:body>
<w:p><w:pPr><w:pStyle w:val="Heading1"/><w:numPr><w:ilvl w:val="0"/><w:numId w:val="10"/></w:numPr></w:pPr><w:r><w:t>Overview</w:t></w:r></w:p>
<w:p><w:pPr><w:pStyle w:val="Title2"/></w:pPr><w:r><w:t>Detail</w:t></w:r></w:p>
<w:p><w:pPr><w:pStyle w:val="Subhead"/></w:pPr><w:r><w:t>Still top</w:t></w:r></w:p>
<w:p><w:r><w:rPr><w:b/><w:i/></w:rPr><w:t>both</w:t></w:r></w:p>
<w:p><w:r><w:t>H</w:t></w:r><w:r><w:rPr><w:vertAlign w:val="subscript"/></w:rPr><w:t>2</w:t></w:r><w:r><w:t>O</w:t></w:r></w:p>
<w:p><w:r><w:t>a*b_c[d]e|f\g</w:t></w:r></w:p>
<w:p><w:hyperlink r:id="rIdL"><w:r><w:rPr><w:b/></w:rPr><w:t>Hello</w:t></w:r><w:r><w:rPr><w:i/></w:rPr><w:t xml:space="preserve"> world</w:t></w:r></w:hyperlink></w:p>
<w:p><w:fldSimple w:instr=" HYPERLINK &quot;https://example.com/go&quot; "><w:r><w:t>Go</w:t></w:r></w:fldSimple></w:p>
<w:p><w:r><w:t>Left</w:t></w:r><w:r><w:br/></w:r><w:r><w:t>Right</w:t></w:r></w:p>
<w:p><w:r><w:t>Before</w:t></w:r><w:r><w:br w:type="page"/></w:r><w:r><w:t>After</w:t></w:r></w:p>
<w:p><w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="10"/></w:numPr></w:pPr><w:r><w:t>Apple</w:t></w:r></w:p>
<w:p><w:pPr><w:numPr><w:ilvl w:val="1"/><w:numId w:val="10"/></w:numPr></w:pPr><w:r><w:t>Pippin</w:t></w:r></w:p>
<w:p><w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="11"/></w:numPr></w:pPr><w:r><w:t>Ranked</w:t></w:r></w:p>
<w:p><w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="0"/></w:numPr></w:pPr><w:r><w:t>Plain</w:t></w:r></w:p>
<w:tbl>
<w:tr><w:tc><w:p><w:r><w:t>Name</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>Note</w:t></w:r></w:p></w:tc></w:tr>
<w:tr><w:trPr><w:hidden/></w:trPr><w:tc><w:p><w:r><w:t>Hidden row</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>skip</w:t></w:r></w:p></w:tc></w:tr>
<w:tr>
<w:tc><w:p><w:r><w:t>Pipe | here</w:t></w:r><w:r><w:br/></w:r><w:r><w:t>more</w:t></w:r></w:p></w:tc>
<w:tc>
<w:p>{logo}</w:p>
<w:tbl><w:tr><w:tc><w:p><w:r><w:t>Inner</w:t></w:r></w:p></w:tc></w:tr><w:tr><w:tc><w:p><w:r><w:t>cell</w:t></w:r></w:p></w:tc></w:tr></w:tbl>
</w:tc>
</w:tr>
</w:tbl>
<w:sdt><w:sdtPr><w:alias w:val="Person"/></w:sdtPr><w:sdtContent><w:p><w:r><w:t>Ada</w:t></w:r></w:p></w:sdtContent></w:sdt>
<mc:AlternateContent><mc:Choice Requires="wps"><w:p><w:r><w:t>Chosen text</w:t></w:r></w:p></mc:Choice><mc:Fallback><w:p><w:r><w:t>Fallback text</w:t></w:r></w:p></mc:Fallback></mc:AlternateContent>
<w:p>{secret}</w:p>
<w:p>{meta}</w:p>
<w:p><w:r><w:t>Visible</w:t></w:r><w:r><w:rPr><w:vanish/></w:rPr><w:t>GONE</w:t></w:r></w:p>
<w:p><w:r><w:t>Above</w:t></w:r></w:p>
<w:p><w:r><w:br w:type="page"/></w:r></w:p>
<w:p><w:r><w:t>Below</w:t></w:r></w:p>
<w:p><w:r><w:t>Note</w:t></w:r><w:r><w:footnoteReference w:id="2"/></w:r><w:r><w:t> and </w:t></w:r><w:r><w:endnoteReference w:id="4"/></w:r></w:p>
<w:sectPr><w:headerReference w:type="default" r:id="rIdH"/></w:sectPr>
</w:body></w:document>"#,
        logo = drawing("rIdLogo", "Logo"),
        secret = drawing("rIdOut", "A]B"),
        meta = drawing("rIdEmf", "Metafile"),
    );
    let styles = br#"<?xml version="1.0"?><w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/><w:pPr><w:outlineLvl w:val="0"/></w:pPr></w:style><w:style w:type="paragraph" w:styleId="Title2"><w:name w:val="heading 2"/></w:style><w:style w:type="paragraph" w:styleId="Subhead"><w:name w:val="Subhead"/><w:basedOn w:val="Heading1"/></w:style></w:styles>"#;
    let numbering = br#"<?xml version="1.0"?><w:numbering xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:abstractNum w:abstractNumId="10"><w:lvl w:ilvl="0"><w:numFmt w:val="bullet"/><w:lvlText w:val="*"/></w:lvl><w:lvl w:ilvl="1"><w:numFmt w:val="decimal"/><w:lvlText w:val="%2."/></w:lvl></w:abstractNum><w:num w:numId="10"><w:abstractNumId w:val="10"/></w:num><w:abstractNum w:abstractNumId="11"><w:lvl w:ilvl="0"><w:numFmt w:val="bullet"/></w:lvl></w:abstractNum><w:num w:numId="11"><w:abstractNumId w:val="11"/><w:lvlOverride w:ilvl="0"><w:lvl w:ilvl="0"><w:numFmt w:val="decimal"/></w:lvl></w:lvlOverride></w:num></w:numbering>"#;
    let footnotes = format!(
        r#"<?xml version="1.0"?><w:footnotes xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><w:footnote w:type="separator" w:id="-1"><w:p><w:r><w:t>separator</w:t></w:r></w:p></w:footnote><w:footnote w:id="2"><w:p><w:r><w:t>Source </w:t></w:r>{shot}</w:p></w:footnote></w:footnotes>"#,
        shot = drawing("rIdN", "Shot"),
    );
    let endnotes = br#"<?xml version="1.0"?><w:endnotes xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:endnote w:id="4"><w:p><w:r><w:t>Closing</w:t></w:r></w:p></w:endnote></w:endnotes>"#;
    let header = format!(
        r#"<?xml version="1.0"?><w:hdr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><w:p><w:r><w:t>Letterhead line</w:t></w:r>{head}</w:p></w:hdr>"#,
        head = drawing("rIdHead", "Letterhead"),
    );
    let comments = br#"<?xml version="1.0"?><w:comments xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:comment w:id="1" w:author="Ada"><w:p><w:r><w:t>Do not export this comment</w:t></w:r></w:p></w:comment></w:comments>"#;
    let rels = r#"<Relationship Id="rIdS" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>
<Relationship Id="rIdN" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering" Target="numbering.xml"/>
<Relationship Id="rIdF" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/footnotes" Target="footnotes.xml"/>
<Relationship Id="rIdE" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/endnotes" Target="endnotes.xml"/>
<Relationship Id="rIdH" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/header" Target="header1.xml"/>
<Relationship Id="rIdC" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments" Target="comments.xml"/>
<Relationship Id="rIdL" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink" Target="https://example.com/a(b)" TargetMode="External"/>
<Relationship Id="rIdLogo" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/logo.png"/>
<Relationship Id="rIdOut" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="../secret.png"/>
<Relationship Id="rIdEmf" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/meta.emf"/>"#;
    let bytes = package(
        &document,
        rels,
        &[
            ("word/styles.xml", styles),
            ("word/numbering.xml", numbering),
            ("word/footnotes.xml", footnotes.as_bytes()),
            ("word/endnotes.xml", endnotes),
            ("word/header1.xml", header.as_bytes()),
            ("word/comments.xml", comments),
            ("word/media/logo.png", b"LOGO"),
            ("word/media/note.gif", b"GIF89a"),
            ("word/media/head.png", b"HEAD"),
            ("word/media/meta.emf", b"EMF"),
            ("word/../secret.png", b"SECRET"),
            (
                "word/_rels/footnotes.xml.rels",
                br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdN" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/note.gif"/></Relationships>"#,
            ),
            (
                "word/_rels/header1.xml.rels",
                br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdHead" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/head.png"/></Relationships>"#,
            ),
        ],
    )?;
    let doc = Document::read_bytes(&bytes).map_err(err)?;
    let header_id = doc
        .parts(PartKind::Header)
        .into_iter()
        .next()
        .ok_or("the header was not read")?;
    let header_debug = format!("{:?}", doc.part_blocks(header_id));
    if !header_debug.contains("Letterhead line") {
        return Err(header_debug);
    }
    if doc.parts(PartKind::Comment).is_empty() {
        return Err("the comment was not read".into());
    }
    let root = scratch("hand");
    let image_dir = root.join("out").join("assets");
    let exported = doc
        .to_markdown_with(&MarkdownOptions::new().image_dir(&image_dir))
        .map_err(err)?;
    let markdown = exported.markdown.as_str();
    for fragment in [
        "# Overview\n",
        "## Detail\n",
        "# Still top\n",
        "***both***",
        "H<sub>2</sub>O",
        r"a\*b\_c\[d\]e\|f\\g",
        "[**Hello** *world*](https://example.com/a%28b%29)",
        "[Go](https://example.com/go)",
        "Left  \nRight",
        "Before\n\n---\n\nAfter",
        "- Apple\n  1. Pippin",
        "1. Ranked",
        "Plain",
        "| Name | Note |\n| --- | --- |\n",
        "Pipe \\| here<br>more",
        "![Logo](assets/logo.png)",
        r"\| Inner \|",
        r"\| cell \|",
        "Ada",
        "Chosen text",
        r"![A\]B](assets/secret.png)",
        "Visible",
        "Above\n\n---\n\nBelow",
        "Note[^2] and [^end-4]",
        "[^2]: Source ![Shot](assets/note.gif)",
        "[^end-4]: Closing",
    ] {
        if !markdown.contains(fragment) {
            return Err(format!("missing {fragment:?}\n\n{markdown}"));
        }
    }
    for absent in [
        "- Overview",
        "- Plain",
        "Hidden row",
        "Fallback text",
        "GONE",
        "separator",
        "Letterhead",
        "Do not export this comment",
        "Metafile",
    ] {
        if markdown.contains(absent) {
            return Err(format!(
                "{absent} should stay out of the markdown\n\n{markdown}"
            ));
        }
    }
    if markdown.matches("![").count() != 3 {
        return Err(format!("image count\n\n{markdown}"));
    }
    let expected = [
        ("Logo", "logo.png", &b"LOGO"[..], "image/png"),
        ("A]B", "secret.png", &b"SECRET"[..], "image/png"),
        ("Shot", "note.gif", &b"GIF89a"[..], "image/gif"),
    ];
    if exported.images.len() != expected.len() {
        return Err(format!("{} images", exported.images.len()));
    }
    for (alt, file_name, bytes, content_type) in expected {
        let image = exported
            .images
            .iter()
            .find(|image| image.alt == alt)
            .ok_or_else(|| format!("missing exported {alt}"))?;
        if image.file_name != file_name || image.markdown_path != format!("assets/{file_name}") {
            return Err(format!("{} -> {}", image.file_name, image.markdown_path));
        }
        if image.bytes != bytes || image.content_type != content_type {
            return Err(format!("{alt} bytes or type {}", image.content_type));
        }
        if image.file_name.contains('/') || image.file_name.contains("..") {
            return Err(image.file_name.clone());
        }
        let stored = image.stored_at.clone().ok_or("image was not written")?;
        if stored != image_dir.join(file_name) {
            return Err(format!("{}", stored.display()));
        }
        if std::fs::read(&stored).map_err(err)? != bytes {
            return Err(format!("{file_name} on disk does not match"));
        }
    }
    let written = files_in(&root)?;
    let mut names: Vec<_> = exported
        .images
        .iter()
        .map(|image| image_dir.join(&image.file_name))
        .collect();
    names.sort();
    if written != names {
        return Err(format!("files {:?}\nexpected {:?}", written, names));
    }
    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}

#[test]
fn an_image_name_cannot_escape_the_image_directory() -> Test {
    let doc = Document::builder()
        .paragraph(
            Paragraph::new().picture(Picture::new("..", b"DOT".to_vec(), "image/png").alt("Dot")),
        )
        .paragraph(Paragraph::new().picture(
            Picture::new("../../etc/passwd.png", b"PW".to_vec(), "image/png").alt("Secret"),
        ))
        .build()
        .map_err(err)?;
    let root = scratch("escape");
    let image_dir = root.join("images");
    let exported = doc
        .to_markdown_with(&MarkdownOptions::new().image_dir(&image_dir))
        .map_err(err)?;
    if image_target(&exported.markdown, "Dot")? != "images/image-1.png" {
        return Err(exported.markdown);
    }
    if image_target(&exported.markdown, "Secret")? != "images/passwd.png" {
        return Err(exported.markdown);
    }
    if std::fs::read(image_dir.join("image-1.png")).map_err(err)? != b"DOT" {
        return Err("the dot image was not written inside the directory".into());
    }
    if std::fs::read(image_dir.join("passwd.png")).map_err(err)? != b"PW" {
        return Err("the renamed image was not written inside the directory".into());
    }
    if root.join("etc").exists() || root.join("passwd.png").exists() {
        return Err("an image was written outside the image directory".into());
    }
    let written = files_in(&root)?;
    if written.len() != 2 {
        return Err(format!("{written:?}"));
    }
    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}

fn files_in(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut out = Vec::new();
    if !dir.exists() {
        return Ok(out);
    }
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        for entry in std::fs::read_dir(&current).map_err(err)? {
            let entry = entry.map_err(err)?;
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                out.push(path);
            }
        }
    }
    out.sort();
    Ok(out)
}
