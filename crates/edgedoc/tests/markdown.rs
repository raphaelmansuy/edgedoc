//! DOCX to Markdown, including images written to a chosen directory.

use std::path::{Path, PathBuf};

use edgedoc::{
    CharProps, Document, ListKind, MarkdownOptions, Paragraph, Picture, RunBuilder, Style, Table,
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
        r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>"#,
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
