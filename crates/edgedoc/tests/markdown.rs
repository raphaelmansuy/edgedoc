//! DOCX to Markdown, including images written to a chosen directory.

use std::path::{Path, PathBuf};

use edgedoc::{
    CharProps, Document, ListKind, MarkdownImage, MarkdownOptions, MarkdownReadOptions, Paragraph,
    PartKind, Picture, RunBuilder, Style, Table,
};
use edgedoc_opc::write_zip;

type Test = Result<(), String>;

fn err(err: impl std::fmt::Display) -> String {
    err.to_string()
}

fn sample() -> Result<Document, String> {
    let mut linked = Paragraph::new();
    let props = CharProps {
        link: Some("https://example.com/a".into()),
        ..Default::default()
    };
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
    if std::fs::read(stored).map_err(err)? != b"PNG-BYTES" {
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
        if std::fs::read(stored).map_err(err)? != bytes {
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

/// Every Office Math construct the exporter understands, read from a hand-written DOCX.
#[test]
fn equations_become_latex() -> Test {
    fn run(text: &str) -> String {
        format!("<m:r><m:t xml:space=\"preserve\">{text}</m:t></m:r>")
    }
    fn styled(style: &str, text: &str) -> String {
        format!(
            "<m:r><m:rPr><m:sty m:val=\"{style}\"/></m:rPr><m:t xml:space=\"preserve\">{text}</m:t></m:r>"
        )
    }
    fn om(inner: &str) -> String {
        format!(
            "<m:oMath xmlns:m=\"http://schemas.openxmlformats.org/officeDocument/2006/math\">{inner}</m:oMath>"
        )
    }
    fn para(inner: &str) -> String {
        format!("<w:p>{inner}</w:p>")
    }
    fn shown(inner: &str) -> String {
        para(&om(inner))
    }
    let fraction = |kind: &str, num: &str, den: &str| {
        let props = if kind.is_empty() {
            String::new()
        } else {
            format!("<m:fPr><m:type m:val=\"{kind}\"/></m:fPr>")
        };
        format!(
            "<m:f>{props}<m:num>{}</m:num><m:den>{}</m:den></m:f>",
            run(num),
            run(den)
        )
    };
    let cases = [
        (shown(&run("x+y")), "$x+y$"),
        (
            format!(
                "<w:p><m:oMathPara xmlns:m=\"http://schemas.openxmlformats.org/officeDocument/2006/math\">{}</m:oMathPara></w:p>",
                om(&fraction("", "a", "b"))
            ),
            "$$\n\\frac{a}{b}\n$$",
        ),
        (
            shown(&format!(
                "<m:sSup><m:e>{}</m:e><m:sup>{}</m:sup></m:sSup>",
                run("x"),
                run("2")
            )),
            "${x}^{2}$",
        ),
        (
            shown(&format!(
                "<m:sSub><m:e>{}</m:e><m:sub>{}</m:sub></m:sSub>",
                run("x"),
                run("i")
            )),
            "${x}_{i}$",
        ),
        (
            shown(&format!(
                "<m:sSubSup><m:e>{}</m:e><m:sub>{}</m:sub><m:sup>{}</m:sup></m:sSubSup>",
                run("x"),
                run("i"),
                run("n")
            )),
            "${x}_{i}^{n}$",
        ),
        (
            shown(&format!(
                "<m:sSup><m:e>{}</m:e><m:sup>{}</m:sup></m:sSup>",
                fraction("", "a", "b"),
                run("2")
            )),
            "${\\frac{a}{b}}^{2}$",
        ),
        (
            shown(&format!(
                "<m:sPre><m:sub>{}</m:sub><m:sup>{}</m:sup><m:e>{}</m:e></m:sPre>",
                run("a"),
                run("b"),
                run("x")
            )),
            "${}_{a}^{b}{x}$",
        ),
        (shown(&fraction("lin", "a", "b")), "${a}/{b}$"),
        (shown(&fraction("skw", "a", "b")), "${}^{a}/_{b}$"),
        (
            shown(&fraction("noBar", "a", "b")),
            "$\\genfrac{}{}{0pt}{}{a}{b}$",
        ),
        (
            shown(
                "<m:rad><m:radPr><m:degHide m:val=\"1\"/></m:radPr><m:e><m:r><m:t>x</m:t></m:r></m:e></m:rad>",
            ),
            "$\\sqrt{x}$",
        ),
        (
            shown(&format!(
                "<m:rad><m:deg>{}</m:deg><m:e>{}</m:e></m:rad>",
                run("3"),
                run("x")
            )),
            "$\\sqrt[3]{x}$",
        ),
        (
            shown(&format!(
                "<m:nary><m:naryPr><m:chr m:val=\"∑\"/><m:limLoc m:val=\"undOvr\"/></m:naryPr><m:sub>{}</m:sub><m:sup>{}</m:sup><m:e>{}</m:e></m:nary>",
                run("i=1"),
                run("n"),
                run("i")
            )),
            "$\\sum\\limits_{i=1}^{n}{i}$",
        ),
        (
            shown(&format!(
                "<m:nary><m:naryPr><m:chr m:val=\"∑\"/><m:limLoc m:val=\"undOvr\"/><m:supHide m:val=\"1\"/></m:naryPr><m:sub>{}</m:sub><m:sup>{}</m:sup><m:e>{}</m:e></m:nary>",
                run("i"),
                run("n"),
                run("i")
            )),
            "$\\sum\\limits_{i}{i}$",
        ),
        (
            shown(&format!(
                "<m:nary><m:sub>{}</m:sub><m:sup>{}</m:sup><m:e>{}</m:e></m:nary>",
                run("0"),
                run("1"),
                run("x")
            )),
            "$\\int_{0}^{1}{x}$",
        ),
        (
            shown(&format!(
                "<m:nary><m:naryPr><m:subHide m:val=\"1\"/></m:naryPr><m:sub>{}</m:sub><m:sup>{}</m:sup><m:e>{}</m:e></m:nary>",
                run("0"),
                run("1"),
                run("x")
            )),
            "$\\int^{1}{x}$",
        ),
        (
            shown(&format!(
                "<m:nary><m:naryPr><m:chr m:val=\"∏\"/></m:naryPr><m:sub>{}</m:sub><m:e>{}</m:e></m:nary>",
                run("i"),
                run("i")
            )),
            "$\\prod_{i}{i}$",
        ),
        (
            shown(&format!(
                "<m:d><m:e>{}</m:e><m:e>{}</m:e></m:d>",
                run("a"),
                run("b")
            )),
            "$\\left(a,b\\right)$",
        ),
        (
            shown(&format!(
                "<m:d><m:dPr><m:begChr m:val=\"{}\"/><m:endChr m:val=\"{}\"/></m:dPr><m:e>{}</m:e></m:d>",
                "{", "}", run("a")
            )),
            "$\\left\\{a\\right\\}$",
        ),
        (
            shown(&format!(
                "<m:d><m:dPr><m:grow m:val=\"0\"/></m:dPr><m:e>{}</m:e></m:d>",
                run("x")
            )),
            "$(x)$",
        ),
        (
            shown(&format!(
                "<m:d><m:dPr><m:begChr m:val=\"\"/></m:dPr><m:e>{}</m:e></m:d>",
                run("x")
            )),
            "$\\left.x\\right)$",
        ),
        (
            shown(&format!(
                "<m:func><m:fName>{}</m:fName><m:e><m:d><m:e>{}</m:e></m:d></m:e></m:func>",
                styled("p", "sin"),
                run("x")
            )),
            "$\\sin \\left(x\\right)$",
        ),
        (
            shown(&format!(
                "<m:limLow><m:e>{}</m:e><m:lim>{}</m:lim></m:limLow>",
                styled("p", "lim"),
                run("x→0")
            )),
            "$\\underset{x\\rightarrow 0}{\\lim }$",
        ),
        (
            shown(&format!(
                "<m:limUpp><m:e>{}</m:e><m:lim>{}</m:lim></m:limUpp>",
                run("x"),
                run("n")
            )),
            "$\\overset{n}{x}$",
        ),
        (
            shown(&format!(
                "<m:acc><m:accPr><m:chr m:val=\"̂\"/></m:accPr><m:e>{}</m:e></m:acc>",
                run("x")
            )),
            "$\\hat{x}$",
        ),
        (
            shown(&format!(
                "<m:acc><m:accPr><m:chr m:val=\"̃\"/></m:accPr><m:e>{}</m:e></m:acc>",
                run("x")
            )),
            "$\\tilde{x}$",
        ),
        (
            shown(&format!(
                "<m:acc><m:accPr><m:chr m:val=\"*\"/></m:accPr><m:e>{}</m:e></m:acc>",
                run("x")
            )),
            "$\\overset{*}{x}$",
        ),
        (
            shown(&format!("<m:bar><m:e>{}</m:e></m:bar>", run("x"))),
            "$\\overline{x}$",
        ),
        (
            shown(&format!(
                "<m:bar><m:barPr><m:pos m:val=\"bot\"/></m:barPr><m:e>{}</m:e></m:bar>",
                run("x")
            )),
            "$\\underline{x}$",
        ),
        (
            shown(&format!(
                "<m:groupChr><m:groupChrPr><m:chr m:val=\"⏞\"/><m:pos m:val=\"top\"/></m:groupChrPr><m:e>{}</m:e></m:groupChr>",
                run("abc")
            )),
            "$\\overbrace{abc}$",
        ),
        (
            shown(&format!(
                "<m:groupChr><m:groupChrPr><m:chr m:val=\"⏟\"/><m:pos m:val=\"bot\"/></m:groupChrPr><m:e>{}</m:e></m:groupChr>",
                run("abc")
            )),
            "$\\underbrace{abc}$",
        ),
        (
            shown(&format!(
                "<m:groupChr><m:groupChrPr><m:chr m:val=\"∗\"/><m:pos m:val=\"bot\"/></m:groupChrPr><m:e>{}</m:e></m:groupChr>",
                run("x")
            )),
            "$\\underset{\\ast }{x}$",
        ),
        (
            shown(&format!("<m:borderBox><m:e>{}</m:e></m:borderBox>", run("x"))),
            "$\\boxed{x}$",
        ),
        (
            shown(&format!(
                "<m:phant><m:phantPr><m:show m:val=\"0\"/></m:phantPr><m:e>{}</m:e></m:phant>",
                run("x")
            )),
            "$\\phantom{x}$",
        ),
        (
            shown(&format!(
                "<m:phant><m:phantPr><m:show m:val=\"0\"/><m:zeroWid m:val=\"1\"/></m:phantPr><m:e>{}</m:e></m:phant>",
                run("x")
            )),
            "$\\vphantom{x}$",
        ),
        (
            shown(&format!(
                "<m:phant><m:phantPr><m:show m:val=\"0\"/><m:zeroAsc m:val=\"1\"/><m:zeroDesc m:val=\"1\"/></m:phantPr><m:e>{}</m:e></m:phant>",
                run("x")
            )),
            "$\\hphantom{x}$",
        ),
        (
            shown(&format!("<m:box><m:e>{}</m:e></m:box>", run("q"))),
            "$q$",
        ),
        (
            shown(&format!(
                "<m:eqArr><m:e>{}<m:aln/>{}</m:e><m:e>{}</m:e></m:eqArr>",
                run("a"),
                run("b"),
                run("c")
            )),
            "$\\begin{aligned}a&b\\\\c\\end{aligned}$",
        ),
        (
            shown(&format!(
                "<m:m><m:mr><m:e>{}</m:e><m:e>{}</m:e></m:mr><m:mr><m:e>{}</m:e><m:e>{}</m:e></m:mr></m:m>",
                run("a"),
                run("b"),
                run("c"),
                run("d")
            )),
            "$\\begin{matrix}a&b\\\\c&d\\end{matrix}$",
        ),
        (shown(&styled("b", "v")), "$\\mathbf{v}$"),
        (shown(&styled("bi", "w")), "$\\boldsymbol{w}$"),
        (shown(&styled("p", "dx")), "$\\mathrm{dx}$"),
        (
            shown("<m:r><m:rPr><m:nor/></m:rPr><m:t xml:space=\"preserve\">a b</m:t></m:r>"),
            "$\\text{a b}$",
        ),
        (shown(&run("a_b")), "$a\\_b$"),
        (shown(&run("α")), "$\\alpha$"),
        (shown(&run("×")), "$\\times$"),
        (
            format!(
                "<w:p><w:r><w:t xml:space=\"preserve\">Area </w:t></w:r>{}<w:r><w:t xml:space=\"preserve\"> now</w:t></w:r></w:p>",
                om(&run("x"))
            ),
            "Area $x$ now",
        ),
        (
            format!(
                "<w:p><w:hyperlink r:id=\"rIdL\">{}</w:hyperlink></w:p>",
                om(&run("y"))
            ),
            "[$y$](https://example.com/m)",
        ),
        (
            format!(
                "<w:sdt><w:sdtContent><w:p>{}</w:p></w:sdtContent></w:sdt>",
                om(&run("k"))
            ),
            "$k$",
        ),
        (
            format!(
                "<w:tbl><w:tr><w:tc><w:p><w:r><w:t>Eq</w:t></w:r></w:p></w:tc></w:tr><w:tr><w:tc><w:p>{}</w:p></w:tc></w:tr></w:tbl>",
                om(&run("c"))
            ),
            "| $c$ |",
        ),
    ];
    let mut body = String::from(
        "<?xml version=\"1.0\"?><w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><w:body>",
    );
    for (block, _) in &cases {
        body.push_str(block);
    }
    body.push_str("<w:p><w:r><w:t>See</w:t></w:r><w:r><w:footnoteReference w:id=\"7\"/></w:r></w:p></w:body></w:document>");
    let footnotes = format!(
        "<?xml version=\"1.0\"?><w:footnotes xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:footnote w:type=\"separator\" w:id=\"-1\"><w:p><w:r><w:t>separator</w:t></w:r></w:p></w:footnote><w:footnote w:id=\"7\"><w:p>{}</w:p></w:footnote></w:footnotes>",
        om(&run("z"))
    );
    let bytes = package(
        &body,
        r#"<Relationship Id="rIdF" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/footnotes" Target="footnotes.xml"/><Relationship Id="rIdL" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink" Target="https://example.com/m" TargetMode="External"/>"#,
        &[("word/footnotes.xml", footnotes.as_bytes())],
    )?;
    let doc = Document::read_bytes(&bytes).map_err(err)?;
    let exported = doc.to_markdown().map_err(err)?;
    let markdown = exported.markdown.as_str();
    for (_, fragment) in &cases {
        if !markdown.contains(fragment) {
            return Err(format!("missing {fragment:?}\n\n{markdown}"));
        }
    }
    for fragment in ["See[^7]", "[^7]: $z$", "separator"] {
        let present = markdown.contains(fragment);
        if fragment == "separator" {
            if present {
                return Err(format!("separator leaked\n\n{markdown}"));
            }
        } else if !present {
            return Err(format!("missing {fragment:?}\n\n{markdown}"));
        }
    }
    let saved = doc.write_bytes().map_err(err)?;
    let package = edgedoc_opc::read_zip(&saved, &edgedoc::Budget::default()).map_err(err)?;
    let document = package
        .part("word/document.xml")
        .ok_or("the saved package has no document")?;
    let xml = String::from_utf8_lossy(&document.bytes);
    for marker in ["<m:f", "<m:nary", "<m:rad", "xmlns:m="] {
        if !xml.contains(marker) {
            return Err(format!("saved document lost {marker}"));
        }
    }
    Ok(())
}

/// Office Math edges: alphabets, argument size, alignment, breaks, borders,
/// phantoms, and formulas that sit in a list, a cell, or next to each other.
#[test]
fn office_math_edge_cases() -> Test {
    fn run(text: &str) -> String {
        format!("<m:r><m:t xml:space=\"preserve\">{text}</m:t></m:r>")
    }
    fn om(inner: &str) -> String {
        format!(
            "<m:oMath xmlns:m=\"http://schemas.openxmlformats.org/officeDocument/2006/math\">{inner}</m:oMath>"
        )
    }
    fn para(inner: &str) -> String {
        format!("<w:p>{inner}</w:p>")
    }
    fn shown(inner: &str) -> String {
        para(&om(inner))
    }
    fn alphabet(script: &str, style: &str, text: &str) -> String {
        let style = if style.is_empty() {
            String::new()
        } else {
            format!("<m:sty m:val=\"{style}\"/>")
        };
        let script = if script.is_empty() {
            String::new()
        } else {
            format!("<m:scr m:val=\"{script}\"/>")
        };
        format!("<m:r><m:rPr>{style}{script}</m:rPr><m:t xml:space=\"preserve\">{text}</m:t></m:r>")
    }
    fn frac(num: &str, den: &str) -> String {
        format!("<m:f><m:num>{num}</m:num><m:den>{den}</m:den></m:f>")
    }
    fn sized(size: &str, num: &str, den: &str) -> String {
        format!(
            "<m:f><m:num><m:argPr><m:argSz m:val=\"{size}\"/></m:argPr>{}</m:num><m:den>{}</m:den></m:f>",
            run(num),
            run(den)
        )
    }
    fn nary(chr: Option<&str>, flags: &str, sub: &str, sup: &str, body: &str) -> String {
        let chr = match chr {
            None => String::new(),
            Some(value) => format!("<m:chr m:val=\"{value}\"/>"),
        };
        let sub = if sub.is_empty() {
            String::new()
        } else {
            format!("<m:sub>{}</m:sub>", run(sub))
        };
        let sup = if sup.is_empty() {
            String::new()
        } else {
            format!("<m:sup>{}</m:sup>", run(sup))
        };
        format!(
            "<m:nary><m:naryPr>{chr}{flags}</m:naryPr>{sub}{sup}<m:e>{}</m:e></m:nary>",
            run(body)
        )
    }
    fn display(jc: &str, inner: &str) -> String {
        let pr = if jc.is_empty() {
            String::new()
        } else {
            format!("<m:oMathParaPr><m:jc m:val=\"{jc}\"/></m:oMathParaPr>")
        };
        para(&format!(
            "<m:oMathPara xmlns:m=\"http://schemas.openxmlformats.org/officeDocument/2006/math\">{pr}<m:oMath>{inner}</m:oMath></m:oMathPara>"
        ))
    }
    fn border(flags: &str, body: &str) -> String {
        let pr = if flags.is_empty() {
            String::new()
        } else {
            format!("<m:borderBoxPr>{flags}</m:borderBoxPr>")
        };
        format!("<m:borderBox>{pr}<m:e>{}</m:e></m:borderBox>", run(body))
    }
    fn phant(flags: &str, body: &str) -> String {
        format!(
            "<m:phant><m:phantPr>{flags}</m:phantPr><m:e>{}</m:e></m:phant>",
            run(body)
        )
    }
    fn delim(pr: &str, args: &[&str]) -> String {
        let pr = if pr.is_empty() {
            String::new()
        } else {
            format!("<m:dPr>{pr}</m:dPr>")
        };
        let args = args.iter().fold(String::new(), |mut xml, arg| {
            xml.push_str("<m:e>");
            xml.push_str(&run(arg));
            xml.push_str("</m:e>");
            xml
        });
        format!("<m:d>{pr}{args}</m:d>")
    }
    let cases = vec![
        (shown(&alphabet("double-struck", "", "R")), "$\\mathbb{R}$"),
        (shown(&alphabet("script", "", "L")), "$\\mathcal{L}$"),
        (shown(&alphabet("fraktur", "", "g")), "$\\mathfrak{g}$"),
        (shown(&alphabet("sans-serif", "", "A")), "$\\mathsf{A}$"),
        (shown(&alphabet("monospace", "", "n")), "$\\mathtt{n}$"),
        (
            shown(&alphabet("script", "b", "L")),
            "$\\boldsymbol{\\mathcal{L}}$",
        ),
        (
            shown(&alphabet("fraktur", "bi", "h")),
            "$\\boldsymbol{\\mathfrak{h}}$",
        ),
        (shown(&alphabet("script", "p", "sin")), "$\\mathcal{sin}$"),
        (shown(&alphabet("", "b", "sin")), "$\\mathbf{sin}$"),
        (shown(&alphabet("", "p", "Sin")), "$\\mathrm{Sin}$"),
        (
            shown(&format!("{}{}", alphabet("", "p", "sin"), run("x"))),
            "$\\sin x$",
        ),
        (
            shown("<m:r><m:rPr><m:lit/></m:rPr><m:t xml:space=\"preserve\">×</m:t></m:r>"),
            "$\\text{×}$",
        ),
        (
            shown(
                "<m:r><m:rPr><m:nor m:val=\"0\"/></m:rPr><m:t xml:space=\"preserve\">×</m:t></m:r>",
            ),
            "$\\times$",
        ),
        (shown(&sized("-1", "x", "y")), "$\\frac{{\\scriptstyle x}}{y}$"),
        (
            shown(&sized("-2", "u", "v")),
            "$\\frac{{\\scriptscriptstyle u}}{v}$",
        ),
        (shown(&sized("1", "p", "q")), "$\\frac{{\\textstyle p}}{q}$"),
        (
            shown(&sized("2", "r", "s")),
            "$\\frac{{\\displaystyle r}}{s}$",
        ),
        (shown(&sized("0", "m7", "n7")), "$\\frac{m7}{n7}$"),
        (shown(&sized("abc", "m8", "n8")), "$\\frac{m8}{n8}$"),
        (
            shown("<m:r><m:t>a</m:t><m:brk/><m:t>b</m:t></m:r>"),
            "$a\\\\b$",
        ),
        (
            shown(&format!(
                "<m:box><m:e>{}</m:e><m:brk/><m:e>{}</m:e></m:box>",
                run("p"),
                run("q")
            )),
            "$p\\\\q$",
        ),
        (
            shown(&format!(
                "<m:eqArr><m:e><m:r><m:t>a</m:t><m:aln/><m:t>b</m:t><m:brk/><m:t>c</m:t></m:r></m:e><m:e>{}</m:e></m:eqArr>",
                run("d")
            )),
            "$\\begin{aligned}a&b\\\\c\\\\d\\end{aligned}$",
        ),
        (
            shown(&format!(
                "<m:sPre><m:sub></m:sub><m:sup></m:sup><m:e>{}</m:e></m:sPre>",
                run("pre")
            )),
            "$pre$",
        ),
        (
            shown(&format!(
                "<m:sPre><m:sub>{}</m:sub><m:sup></m:sup><m:e>{}</m:e></m:sPre>",
                run("n"),
                run("C")
            )),
            "${}_{n}{C}$",
        ),
        (
            shown(&format!(
                "<m:sSup><m:e></m:e><m:sup>{}</m:sup></m:sSup>",
                run("2")
            )),
            "${}^{2}$",
        ),
        (
            shown(&nary(
                Some("∫"),
                "<m:subHide m:val=\"1\"/><m:supHide m:val=\"1\"/>",
                "0",
                "1",
                "x",
            )),
            "$\\int{x}$",
        ),
        (
            shown(&nary(Some("∫"), "<m:subHide m:val=\"1\"/>", "0", "1", "x")),
            "$\\int^{1}{x}$",
        ),
        (
            shown(&nary(Some("∫"), "<m:supHide m:val=\"1\"/>", "0", "1", "x")),
            "$\\int_{0}{x}$",
        ),
        (
            shown(&nary(Some(""), "", "i", "", "i")),
            "$_{i}{i}$",
        ),
        (
            shown(&nary(Some("∑"), "<m:grow m:val=\"0\"/>", "i", "", "i")),
            "${\\textstyle \\sum_{i}{i}}$",
        ),
        (
            shown(&nary(
                Some("∑"),
                "<m:limLoc m:val=\"subSup\"/>",
                "i",
                "n",
                "i",
            )),
            "$\\sum_{i}^{n}{i}$",
        ),
        (
            shown(&nary(Some("∬"), "", "0", "1", "x")),
            "$\\iint_{0}^{1}{x}$",
        ),
        (shown(&nary(Some("∮"), "", "", "", "y")), "$\\oint{y}$"),
        (
            shown(&format!(
                "<m:rad><m:radPr><m:degHide m:val=\"0\"/></m:radPr><m:deg>{}</m:deg><m:e>{}</m:e></m:rad>",
                run("5"),
                run("y")
            )),
            "$\\sqrt[5]{y}$",
        ),
        (
            shown(&format!(
                "<m:rad><m:deg></m:deg><m:e>{}</m:e></m:rad>",
                run("w")
            )),
            "$\\sqrt{w}$",
        ),
        (
            shown(&frac(
                &frac(&run("a"), &run("b")),
                &run("c"),
            )),
            "$\\frac{\\frac{a}{b}}{c}$",
        ),
        (
            shown(&format!(
                "<m:m><m:mPr><m:mcs><m:mc><m:mcPr><m:mcJc m:val=\"right\"/></m:mcPr></m:mc><m:mc><m:mcPr><m:mcJc m:val=\"left\"/></m:mcPr></m:mc></m:mcs></m:mPr><m:mr><m:e>{}</m:e><m:e>{}</m:e></m:mr></m:m>",
                run("a"),
                run("b")
            )),
            "$\\begin{array}{rl}a&b\\end{array}$",
        ),
        (
            shown(&format!(
                "<m:m><m:mPr><m:mcs><m:mc><m:mcPr><m:count m:val=\"2\"/><m:mcJc m:val=\"left\"/></m:mcPr></m:mc></m:mcs></m:mPr><m:mr><m:e>{}</m:e></m:mr></m:m>",
                run("a")
            )),
            "$\\begin{array}{ll}a&\\end{array}$",
        ),
        (
            shown(&format!(
                "<m:m><m:mPr><m:mcs><m:mc><m:mcPr><m:count m:val=\"0\"/><m:mcJc m:val=\"left\"/></m:mcPr></m:mc></m:mcs></m:mPr><m:mr><m:e>{}</m:e></m:mr></m:m>",
                run("z")
            )),
            "$\\begin{array}{l}z\\end{array}$",
        ),
        (
            shown(&format!(
                "<m:m><m:mPr><m:mcs><m:mc><m:mcPr><m:mcJc m:val=\"center\"/></m:mcPr></m:mc></m:mcs></m:mPr><m:mr><m:e>{}</m:e></m:mr></m:m>",
                run("a")
            )),
            "$\\begin{matrix}a\\end{matrix}$",
        ),
        (
            shown(&format!(
                "<m:m><m:mr><m:e>{}</m:e><m:e></m:e><m:e>{}</m:e></m:mr></m:m>",
                run("a"),
                run("c")
            )),
            "$\\begin{matrix}a&&c\\end{matrix}$",
        ),
        (
            shown(&format!(
                "<m:d><m:e><m:m><m:mr><m:e>{}</m:e><m:e>{}</m:e></m:mr><m:mr><m:e>{}</m:e><m:e>{}</m:e></m:mr></m:m></m:e></m:d>",
                run("a"),
                run("b"),
                run("c"),
                run("d")
            )),
            "$\\left(\\begin{matrix}a&b\\\\c&d\\end{matrix}\\right)$",
        ),
        (
            shown(&delim(
                "<m:begChr m:val=\"|\"/><m:endChr m:val=\"|\"/>",
                &["v"],
            )),
            "$\\left|v\\right|$",
        ),
        (
            shown(&delim(
                "<m:begChr m:val=\"‖\"/><m:endChr m:val=\"‖\"/>",
                &["w2"],
            )),
            "$\\left\\|w2\\right\\|$",
        ),
        (
            shown(&delim("<m:sepChr m:val=\"|\"/>", &["a", "b", "c"])),
            "$\\left(a|b|c\\right)$",
        ),
        (
            shown(&delim("<m:sepChr m:val=\"\"/>", &["a", "b", "c"])),
            "$\\left(abc\\right)$",
        ),
        (shown(&delim("<m:grow/>", &["q"])), "$\\left(q\\right)$"),
        (shown(&delim("<m:grow m:val=\"0\"/>", &["g"])), "$(g)$"),
        (shown(&delim("<m:begChr/>", &["x"])), "$\\left.x\\right)$"),
        (
            shown(&delim("<m:begChr m:val=\"\"/>", &["e2"])),
            "$\\left.e2\\right)$",
        ),
        (
            shown(&delim(
                "<m:begChr m:val=\"⟨\"/><m:endChr m:val=\"⟩\"/>",
                &["v"],
            )),
            "$\\left\\langle v\\right\\rangle$",
        ),
        (
            shown(&delim(
                "<m:begChr m:val=\"⌊\"/><m:endChr m:val=\"⌋\"/>",
                &["v"],
            )),
            "$\\left\\lfloor v\\right\\rfloor$",
        ),
        (
            shown(&delim(
                "<m:begChr m:val=\"⌈\"/><m:endChr m:val=\"⌉\"/>",
                &["v"],
            )),
            "$\\left\\lceil v\\right\\rceil$",
        ),
        (
            display("left", &frac(&run("a"), &run("b"))),
            "$$\n\\begin{aligned}&\\frac{a}{b}\\end{aligned}\n$$",
        ),
        (
            display("right", &frac(&run("c"), &run("d"))),
            "$$\n\\begin{aligned}\\frac{c}{d}&\\end{aligned}\n$$",
        ),
        (shown(&run("a^b")), "$a\\^{}b$"),
        (shown(&run("a&amp;b")), "$a\\&b$"),
        (shown(&run("a\\b")), "$a\\backslash b$"),
        (shown(&run("\u{1D400}")), "$\\mathbf{A}$"),
        (shown(&run("\u{1D45E}")), "$q$"),
        (shown(&run("\u{1D5A1}")), "$\\mathsf{B}$"),
        (shown(&run("\u{1D672}")), "$\\mathtt{C}$"),
        (shown(&run("\u{1D482}")), "$\\boldsymbol{a}$"),
        (shown(&run("ℕ")), "$\\mathbb{N}$"),
        (shown(&run("ℓ")), "$\\ell$"),
        (shown(&run("ℏ")), "$\\hbar$"),
        (shown(&run("℘")), "$\\wp$"),
        (shown(&run("ℑ")), "$\\Im$"),
        (shown(&run("ℜ")), "$\\Re$"),
        (shown(&run("∴")), "$\\therefore$"),
        (shown(&run("∵")), "$\\because$"),
        (shown(&run("≪")), "$\\ll$"),
        (shown(&run("≫")), "$\\gg$"),
        (shown(&run("x″")), "$x''$"),
        (shown(&run("90°")), "$90^{\\circ}$"),
        (
            shown(&run("±∓÷·∞∂∀∃∅⊕⊗⊥∠…←↔⇒")),
            "$\\pm \\mp \\div \\cdot \\infty \\partial \\forall \\exists \\emptyset \\oplus \\otimes \\perp \\angle \\ldots \\leftarrow \\leftrightarrow \\Rightarrow$",
        ),
        (shown(&run("εϵ")), "$\\varepsilon \\epsilon$"),
        (
            shown(&phant("<m:show m:val=\"0\"/><m:zeroAsc m:val=\"1\"/>", "xa")),
            "$\\smash[t]{\\phantom{xa}}$",
        ),
        (
            shown(&phant(
                "<m:show m:val=\"0\"/><m:zeroWid m:val=\"1\"/><m:zeroAsc m:val=\"1\"/>",
                "x",
            )),
            "$\\smash[t]{\\vphantom{x}}$",
        ),
        (
            shown(&phant(
                "<m:show m:val=\"0\"/><m:zeroWid m:val=\"1\"/><m:zeroDesc m:val=\"1\"/>",
                "x",
            )),
            "$\\smash[b]{\\vphantom{x}}$",
        ),
        (
            shown(&phant("<m:show m:val=\"0\"/><m:zeroDesc m:val=\"1\"/>", "x")),
            "$\\smash[b]{\\phantom{x}}$",
        ),
        (
            shown(&phant("<m:show m:val=\"0\"/><m:zeroWid m:val=\"1\"/>", "h")),
            "$\\vphantom{h}$",
        ),
        (
            shown(&phant("<m:show m:val=\"0\"/>", "ph")),
            "$\\phantom{ph}$",
        ),
        (
            shown(&phant(
                "<m:show m:val=\"1\"/><m:zeroWid m:val=\"1\"/>",
                "see",
            )),
            "$see$",
        ),
        (
            shown(&border(
                "<m:hideTop m:val=\"1\"/><m:hideBot m:val=\"1\"/><m:hideLeft m:val=\"1\"/><m:hideRight m:val=\"1\"/>",
                "bare",
            )),
            "$bare$",
        ),
        (
            shown(&border("<m:hideLeft m:val=\"1\"/>", "side")),
            "$\\boxed{side}$",
        ),
        (
            shown(&border(
                "<m:hideBot m:val=\"1\"/><m:hideLeft m:val=\"1\"/><m:hideRight m:val=\"1\"/>",
                "top",
            )),
            "$\\overline{top}$",
        ),
        (
            shown(&border(
                "<m:hideTop m:val=\"1\"/><m:hideLeft m:val=\"1\"/><m:hideRight m:val=\"1\"/>",
                "bot",
            )),
            "$\\underline{bot}$",
        ),
        (
            shown(&border(
                "<m:hideLeft m:val=\"1\"/><m:hideRight m:val=\"1\"/>",
                "tb",
            )),
            "$\\overline{\\underline{tb}}$",
        ),
        (
            shown(&border(
                "<m:hideTop m:val=\"1\"/><m:hideBot m:val=\"1\"/><m:hideLeft m:val=\"1\"/><m:hideRight m:val=\"1\"/><m:strikeTLBR m:val=\"1\"/>",
                "s1",
            )),
            "$\\cancel{s1}$",
        ),
        (
            shown(&border(
                "<m:hideTop m:val=\"1\"/><m:hideBot m:val=\"1\"/><m:hideLeft m:val=\"1\"/><m:hideRight m:val=\"1\"/><m:strikeBLTR m:val=\"1\"/>",
                "s2",
            )),
            "$\\bcancel{s2}$",
        ),
        (
            shown(&border(
                "<m:hideTop m:val=\"1\"/><m:hideBot m:val=\"1\"/><m:hideLeft m:val=\"1\"/><m:hideRight m:val=\"1\"/><m:strikeTLBR m:val=\"1\"/><m:strikeBLTR m:val=\"1\"/>",
                "s3",
            )),
            "$\\xcancel{s3}$",
        ),
        (
            shown(&border(
                "<m:hideTop m:val=\"1\"/><m:hideBot m:val=\"1\"/><m:hideLeft m:val=\"1\"/><m:hideRight m:val=\"1\"/><m:strikeH m:val=\"1\"/>",
                "sh",
            )),
            "$\\overline{\\underline{sh}}$",
        ),
        (
            shown(&border(
                "<m:hideTop m:val=\"1\"/><m:hideBot m:val=\"1\"/><m:hideLeft m:val=\"1\"/><m:hideRight m:val=\"1\"/><m:strikeV m:val=\"1\"/>",
                "sv",
            )),
            "$\\left.sv\\middle|\\right.$",
        ),
        (
            shown(&border("<m:strikeTLBR m:val=\"1\"/>", "bx")),
            "$\\cancel{\\boxed{bx}}$",
        ),
        (shown("<m:custom><m:r><m:t>z9</m:t></m:r></m:custom>"), "$z9$"),
    ];
    let mut body = String::from(
        "<?xml version=\"1.0\"?><w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><w:body>",
    );
    for (block, _) in &cases {
        body.push_str(block);
    }
    body.push_str(&format!(
        "<w:p><w:r><w:t>Keep</w:t></w:r>{}<w:r><w:t>End</w:t></w:r></w:p>",
        om(&phant(
            "<m:show m:val=\"0\"/><m:zeroWid m:val=\"1\"/><m:zeroAsc m:val=\"1\"/><m:zeroDesc m:val=\"1\"/>",
            "x"
        ))
    ));
    body.push_str(&format!(
        "<w:p><w:r><w:t xml:space=\"preserve\">Hello </w:t></w:r>{}<w:r><w:t>there</w:t></w:r></w:p>",
        om("")
    ));
    body.push_str(&format!(
        "<w:p><w:r><w:t xml:space=\"preserve\">Pad </w:t></w:r>{}<w:r><w:t>side</w:t></w:r></w:p>",
        om(&run("   "))
    ));
    body.push_str(&para(&format!("{}{}", om(&run("aa")), om(&run("bb")))));
    body.push_str("<w:p><w:r><w:t>BEFORE</w:t></w:r></w:p>");
    body.push_str(&display("", ""));
    body.push_str("<w:p><w:r><w:t>AFTER</w:t></w:r></w:p>");
    body.push_str(&format!(
        "<w:p><w:pPr><w:numPr><w:ilvl w:val=\"0\"/><w:numId w:val=\"1\"/></w:numPr></w:pPr><m:oMathPara xmlns:m=\"http://schemas.openxmlformats.org/officeDocument/2006/math\"><m:oMath>{}</m:oMath></m:oMathPara></w:p>",
        frac(&run("1"), &run("2"))
    ));
    body.push_str(&format!(
        "<w:p><w:r><w:t xml:space=\"preserve\">in </w:t></w:r><w:sdt><w:sdtContent>{}</w:sdtContent></w:sdt></w:p>",
        om(&run("k"))
    ));
    body.push_str(&format!(
        "<w:p><w:r><w:t xml:space=\"preserve\">new </w:t></w:r><w:ins w:id=\"1\" w:author=\"Ada\">{}</w:ins></w:p>",
        om(&run("w"))
    ));
    body.push_str(&format!(
        "<w:p><w:r><w:t xml:space=\"preserve\">Eq </w:t></w:r><m:oMathPara xmlns:m=\"http://schemas.openxmlformats.org/officeDocument/2006/math\"><m:oMath>{}</m:oMath></m:oMathPara></w:p>",
        frac(&run("9"), &run("8"))
    ));
    body.push_str(
        "<w:p><w:r><w:t>End</w:t></w:r><w:r><w:endnoteReference w:id=\"3\"/></w:r></w:p>",
    );
    body.push_str(
        "<w:p><w:r><w:t>Noted</w:t></w:r><w:r><w:commentReference w:id=\"1\"/></w:r></w:p>",
    );
    body.push_str(&format!(
        "<w:tbl><w:tr><w:tc><w:p><w:r><w:t>Cell</w:t></w:r></w:p></w:tc></w:tr><w:tr><w:tc><w:p><m:oMathPara xmlns:m=\"http://schemas.openxmlformats.org/officeDocument/2006/math\"><m:oMath>{}</m:oMath></m:oMathPara></w:p></w:tc></w:tr></w:tbl>",
        frac(&run("3"), &run("4"))
    ));
    body.push_str(&format!(
        "<w:tbl><w:tr><w:tc><w:p><w:r><w:t>Nest</w:t></w:r></w:p></w:tc></w:tr><w:tr><w:tc><w:tbl><w:tr><w:tc><w:p>{}</w:p></w:tc></w:tr><w:tr><w:tc><w:p><w:r><w:t>Eq</w:t></w:r></w:p></w:tc></w:tr></w:tbl></w:tc></w:tr></w:tbl>",
        om(&delim(
            "<m:begChr m:val=\"|\"/><m:endChr m:val=\"|\"/>",
            &["n"]
        ))
    ));
    body.push_str("<w:sectPr><w:headerReference w:type=\"default\" r:id=\"rIdH\"/><w:footerReference w:type=\"default\" r:id=\"rIdFoot\"/></w:sectPr></w:body></w:document>");
    let numbering = r#"<?xml version="1.0"?><w:numbering xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:abstractNum w:abstractNumId="1"><w:lvl w:ilvl="0"><w:numFmt w:val="bullet"/></w:lvl></w:abstractNum><w:num w:numId="1"><w:abstractNumId w:val="1"/></w:num></w:numbering>"#;
    let endnotes = format!(
        "<?xml version=\"1.0\"?><w:endnotes xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:endnote w:id=\"3\"><w:p>{}</w:p></w:endnote></w:endnotes>",
        om(&run("e"))
    );
    let header = format!(
        "<?xml version=\"1.0\"?><w:hdr xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:p>{}</w:p></w:hdr>",
        om(&run("HEADERONLY"))
    );
    let footer = format!(
        "<?xml version=\"1.0\"?><w:ftr xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:p>{}</w:p></w:ftr>",
        om(&run("FOOTERONLY"))
    );
    let comments = format!(
        "<?xml version=\"1.0\"?><w:comments xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:comment w:id=\"1\" w:author=\"Ada\"><w:p>{}</w:p></w:comment></w:comments>",
        om(&run("COMMENTONLY"))
    );
    let bytes = package(
        &body,
        r#"<Relationship Id="rIdN" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering" Target="numbering.xml"/><Relationship Id="rIdE" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/endnotes" Target="endnotes.xml"/><Relationship Id="rIdH" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/header" Target="header1.xml"/><Relationship Id="rIdFoot" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/footer" Target="footer1.xml"/><Relationship Id="rIdC" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments" Target="comments.xml"/>"#,
        &[
            ("word/numbering.xml", numbering.as_bytes()),
            ("word/endnotes.xml", endnotes.as_bytes()),
            ("word/header1.xml", header.as_bytes()),
            ("word/footer1.xml", footer.as_bytes()),
            ("word/comments.xml", comments.as_bytes()),
        ],
    )?;
    let doc = Document::read_bytes(&bytes).map_err(err)?;
    let exported = doc.to_markdown().map_err(err)?;
    let markdown = exported.markdown.as_str();
    for (_, fragment) in &cases {
        if !markdown.contains(fragment) {
            return Err(format!("missing {fragment:?}\n\n{markdown}"));
        }
    }
    for fragment in [
        "KeepEnd",
        "Hello there",
        "Pad side",
        "$aa$ $bb$",
        "BEFORE\n\nAFTER",
        "- $$\\frac{1}{2}$$",
        "in $k$",
        "new $w$",
        "Eq \n\n$$\n\\frac{9}{8}\n$$",
        "End[^end-3]",
        "[^end-3]: $e$",
        "Noted",
        "| $$\\frac{3}{4}$$ |",
        "\\| $\\left|n\\right|$ \\|<br>\\| --- \\|<br>\\| Eq \\|",
    ] {
        if !markdown.contains(fragment) {
            return Err(format!("missing {fragment:?}\n\n{markdown}"));
        }
    }
    for absent in ["HEADERONLY", "FOOTERONLY", "COMMENTONLY", "$$$$"] {
        if markdown.contains(absent) {
            return Err(format!("{absent} leaked\n\n{markdown}"));
        }
    }
    if markdown.contains("\\limits") && markdown.contains("\\sum_{i}^{n}") {
        return Err(format!(
            "an explicit side limit gained \\limits\n\n{markdown}"
        ));
    }
    let saved = doc.write_bytes().map_err(err)?;
    let package = edgedoc_opc::read_zip(&saved, &edgedoc::Budget::default()).map_err(err)?;
    let document = package
        .part("word/document.xml")
        .ok_or("the saved package has no document")?;
    let xml = String::from_utf8_lossy(&document.bytes);
    for marker in [
        "<m:scr",
        "<m:argSz",
        "<m:mcs",
        "<m:brk",
        "<m:aln",
        "<m:borderBox",
        "xmlns:m=",
    ] {
        if !xml.contains(marker) {
            return Err(format!("saved document lost {marker}"));
        }
    }
    Ok(())
}

/// Markdown comes back as a document, and that document exports the same Markdown.
#[test]
fn markdown_reads_back_into_a_document() -> Test {
    let doc = sample()?;
    let exported = doc.to_markdown().map_err(err)?;
    let options = MarkdownReadOptions::new().images(exported.images.iter().map(|image| {
        MarkdownImage::new(&image.markdown_path, image.bytes.clone())
            .content_type(image.content_type.clone())
    }));
    let read = Document::from_markdown_with(&exported.markdown, &options).map_err(err)?;
    let again = read.to_markdown().map_err(err)?;
    if again.markdown != exported.markdown {
        return Err(format!(
            "round trip\n{}\n---\n{}",
            again.markdown, exported.markdown
        ));
    }
    let saved = read.write_bytes().map_err(err)?;
    let reread = Document::read_bytes(&saved).map_err(err)?;
    let stored = reread.to_markdown().map_err(err)?;
    for fragment in [
        "# Overview",
        "- Apples",
        "| Pipe \\| here | 2 |",
        "![Chart]",
        "![Photo]",
    ] {
        if !stored.markdown.contains(fragment) {
            return Err(format!("saved missing {fragment:?}\n\n{}", stored.markdown));
        }
    }
    let png = stored
        .images
        .iter()
        .any(|image| image.bytes == b"PNG-BYTES");
    let jpeg = stored
        .images
        .iter()
        .any(|image| image.bytes == b"JPEG-BYTES");
    if !png || !jpeg {
        return Err("saved images lost their bytes".into());
    }

    let source = "\
# Title

Setext
=====

Hello **bold** *lean* ~~gone~~ <sub>2</sub><sup>3</sup>

A line  
break

[Site](https://example.com/a(b))

- Apples
- Pears

1. First
  1. Nested

| Name | Note |
| --- | --- |
| Pipe \\| here | $x+y$ |

Before

---

After

$$
\\frac{a}{b}
$$

See[^4] and more[^end-3].

[^4]: A note

[^end-3]: An end
";
    let rich = Document::from_markdown(source).map_err(err)?;
    let rich_md = rich.to_markdown().map_err(err)?;
    for fragment in [
        "# Title",
        "# Setext",
        "Hello **bold** *lean* ~~gone~~ <sub>2</sub><sup>3</sup>",
        "A line  \nbreak",
        "[Site](https://example.com/a%28b))",
        "- Apples",
        "- Pears",
        "1. First",
        "  1. Nested",
        "| Pipe \\| here | $x+y$ |",
        "---",
        "$$\n\\frac{a}{b}\n$$",
        "See[^4]",
        "[^4]: A note",
        "[^end-3]: An end",
    ] {
        if !rich_md.markdown.contains(fragment) {
            return Err(format!("missing {fragment:?}\n\n{}", rich_md.markdown));
        }
    }
    let saved = rich.write_bytes().map_err(err)?;
    let reread = Document::read_bytes(&saved).map_err(err)?;
    let after = reread.to_markdown().map_err(err)?;
    for fragment in [
        "# Title",
        "- Apples",
        "| Pipe \\| here | $x+y$ |",
        "$$\n\\frac{a}{b}\n$$",
        "[^4]: A note",
    ] {
        if !after.markdown.contains(fragment) {
            return Err(format!("saved missing {fragment:?}\n\n{}", after.markdown));
        }
    }

    let dir = scratch("read-md");
    let image = dir.join("images");
    std::fs::create_dir_all(&image).map_err(err)?;
    std::fs::write(image.join("dot.png"), b"PNG-BYTES").map_err(err)?;
    std::fs::write(dir.join("note.md"), "Look ![Dot](images/dot.png)\n").map_err(err)?;
    let opened = Document::open_markdown(dir.join("note.md")).map_err(err)?;
    let opened_md = opened.to_markdown().map_err(err)?;
    if opened_md.markdown != "Look ![Dot](images/dot.png)\n" {
        return Err(opened_md.markdown);
    }
    if opened_md.images[0].bytes != b"PNG-BYTES" {
        return Err("the file image was not read".into());
    }

    match Document::from_markdown("![Gone](images/missing.png)") {
        Err(failure) if failure.code == "markdown.image_missing" => {}
        Err(failure) => return Err(failure.to_string()),
        Ok(_) => return Err("a missing image was accepted".into()),
    }
    match Document::from_markdown_with(
        "![Gone](../secret.png)",
        &MarkdownReadOptions::new().image_dir(&dir),
    ) {
        Err(failure) if failure.code == "markdown.image_dir" => {}
        Err(failure) => return Err(failure.to_string()),
        Ok(_) => return Err("a path outside the image directory was accepted".into()),
    }
    let opened_docx = edgedoc_py::open_markdown_file(&dir.join("note.md")).map_err(err)?;
    if !opened_docx.starts_with(b"PK") {
        return Err("python file read did not return a package".into());
    }
    let _ = std::fs::remove_dir_all(&dir);

    let docx = edgedoc_py::markdown_to_docx("# Hello\n\n**Ada**\n", &[]).map_err(err)?;
    if !docx.starts_with(b"PK") {
        return Err("python adapter did not return a package".into());
    }
    let text =
        edgedoc_wasm::markdown_text("# Hello\n\n**Ada**\n").map_err(|failure| failure.render())?;
    if text != "Hello\nAda" {
        return Err(text);
    }
    let packed = edgedoc_wasm::markdown_to_document_with_images(
        "![Dot](images/dot.png)\n",
        &["images/dot.png".to_string()],
        b"PNG-BYTES",
        &[9],
    )
    .map_err(|failure| failure.render())?;
    if !packed.starts_with(b"PK") {
        return Err("wasm image read did not return a package".into());
    }
    let packed_doc = Document::read_bytes(&packed).map_err(err)?;
    let packed_md = packed_doc.to_markdown().map_err(err)?;
    if packed_md.images.first().map(|image| image.bytes.as_slice()) != Some(b"PNG-BYTES".as_slice())
    {
        return Err("wasm image bytes were dropped".into());
    }
    match edgedoc_wasm::markdown_to_document_with_images("![Dot](x.png)", &[], b"", &[1]) {
        Err(failure) if failure.render().starts_with("markdown.image_bytes") => {}
        Err(failure) => return Err(failure.render()),
        Ok(_) => return Err("a short image buffer was accepted".into()),
    }
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
