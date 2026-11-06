//! Synthetic OOXML cases that exercise the entire reader → layout → paint pipeline.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::io::{Cursor, Read, Write};

use edgedoc_display::{DisplayDocument, OpenOptions};
use edgedoc_flow::{Fragment, ParagraphFragment};

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

fn package(body: &str, section: &str, settings: &str, headers: &[(&str, &str)]) -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let mut put = |name: &str, text: String| {
        zip.start_file(name, zip::write::FileOptions::default())
            .unwrap();
        zip.write_all(text.as_bytes()).unwrap();
    };
    put("[Content_Types].xml", format!(r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>{}</Types>"#, headers.iter().map(|(id, _)| format!(r#"<Override PartName="/word/{id}.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.{}+xml"/>"#, if id.starts_with("footer") { "footer" } else { "header" })).collect::<String>()));
    put(
        "_rels/.rels",
        format!(
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="doc" Type="{R}/officeDocument" Target="word/document.xml"/></Relationships>"#
        ),
    );
    put(
        "word/document.xml",
        format!(
            r#"<w:document xmlns:w="{W}" xmlns:r="{R}"><w:body>{body}<w:sectPr>{section}</w:sectPr></w:body></w:document>"#
        ),
    );
    put(
        "word/settings.xml",
        format!(r#"<w:settings xmlns:w="{W}">{settings}</w:settings>"#),
    );
    put(
        "word/_rels/document.xml.rels",
        format!(
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="settings" Type="{R}/settings" Target="settings.xml"/>{}</Relationships>"#,
            headers
                .iter()
                .map(|(id, _)| format!(
                    r#"<Relationship Id="{id}" Type="{R}/{}" Target="{id}.xml"/>"#,
                    if id.starts_with("footer") {
                        "footer"
                    } else {
                        "header"
                    }
                ))
                .collect::<String>()
        ),
    );
    for (id, contents) in headers {
        put(
            &format!("word/{id}.xml"),
            format!(
                r#"<w:{tag} xmlns:w="{W}">{contents}</w:{tag}>"#,
                tag = if id.starts_with("footer") {
                    "ftr"
                } else {
                    "hdr"
                }
            ),
        );
    }
    zip.finish().unwrap().into_inner()
}

fn render(body: &str, section: &str, settings: &str, headers: &[(&str, &str)]) -> DisplayDocument {
    let mut doc = open_document(
        &package(body, section, settings, headers),
        &OpenOptions::default(),
    )
    .unwrap();
    doc.layout().unwrap();
    doc
}

fn p(text: &str) -> String {
    format!("<w:p><w:r><w:t>{text}</w:t></w:r></w:p>")
}

#[test]
fn numbered_header_and_footer_stories_follow_the_paginated_body() {
    fn labels(fragments: &[Fragment], out: &mut Vec<String>) {
        for fragment in fragments {
            match fragment {
                Fragment::Paragraph(paragraph) => {
                    if let Some((label, _)) = &paragraph.label {
                        out.push(label.clone());
                    }
                }
                Fragment::Table(table) => labels(&table.content, out),
                _ => {}
            }
        }
    }
    for (bytes, expected) in [
        (
            include_bytes!("fixtures/numbered-stories/header-table.docx").as_slice(),
            [vec!["5.", "2.", "3."], vec!["6.", "4."]],
        ),
        (
            include_bytes!("fixtures/numbered-stories/footer.docx").as_slice(),
            [vec!["6.", "2.", "3."], vec!["8.", "4."]],
        ),
        (
            include_bytes!("fixtures/numbered-stories/two-sections.docx").as_slice(),
            [vec!["6.", "2.", "3."], vec!["7.", "5."]],
        ),
    ] {
        let mut doc = DisplayDocument::open(bytes, &OpenOptions::default()).unwrap();
        doc.layout().unwrap();
        assert_eq!(doc.pages().len(), 2);
        for (page, expected) in doc.pages().iter().zip(expected) {
            let mut actual = Vec::new();
            labels(&page.fragments, &mut actual);
            assert_eq!(actual, expected);
        }
    }
}

fn replace_part(bytes: &[u8], name: &str, replace: impl Fn(&str) -> String) -> Vec<u8> {
    let mut source = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut output = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for i in 0..source.len() {
        let mut file = source.by_index(i).unwrap();
        let mut data = Vec::new();
        file.read_to_end(&mut data).unwrap();
        if file.name() == name {
            data = replace(std::str::from_utf8(&data).unwrap()).into_bytes();
        }
        output
            .start_file(file.name(), zip::write::FileOptions::default())
            .unwrap();
        output.write_all(&data).unwrap();
    }
    output.finish().unwrap().into_inner()
}

fn paragraphs(doc: &DisplayDocument, page: usize) -> Vec<&ParagraphFragment> {
    doc.pages()[page]
        .fragments
        .iter()
        .filter_map(|f| match f {
            Fragment::Paragraph(p) => Some(p),
            _ => None,
        })
        .collect()
}

#[test]
fn section_properties_apply_to_the_section_they_end() {
    let body = format!(
        r#"{}<w:p><w:pPr><w:sectPr><w:pgSz w:w="15840" w:h="12240"/></w:sectPr></w:pPr></w:p>{}"#,
        p("Landscape"),
        p("Portrait")
    );
    let body = format!("<w:sdt><w:sdtContent>{body}</w:sdtContent></w:sdt>");
    let doc = render(&body, r#"<w:pgSz w:w="12240" w:h="15840"/>"#, "", &[]);
    assert_eq!(doc.pages().len(), 2);
    assert_eq!((doc.pages()[0].w, doc.pages()[0].h), (792.0, 612.0));
    assert_eq!((doc.pages()[1].w, doc.pages()[1].h), (612.0, 792.0));
}

#[test]
fn header_references_choose_variants_by_id_and_link_to_previous() {
    let refs = r#"<w:headerReference w:type="first" r:id="first"/><w:headerReference w:type="even" r:id="even"/><w:headerReference w:type="default" r:id="normal"/><w:titlePg/>"#;
    let body = format!(
        r#"{}<w:p><w:pPr><w:sectPr>{refs}</w:sectPr></w:pPr></w:p>{}{}"#,
        p("First section"),
        p("Next section"),
        r#"<w:p><w:pPr><w:pageBreakBefore/></w:pPr><w:r><w:t>Next page</w:t></w:r></w:p>"#
    );
    let doc = render(
        &body,
        "",
        "<w:evenAndOddHeaders/>",
        &[
            ("first", &p("FIRST")),
            ("even", &p("EVEN")),
            ("normal", &p("DEFAULT")),
        ],
    );
    assert_eq!(doc.pages().len(), 3);
    let text = |i| {
        paragraphs(&doc, i)
            .iter()
            .map(|p| p.text.to_string())
            .collect::<Vec<_>>()
            .join(" ")
    };
    assert!(text(0).contains("FIRST"), "{}", text(0));
    assert!(text(1).contains("EVEN"), "{}", text(1));
    assert!(text(2).contains("DEFAULT"), "{}", text(2));
    assert!(!text(1).contains("FIRST"));
}

#[test]
fn mirror_margins_move_the_inside_gutter_on_even_pages() {
    let body = format!(
        r#"{}<w:p><w:pPr><w:pageBreakBefore/></w:pPr><w:r><w:t>Even</w:t></w:r></w:p>"#,
        p("Odd")
    );
    let section =
        r#"<w:pgMar w:left="2160" w:right="720" w:top="1440" w:bottom="1440" w:gutter="200"/>"#;
    let doc = render(&body, section, "<w:mirrorMargins/>", &[]);
    assert_eq!(doc.pages().len(), 2);
    assert_eq!(doc.pages()[0].body.x, 118.0);
    assert_eq!(doc.pages()[1].body.x, 36.0);
    assert_eq!(doc.pages()[0].body.w, doc.pages()[1].body.w);
    let plain = render(&body, section, r#"<w:mirrorMargins w:val="0"/>"#, &[]);
    assert_eq!(plain.pages()[1].body.x, 118.0);
}

#[test]
fn missing_even_header_and_footer_fall_back_to_default() {
    let body = format!(
        "{}<w:p><w:pPr><w:pageBreakBefore/></w:pPr><w:r><w:t>Second</w:t></w:r></w:p>",
        p("First")
    );
    let doc = render(
        &body,
        r#"<w:headerReference w:type="default" r:id="header"/><w:footerReference w:type="default" r:id="footer"/>"#,
        r#"<w:evenAndOddHeaders/>"#,
        &[
            ("header", &p("Default header")),
            ("footer", &p("Default footer")),
        ],
    );
    let texts = paragraphs(&doc, 1)
        .iter()
        .map(|p| p.text.to_string())
        .collect::<Vec<_>>();
    assert!(texts.iter().any(|text| text == "Default header"));
    assert!(texts.iter().any(|text| text == "Default footer"));
}

#[test]
fn signed_page_margins_use_absolute_distances() {
    let doc = render(
        &p("Body"),
        r#"<w:pgMar w:top="-2268" w:bottom="-1440" w:left="-1440" w:right="1440"/>"#,
        "",
        &[],
    );
    let body = doc.pages()[0].body;
    assert!((body.y - 113.4).abs() < 0.01);
    assert!((body.x - 72.0).abs() < 0.01);
    assert!((body.h - (doc.pages()[0].h - 113.4 - 72.0)).abs() < 0.01);
}

#[test]
fn paragraph_shading_covers_the_hanging_indent() {
    let body = r#"<w:p><w:pPr><w:shd w:val="clear" w:fill="000000"/><w:ind w:left="1020" w:hanging="1020"/></w:pPr><w:r><w:t>Shaded</w:t></w:r></w:p>"#;
    let doc = render(body, "", "", &[]);
    let page = doc.page(0).unwrap();
    let rect = page
        .ops
        .iter()
        .find_map(|op| match op {
            edgedoc_display::Op::FillRect { rect, paint }
                if *paint == edgedoc_paint::Paint::Solid(edgedoc_geom::Rgba::BLACK) =>
            {
                Some(*rect)
            }
            _ => None,
        })
        .expect("paragraph shading");
    assert!((rect.x - doc.pages()[0].body.x).abs() < 0.01);
    assert!((rect.right() - doc.pages()[0].body.right()).abs() < 0.01);
}

#[test]
fn section_fields_count_their_own_section_and_page_uses_its_format() {
    let field = r#"<w:p><w:r><w:t>Section </w:t></w:r><w:fldSimple w:instr="SECTION"/><w:r><w:t>: </w:t></w:r><w:fldSimple w:instr="SECTIONPAGES"/><w:r><w:t> pages; page </w:t></w:r><w:fldSimple w:instr="PAGE"/><w:r><w:t> of </w:t></w:r><w:fldSimple w:instr="NUMPAGES"/></w:p>"#;
    let body = format!(
        r#"{field}<w:p><w:pPr><w:sectPr><w:pgNumType w:start="3" w:fmt="upperRoman"/></w:sectPr></w:pPr></w:p>{field}<w:p><w:pPr><w:pageBreakBefore/></w:pPr><w:r><w:t>Third</w:t></w:r></w:p>"#
    );
    let doc = render(&body, "", "", &[]);
    assert_eq!(doc.pages().len(), 3);
    assert_eq!(
        paragraphs(&doc, 0)[0].text.as_ref(),
        "Section 1: 1 pages; page III of 3"
    );
    assert_eq!(
        paragraphs(&doc, 1)[0].text.as_ref(),
        "Section 2: 2 pages; page 4 of 3"
    );
}

#[test]
fn a_long_paragraph_continues_without_repeating_or_dropping_lines() {
    let text = (0..90)
        .map(|i| format!("Line{i:03}"))
        .collect::<Vec<_>>()
        .join("<w:br/>");
    let text = text.replace("<w:br/>", "</w:t><w:br/><w:t>");
    let doc = render(
        &format!("<w:p><w:r><w:t>{text}</w:t></w:r></w:p>"),
        "",
        "",
        &[],
    );
    assert!(doc.pages().len() >= 2);
    let mut shown = String::new();
    for i in 0..doc.pages().len() {
        for para in paragraphs(&doc, i) {
            for line in &para.layout.lines[para.l0..para.l1] {
                shown.push_str(&para.text[line.start..line.stop]);
            }
        }
    }
    for i in 0..90 {
        let word = format!("Line{i:03}");
        assert_eq!(
            shown.matches(&word).count(),
            1,
            "{word} must paint exactly once"
        );
    }
}

#[test]
fn identical_text_with_different_alignment_does_not_share_the_wrong_layout() {
    let body = format!(
        r#"{}<w:p><w:pPr><w:jc w:val="right"/></w:pPr><w:r><w:t>Same</w:t></w:r></w:p>"#,
        p("Same")
    );
    let doc = render(&body, "", "", &[]);
    let ps = paragraphs(&doc, 0);
    assert!(ps[1].layout.lines[0].xs[0] > ps[0].layout.lines[0].xs[0] + 200.0);
}

#[test]
fn line_spacing_exact_at_least_and_multiple_reach_geometry() {
    let body = [24, 36].iter().map(|size| format!(r#"<w:p><w:pPr><w:spacing w:line="{}" w:lineRule="exact"/></w:pPr><w:r><w:t>Same</w:t></w:r></w:p>"#, size * 20)).collect::<String>();
    let doc = render(&body, "", "", &[]);
    let ps = paragraphs(&doc, 0);
    assert!((ps[0].layout.lines[0].height - 24.0).abs() < 0.01);
    assert!((ps[1].layout.lines[0].height - 36.0).abs() < 0.01);
    assert_eq!(
        doc.page(0)
            .unwrap()
            .ops
            .iter()
            .filter(|op| matches!(op, edgedoc_display::Op::PushClip(_)))
            .count(),
        2
    );
    let natural = render(
        r#"<w:p><w:pPr><w:spacing w:line="240" w:lineRule="auto"/></w:pPr><w:r><w:t>Same</w:t></w:r></w:p>"#,
        "",
        "",
        &[],
    );
    let h = paragraphs(&natural, 0)[0].layout.lines[0].height;
    for (rule, value, expected) in [
        ("auto", 480, h * 2.0),
        ("atLeast", 20, h),
        ("atLeast", 720, 36.0),
    ] {
        let body = format!(
            r#"<w:p><w:pPr><w:spacing w:line="{value}" w:lineRule="{rule}"/></w:pPr><w:r><w:t>Same</w:t></w:r></w:p>"#
        );
        let doc = render(&body, "", "", &[]);
        assert!(
            (paragraphs(&doc, 0)[0].layout.lines[0].height - expected).abs() < 0.01,
            "{rule} {value}"
        );
    }
}

#[test]
fn inline_page_and_column_breaks_change_the_flow() {
    let doc = render(
        r#"<w:p><w:r><w:t>Before</w:t><w:br w:type="page"/><w:t>After</w:t></w:r></w:p>"#,
        "",
        "",
        &[],
    );
    assert_eq!(doc.pages().len(), 2, "{:?}", paragraphs(&doc, 0));
    assert_eq!(paragraphs(&doc, 0)[0].l1, 1);
    assert_eq!(paragraphs(&doc, 1)[0].l0, 1);
    let doc = render(
        r#"<w:p><w:r><w:t>Before</w:t><w:br w:type="column"/><w:t>After</w:t></w:r></w:p>"#,
        r#"<w:cols w:num="2"/>"#,
        "",
        &[],
    );
    assert_eq!(doc.pages().len(), 1);
    let ps = paragraphs(&doc, 0);
    assert_eq!(ps.len(), 2);
    assert!(ps[1].rect.x > ps[0].rect.x + 100.0);
}

#[test]
fn bottom_vertical_alignment_reaches_the_bottom_margin() {
    let doc = render(&p("Bottom"), r#"<w:vAlign w:val="bottom"/>"#, "", &[]);
    assert!(
        (paragraphs(&doc, 0)[0].rect.bottom() + paragraphs(&doc, 0)[0].para.space_after
            - doc.pages()[0].body.bottom())
        .abs()
            < 0.01
    );
}

#[test]
fn continuous_sections_keep_the_current_page() {
    let body = format!(
        r#"{}<w:p><w:pPr><w:sectPr/></w:pPr></w:p>{}"#,
        p("Before"),
        p("After")
    );
    let doc = render(&body, r#"<w:type w:val="continuous"/>"#, "", &[]);
    assert_eq!(doc.pages().len(), 1);
    let ps = paragraphs(&doc, 0);
    assert!(ps.last().unwrap().rect.y > ps[0].rect.y);
}

#[test]
fn header_and_footer_distances_are_measured_from_the_page_edges() {
    let doc = render(
        &p("Body"),
        r#"<w:headerReference w:type="default" r:id="header"/><w:footerReference w:type="default" r:id="footer"/><w:pgMar w:top="1440" w:bottom="1440" w:header="400" w:footer="600"/>"#,
        "",
        &[("header", &p("Header")), ("footer", &p("Footer"))],
    );
    let ps = paragraphs(&doc, 0);
    let head = ps.iter().find(|p| p.text.as_ref() == "Header").unwrap();
    let foot = ps.iter().find(|p| p.text.as_ref() == "Footer").unwrap();
    assert!((head.rect.y - 20.0).abs() < 0.01);
    assert!((foot.rect.bottom() + foot.para.space_after - (doc.pages()[0].h - 30.0)).abs() < 0.01);
}

#[test]
fn tall_headers_and_footers_reserve_body_space_and_keep_all_their_lines() {
    let header = (0..8).map(|i| p(&format!("Header{i}"))).collect::<String>();
    let footer = (0..8).map(|i| p(&format!("Footer{i}"))).collect::<String>();
    let body = (0..80)
        .map(|i| p(&format!("Body{i:03}")))
        .collect::<String>();
    let doc = render(
        &body,
        r#"<w:headerReference w:type="default" r:id="header"/><w:footerReference w:type="default" r:id="footer"/>"#,
        "",
        &[("header", &header), ("footer", &footer)],
    );
    assert!(doc.pages().len() >= 2);
    let mut body_count = 0;
    for (i, page) in doc.pages().iter().enumerate() {
        let ps = paragraphs(&doc, i);
        let heads = ps
            .iter()
            .filter(|p| p.text.starts_with("Header"))
            .collect::<Vec<_>>();
        let feet = ps
            .iter()
            .filter(|p| p.text.starts_with("Footer"))
            .collect::<Vec<_>>();
        assert_eq!(heads.len(), 8);
        assert_eq!(feet.len(), 8);
        let head_bottom = heads.iter().map(|p| p.rect.bottom()).fold(0.0, f32::max);
        let foot_top = feet.iter().map(|p| p.rect.y).fold(page.h, f32::min);
        assert!(page.body.y >= head_bottom);
        assert!(page.body.bottom() <= foot_top);
        for p in ps.iter().filter(|p| p.text.starts_with("Body")) {
            body_count += 1;
            assert!(p.rect.y >= head_bottom && p.rect.bottom() <= foot_top + 0.01);
        }
    }
    assert_eq!(body_count, 80);
}

#[test]
fn painted_baselines_use_one_line_offset_and_respect_justified_positions() {
    let doc = render(
        r#"<w:p><w:pPr><w:jc w:val="both"/></w:pPr><w:r><w:t>One two three four five six seven eight nine ten eleven twelve thirteen fourteen fifteen sixteen seventeen eighteen nineteen twenty.</w:t></w:r></w:p>"#,
        r#"<w:pgSz w:w="6000" w:h="10000"/><w:pgMar w:left="500" w:right="500"/>"#,
        "",
        &[],
    );
    let para = paragraphs(&doc, 0)[0];
    assert!(para.layout.lines.len() >= 3);
    for line in &para.layout.lines {
        if line.end == edgedoc_text::LineEnd::Wrap {
            let last_visible = para.layout.clusters[line.c0..line.c1]
                .iter()
                .rposition(|c| c.kind != edgedoc_text::ClusterKind::Space)
                .unwrap();
            let right = line.xs[last_visible] + para.layout.clusters[line.c0 + last_visible].adv;
            assert!(
                (right - line.right).abs() < 0.01,
                "justification fills the band even when a gap needs more than 3 pt"
            );
        }
        let expected = para.rect.y + line.baseline;
        let glyphs = doc
            .page(0)
            .unwrap()
            .ops
            .iter()
            .filter_map(|op| match op {
                edgedoc_display::Op::Glyphs(g) if (g.y - expected).abs() < 0.01 => Some(g),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(!glyphs.is_empty(), "a painted baseline at {expected}");
        for g in glyphs {
            let mut pen = g.x;
            for (i, byte) in g.clusters.iter().enumerate() {
                let cluster = para
                    .layout
                    .clusters
                    .iter()
                    .find(|c| c.start == *byte as usize)
                    .unwrap();
                let k = para
                    .layout
                    .clusters
                    .iter()
                    .position(|c| c == cluster)
                    .unwrap();
                let want = para.rect.x + line.xs[k - line.c0];
                let dx = g.offsets[i].0;
                assert!(
                    (pen + dx - want).abs() < 0.01,
                    "painted glyph at {}, layout at {want}",
                    pen + dx
                );
                pen += g.advances[i];
            }
        }
    }
}

#[test]
fn a_multi_page_table_keeps_each_row_once_and_repeats_its_header() {
    let row = |text: &str, header: bool| {
        format!(
            "<w:tr><w:trPr>{}</w:trPr><w:tc>{}</w:tc></w:tr>",
            if header { "<w:tblHeader/>" } else { "" },
            p(text)
        )
    };
    let body = format!(
        "<w:tbl><w:tblGrid><w:gridCol w:w=\"5000\"/></w:tblGrid>{}{}</w:tbl>",
        row("Heading", true),
        (0..90)
            .map(|i| row(&format!("Row{i:03}"), false))
            .collect::<String>()
    );
    let doc = render(&body, "", "", &[]);
    assert!(doc.pages().len() >= 2);
    let mut all = Vec::new();
    for page in doc.pages() {
        let tables = page
            .fragments
            .iter()
            .filter_map(|f| match f {
                Fragment::Table(t) => Some(t),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(tables.len(), 1);
        assert!(
            tables[0].rect.bottom() <= page.body.bottom() + 0.01,
            "repeated header must fit"
        );
        let texts = tables[0]
            .content
            .iter()
            .filter_map(|f| match f {
                Fragment::Paragraph(p) => Some(p.text.to_string()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(texts.iter().filter(|s| *s == "Heading").count(), 1);
        all.extend(texts);
    }
    for i in 0..90 {
        assert_eq!(
            all.iter().filter(|s| **s == format!("Row{i:03}")).count(),
            1
        );
    }
}

#[test]
fn keep_with_next_moves_the_heading_with_its_following_paragraph() {
    let body = r#"<w:p><w:pPr><w:spacing w:line="2400" w:lineRule="exact"/></w:pPr><w:r><w:t>Filler</w:t></w:r></w:p><w:p><w:pPr><w:keepNext/><w:spacing w:line="480" w:lineRule="exact"/></w:pPr><w:r><w:t>Heading</w:t></w:r></w:p><w:p><w:pPr><w:spacing w:line="480" w:lineRule="exact"/></w:pPr><w:r><w:t>Following</w:t></w:r></w:p>"#;
    let doc = render(
        body,
        r#"<w:pgSz w:w="6000" w:h="4000"/><w:pgMar w:top="400" w:bottom="400"/>"#,
        "",
        &[],
    );
    assert_eq!(doc.pages().len(), 2);
    assert_eq!(paragraphs(&doc, 0).len(), 1);
    assert_eq!(
        paragraphs(&doc, 1)
            .iter()
            .map(|p| p.text.as_ref())
            .collect::<Vec<_>>(),
        vec!["Heading", "Following"]
    );
}

#[test]
fn page_budget_is_enforced_and_hidden_text_can_be_shown() {
    let body = format!(
        "<w:p><w:r><w:rPr><w:vanish/></w:rPr><w:t>Secret</w:t></w:r></w:p>{}",
        p("Visible")
    );
    let bytes = package(&body, "", "", &[]);
    let mut hidden = open_document(&bytes, &OpenOptions::default()).unwrap();
    hidden.layout().unwrap();
    assert!(!hidden.page_text(0).unwrap().contains("Secret"));
    let mut shown = open_document(
        &bytes,
        &OpenOptions {
            show_hidden_text: true,
            ..OpenOptions::default()
        },
    )
    .unwrap();
    shown.layout().unwrap();
    assert!(shown.page_text(0).unwrap().contains("Secret"));
    let body = format!(
        "{}<w:p><w:pPr><w:pageBreakBefore/></w:pPr><w:r><w:t>Second</w:t></w:r></w:p>",
        p("First")
    );
    let bytes = package(&body, "", "", &[]);
    let mut limited = open_document(
        &bytes,
        &OpenOptions {
            max_pages: 1,
            ..OpenOptions::default()
        },
    )
    .unwrap();
    let report = limited.layout().unwrap();
    assert_eq!(report.pages, 1);
    assert!(report
        .diagnostics
        .iter()
        .any(|d| d.code == "display.page_limit"));
}

#[test]
fn numbering_formats_and_start_overrides_are_read_from_their_elements() {
    let doc = edgedoc::Document::builder()
        .paragraph(
            edgedoc::Paragraph::new()
                .text("First")
                .list(edgedoc::ListKind::LowerRoman, 0),
        )
        .paragraph(
            edgedoc::Paragraph::new()
                .text("Second")
                .list(edgedoc::ListKind::LowerRoman, 0),
        )
        .build()
        .unwrap();
    let bytes = doc.write_bytes().unwrap();
    let bytes = replace_part(&bytes, "word/numbering.xml", |xml| {
        xml.replace(
            "</w:num>",
            "<w:lvlOverride w:ilvl=\"0\"><w:startOverride w:val=\"7\"/></w:lvlOverride></w:num>",
        )
    });
    let mut display = open_document(&bytes, &OpenOptions::default()).unwrap();
    display.layout().unwrap();
    let labels = paragraphs(&display, 0)
        .iter()
        .map(|p| p.label.as_ref().map(|(s, _)| s.as_str()))
        .collect::<Vec<_>>();
    assert_eq!(labels, vec![Some("vii."), Some("viii.")]);
}

#[test]
fn nested_footer_controls_place_page_fields_at_footer_distance() {
    let bytes = include_bytes!("fixtures/footer-control/source.docx");
    let mut doc = open_document(bytes, &OpenOptions::default()).unwrap();
    doc.layout().unwrap();
    let footer = paragraphs(&doc, 0)
        .into_iter()
        .find(|p| p.text.starts_with("Page "))
        .expect("footer paragraph");
    assert_eq!(footer.para.space_after, 0.0);
    assert_eq!(footer.para.line_height, 12.0);
    assert!((footer.rect.bottom() - 756.0).abs() < 0.01);
}

#[test]
fn cropped_inline_picture_preserves_source_rectangle_and_edited_save() {
    let bytes = include_bytes!("fixtures/inline-picture/wordcraft-case.docx");
    let bytes = replace_part(bytes, "word/document.xml", |xml| {
        xml.replace(r#"<a:blip r:embed="rId4" cstate="print"/>"#,
            r#"<a:blip r:embed="rId4" cstate="print"/><a:srcRect l="20000" t="10000" r="40000" b="30000"/>"#)
    });
    let mut doc = open_document(&bytes, &OpenOptions::default()).unwrap();
    doc.layout().unwrap();
    let source = doc
        .page(0)
        .unwrap()
        .ops
        .iter()
        .find_map(|op| match op {
            edgedoc_display::Op::Image { src, .. } => Some(*src),
            _ => None,
        })
        .expect("cropped image");
    for (actual, expected) in source.into_iter().zip([0.2, 0.1, 0.4, 0.6]) {
        assert!((actual - expected).abs() < 0.00001);
    }
    assert!(doc
        .to_svg(0)
        .unwrap()
        .contains(r#"viewBox="0.200000 0.100000 0.400000 0.600000""#));
    let mut model = edgedoc::Document::read_bytes(&bytes).unwrap();
    model.add_style(edgedoc_model::Style::paragraph("ForceSave"));
    let saved = model.write_bytes().unwrap();
    let mut archive = zip::ZipArchive::new(Cursor::new(saved)).unwrap();
    let mut xml = String::new();
    archive
        .by_name("word/document.xml")
        .unwrap()
        .read_to_string(&mut xml)
        .unwrap();
    assert!(xml.contains(r#"<a:srcRect l="20000" t="10000" r="40000" b="30000"/>"#));
}

#[test]
fn picture_only_paragraph_keeps_its_line_descent_before_following_text() {
    // Apache POI 56392.docx, pinned in tests/corpus/external-expanded-manifest.json.
    let bytes = include_bytes!("fixtures/inline-picture/picture-only-poi.docx");
    let mut display = open_document(bytes, &OpenOptions::default()).unwrap();
    display.layout().unwrap();
    let page = display.page(0).unwrap();
    let picture = page
        .ops
        .iter()
        .find_map(|op| match op {
            edgedoc_display::Op::Image { rect, .. } => Some(*rect),
            _ => None,
        })
        .expect("signature image");
    let picture_line = paragraphs(&display, 0)
        .into_iter()
        .find(|p| p.text.contains('\u{FFFC}'))
        .expect("picture remains in its paragraph");
    assert!(picture_line.rect.bottom() > picture.bottom() + 1.0);
    let following = page
        .ops
        .iter()
        .find_map(|op| match op {
            edgedoc_display::Op::Glyphs(glyphs) if glyphs.text.contains("Ing.") => Some(glyphs),
            _ => None,
        })
        .expect("following signature text");
    assert!(following.y > picture_line.rect.bottom());
}

#[test]
fn inline_picture_shares_a_line_and_preserves_effect_extents_after_edit() {
    // docx4j sample chunk.docx, pinned in tests/corpus/external-expanded-manifest.json.
    let bytes = include_bytes!("fixtures/inline-picture/wordcraft-case.docx");
    let mut display = open_document(bytes, &OpenOptions::default()).unwrap();
    display.layout().unwrap();
    let page = display.page(0).unwrap();
    let picture = page
        .ops
        .iter()
        .find_map(|op| match op {
            edgedoc_display::Op::Image { rect, .. } => Some(*rect),
            _ => None,
        })
        .expect("inline image");
    let text = page
        .ops
        .iter()
        .find_map(|op| match op {
            edgedoc_display::Op::Glyphs(glyphs) if glyphs.text.contains("This is my chunk") => {
                Some(glyphs)
            }
            _ => None,
        })
        .expect("adjacent text");
    assert!((picture.x - 73.5).abs() < 0.01);
    assert!((picture.bottom() - text.y).abs() < 0.1);
    assert!(text.x >= picture.right());

    let mut model = edgedoc::Document::read_bytes(bytes).unwrap();
    model.add_style(edgedoc_model::Style::paragraph("EditedStyle"));
    let saved = model.write_bytes().unwrap();
    let mut archive = zip::ZipArchive::new(Cursor::new(saved)).unwrap();
    let mut xml = String::new();
    archive
        .by_name("word/document.xml")
        .unwrap()
        .read_to_string(&mut xml)
        .unwrap();
    assert!(xml.contains(r#"<wp:effectExtent l="19050" t="0" r="0" b="0"/>"#));
}

#[test]
fn character_indents_use_hundredths_and_preserve_fractional_values() {
    fn x(indent: &str) -> f32 {
        let body = format!(
            r#"<w:p><w:pPr>{indent}<w:rPr><w:sz w:val="22"/></w:rPr></w:pPr><w:r><w:t>Indent</w:t></w:r></w:p>"#
        );
        let doc = render(&body, "", "", &[]);
        doc.page(0)
            .unwrap()
            .ops
            .iter()
            .find_map(|op| match op {
                edgedoc_display::Op::Glyphs(glyphs) if glyphs.text.contains("Indent") => {
                    Some(glyphs.x)
                }
                _ => None,
            })
            .unwrap()
    }
    let baseline = x("");
    assert!((x(r#"<w:ind w:leftChars="200"/>"#) - baseline - 22.0).abs() < 0.02);
    assert!((x(r#"<w:ind w:left="719" w:hangingChars="327"/>"#) - baseline + 0.02).abs() < 0.03);
}

#[test]
fn paragraph_alignment_keeps_text_before_last_tab_anchored() {
    fn position(alignment: &str, tail: &str) -> f32 {
        let body = format!(
            r#"<w:p><w:pPr><w:ind w:firstLine="342"/><w:jc w:val="{alignment}"/></w:pPr><w:r><w:t>Before</w:t><w:tab/>{tail}</w:r></w:p>"#
        );
        let doc = render(&body, "", "", &[]);
        doc.page(0)
            .unwrap()
            .ops
            .iter()
            .find_map(|op| match op {
                edgedoc_display::Op::Glyphs(glyphs) if glyphs.text.contains("Before") => {
                    Some(glyphs.x)
                }
                _ => None,
            })
            .unwrap()
    }
    for tail in ["", "<w:t>After</w:t>"] {
        let left = position("left", tail);
        assert!((position("center", tail) - left).abs() < 0.01);
        assert!((position("right", tail) - left).abs() < 0.01);
    }
}

#[test]
fn cell_paragraph_indents_move_centered_and_first_line_text() {
    fn position(indent: &str, alignment: &str) -> f32 {
        let body = format!(
            r#"<w:tbl><w:tblPr><w:tblW w:w="4000" w:type="dxa"/></w:tblPr><w:tblGrid><w:gridCol w:w="4000"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:w="4000" w:type="dxa"/></w:tcPr><w:p><w:pPr>{indent}<w:jc w:val="{alignment}"/></w:pPr><w:r><w:t>Cell text</w:t></w:r></w:p></w:tc></w:tr></w:tbl>"#
        );
        let doc = render(&body, "", "", &[]);
        doc.page(0)
            .unwrap()
            .ops
            .iter()
            .find_map(|op| match op {
                edgedoc_display::Op::Glyphs(glyphs) if glyphs.text.contains("Cell text") => {
                    Some(glyphs.x)
                }
                _ => None,
            })
            .unwrap()
    }
    let centered = position("", "center");
    let indented = position(r#"<w:ind w:left="141" w:right="-12"/>"#, "center");
    assert!((indented - centered - 3.825).abs() < 0.02);
    let left = position("", "left");
    let first_line = position(r#"<w:ind w:left="120" w:firstLine="120"/>"#, "left");
    assert!((first_line - left - 12.0).abs() < 0.02);
}

#[test]
fn consecutive_equal_simple_paragraph_borders_share_one_box() {
    let border = r#"<w:pBdr><w:top w:val="single" w:sz="36" w:space="6"/><w:bottom w:val="single" w:sz="36" w:space="1"/></w:pBdr>"#;
    let bordered = |text: &str, indent: &str| {
        format!("<w:p><w:pPr>{border}{indent}</w:pPr><w:r><w:t>{text}</w:t></w:r></w:p>")
    };
    let body = format!(
        "{}{}{}{}",
        bordered("first", ""),
        bordered("second", ""),
        bordered("third", ""),
        bordered("different indent", r#"<w:ind w:left="120"/>"#)
    );
    let doc = render(&body, "", "", &[]);
    let paras = paragraphs(&doc, 0);
    assert_eq!(paras.len(), 4);
    assert!(paras[0].para.borders.top.is_some());
    assert!(paras[0].para.borders.bottom.is_none());
    assert!(paras[1].para.borders.top.is_none() && paras[1].para.borders.bottom.is_none());
    assert!(paras[2].para.borders.top.is_none() && paras[2].para.borders.bottom.is_some());
    assert!(paras[3].para.borders.top.is_some() && paras[3].para.borders.bottom.is_some());
    let plain = render(
        &format!("{}{}{}", p("first"), p("second"), p("third")),
        "",
        "",
        &[],
    );
    let plain_paras = paragraphs(&plain, 0);
    assert!(
        (paras[1].rect.y - paras[0].rect.y - (plain_paras[1].rect.y - plain_paras[0].rect.y)).abs()
            < 0.01
    );
    assert!(
        (paras[2].rect.y - paras[1].rect.y - (plain_paras[2].rect.y - plain_paras[1].rect.y)).abs()
            < 0.01
    );
}

#[test]
fn behind_page_anchored_image_does_not_consume_line_space_and_round_trips() {
    let bytes = include_bytes!("fixtures/inline-picture/wordcraft-case.docx");
    let bytes = replace_part(bytes, "word/document.xml", |xml| {
        let start = xml.find("<wp:inline ").unwrap();
        let end = xml[start..].find('>').unwrap() + start + 1;
        let mut value = xml.to_string();
        value.replace_range(start..end, r#"<wp:anchor behindDoc="1"><wp:positionH relativeFrom="page"><wp:align>right</wp:align></wp:positionH><wp:positionV relativeFrom="page"><wp:align>bottom</wp:align></wp:positionV><wp:wrapNone/>"#);
        value.replace("</wp:inline>", "</wp:anchor>")
    });
    let mut display = open_document(&bytes, &OpenOptions::default()).unwrap();
    display.layout().unwrap();
    let page = display.page(0).unwrap();
    let picture = page
        .ops
        .iter()
        .find_map(|op| match op {
            edgedoc_display::Op::Image { rect, .. } => Some(*rect),
            _ => None,
        })
        .expect("page picture");
    assert!((picture.right() - page.w).abs() < 0.02);
    assert!((picture.bottom() - page.h).abs() < 0.02);
    let text = page
        .ops
        .iter()
        .find_map(|op| match op {
            edgedoc_display::Op::Glyphs(glyphs) if glyphs.text.contains("This is my chunk") => {
                Some(glyphs)
            }
            _ => None,
        })
        .expect("text next to anchor");
    assert!(text.x < picture.x);
    let mut model = edgedoc::Document::read_bytes(&bytes).unwrap();
    model.add_style(edgedoc_model::Style::paragraph("ForceSave"));
    let saved = model.write_bytes().unwrap();
    let mut archive = zip::ZipArchive::new(Cursor::new(saved)).unwrap();
    let mut xml = String::new();
    archive
        .by_name("word/document.xml")
        .unwrap()
        .read_to_string(&mut xml)
        .unwrap();
    assert!(xml.contains(r#"<wp:positionH relativeFrom="page">"#));
    assert!(xml.contains(r#"<wp:positionV relativeFrom="page">"#));
}

#[test]
fn inline_pictures_share_lines_in_headers_and_table_cells() {
    for bytes in [
        include_bytes!("fixtures/inline-picture/header.docx").as_slice(),
        include_bytes!("fixtures/inline-picture/table.docx").as_slice(),
    ] {
        let mut display = open_document(bytes, &OpenOptions::default()).unwrap();
        display.layout().unwrap();
        let page = display.page(0).unwrap();
        let picture = page
            .ops
            .iter()
            .find_map(|op| match op {
                edgedoc_display::Op::Image { rect, .. } => Some(*rect),
                _ => None,
            })
            .expect("inline image");
        let text = page
            .ops
            .iter()
            .find_map(|op| match op {
                edgedoc_display::Op::Glyphs(glyphs) if glyphs.text.contains("This is my chunk") => {
                    Some(glyphs)
                }
                _ => None,
            })
            .expect("adjacent text");
        assert!((picture.bottom() - text.y).abs() < 0.1);
        assert!(text.x >= picture.right());
    }
}

#[test]
fn underline_without_a_value_stays_unpainted_and_survives_an_edit() {
    let body = r#"<w:p><w:r><w:rPr><w:u w:color="000000"/></w:rPr><w:t>Plain</w:t></w:r><w:r><w:rPr><w:u w:val="single"/></w:rPr><w:t>Underlined</w:t></w:r></w:p>"#;
    let bytes = package(body, "", "", &[]);
    let mut display = open_document(&bytes, &OpenOptions::default()).unwrap();
    display.layout().unwrap();
    let paragraph = paragraphs(&display, 0).into_iter().next().unwrap();
    assert!(!paragraph.run_styles[0].underline);
    assert!(paragraph.run_styles[1].underline);

    let mut model = edgedoc::Document::read_bytes(&bytes).unwrap();
    model.add_style(edgedoc_model::Style::paragraph("EditedStyle"));
    let saved = model.write_bytes().unwrap();
    let mut archive = zip::ZipArchive::new(Cursor::new(saved)).unwrap();
    let mut xml = String::new();
    archive
        .by_name("word/document.xml")
        .unwrap()
        .read_to_string(&mut xml)
        .unwrap();
    assert!(xml.contains(r#"<w:u w:color="000000"/>"#));
}

#[test]
fn underline_uses_its_own_color() {
    let body = r#"<w:p><w:r><w:rPr><w:u w:val="single" w:color="FF0000"/></w:rPr><w:t>Colored underline</w:t></w:r></w:p>"#;
    let doc = render(body, "", "", &[]);
    assert!(doc.page(0).unwrap().ops.iter().any(|op| matches!(op,
        edgedoc_display::Op::FillRect { paint: edgedoc_paint::Paint::Solid(color), .. }
        if *color == edgedoc_geom::Rgba::opaque(255, 0, 0))));
}

#[test]
fn small_caps_shape_lowercase_letters_as_smaller_capitals() {
    let body = r#"<w:p><w:r><w:rPr><w:smallCaps/><w:sz w:val="20"/></w:rPr><w:t>Small Caps</w:t></w:r></w:p>"#;
    let doc = render(body, "", "", &[]);
    let paragraph = paragraphs(&doc, 0).into_iter().next().unwrap();
    let used = paragraph
        .layout
        .clusters
        .iter()
        .filter_map(|cluster| paragraph.run_styles.get(cluster.style as usize))
        .collect::<Vec<_>>();
    assert!(used.iter().any(|style| (style.size - 10.0).abs() < 0.01));
    assert!(used.iter().any(|style| (style.size - 8.0).abs() < 0.01));
    let text = doc
        .page(0)
        .unwrap()
        .ops
        .iter()
        .filter_map(|op| match op {
            edgedoc_display::Op::Glyphs(glyphs) => Some(glyphs.text.as_str()),
            _ => None,
        })
        .collect::<String>();
    assert!(text.contains("SMALL CAPS"));
}

#[test]
fn double_underline_paints_two_distinct_strokes() {
    let body = r#"<w:p><w:r><w:rPr><w:u w:val="double"/></w:rPr><w:t>Underlined</w:t></w:r></w:p>"#;
    let bytes = package(body, "", "", &[]);
    let mut doc = open_document(&bytes, &OpenOptions::default()).unwrap();
    doc.layout().unwrap();
    let page = doc.page(0).unwrap();
    let glyph = page
        .ops
        .iter()
        .find_map(|op| match op {
            edgedoc_display::Op::Glyphs(g) if g.text.contains("Underlined") => Some(g),
            _ => None,
        })
        .unwrap();
    let center = glyph.y + glyph.size * 0.12;
    let strokes: Vec<_> = page
        .ops
        .iter()
        .filter_map(|op| match op {
            edgedoc_display::Op::FillRect { rect, .. }
                if rect.w > glyph.size && (rect.y - center).abs() < glyph.size * 0.2 =>
            {
                Some(rect)
            }
            _ => None,
        })
        .collect();
    assert_eq!(strokes.len(), 2, "{strokes:?}");
    assert!(strokes.iter().any(|r| r.y < center));
    assert!(strokes.iter().any(|r| r.y > center));
}

#[test]
fn list_label_keeps_shaped_kerning_between_glyphs() {
    use edgedoc_fonts::{DbShaper, Shaper};
    let body = r#"<w:p><w:pPr><w:rPr><w:kern w:val="1"/></w:rPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr></w:pPr><w:r><w:t>Heading</w:t></w:r></w:p>"#;
    let numbering = format!(
        r#"<w:numbering xmlns:w="{W}"><w:abstractNum w:abstractNumId="0"><w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="decimal"/><w:lvlText w:val="AV%1"/><w:rPr><w:rFonts w:ascii="Source Sans 3" w:hAnsi="Source Sans 3"/><w:sz w:val="32"/></w:rPr></w:lvl></w:abstractNum><w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num></w:numbering>"#
    );
    let bytes = add_part(
        &package(body, "", "", &[]),
        "word/numbering.xml",
        &numbering,
        "numbering",
        "application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml",
    );
    let mut doc = open_document(&bytes, &OpenOptions::default()).unwrap();
    doc.fonts_mut()
        .load(
            include_bytes!("../../../assets/fonts/SourceSans3-Regular.ttf").to_vec(),
            None,
            false,
            false,
            false,
        )
        .unwrap();
    doc.layout().unwrap();
    let page = doc.page(0).unwrap();
    let runs = page
        .ops
        .iter()
        .filter_map(|op| match op {
            edgedoc_display::Op::Glyphs(g) if g.text == "A" || g.text == "V" => Some(g),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(runs.len(), 2);
    let shaper = DbShaper { db: doc.fonts() };
    let shaped = shaper.shape_run(runs[0].face, "AV1", runs[0].size, false);
    assert!(
        (shaped[0].advance - shaper.advance(runs[0].face, shaped[0].gid, runs[0].size)).abs()
            > 0.01,
        "fixture must exercise a kerning pair"
    );
    assert!((runs[1].x - runs[0].x - shaped[0].advance).abs() < 0.01);
}

#[test]
fn list_label_does_not_kern_below_the_kerning_threshold() {
    use edgedoc_fonts::{DbShaper, Shaper};
    let body = r#"<w:p><w:pPr><w:rPr><w:kern w:val="200"/></w:rPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr></w:pPr><w:r><w:t>Heading</w:t></w:r></w:p>"#;
    let numbering = format!(
        r#"<w:numbering xmlns:w="{W}"><w:abstractNum w:abstractNumId="0"><w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="decimal"/><w:lvlText w:val="AV%1"/><w:rPr><w:rFonts w:ascii="Source Sans 3" w:hAnsi="Source Sans 3"/><w:sz w:val="32"/></w:rPr></w:lvl></w:abstractNum><w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num></w:numbering>"#
    );
    let bytes = add_part(
        &package(body, "", "", &[]),
        "word/numbering.xml",
        &numbering,
        "numbering",
        "application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml",
    );
    let mut doc = open_document(&bytes, &OpenOptions::default()).unwrap();
    doc.fonts_mut()
        .load(
            include_bytes!("../../../assets/fonts/SourceSans3-Regular.ttf").to_vec(),
            None,
            false,
            false,
            false,
        )
        .unwrap();
    doc.layout().unwrap();
    let page = doc.page(0).unwrap();
    let runs = page
        .ops
        .iter()
        .filter_map(|op| match op {
            edgedoc_display::Op::Glyphs(g) if g.text == "A" || g.text == "V" => Some(g),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(runs.len(), 2);
    let shaper = DbShaper { db: doc.fonts() };
    let shaped = shaper.shape_run_kerning(runs[0].face, "AV1", runs[0].size, false, false);
    assert!(
        (shaped[0].advance - shaper.advance(runs[0].face, shaped[0].gid, runs[0].size)).abs()
            < 0.01,
        "the disabled pair keeps its nominal advance"
    );
    assert!((runs[1].x - runs[0].x - shaped[0].advance).abs() < 0.01);
}

#[test]
fn numbering_level_formats_its_label_independently_of_body_text() {
    let body = r#"<w:p><w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr></w:pPr><w:r><w:t>Heading</w:t></w:r></w:p>"#;
    let numbering = format!(
        r#"<w:numbering xmlns:w="{W}"><w:abstractNum w:abstractNumId="0"><w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="decimal"/><w:lvlText w:val="Section %1."/><w:rPr><w:rFonts w:ascii="Calibri"/><w:b/><w:caps/><w:sz w:val="24"/></w:rPr></w:lvl></w:abstractNum><w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num></w:numbering>"#
    );
    let bytes = add_part(
        &package(body, "", "", &[]),
        "word/numbering.xml",
        &numbering,
        "numbering",
        "application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml",
    );
    let mut doc = open_document(&bytes, &OpenOptions::default()).unwrap();
    doc.layout().unwrap();
    let paras = paragraphs(&doc, 0);
    let para = paras.first().unwrap();
    assert_eq!(
        para.label.as_ref().map(|(label, _)| label.as_str()),
        Some("SECTION 1.")
    );
    assert_eq!(para.para.label_size, Some(12.0));
    let face = para
        .para
        .label_face
        .unwrap_or(para.para.label_fallback_face);
    assert!(doc.fonts().face(face).is_some_and(|font| font.bold));
}

#[test]
fn table_list_numbers_continue_and_repeated_headers_do_not_advance_them() {
    let list = |text: &str| {
        format!(
            r#"<w:p><w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr></w:pPr><w:r><w:t>{text}</w:t></w:r></w:p>"#
        )
    };
    let row = |text: &str, header: bool| {
        format!(
            "<w:tr><w:trPr>{}</w:trPr><w:tc>{}</w:tc></w:tr>",
            if header { "<w:tblHeader/>" } else { "" },
            list(text)
        )
    };
    let body = format!(
        "<w:tbl><w:tblGrid><w:gridCol w:w=\"5000\"/></w:tblGrid>{}{}</w:tbl>{}",
        row("Heading", true),
        (0..90)
            .map(|i| row(&format!("Row{i:03}"), false))
            .collect::<String>(),
        list("After")
    );
    let numbering = format!(
        r#"<w:numbering xmlns:w="{W}"><w:abstractNum w:abstractNumId="0"><w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="decimal"/><w:lvlText w:val="%1."/><w:pPr><w:ind w:left="720" w:hanging="360"/></w:pPr></w:lvl></w:abstractNum><w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num></w:numbering>"#
    );
    let bytes = add_part(
        &package(&body, "", "", &[]),
        "word/numbering.xml",
        &numbering,
        "numbering",
        "application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml",
    );
    let mut doc = open_document(&bytes, &OpenOptions::default()).unwrap();
    doc.layout().unwrap();
    assert!(doc.pages().len() >= 2);
    let mut headings = 0;
    let mut rows = Vec::new();
    let mut after = None;
    for page in doc.pages() {
        for fragment in &page.fragments {
            match fragment {
                Fragment::Table(table) => {
                    for content in &table.content {
                        if let Fragment::Paragraph(paragraph) = content {
                            let label = paragraph.label.as_ref().map(|(s, _)| s.as_str());
                            if paragraph.text.as_ref() == "Heading" {
                                headings += 1;
                                assert_eq!(label, Some("1."));
                            } else if paragraph.text.starts_with("Row") {
                                rows.push(label.unwrap().to_string());
                            }
                        }
                    }
                }
                Fragment::Paragraph(paragraph) if paragraph.text.as_ref() == "After" => {
                    after = paragraph.label.as_ref().map(|(s, _)| s.clone())
                }
                _ => {}
            }
        }
    }
    assert!(headings >= 2);
    assert_eq!(rows, (2..=91).map(|n| format!("{n}.")).collect::<Vec<_>>());
    assert_eq!(after.as_deref(), Some("92."));
}

#[test]
fn merged_cells_do_not_draw_internal_table_grid_borders() {
    let border = |name: &str| format!("<w:{name} w:val=\"single\" w:sz=\"8\" w:color=\"000000\"/>");
    let borders = ["top", "bottom", "left", "right", "insideH", "insideV"]
        .iter()
        .map(|name| border(name))
        .collect::<String>();
    let cell =
        |text: &str, props: &str| format!("<w:tc><w:tcPr>{props}</w:tcPr>{}</w:tc>", p(text));
    let body = format!("<w:tbl><w:tblPr><w:tblW w:w=\"7200\" w:type=\"dxa\"/><w:tblLayout w:type=\"fixed\"/><w:tblBorders>{borders}</w:tblBorders></w:tblPr><w:tblGrid><w:gridCol w:w=\"2400\"/><w:gridCol w:w=\"2400\"/><w:gridCol w:w=\"2400\"/></w:tblGrid><w:tr>{}{}</w:tr><w:tr>{}{}</w:tr><w:tr>{}{}{}</w:tr></w:tbl>",
        cell("Merged", "<w:gridSpan w:val=\"2\"/><w:vMerge w:val=\"restart\"/>"), cell("A", ""),
        cell("", "<w:gridSpan w:val=\"2\"/><w:vMerge/>"), cell("B", ""), cell("C", ""), cell("D", ""), cell("E", ""));
    let doc = render(&body, "", "", &[]);
    let table = doc.pages()[0]
        .fragments
        .iter()
        .find_map(|f| match f {
            Fragment::Table(t) => Some(t),
            _ => None,
        })
        .unwrap();
    let merged = table
        .cells
        .iter()
        .find(|c| c.row == 0 && c.col == 0)
        .unwrap()
        .rect;
    let middle = merged.x + merged.w / 2.0;
    let boundary = table
        .cells
        .iter()
        .find(|c| c.row == 0 && c.col == 2)
        .unwrap()
        .rect
        .bottom();
    assert!(!table.borders.iter().any(|b| b.rect.w <= 2.0
        && (b.rect.x + b.rect.w / 2.0 - middle).abs() < 0.1
        && b.rect.y < boundary - 0.1));
    assert!(!table.borders.iter().any(|b| b.rect.h <= 2.0
        && (b.rect.y + b.rect.h / 2.0 - boundary).abs() < 0.1
        && b.rect.x < middle
        && b.rect.right() > middle));
    assert!(table.borders.iter().any(|b| b.rect.h <= 2.0
        && (b.rect.y + b.rect.h / 2.0 - boundary).abs() < 0.1
        && b.rect.x >= merged.right() - 1.0));
}

#[test]
fn row_table_property_exceptions_set_borders_and_text_insets() {
    let row = |left: &str, right: &str, properties: &str| {
        format!(
            "<w:tr>{properties}<w:tc>{}</w:tc><w:tc>{}</w:tc></w:tr>",
            p(left),
            p(right)
        )
    };
    let exception = r#"<w:tblPrEx><w:tblBorders><w:insideH w:val="single" w:sz="8"/><w:insideV w:val="single" w:sz="8"/></w:tblBorders></w:tblPrEx>"#;
    let body = format!("<w:tbl><w:tblPr><w:tblW w:w=\"4800\" w:type=\"dxa\"/><w:tblLayout w:type=\"fixed\"/></w:tblPr><w:tblGrid><w:gridCol w:w=\"2400\"/><w:gridCol w:w=\"2400\"/></w:tblGrid>{}{}</w:tbl>", row("A", "B", ""), row("C", "D", exception));
    let doc = render(&body, "", "", &[]);
    let table = doc.pages()[0]
        .fragments
        .iter()
        .find_map(|f| match f {
            Fragment::Table(t) => Some(t),
            _ => None,
        })
        .unwrap();
    let top = table
        .cells
        .iter()
        .find(|cell| cell.row == 0 && cell.col == 0)
        .unwrap()
        .rect;
    let bottom = table
        .cells
        .iter()
        .find(|cell| cell.row == 1 && cell.col == 0)
        .unwrap()
        .rect;
    let x = bottom.right();
    assert!(!table.borders.iter().any(|b| b.rect.w <= 2.0
        && (b.rect.x + b.rect.w / 2.0 - x).abs() < 0.1
        && b.rect.y < top.bottom() - 0.1));
    assert!(table.borders.iter().any(|b| b.rect.w <= 2.0
        && (b.rect.x + b.rect.w / 2.0 - x).abs() < 0.1
        && (b.rect.y - bottom.y).abs() < 0.1));
    assert!(table
        .borders
        .iter()
        .any(|b| b.rect.h <= 2.0 && (b.rect.y + b.rect.h / 2.0 - bottom.y).abs() < 0.1));
    let c = table
        .content
        .iter()
        .find_map(|f| match f {
            Fragment::Paragraph(p) if p.text.as_ref() == "C" => Some(p),
            _ => None,
        })
        .unwrap();
    assert!((c.rect.y - bottom.y - 1.0).abs() < 0.01);
}

#[test]
fn vertically_merged_cell_spans_rows_and_grows_the_last_row() {
    let body = r#"<w:tbl><w:tblPr><w:tblW w:w="4800" w:type="dxa"/><w:tblLayout w:type="fixed"/></w:tblPr><w:tblGrid><w:gridCol w:w="2400"/><w:gridCol w:w="2400"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:vMerge w:val="restart"/><w:shd w:val="clear" w:fill="FFFF00"/></w:tcPr><w:p><w:r><w:t>One</w:t><w:br/><w:t>Two</w:t><w:br/><w:t>Three</w:t><w:br/><w:t>Four</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>A</w:t></w:r></w:p></w:tc></w:tr><w:tr><w:tc><w:tcPr><w:vMerge/></w:tcPr><w:p/></w:tc><w:tc><w:p><w:r><w:t>B</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p><w:r><w:t>After</w:t></w:r></w:p>"#;
    let doc = render(body, "", "", &[]);
    let table = doc.pages()[0]
        .fragments
        .iter()
        .find_map(|f| match f {
            Fragment::Table(t) => Some(t),
            _ => None,
        })
        .unwrap();
    let merged = table
        .cells
        .iter()
        .find(|c| c.row == 0 && c.col == 0)
        .unwrap();
    let first = table
        .cells
        .iter()
        .find(|c| c.row == 0 && c.col == 1)
        .unwrap();
    let second = table
        .cells
        .iter()
        .find(|c| c.row == 1 && c.col == 1)
        .unwrap();
    assert!(merged.rect.h > first.rect.h * 2.0);
    assert!((merged.rect.h - first.rect.h - second.rect.h).abs() < 0.01);
    assert!(merged.shading.is_some());
    let b = table
        .content
        .iter()
        .find_map(|f| match f {
            Fragment::Paragraph(p) if p.text.as_ref() == "B" => Some(p),
            _ => None,
        })
        .unwrap();
    assert!(b.rect.y < merged.rect.bottom());
    let after = paragraphs(&doc, 0)
        .into_iter()
        .find(|p| p.text.as_ref() == "After")
        .unwrap();
    assert!((after.rect.y - merged.rect.bottom()).abs() < 0.01);
}

#[test]
fn row_grid_before_and_after_position_cells_on_the_table_grid() {
    let borders = ["top", "bottom", "left", "right", "insideH", "insideV"]
        .iter()
        .map(|n| format!("<w:{n} w:val=\"single\" w:sz=\"8\"/>"))
        .collect::<String>();
    let cell = |text: &str| format!("<w:tc>{}</w:tc>", p(text));
    let body = format!("<w:tbl><w:tblPr><w:tblW w:w=\"7200\" w:type=\"dxa\"/><w:tblLayout w:type=\"fixed\"/><w:tblBorders>{borders}</w:tblBorders></w:tblPr><w:tblGrid>{}</w:tblGrid><w:tr><w:trPr><w:gridBefore w:val=\"1\"/><w:gridAfter w:val=\"1\"/></w:trPr>{}{}</w:tr><w:tr>{}{}{}{}</w:tr></w:tbl>", "<w:gridCol w:w=\"1800\"/>".repeat(4), cell("A"), cell("B"), cell("C"), cell("D"), cell("E"), cell("F"));
    let doc = render(&body, "", "", &[]);
    let table = doc.pages()[0]
        .fragments
        .iter()
        .find_map(|f| match f {
            Fragment::Table(t) => Some(t),
            _ => None,
        })
        .unwrap();
    let a = table
        .cells
        .iter()
        .find(|c| c.row == 0 && c.col == 1)
        .unwrap()
        .rect;
    let d = table
        .cells
        .iter()
        .find(|c| c.row == 1 && c.col == 1)
        .unwrap()
        .rect;
    let e = table
        .cells
        .iter()
        .find(|c| c.row == 1 && c.col == 2)
        .unwrap()
        .rect;
    let b = table
        .cells
        .iter()
        .find(|c| c.row == 0 && c.col == 2)
        .unwrap()
        .rect;
    assert!((a.x - d.x).abs() < 0.01);
    assert!((b.right() - e.right()).abs() < 0.01);
    assert!(table
        .cells
        .iter()
        .all(|c| c.row != 0 || (1..=2).contains(&c.col)));
}

#[test]
fn explicit_table_widths_use_points_or_percent_of_the_column() {
    let table = |value: &str, kind: &str| {
        format!("<w:tbl><w:tblPr><w:tblW w:w=\"{value}\" w:type=\"{kind}\"/></w:tblPr><w:tblGrid><w:gridCol w:w=\"5040\"/><w:gridCol w:w=\"5040\"/></w:tblGrid><w:tr><w:tc>{}</w:tc><w:tc>{}</w:tc></w:tr></w:tbl>", p("A"), p("B"))
    };
    let width = |doc: &DisplayDocument| {
        doc.pages()[0]
            .fragments
            .iter()
            .find_map(|f| match f {
                Fragment::Table(t) => Some(t.rect.w),
                _ => None,
            })
            .unwrap()
    };
    let wide = render(&table("10080", "dxa"), "", "", &[]);
    assert!((width(&wide) - 504.0).abs() < 0.01);
    assert!(width(&wide) > wide.pages()[0].body.w);
    let half = render(&table("2500", "pct"), "", "", &[]);
    assert!((width(&half) - half.pages()[0].body.w * 0.5).abs() < 0.01);
    let literal_half = render(&table("50%", "pct"), "", "", &[]);
    assert!((width(&literal_half) - literal_half.pages()[0].body.w * 0.5).abs() < 0.01);
    let oversized = render(&table("7500", "pct"), "", "", &[]);
    assert!((width(&oversized) - oversized.pages()[0].body.w * 1.5).abs() < 0.01);
    let inner = table("2500", "pct");
    let outer = format!("<w:tbl><w:tblPr><w:tblW w:w=\"6000\" w:type=\"dxa\"/><w:tblLayout w:type=\"fixed\"/></w:tblPr><w:tblGrid><w:gridCol w:w=\"6000\"/></w:tblGrid><w:tr><w:tc>{inner}<w:p/></w:tc></w:tr></w:tbl>");
    let nested = render(&outer, "", "", &[]);
    let outer = nested.pages()[0]
        .fragments
        .iter()
        .find_map(|f| match f {
            Fragment::Table(t) => Some(t),
            _ => None,
        })
        .unwrap();
    let inner = outer
        .content
        .iter()
        .find_map(|f| match f {
            Fragment::Table(t) => Some(t),
            _ => None,
        })
        .unwrap();
    assert!((inner.rect.w - (outer.rect.w - 10.8) * 0.5).abs() < 0.01);
}

#[test]
fn a_column_break_survives_an_edited_save() {
    let bytes = package(
        r#"<w:p><w:r><w:t>Before</w:t><w:br w:type="column"/><w:t>After</w:t></w:r></w:p>"#,
        r#"<w:cols w:num="2"/>"#,
        "",
        &[],
    );
    let mut doc = edgedoc::Document::read_bytes(&bytes).unwrap();
    doc.edit(edgedoc::EditOptions::default(), |edit| {
        edit.body().paragraph_at(0, |p| p.text("Added"))?;
        Ok(())
    })
    .unwrap();
    let saved = doc.write_bytes().unwrap();
    let mut display = open_document(&saved, &OpenOptions::default()).unwrap();
    display.layout().unwrap();
    let ps = paragraphs(&display, 0);
    assert_eq!(ps.len(), 2);
    assert!(ps[1].rect.x > ps[0].rect.x + 100.0);
    assert!(ps[1].text.ends_with("Added"));
}

#[test]
fn continuous_column_changes_start_below_the_previous_column_set() {
    let body = format!(
        r#"{}<w:p><w:pPr><w:sectPr><w:cols w:num="2"/></w:sectPr></w:pPr></w:p>{}"#,
        (0..8).map(|i| p(&format!("Before{i}"))).collect::<String>(),
        p("After")
    );
    let doc = render(&body, r#"<w:type w:val="continuous"/>"#, "", &[]);
    assert_eq!(doc.pages().len(), 1);
    let ps = paragraphs(&doc, 0);
    let bottom = ps
        .iter()
        .filter(|p| p.text.starts_with("Before"))
        .map(|p| p.rect.bottom())
        .fold(0.0, f32::max);
    let after = ps.iter().find(|p| p.text.as_ref() == "After").unwrap();
    assert!(
        (after.rect.y - bottom - 8.0).abs() < 0.01,
        "the preceding paragraph's trailing space survives the column change"
    );
    assert!(after.rect.w > ps[0].rect.w * 1.8);
}

#[test]
fn sections_sharing_a_page_each_count_that_page() {
    let field = r#"<w:p><w:fldSimple w:instr="SECTION"/><w:r><w:t> of </w:t></w:r><w:fldSimple w:instr="SECTIONPAGES"/></w:p>"#;
    let body = format!("{field}<w:p><w:pPr><w:sectPr/></w:pPr></w:p>{field}");
    let doc = render(&body, r#"<w:type w:val="continuous"/>"#, "", &[]);
    assert_eq!(doc.pages().len(), 1);
    let ps = paragraphs(&doc, 0);
    assert_eq!(ps[0].text.as_ref(), "1 of 1");
    assert_eq!(ps.last().unwrap().text.as_ref(), "2 of 1");
}

#[test]
fn page_limit_also_applies_between_sections() {
    let body = format!(
        "{}<w:p><w:pPr><w:sectPr/></w:pPr></w:p>{}",
        p("First"),
        p("Second")
    );
    let mut doc = open_document(
        &package(&body, "", "", &[]),
        &OpenOptions {
            max_pages: 1,
            ..OpenOptions::default()
        },
    )
    .unwrap();
    doc.layout().unwrap();
    assert_eq!(doc.pages().len(), 1);
    assert!(!doc.all_text().contains("Second"));
    assert!(doc
        .diagnostics()
        .iter()
        .any(|d| d.code == "display.page_limit"));
}

#[test]
fn known_fidelity_gaps_are_reported_and_relayout_does_not_duplicate_diagnostics() {
    let body = r#"<w:p><w:pPr><w:framePr w:dropCap="drop" w:lines="3"/></w:pPr><w:r><w:t>Drop cap</w:t></w:r></w:p>"#;
    let mut doc = render(body, "", "", &[]);
    assert!(doc
        .diagnostics()
        .iter()
        .any(|d| d.code == "display.unsupported_layout" && d.detail.contains("drop caps")));
    let before = doc.diagnostics().to_vec();
    doc.layout().unwrap();
    assert_eq!(doc.diagnostics(), before);
    let summary = edgedoc_display::diagnose::summary_for("display.unsupported_layout");
    assert!(summary.contains("not fully rendered"));
}

#[test]
fn final_columns_balance_before_a_continuous_section() {
    let body = format!(
        r#"{}<w:p><w:pPr><w:sectPr><w:cols w:num="2"/></w:sectPr></w:pPr></w:p>{}"#,
        (0..8).map(|i| p(&format!("Before{i}"))).collect::<String>(),
        p("After")
    );
    let doc = render(&body, r#"<w:type w:val="continuous"/>"#, "", &[]);
    let page = &doc.pages()[0];
    let ps = paragraphs(&doc, 0);
    let left = ps
        .iter()
        .filter(|p| p.text.as_ref() != "After" && p.rect.x < page.body.x + page.body.w / 2.0)
        .collect::<Vec<_>>();
    let right = ps
        .iter()
        .filter(|p| p.text.as_ref() != "After" && p.rect.x > page.body.x + page.body.w / 2.0)
        .collect::<Vec<_>>();
    assert!(
        !left.is_empty() && !right.is_empty(),
        "both columns contain text"
    );
    let left_bottom = left.iter().map(|p| p.rect.bottom()).fold(0.0, f32::max);
    let right_bottom = right.iter().map(|p| p.rect.bottom()).fold(0.0, f32::max);
    assert!(
        (left_bottom - right_bottom).abs() < 25.0,
        "left {left_bottom}, right {right_bottom}: {ps:?}"
    );
}

#[test]
fn balancing_the_last_page_preserves_a_paragraph_continued_from_an_earlier_page() {
    let text = (0..140)
        .map(|i| format!("L{i:03}"))
        .collect::<Vec<_>>()
        .join("</w:t><w:br/><w:t>");
    let body = format!(
        r#"<w:p><w:r><w:t>{text}</w:t></w:r></w:p><w:p><w:pPr><w:sectPr><w:cols w:num="2"/></w:sectPr></w:pPr></w:p>{}"#,
        p("After")
    );
    let doc = render(&body, r#"<w:type w:val="continuous"/>"#, "", &[]);
    assert!(doc.pages().len() >= 2);
    let shown = doc.all_text();
    for i in 0..140 {
        assert_eq!(shown.matches(&format!("L{i:03}")).count(), 1);
    }
    let last = doc.pages().last().unwrap();
    let mid = last.body.x + last.body.w / 2.0;
    let ps = paragraphs(&doc, doc.pages().len() - 1);
    let before = ps
        .iter()
        .filter(|p| p.text.as_ref() != "After")
        .collect::<Vec<_>>();
    assert!(before.iter().any(|p| p.rect.x < mid));
    assert!(before.iter().any(|p| p.rect.x > mid));
    let left = before
        .iter()
        .filter(|p| p.rect.x < mid)
        .map(|p| p.rect.bottom())
        .fold(0.0, f32::max);
    let right = before
        .iter()
        .filter(|p| p.rect.x > mid)
        .map(|p| p.rect.bottom())
        .fold(0.0, f32::max);
    assert!((left - right).abs() < 25.0, "{left} versus {right}");
}

#[test]
fn fixed_and_multiple_spacing_follow_wordcraft_baseline_rules() {
    let p = |line, rule| {
        format!(
            r#"<w:p><w:pPr><w:spacing w:line="{line}" w:lineRule="{rule}"/></w:pPr><w:r><w:t>Baseline</w:t></w:r></w:p>"#
        )
    };
    for rule in ["exact", "atLeast"] {
        let doc = render(&p(600, rule), "", "", &[]);
        let line = &paragraphs(&doc, 0)[0].layout.lines[0];
        assert!((line.baseline - (line.height - line.text_desc)).abs() < 0.001);
    }
    let single = render(&p(240, "auto"), "", "", &[]);
    let double = render(&p(480, "auto"), "", "", &[]);
    let a = &paragraphs(&single, 0)[0].layout.lines[0];
    let b = &paragraphs(&double, 0)[0].layout.lines[0];
    assert!(
        (a.baseline - b.baseline).abs() < 0.001,
        "leading is below the baseline"
    );
    assert!((a.height * 2.0 - b.height).abs() < 0.001);
}

#[test]
fn separate_clipped_paragraphs_have_distinct_svg_clip_paths() {
    let body = ["First", "Second", "Third"].iter().map(|text| format!(r#"<w:p><w:pPr><w:spacing w:line="480" w:lineRule="exact"/></w:pPr><w:r><w:t>{text}</w:t></w:r></w:p>"#)).collect::<String>();
    let doc = render(&body, "", "", &[]);
    let svg = doc.to_svg(0).unwrap();
    let ids = svg
        .split("<clipPath id=\"")
        .skip(1)
        .map(|s| s.split('"').next().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(ids.len(), 3);
    let unique = ids.iter().collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        unique.len(),
        3,
        "reusing an ID clips later paragraphs to the first paragraph's box"
    );
}

#[test]
fn changing_fonts_invalidates_previous_layout() {
    let mut doc = render(&p("Fonts"), "", "", &[]);
    assert!(doc.report().is_some());
    assert!(doc.page(0).is_some());
    let _ = doc.fonts_mut();
    assert!(doc.pages().is_empty());
    assert!(doc.report().is_none());
    assert!(doc.page(0).is_none());
    doc.layout().unwrap();
    assert!(doc.page(0).is_some());
}

#[test]
fn omitted_import_properties_follow_wordcraft_defaults() {
    let bytes = package(&format!("{}{}", p("First"), p("Second")), "", "", &[]);
    let source = edgedoc::Document::read_bytes(&bytes).unwrap();
    let (run, para) = source.default_formatting().unwrap();
    assert_eq!(run.size_pt, Some(12.0));
    assert_eq!(para.spacing_after, Some(edgedoc::Twips(160)));
    let mut doc = open_document(&bytes, &OpenOptions::default()).unwrap();
    doc.layout().unwrap();
    assert_eq!((doc.pages()[0].w, doc.pages()[0].h), (612.0, 792.0));
    let ps = paragraphs(&doc, 0);
    assert_eq!(ps[0].run_styles[0].size, 12.0);
    assert!((ps[1].rect.y - ps[0].rect.bottom() - 8.0).abs() < 0.001);
}

#[test]
fn document_defaults_are_cascaded_without_rewriting_the_preserved_markup() {
    let source = edgedoc::Document::builder()
        .paragraph(edgedoc::Paragraph::new().text("Defaults"))
        .build()
        .unwrap();
    let bytes = replace_part(&source.write_bytes().unwrap(), "word/styles.xml", |_| {
        format!(
            r#"<w:styles xmlns:w="{W}"><w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="Carlito"/><w:sz w:val="32"/><w:b/><w:color w:val="123456"/></w:rPr></w:rPrDefault><w:pPrDefault><w:pPr><w:jc w:val="right"/><w:spacing w:line="600" w:lineRule="exact" w:after="120"/></w:pPr></w:pPrDefault></w:docDefaults></w:styles>"#
        )
    });
    let source = edgedoc::Document::read_bytes(&bytes).unwrap();
    let raw = source.styles().doc_defaults_xml.clone();
    let (run, para) = source.default_formatting().unwrap();
    assert_eq!(run.size_pt, Some(16.0));
    assert!(run.bold);
    assert_eq!(para.spacing_after, Some(edgedoc::Twips(120)));
    assert_eq!(source.styles().doc_defaults_xml, raw);
    let mut doc = open_document(&bytes, &OpenOptions::default()).unwrap();
    doc.layout().unwrap();
    let line = &paragraphs(&doc, 0)[0].layout.lines[0];
    assert!((line.height - 30.0).abs() < 0.001);
    assert!(line.xs[0] > 200.0);
    assert!(doc.page(0).unwrap().ops.iter().any(|op| matches!(op, edgedoc_display::Op::Glyphs(g) if g.size == 16.0 && doc.fonts().face(g.face).unwrap().bold)));
}

#[test]
fn ligatures_and_kerning_survive_layout_and_paint() {
    let doc = render(&p("office AVATAR"), "", "", &[]);
    let para = paragraphs(&doc, 0)[0];
    assert!(
        para.layout
            .clusters
            .iter()
            .any(|c| para.text[c.start..c.end].chars().count() > 1),
        "Carlito's fi/ffi ligature must survive contextual shaping"
    );
    assert_eq!(doc.page_text(0).unwrap(), "office AVATAR");
    let face = doc.fonts().face(para.run_styles[0].face).unwrap();
    let unkerned: f32 = "AVATAR"
        .chars()
        .map(|ch| face.advance(face.glyph_for(ch).unwrap(), 12.0))
        .sum();
    let kerned: f32 = para
        .layout
        .clusters
        .iter()
        .filter(|c| c.start >= 7)
        .map(|c| c.adv)
        .sum();
    assert!(kerned < unkerned - 0.5);
    let svg = doc.to_svg(0).unwrap();
    assert!(
        svg.contains("<path d="),
        "SVG uses the actual glyph outlines"
    );
    assert!(
        svg.contains("fill-opacity=\"0\""),
        "SVG retains readable text without repainting with a substitute font"
    );
}

#[test]
fn hanging_indent_supplies_a_tab_before_a_later_explicit_stop() {
    let body = r#"<w:p><w:pPr><w:tabs><w:tab w:val="left" w:pos="737"/></w:tabs><w:ind w:left="720" w:hanging="720"/></w:pPr><w:r><w:t>15</w:t><w:tab/></w:r><w:r><w:rPr><w:b/></w:rPr><w:t>Heading</w:t></w:r></w:p>"#;
    let doc = render(body, "", "", &[]);
    let heading = doc
        .page(0)
        .unwrap()
        .ops
        .iter()
        .find_map(|op| match op {
            edgedoc_display::Op::Glyphs(g) if g.text.contains("Heading") => Some(g),
            _ => None,
        })
        .unwrap();
    assert!(
        (heading.x - (doc.pages()[0].body.x + 36.0)).abs() < 0.01,
        "heading {}, body {}",
        heading.x,
        doc.pages()[0].body.x
    );
}

#[test]
fn automatic_width_fixed_table_keeps_grid_beyond_column() {
    let body = format!(
        r#"<w:tbl><w:tblPr><w:tblW w:w="0" w:type="auto"/><w:tblLayout w:type="fixed"/></w:tblPr><w:tblGrid><w:gridCol w:w="2376"/><w:gridCol w:w="6804"/><w:gridCol w:w="6"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:w="2376" w:type="dxa"/></w:tcPr>{}</w:tc><w:tc><w:tcPr><w:tcW w:w="6804" w:type="dxa"/></w:tcPr>{}</w:tc></w:tr></w:tbl>"#,
        p("First"),
        p("Second")
    );
    let section = r#"<w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1304" w:bottom="1134" w:left="1701" w:right="1701"/>"#;
    let doc = render(&body, section, "", &[]);
    let page = &doc.pages()[0];
    let table = page
        .fragments
        .iter()
        .find_map(|fragment| match fragment {
            Fragment::Table(table) => Some(table),
            _ => None,
        })
        .unwrap();
    assert!(table.rect.w > page.body.w);
    assert!((table.rect.w - 459.3).abs() < 0.01);
    assert!((table.cells[0].rect.w - 118.8).abs() < 0.01);
}

#[test]
fn kept_heading_moved_to_new_page_drops_consumed_space_before() {
    let body = r#"<w:p><w:pPr><w:spacing w:line="1200" w:lineRule="exact"/></w:pPr><w:r><w:t>Leading</w:t></w:r></w:p><w:p><w:pPr><w:keepNext/><w:spacing w:before="120"/></w:pPr><w:r><w:t>Heading</w:t></w:r></w:p><w:p><w:r><w:t>Following</w:t></w:r></w:p>"#;
    let section = r#"<w:pgSz w:w="8000" w:h="2000"/><w:pgMar w:top="200" w:bottom="200" w:left="400" w:right="400"/>"#;
    let doc = render(body, section, "", &[]);
    assert_eq!(doc.pages().len(), 2);
    let heading = paragraphs(&doc, 1)
        .into_iter()
        .find(|p| p.text.contains("Heading"))
        .unwrap();
    assert!((heading.rect.y - doc.pages()[1].body.y).abs() < 0.01);
}

#[test]
fn keep_with_next_lookahead_reserves_each_paragraph_spacing() {
    fn para(text: &str, keep: bool, before: i32, after: i32, exact: Option<i32>) -> String {
        let font =
            r#"<w:rPr><w:rFonts w:ascii="Arial" w:hAnsi="Arial"/><w:sz w:val="24"/></w:rPr>"#;
        let line = exact.map_or_else(String::new, |n| {
            format!(r#" w:line="{n}" w:lineRule="exact""#)
        });
        let run = if text.is_empty() {
            String::new()
        } else {
            format!("<w:r>{font}<w:t>{text}</w:t></w:r>")
        };
        format!(
            r#"<w:p><w:pPr>{}<w:spacing w:before="{before}" w:after="{after}"{line}/>{font}</w:pPr>{run}</w:p>"#,
            if keep { "<w:keepNext/>" } else { "" }
        )
    }
    let body = format!(
        "{}{}{}{}{}{}{}",
        para("Leading", false, 0, 0, Some(400)),
        para("", true, 0, 0, None),
        para("Heading", true, 120, 60, None),
        para("omit", true, 120, 0, None),
        para("OCA", true, 60, 60, None),
        para("insert", true, 120, 0, None),
        para("CTR", false, 60, 60, None)
    );
    let section = r#"<w:pgSz w:w="8000" w:h="3300"/><w:pgMar w:top="200" w:bottom="200" w:left="400" w:right="400"/>"#;
    let mut doc =
        open_document(&package(&body, section, "", &[]), &OpenOptions::default()).unwrap();
    doc.fonts_mut()
        .load(
            include_bytes!("../../../assets/fonts/subset/Arimo-Regular.ttf").to_vec(),
            Some("Arial"),
            false,
            false,
            false,
        )
        .unwrap();
    doc.layout().unwrap();
    assert_eq!(doc.pages().len(), 2);
    let first = paragraphs(&doc, 0);
    assert!(first.iter().any(|p| p.text.contains("Leading")));
    assert!(!first.iter().any(|p| p.text.contains("Heading")));
    assert!(paragraphs(&doc, 1)
        .iter()
        .any(|p| p.text.contains("Heading")));
}

#[test]
fn only_explicitly_disabled_known_paragraph_switches_are_diagnostic_free() {
    fn has_extended(extra: &str) -> bool {
        let body = format!(r#"<w:p><w:pPr>{extra}</w:pPr><w:r><w:t>Text</w:t></w:r></w:p>"#);
        render(&body, "", "", &[])
            .diagnostics()
            .iter()
            .any(|d| d.detail == "Extended paragraph properties")
    }
    assert!(!has_extended(
        r#"<w:autoSpaceDE w:val="0"/><w:autoSpaceDN w:val="false"/><w:adjustRightInd w:val="off"/>"#
    ));
    assert!(has_extended(r#"<w:autoSpaceDE/>"#));
    assert!(has_extended(r#"<w:autoSpaceDN w:val="1"/>"#));
    assert!(has_extended(
        r#"<w:adjustRightInd w:val="0" w:unknown="1"/>"#
    ));
    assert!(has_extended(r#"<w:other w:val="0"/>"#));
}

#[test]
fn table_after_page_break_does_not_repeat_previous_paragraph_spacing() {
    let body = format!(
        r#"<w:p><w:pPr><w:spacing w:after="200"/></w:pPr><w:r><w:t>Before</w:t><w:br w:type="page"/></w:r></w:p><w:tbl><w:tblGrid><w:gridCol w:w="4000"/></w:tblGrid><w:tr><w:tc>{}</w:tc></w:tr></w:tbl>"#,
        p("Table")
    );
    let doc = render(&body, "", "", &[]);
    assert_eq!(doc.pages().len(), 2);
    let page = &doc.pages()[1];
    let table = page
        .fragments
        .iter()
        .find_map(|fragment| match fragment {
            Fragment::Table(table) => Some(table),
            _ => None,
        })
        .expect("table on the second page");
    assert!((table.rect.y - page.body.y).abs() < 0.01);
}

#[test]
fn modern_start_aligned_table_uses_table_left_border_when_cell_has_none() {
    fn table_x(border: &str) -> f32 {
        let body = format!(
            r#"<w:tbl><w:tblPr><w:tblInd w:w="-572" w:type="dxa"/>{border}</w:tblPr><w:tblGrid><w:gridCol w:w="4000"/></w:tblGrid><w:tr><w:tc>{}</w:tc></w:tr></w:tbl>"#,
            p("Cell")
        );
        let settings = r#"<w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/></w:compat>"#;
        let doc = render(&body, "", settings, &[]);
        doc.pages()[0]
            .fragments
            .iter()
            .find_map(|fragment| match fragment {
                Fragment::Table(table) => Some(table.rect.x),
                _ => None,
            })
            .unwrap()
    }
    let plain = table_x("");
    let bordered = table_x(r#"<w:tblBorders><w:left w:val="single" w:sz="4"/></w:tblBorders>"#);
    assert!((bordered - plain - 0.25).abs() < 0.001);
}

#[test]
fn table_compatibility_mode_controls_whether_text_or_border_aligns_to_margin() {
    let body = format!("<w:tbl><w:tblGrid><w:gridCol w:w=\"4000\"/></w:tblGrid><w:tr><w:tc>{}</w:tc></w:tr></w:tbl>", p("Cell"));
    let legacy = render(&body, "", "", &[]);
    let modern = render(
        &body,
        "",
        r#"<w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/></w:compat>"#,
        &[],
    );
    let cell_x = |doc: &DisplayDocument| {
        doc.page(0)
            .unwrap()
            .ops
            .iter()
            .find_map(|op| match op {
                edgedoc_display::Op::Glyphs(g) => Some(g.x),
                _ => None,
            })
            .unwrap()
    };
    assert!((cell_x(&legacy) - legacy.pages()[0].body.x).abs() < 0.001);
    assert!((cell_x(&modern) - modern.pages()[0].body.x - 5.4).abs() < 0.001);
}

#[test]
fn missing_styles_defaults_survive_an_edited_save() {
    let bytes = package(&p("Before"), "", "", &[]);
    let mut source = edgedoc::Document::read_bytes(&bytes).unwrap();
    let before = source.default_formatting().unwrap();
    source
        .edit(edgedoc::EditOptions::default(), |edit| {
            edit.body()
                .paragraph_at(0, |paragraph| paragraph.text(" after"))?;
            Ok(())
        })
        .unwrap();
    let saved = source.write_bytes().unwrap();
    let restored = edgedoc::Document::read_bytes(&saved).unwrap();
    let after = restored.default_formatting().unwrap();
    assert_eq!(
        before, after,
        "creating a styles part must not change the import fallback"
    );
    let mut display = open_document(&saved, &OpenOptions::default()).unwrap();
    display.layout().unwrap();
    assert_eq!(paragraphs(&display, 0)[0].run_styles[0].size, 12.0);
    assert_eq!(display.page_text(0).unwrap(), "Before after");
}

fn add_part(bytes: &[u8], name: &str, xml: &str, relation: &str, content_type: &str) -> Vec<u8> {
    let bytes = replace_part(bytes, "[Content_Types].xml", |source| {
        source.replace(
            "</Types>",
            &format!(r#"<Override PartName="/{name}" ContentType="{content_type}"/></Types>"#),
        )
    });
    let bytes = replace_part(&bytes, "word/_rels/document.xml.rels", |source| {
        source.replace("</Relationships>", &format!(r#"<Relationship Id="extra-{relation}" Type="{R}/{relation}" Target="{}"/></Relationships>"#, name.strip_prefix("word/").unwrap()))
    });
    let mut archive = zip::ZipWriter::new_append(Cursor::new(bytes)).unwrap();
    archive
        .start_file(name, zip::write::FileOptions::default())
        .unwrap();
    archive.write_all(xml.as_bytes()).unwrap();
    archive.finish().unwrap().into_inner()
}

fn styled_package(body: &str, styles: &str) -> Vec<u8> {
    add_part(
        &package(body, "", "", &[]),
        "word/styles.xml",
        styles,
        "styles",
        "application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml",
    )
}

#[test]
fn a_localized_default_style_applies_when_the_paragraph_omits_pstyle() {
    // Apache POI Styles.docx uses Standard as its default paragraph style ID.
    let styles = format!(
        r#"<w:styles xmlns:w="{W}"><w:style w:type="paragraph" w:default="1" w:styleId="Standard"><w:name w:val="Normal"/><w:pPr><w:jc w:val="right"/></w:pPr><w:rPr><w:sz w:val="36"/><w:b/></w:rPr></w:style></w:styles>"#
    );
    let bytes = styled_package(&p("Default style"), &styles);
    let mut doc = open_document(&bytes, &OpenOptions::default()).unwrap();
    doc.layout().unwrap();
    let para = paragraphs(&doc, 0)[0];
    assert_eq!(para.run_styles[0].size, 18.0);
    assert!(doc.fonts().face(para.run_styles[0].face).unwrap().bold);
    assert!(para.layout.lines[0].xs[0] > 200.0);
}

#[test]
fn utf8_bom_does_not_truncate_preserved_style_defaults() {
    // docx4j NumberingImplicitNumId and arabic_numbering have BOM-prefixed styles parts.
    let styles = format!("\u{FEFF}<w:styles xmlns:w=\"{W}\"><w:docDefaults><w:rPrDefault><w:rPr><w:sz w:val=\"28\"/></w:rPr></w:rPrDefault></w:docDefaults></w:styles>");
    let bytes = styled_package(&p("BOM defaults"), &styles);
    let source = edgedoc::Document::read_bytes(&bytes).unwrap();
    let raw = source.styles().doc_defaults_xml.as_ref().unwrap();
    assert!(raw.starts_with("<w:docDefaults>"));
    assert!(raw.ends_with("</w:docDefaults>"));
    assert_eq!(source.default_formatting().unwrap().0.size_pt, Some(14.0));
    let mut doc = open_document(&bytes, &OpenOptions::default()).unwrap();
    doc.layout().unwrap();
    assert_eq!(paragraphs(&doc, 0)[0].run_styles[0].size, 14.0);
}

#[test]
fn style_extensions_are_sliced_from_the_styles_part() {
    let styles = format!(
        r#"<w:styles xmlns:w="{W}"><w:style w:type="paragraph" w:styleId="Normal"><w:pPr><w:suppressAutoHyphens w:val="1"/></w:pPr></w:style></w:styles>"#
    );
    let bytes = styled_package(&p("Body source differs from styles source"), &styles);
    let source = edgedoc::Document::read_bytes(&bytes).unwrap();
    assert_eq!(
        source.styles().by_id("Normal").unwrap().para_props.extra,
        vec![r#"<w:suppressAutoHyphens w:val="1"/>"#]
    );
}

#[test]
fn theme_fonts_override_literal_names_and_survive_an_edited_save() {
    let styles = format!(r#"<w:styles xmlns:w="{W}"/>"#);
    let body = r#"<w:p><w:r><w:rPr><w:rFonts w:ascii="Arimo" w:asciiTheme="majorAscii" w:hAnsi="Arimo"/></w:rPr><w:t>Major</w:t></w:r></w:p><w:p><w:r><w:rPr><w:rFonts w:ascii="Carlito" w:asciiTheme="minorAscii"/></w:rPr><w:t>Minor</w:t></w:r></w:p>"#;
    let theme = r#"<a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><a:themeElements><a:fontScheme name="Original"><a:majorFont><a:latin typeface="Carlito"/></a:majorFont><a:minorFont><a:latin typeface="Arimo"/></a:minorFont></a:fontScheme></a:themeElements></a:theme>"#;
    let bytes = add_part(
        &styled_package(body, &styles),
        "word/theme/theme1.xml",
        theme,
        "theme",
        "application/vnd.openxmlformats-officedocument.theme+xml",
    );
    let mut source = edgedoc::Document::read_bytes(&bytes).unwrap();
    source
        .edit(edgedoc::EditOptions::default(), |edit| {
            edit.body()
                .paragraph_at(0, |paragraph| paragraph.text(" edited"))?;
            Ok(())
        })
        .unwrap();
    let bytes = source.write_bytes().unwrap();
    let mut doc = open_document(&bytes, &OpenOptions::default()).unwrap();
    doc.fonts_mut()
        .load(
            include_bytes!("../../../assets/fonts/subset/Arimo-Regular.ttf").to_vec(),
            None,
            false,
            false,
            false,
        )
        .unwrap();
    doc.layout().unwrap();
    let ps = paragraphs(&doc, 0);
    assert_eq!(
        doc.fonts().face(ps[0].run_styles[0].face).unwrap().family,
        "Carlito"
    );
    assert_eq!(
        doc.fonts().face(ps[1].run_styles[0].face).unwrap().family,
        "Arimo"
    );
}

#[test]
fn proofing_and_bookmark_markers_do_not_insert_visible_object_widths() {
    let body = r#"<w:p><w:r><w:t>First</w:t></w:r><w:proofErr w:type="spellStart"/><w:bookmarkStart w:id="0" w:name="Original"/><w:permStart w:id="1"/><w:r><w:t>Second</w:t></w:r><w:proofErr w:type="spellEnd"/><w:bookmarkEnd w:id="0"/><w:permEnd w:id="1"/></w:p>"#;
    let annotated = render(body, "", "", &[]);
    let plain = render(&p("FirstSecond"), "", "", &[]);
    assert_eq!(annotated.page_text(0).unwrap(), "FirstSecond");
    assert_eq!(
        paragraphs(&annotated, 0)[0].layout.height,
        paragraphs(&plain, 0)[0].layout.height
    );
    assert_eq!(
        paragraphs(&annotated, 0)[0].layout.max_width,
        paragraphs(&plain, 0)[0].layout.max_width
    );
    assert!(!annotated
        .diagnostics()
        .iter()
        .any(|d| d.code == "display.object_placeholder" || d.code == "display.unsupported_layout"));
}

#[test]
fn only_enabled_document_grids_request_unsupported_layout() {
    for section in [
        r#"<w:docGrid w:linePitch="360"/>"#,
        r#"<w:docGrid w:type="default" w:linePitch="360"/>"#,
    ] {
        let doc = render(&p("No active grid"), section, "", &[]);
        assert!(!doc
            .diagnostics()
            .iter()
            .any(|d| d.detail == "Document grid"));
    }
    let active = render(
        &p("Active grid"),
        r#"<w:docGrid w:type="lines" w:linePitch="360"/>"#,
        "",
        &[],
    );
    assert!(active
        .diagnostics()
        .iter()
        .any(|d| d.detail == "Document grid"));
}

#[test]
fn positional_tabs_align_to_margins_or_indents_and_roundtrip() {
    for relative in ["margin", "indent"] {
        let body = format!(
            r#"<w:p><w:pPr><w:ind w:left="720" w:right="360"/></w:pPr><w:r><w:t>Left</w:t><w:ptab w:alignment="right" w:relativeTo="{relative}" w:leader="none"/><w:t>Right</w:t></w:r></w:p>"#
        );
        let bytes = package(&body, "", "", &[]);
        let mut source = edgedoc::Document::read_bytes(&bytes).unwrap();
        source
            .edit(edgedoc::EditOptions::default(), |edit| {
                edit.body().paragraph_at(0, |p| p.text("!"))?;
                Ok(())
            })
            .unwrap();
        let mut doc =
            open_document(&source.write_bytes().unwrap(), &OpenOptions::default()).unwrap();
        doc.layout().unwrap();
        let right = doc
            .page(0)
            .unwrap()
            .ops
            .iter()
            .find_map(|op| match op {
                edgedoc_display::Op::Glyphs(g) if g.text.contains("Right") => Some(g),
                _ => None,
            })
            .unwrap();
        let expected = doc.pages()[0].body.right() - if relative == "indent" { 18.0 } else { 0.0 };
        assert!(
            (right.x + right.width() - expected).abs() < 0.01,
            "{relative}: {right:?}"
        );
        assert!(!doc
            .diagnostics()
            .iter()
            .any(|d| d.code == "display.object_placeholder"));
    }
}

#[test]
fn right_center_and_decimal_tabs_measure_the_text_that_follows() {
    for alignment in ["right", "center", "decimal"] {
        let body = format!(
            r#"<w:p><w:pPr><w:tabs><w:tab w:val="{alignment}" w:pos="4000"/></w:tabs></w:pPr><w:r><w:t>Left</w:t><w:tab/><w:t>12.34</w:t></w:r></w:p>"#
        );
        let doc = render(&body, "", "", &[]);
        let para = paragraphs(&doc, 0)[0];
        let line = &para.layout.lines[0];
        let first = para
            .layout
            .clusters
            .iter()
            .position(|c| &para.text[c.start..c.end] == "1")
            .unwrap();
        let last = para.layout.clusters.len() - 1;
        let end = line.xs[last] + para.layout.clusters[last].adv;
        let anchor = match alignment {
            "right" => end,
            "center" => (line.xs[first] + end) / 2.0,
            _ => {
                line.xs[para
                    .layout
                    .clusters
                    .iter()
                    .position(|c| &para.text[c.start..c.end] == ".")
                    .unwrap()]
            }
        };
        assert!((anchor - 200.0).abs() < 0.01, "{alignment}: {anchor}");
    }
}

#[test]
fn paragraph_space_before_is_retained_at_document_start() {
    let doc = render(
        r#"<w:p><w:pPr><w:spacing w:before="480"/></w:pPr><w:r><w:t>Heading</w:t></w:r></w:p>"#,
        "",
        "",
        &[],
    );
    assert!((paragraphs(&doc, 0)[0].rect.y - doc.pages()[0].body.y - 24.0).abs() < 0.01);
}

fn open_document(bytes: &[u8], options: &OpenOptions) -> edgedoc_display::Result<DisplayDocument> {
    if let Ok(directory) = std::env::var("EDGEDOC_PARITY_FIXTURES") {
        let mut hash = 0xcbf2_9ce4_8422_2325u64;
        for byte in bytes {
            hash = (hash ^ u64::from(*byte)).wrapping_mul(0x100_0000_01b3);
        }
        let thread = std::thread::current();
        let name = thread.name().unwrap_or("fixture");
        let directory = std::path::Path::new(&directory);
        std::fs::create_dir_all(directory).unwrap();
        std::fs::write(directory.join(format!("{name}-{hash:016x}.docx")), bytes).unwrap();
    }
    DisplayDocument::open(bytes, options)
}

#[test]
fn explicit_page_break_off_overrides_style_and_survives_an_edit() {
    let source = include_bytes!("fixtures/paragraph/break-override.docx");
    let bytes = replace_part(source, "word/document.xml", |xml| {
        xml.replace(
            r#"<w:pageBreakBefore w:val="0"/>"#,
            r#"<w:pageBreakBefore w:val="0"/><w:keepNext w:val="0"/><w:keepLines w:val="0"/><w:contextualSpacing w:val="0"/><w:suppressLineNumbers w:val="0"/><w:widowControl/>"#,
        )
    });
    let mut doc = open_document(&bytes, &OpenOptions::default()).unwrap();
    doc.layout().unwrap();
    assert_eq!(doc.pages().len(), 1);
    assert_eq!(doc.page_text(0).unwrap(), "First paragraphSecond paragraph");

    let mut model = edgedoc::Document::read_bytes(&bytes).unwrap();
    model.add_style(edgedoc_model::Style::paragraph("EditedStyle"));
    let saved = model.write_bytes().unwrap();
    let mut zip = zip::ZipArchive::new(Cursor::new(&saved)).unwrap();
    let mut xml = String::new();
    zip.by_name("word/document.xml")
        .unwrap()
        .read_to_string(&mut xml)
        .unwrap();
    assert!(xml.contains(r#"<w:pageBreakBefore w:val="0"/>"#));
    for name in [
        "keepNext",
        "keepLines",
        "contextualSpacing",
        "suppressLineNumbers",
    ] {
        assert!(xml.contains(&format!(r#"<w:{name} w:val="0"/>"#)));
    }
    assert!(xml.contains("<w:widowControl/>"));
    let mut after = open_document(&saved, &OpenOptions::default()).unwrap();
    after.layout().unwrap();
    assert_eq!(after.pages().len(), 1);
}

#[test]
fn disabled_word_wrap_breaks_latin_inside_words_and_survives_editing() {
    let bytes = include_bytes!("fixtures/paragraph/wordwrap-off.docx");
    let mut doc = open_document(bytes, &OpenOptions::default()).unwrap();
    doc.layout().unwrap();
    let paragraph = paragraphs(&doc, 0)[0];
    let lines: Vec<_> = paragraph
        .layout
        .lines
        .iter()
        .map(|line| &paragraph.text[line.start..line.stop])
        .collect();
    assert_eq!(lines, ["Start abcdefgh", "ijklmnopqrstuv", "wxyz"]);

    let mut model = edgedoc::Document::read_bytes(bytes).unwrap();
    model.add_style(edgedoc_model::Style::paragraph("EditedStyle"));
    let saved = model.write_bytes().unwrap();
    let mut zip = zip::ZipArchive::new(Cursor::new(&saved)).unwrap();
    let mut xml = String::new();
    zip.by_name("word/document.xml")
        .unwrap()
        .read_to_string(&mut xml)
        .unwrap();
    assert!(xml.contains(r#"<w:wordWrap w:val="0"/>"#));
    let mut after = open_document(&saved, &OpenOptions::default()).unwrap();
    after.layout().unwrap();
    let paragraph = paragraphs(&after, 0)[0];
    let lines: Vec<_> = paragraph
        .layout
        .lines
        .iter()
        .map(|line| &paragraph.text[line.start..line.stop])
        .collect();
    assert_eq!(lines, ["Start abcdefgh", "ijklmnopqrstuv", "wxyz"]);
}

#[test]
fn spaces_and_tabs_do_not_enlarge_line_height() {
    let mark = r#"<w:pPr><w:rPr><w:sz w:val="16"/></w:rPr></w:pPr>"#;
    let body = format!(
        r#"<w:p>{mark}<w:r><w:rPr><w:sz w:val="40"/></w:rPr><w:t xml:space="preserve">  </w:t><w:tab/></w:r></w:p><w:p>{mark}</w:p><w:p><w:r><w:rPr><w:sz w:val="16"/></w:rPr><w:t>A</w:t></w:r><w:r><w:rPr><w:sz w:val="40"/></w:rPr><w:t xml:space="preserve"> </w:t><w:tab/></w:r></w:p><w:p><w:r><w:rPr><w:sz w:val="16"/></w:rPr><w:t>A</w:t></w:r></w:p>"#
    );
    let doc = render(&body, "", "", &[]);
    let p = paragraphs(&doc, 0);
    assert!((p[0].layout.height - p[1].layout.height).abs() < 0.01);
    assert!((p[2].layout.height - p[3].layout.height).abs() < 0.01);
}

#[test]
fn createdate_field_displays_its_saved_result() {
    let body = r#"<w:p><w:r><w:t>Date: </w:t></w:r><w:fldSimple w:instr="CREATEDATE \@ &quot;d MMMM yyyy&quot;"><w:r><w:t>16 June 2010</w:t></w:r></w:fldSimple></w:p>"#;
    let doc = render(body, "", "", &[]);
    assert_eq!(doc.page_text(0).unwrap(), "Date: 16 June 2010");
    assert!(!doc
        .diagnostics()
        .iter()
        .any(|d| d.detail.contains("Fields other than")));
}

#[test]
fn metadata_fields_use_document_properties_and_default_name() {
    let bytes = include_bytes!("fixtures/fields/metadata.docx");
    let mut doc = open_document(bytes, &OpenOptions::default()).unwrap();
    doc.layout().unwrap();
    assert_eq!(
        doc.page_text(0).unwrap(),
        "Title: Example report; Author: JANE DOE; File: Document1"
    );
    assert!(!doc
        .diagnostics()
        .iter()
        .any(|d| d.detail.contains("Fields other than")));

    let mut model = edgedoc::Document::read_bytes(bytes).unwrap();
    let mut core = model.core_props().clone();
    core.title = "Another title".into();
    core.creator = "Other author".into();
    model.set_core_props(core);
    let mut changed =
        open_document(&model.write_bytes().unwrap(), &OpenOptions::default()).unwrap();
    changed.layout().unwrap();
    assert_eq!(
        changed.page_text(0).unwrap(),
        "Title: Another title; Author: OTHER AUTHOR; File: Document1"
    );
}

#[test]
fn assignment_fields_do_not_paint_their_cached_results() {
    for field in [
        r#"<w:fldSimple w:instr="ask name"><w:r><w:t>Hidden</w:t></w:r></w:fldSimple>"#,
        r#"<w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText>SET name value</w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>Hidden</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r>"#,
    ] {
        let doc = render(
            &format!("<w:p><w:r><w:t>Left</w:t></w:r>{field}<w:r><w:t>Right</w:t></w:r></w:p>"),
            "",
            "",
            &[],
        );
        assert_eq!(doc.page_text(0).unwrap(), "LeftRight");
    }
}

#[test]
fn fields_keep_the_result_runs_font_and_do_not_display_xml_indentation() {
    let properties = r#"<w:rPr><w:rFonts w:ascii="Tinos"/><w:sz w:val="40"/><w:b/></w:rPr>"#;
    for field in [
        format!("<w:fldSimple w:instr=\"PAGE\">\n  <w:r>{properties}<w:t>7</w:t></w:r>\n</w:fldSimple>"),
        format!("<w:r><w:fldChar w:fldCharType=\"begin\"/></w:r><w:r><w:instrText>PAGE</w:instrText></w:r><w:r><w:fldChar w:fldCharType=\"separate\"/></w:r><w:r>{properties}<w:t>7</w:t></w:r><w:r><w:fldChar w:fldCharType=\"end\"/></w:r>"),
    ] {
        let doc = render(&format!("<w:p>{field}</w:p>"), "", "", &[]);
        assert_eq!(doc.page_text(0).unwrap(), "1");
        let style = &paragraphs(&doc, 0)[0].run_styles[0];
        assert_eq!(style.size, 20.0);
        let face = doc.fonts().face(style.face).unwrap();
        assert_eq!(face.family, "Tinos");
        assert!(face.bold);
    }
}

#[test]
fn space_before_after_explicit_breaks_follows_compatibility_mode() {
    for (mode, expected) in [(12, 16.0), (15, 0.0)] {
        let body = format!("{}<w:p><w:pPr><w:pageBreakBefore/><w:spacing w:before=\"480\"/></w:pPr><w:r><w:t>Next</w:t></w:r></w:p>", p("First"));
        let settings = format!(
            r#"<w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="{mode}"/></w:compat>"#
        );
        let doc = render(&body, "", &settings, &[]);
        assert!(
            (paragraphs(&doc, 1)[0].rect.y - doc.pages()[1].body.y - expected).abs() < 0.01,
            "mode {mode}"
        );
    }
    let body = r#"<w:p><w:r><w:t>First</w:t><w:br w:type="page"/></w:r></w:p><w:p><w:pPr><w:spacing w:before="480"/></w:pPr><w:r><w:t>Next</w:t></w:r></w:p>"#;
    let doc = render(body, "", "", &[]);
    assert!((paragraphs(&doc, 1)[0].rect.y - doc.pages()[1].body.y).abs() < 0.01);
}

#[test]
fn leader_glyphs_paint_without_polluting_the_text_layer() {
    let doc = render(
        r#"<w:p><w:pPr><w:tabs><w:tab w:pos="5000" w:val="right" w:leader="dot"/></w:tabs></w:pPr><w:r><w:t>Heading</w:t><w:tab/><w:t>4</w:t></w:r></w:p>"#,
        "",
        "",
        &[],
    );
    assert_eq!(doc.page_text(0).unwrap(), "Heading\t4");
    assert!(doc.page(0).unwrap().ops.iter().any(|op| matches!(op, edgedoc_display::Op::Glyphs(g) if g.text.is_empty() && !g.glyphs.is_empty())));
    assert!(doc.to_svg(0).unwrap().contains("<path d="));
}

#[test]
fn footer_borders_keep_their_width_and_distance_from_text() {
    let doc = render(
        &p("Body"),
        r#"<w:footerReference w:type="default" r:id="footer1"/>"#,
        "",
        &[(
            "footer1",
            r#"<w:p><w:pPr><w:pBdr><w:top w:val="single" w:sz="24" w:space="1" w:color="622423"/><w:bottom w:val="single" w:sz="16" w:space="2" w:color="224466"/></w:pBdr></w:pPr><w:r><w:t>Bordered footer</w:t></w:r></w:p>"#,
        )],
    );
    let footer = paragraphs(&doc, 0)
        .into_iter()
        .find(|p| p.text.contains("Bordered footer"))
        .unwrap();
    let fills: Vec<_> = doc
        .page(0)
        .unwrap()
        .ops
        .iter()
        .filter_map(|op| {
            if let edgedoc_display::Op::FillRect { rect, paint } = op {
                Some((*rect, paint))
            } else {
                None
            }
        })
        .collect();
    assert!(fills
        .iter()
        .any(|(r, _)| (r.y - (footer.rect.y - 4.0)).abs() < 0.01 && (r.h - 3.0).abs() < 0.01));
    assert!(fills.iter().any(
        |(r, _)| (r.y - (footer.rect.bottom() + 2.0)).abs() < 0.01 && (r.h - 2.0).abs() < 0.01
    ));
}

#[test]
fn leading_tabs_preserve_the_first_glyph_origin_across_style_changes() {
    let doc = render(
        r#"<w:p><w:r><w:tab/><w:t>Prefix.</w:t></w:r><w:r><w:rPr><w:i/></w:rPr><w:t>method</w:t></w:r><w:r><w:t>(value);</w:t></w:r></w:p>"#,
        "",
        "",
        &[],
    );
    let paragraph = paragraphs(&doc, 0)[0];
    let page = doc.page(0).unwrap();
    let run = page
        .ops
        .iter()
        .find_map(|op| match op {
            edgedoc_display::Op::Glyphs(g) if g.text == "Prefix." => Some(g),
            _ => None,
        })
        .unwrap();
    assert!((run.x - paragraph.rect.x - 36.0).abs() < 0.01, "{run:?}");
    assert!(page.ops.iter().any(
        |op| matches!(op, edgedoc_display::Op::Glyphs(g) if g.text == "method" && g.x > run.x)
    ));
}

#[test]
fn explicit_cell_borders_paint_shared_edges_without_table_inside_rules() {
    let cell = |text: &str| {
        format!(
            r#"<w:tc><w:tcPr><w:tcBorders><w:top w:val="single" w:sz="8"/><w:bottom w:val="single" w:sz="8"/><w:start w:val="single" w:sz="8"/><w:end w:val="single" w:sz="8"/></w:tcBorders></w:tcPr>{}</w:tc>"#,
            p(text)
        )
    };
    let body = format!(
        r#"<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w="2500"/><w:gridCol w:w="2500"/></w:tblGrid><w:tr>{}{}</w:tr><w:tr>{}{}</w:tr></w:tbl>"#,
        cell("A"),
        cell("B"),
        cell("C"),
        cell("D")
    );
    let doc = render(&body, "", "", &[]);
    let table = doc.pages()[0]
        .fragments
        .iter()
        .find_map(|f| match f {
            Fragment::Table(t) => Some(t),
            _ => None,
        })
        .unwrap();
    assert_eq!(table.cells.len(), 4);
    assert_eq!(
        table.borders.len(),
        12,
        "each of the 12 cell-edge segments paints once"
    );
    let middle_x = table.cells[0].rect.right();
    let middle_y = table.cells[0].rect.bottom();
    assert!(table
        .borders
        .iter()
        .any(|b| b.rect.h > b.rect.w && (b.rect.x + b.rect.w / 2.0 - middle_x).abs() < 0.01));
    assert!(table
        .borders
        .iter()
        .any(|b| b.rect.w > b.rect.h && (b.rect.y - middle_y).abs() <= 1.01));
    let first = table.cells[0].rect;
    assert!(table.borders.iter().any(|b| {
        b.rect.w > b.rect.h
            && (b.rect.y + b.rect.h / 2.0 - first.y).abs() < 0.01
            && (b.rect.x - (first.x - b.rect.h / 2.0)).abs() < 0.01
            && (b.rect.right() - (first.right() + b.rect.h / 2.0)).abs() < 0.01
    }));
    let top_text = table
        .content
        .iter()
        .find_map(|fragment| match fragment {
            Fragment::Paragraph(p) if p.text.as_ref() == "A" => Some(p),
            _ => None,
        })
        .unwrap();
    assert!(
        (top_text.rect.y - table.cells[0].rect.y - 1.0).abs() < 0.01,
        "top border width must be reserved before cell text"
    );
    let lower = table
        .cells
        .iter()
        .find(|cell| cell.row == 1 && cell.col == 0)
        .unwrap();
    let bottom_text = table
        .content
        .iter()
        .find_map(|fragment| match fragment {
            Fragment::Paragraph(p) if p.text.as_ref() == "C" => Some(p),
            _ => None,
        })
        .unwrap();
    assert!(
        (lower.rect.bottom() - bottom_text.rect.bottom() - bottom_text.para.space_after - 1.0)
            .abs()
            < 0.01,
        "last-row bottom border must have room after the text"
    );
}

#[test]
fn row_content_controls_render_once_and_survive_an_edited_save() {
    let body = r#"<w:p><w:r><w:t>Before</w:t></w:r></w:p><w:tbl><w:tblPr/><w:sdt><w:sdtPr><w:id w:val="1001"/></w:sdtPr><w:sdtContent><w:tr><w:tc><w:p><w:r><w:t>SDT Cell 1</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>SDT Cell 2</w:t></w:r></w:p></w:tc></w:tr></w:sdtContent></w:sdt></w:tbl>"#;
    let original = package(body, "", "", &[]);
    let mut source = edgedoc::Document::read_bytes(&original).unwrap();
    source
        .edit(edgedoc::EditOptions::default(), |edit| {
            edit.body().paragraph_at(0, |p| p.text(" edited"))?;
            Ok(())
        })
        .unwrap();
    let saved = source.write_bytes().unwrap();
    let mut zip = zip::ZipArchive::new(Cursor::new(&saved)).unwrap();
    let mut xml = String::new();
    zip.by_name("word/document.xml")
        .unwrap()
        .read_to_string(&mut xml)
        .unwrap();
    assert!(
        xml.contains("<w:sdt>")
            && xml.contains("<w:id w:val=\"1001\"/>")
            && xml.contains("</w:sdt>"),
        "{xml}"
    );
    let mut doc = open_document(&saved, &OpenOptions::default()).unwrap();
    doc.layout().unwrap();
    assert_eq!(doc.pages().len(), 1);
    let text = doc.all_text();
    assert!(
        text.contains("SDT Cell 1") && text.contains("SDT Cell 2"),
        "{text}"
    );
}

#[test]
fn a_table_without_rows_does_not_create_endless_pages() {
    let doc = render(
        r#"<w:tbl><w:tblPr/></w:tbl><w:p><w:r><w:t>After empty table</w:t></w:r></w:p>"#,
        "",
        "",
        &[],
    );
    assert_eq!(doc.pages().len(), 1);
    assert!(doc.all_text().contains("After empty table"));
    assert!(!doc
        .diagnostics()
        .iter()
        .any(|d| d.code == "display.page_limit"));
}

#[test]
fn explicit_and_default_tabs_are_measured_from_the_column_margin_with_indents() {
    let body = r#"<w:p><w:pPr><w:ind w:left="360"/><w:tabs><w:tab w:val="right" w:pos="8000" w:leader="dot"/></w:tabs></w:pPr><w:r><w:t>Heading</w:t><w:tab/><w:t>4</w:t></w:r></w:p><w:p><w:pPr><w:tabs><w:tab w:val="right" w:pos="8000" w:leader="dot"/></w:tabs></w:pPr><w:r><w:t>Heading</w:t><w:tab/><w:t>4</w:t></w:r></w:p><w:p><w:pPr><w:ind w:left="360"/></w:pPr><w:r><w:t>A</w:t><w:tab/><w:t>B</w:t></w:r></w:p>"#;
    let doc = render(body, "", "", &[]);
    let paras = paragraphs(&doc, 0);
    for para in &paras[..2] {
        assert_eq!(
            para.layout.lines.len(),
            1,
            "indented right tab must fit on one line"
        );
        let cluster = para
            .layout
            .clusters
            .iter()
            .position(|c| &para.text[c.start..c.end] == "4")
            .unwrap();
        let right =
            para.rect.x + para.layout.lines[0].xs[cluster] + para.layout.clusters[cluster].adv;
        assert!(
            (right - (doc.pages()[0].body.x + 400.0)).abs() < 0.05,
            "right edge {right}"
        );
    }
    let para = paras[2];
    let cluster = para
        .layout
        .clusters
        .iter()
        .position(|c| &para.text[c.start..c.end] == "B")
        .unwrap();
    let left = para.rect.x + para.layout.lines[0].xs[cluster];
    assert!(
        (left - (doc.pages()[0].body.x + 36.0)).abs() < 0.05,
        "default tab {left}"
    );
}

#[test]
fn paragraph_borders_inherit_until_an_explicit_set_replaces_them() {
    let styles = format!(
        r#"<w:styles xmlns:w="{W}"><w:style w:type="paragraph" w:styleId="Base"><w:pPr><w:pBdr><w:top w:val="single" w:sz="8"/><w:bottom w:val="single" w:sz="8" w:space="4" w:color="4F81BD"/></w:pBdr></w:pPr></w:style><w:style w:type="paragraph" w:styleId="Child"><w:basedOn w:val="Base"/><w:pPr><w:pBdr><w:top w:val="nil"/></w:pBdr></w:pPr></w:style></w:styles>"#
    );
    let body = r#"<w:p><w:pPr><w:pStyle w:val="Base"/></w:pPr><w:r><w:t>Inherited</w:t></w:r></w:p><w:p><w:pPr><w:pStyle w:val="Child"/></w:pPr><w:r><w:t>Replaced</w:t></w:r></w:p><w:p><w:pPr><w:pStyle w:val="Base"/><w:pBdr/></w:pPr><w:r><w:t>Empty</w:t></w:r></w:p>"#;
    let bytes = styled_package(body, &styles);
    let mut doc = open_document(&bytes, &OpenOptions::default()).unwrap();
    doc.layout().unwrap();
    let paras = paragraphs(&doc, 0);
    assert!(paras[0]
        .para
        .borders
        .bottom
        .is_some_and(|b| b.style.is_visible() && b.space == 4.0));
    assert!(paras[1].para.borders.bottom.is_none());
    assert!(paras[1]
        .para
        .borders
        .top
        .is_some_and(|b| !b.style.is_visible()));
    assert!(paras[2].para.borders.bottom.is_none());
}
#[test]
fn paragraph_border_spacing_takes_space_in_the_body_flow() {
    fn body(top: &str, bottom: &str) -> String {
        format!(
            r#"<w:p><w:pPr><w:pBdr>{top}{bottom}</w:pBdr></w:pPr><w:r><w:t>Title</w:t></w:r></w:p><w:p><w:r><w:t>After</w:t></w:r></w:p>"#
        )
    }
    let plain = render(&body("", ""), "", "", &[]);
    let bottom = render(
        &body("", r#"<w:bottom w:val="single" w:sz="8" w:space="4"/>"#),
        "",
        "",
        &[],
    );
    let top = render(
        &body(r#"<w:top w:val="single" w:sz="8" w:space="4"/>"#, ""),
        "",
        "",
        &[],
    );
    assert!(
        (paragraphs(&bottom, 0)[1].rect.y - paragraphs(&plain, 0)[1].rect.y - 5.0).abs() < 0.01
    );
    assert!((paragraphs(&top, 0)[0].rect.y - paragraphs(&plain, 0)[0].rect.y - 5.0).abs() < 0.01);
}

#[test]
fn explicit_off_character_properties_override_inherited_formatting_after_save() {
    let styles = format!(
        r#"<w:styles xmlns:w="{W}"><w:style w:type="paragraph" w:styleId="Base"><w:rPr><w:b/><w:i/><w:strike/><w:u w:val="single"/></w:rPr></w:style><w:style w:type="paragraph" w:styleId="Child"><w:basedOn w:val="Base"/><w:rPr><w:b w:val="0"/><w:i w:val="0"/><w:strike w:val="0"/><w:u w:val="none"/></w:rPr></w:style></w:styles>"#
    );
    let body = r#"<w:p><w:pPr><w:pStyle w:val="Child"/></w:pPr><w:r><w:t>Child</w:t></w:r></w:p><w:p><w:pPr><w:pStyle w:val="Base"/></w:pPr><w:r><w:rPr><w:b w:val="0"/><w:i w:val="0"/><w:strike w:val="0"/><w:u w:val="none"/></w:rPr><w:t>Direct</w:t></w:r></w:p>"#;
    let bytes = styled_package(body, &styles);
    let mut source = edgedoc::Document::read_bytes(&bytes).unwrap();
    source
        .edit(edgedoc::EditOptions::default(), |edit| {
            edit.body().paragraph_after_last(|para| para.text("Tail"))?;
            Ok(())
        })
        .unwrap();
    let saved = source.write_bytes().unwrap();
    let mut doc = open_document(&saved, &OpenOptions::default()).unwrap();
    doc.layout().unwrap();
    for para in &paragraphs(&doc, 0)[..2] {
        let style = &para.run_styles[0];
        let face = doc.fonts().face(style.face).unwrap();
        assert!(
            !face.bold && !face.italic && !style.synth_bold && !style.synth_italic,
            "face is bold={} italic={}",
            face.bold,
            face.italic
        );
        assert!(!style.strike && !style.underline, "{style:?}");
    }
}

#[test]
fn automatic_paragraph_spacing_overrides_numeric_spacing_and_survives_save() {
    let body = r#"<w:p><w:pPr><w:spacing w:before="100" w:beforeAutospacing="1" w:after="100" w:afterAutospacing="1"/></w:pPr><w:r><w:t>First</w:t></w:r></w:p><w:p><w:pPr><w:spacing w:before="100" w:beforeAutospacing="1" w:after="100" w:afterAutospacing="1"/></w:pPr><w:r><w:t>Second</w:t></w:r></w:p><w:p><w:pPr><w:spacing w:before="100" w:beforeAutospacing="0"/></w:pPr><w:r><w:t>Third</w:t></w:r></w:p>"#;
    let bytes = package(body, "", "", &[]);
    let mut source = edgedoc::Document::read_bytes(&bytes).unwrap();
    source
        .edit(edgedoc::EditOptions::default(), |edit| {
            edit.body().paragraph_after_last(|p| p.text("Tail"))?;
            Ok(())
        })
        .unwrap();
    let mut doc = open_document(&source.write_bytes().unwrap(), &OpenOptions::default()).unwrap();
    doc.layout().unwrap();
    let ps = paragraphs(&doc, 0);
    assert!((ps[0].rect.y - doc.pages()[0].body.y).abs() < 0.01);
    assert!((ps[1].rect.y - ps[0].rect.bottom() - 14.0).abs() < 0.01);
    assert!((ps[2].rect.y - ps[1].rect.bottom() - 14.0).abs() < 0.01);
}
#[test]
fn right_margin_aligned_footer_page_field_survives_an_edit_and_renders_at_the_margin() {
    let footer = r#"<w:p><w:pPr><w:framePr w:wrap="around" w:hAnchor="margin" w:vAnchor="text" w:xAlign="right" w:y="1"/></w:pPr><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText> PAGE </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>1</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r></w:p><w:p/>"#;
    let bytes = package(
        &p("Body"),
        r#"<w:footerReference w:type="default" r:id="footer1"/>"#,
        "",
        &[("footer1", footer)],
    );
    let mut source = edgedoc::Document::read_bytes(&bytes).unwrap();
    source
        .edit(edgedoc::EditOptions::default(), |edit| {
            edit.body().paragraph_after_last(|p| p.text("Tail"))?;
            Ok(())
        })
        .unwrap();
    let saved = source.write_bytes().unwrap();
    let mut doc = open_document(&saved, &OpenOptions::default()).unwrap();
    doc.layout().unwrap();
    let number = doc
        .page(0)
        .unwrap()
        .ops
        .iter()
        .find_map(|op| match op {
            edgedoc_display::Op::Glyphs(g) if g.text == "1" => Some(g),
            _ => None,
        })
        .unwrap();
    assert!(
        (number.x + number.width() - doc.pages()[0].body.right()).abs() < 0.05,
        "footer page number x {}",
        number.x
    );
}

#[test]
fn final_hard_break_keeps_an_empty_line_before_the_next_paragraph() {
    let body =
        r#"<w:p><w:r><w:t>First</w:t><w:br/></w:r></w:p><w:p><w:r><w:t>Second</w:t></w:r></w:p>"#;
    let doc = render(body, "", "", &[]);
    let ps = paragraphs(&doc, 0);
    assert_eq!(ps[0].layout.lines.len(), 2);
    let no_break = render(&format!("{}{}", p("First"), p("Second")), "", "", &[]);
    let baseline = paragraphs(&no_break, 0);
    assert!((ps[1].rect.y - baseline[1].rect.y - ps[0].layout.lines[1].height).abs() < 0.01);
    assert!(ps[0].layout.height > ps[0].layout.lines[0].height * 1.9);
}

#[test]
fn symbol_font_bullets_use_unicode_and_share_the_text_baseline() {
    let body = r#"<w:p><w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr></w:pPr><w:r><w:t>Bullet text</w:t></w:r></w:p>"#;
    let numbering = format!(
        r#"<w:numbering xmlns:w="{W}"><w:abstractNum w:abstractNumId="0"><w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="bullet"/><w:lvlText w:val="&#xF0B7;"/><w:pPr><w:ind w:left="720" w:hanging="360"/></w:pPr><w:rPr><w:rFonts w:ascii="Symbol" w:hAnsi="Symbol"/></w:rPr></w:lvl></w:abstractNum><w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num></w:numbering>"#
    );
    let bytes = add_part(
        &package(body, "", "", &[]),
        "word/numbering.xml",
        &numbering,
        "numbering",
        "application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml",
    );
    let mut doc = open_document(&bytes, &OpenOptions::default()).unwrap();
    doc.layout().unwrap();
    let page = doc.page(0).unwrap();
    let glyph = |text: &str| {
        page.ops
            .iter()
            .find_map(|op| match op {
                edgedoc_display::Op::Glyphs(g) if g.text == text => Some(g),
                _ => None,
            })
            .unwrap()
    };
    assert_eq!(
        paragraphs(&doc, 0)[0].label.as_ref().map(|l| l.0.as_str()),
        Some("•")
    );
    assert!((glyph("•").y - glyph("Bullet text").y).abs() < 0.01);
    assert!((glyph("•").x - (doc.pages()[0].body.x + 18.0)).abs() < 0.05);
    assert!((glyph("Bullet text").x - (doc.pages()[0].body.x + 36.0)).abs() < 0.05);
}

#[test]
fn bullet_without_a_level_font_uses_the_paragraph_mark_face() {
    let body = r#"<w:p><w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr></w:pPr><w:r><w:rPr><w:rFonts w:ascii="Arial" w:hAnsi="Arial"/></w:rPr><w:t>Arial bullet</w:t></w:r></w:p><w:p><w:r><w:t>Normal</w:t></w:r></w:p>"#;
    let numbering = format!(
        r#"<w:numbering xmlns:w="{W}"><w:abstractNum w:abstractNumId="0"><w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="bullet"/><w:lvlText w:val="●"/><w:pPr><w:ind w:left="720" w:hanging="360"/></w:pPr></w:lvl></w:abstractNum><w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num></w:numbering>"#
    );
    let bytes = add_part(
        &package(body, "", "", &[]),
        "word/numbering.xml",
        &numbering,
        "numbering",
        "application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml",
    );
    let mut doc = open_document(&bytes, &OpenOptions::default()).unwrap();
    doc.layout().unwrap();
    let page = doc.page(0).unwrap();
    let glyph = |text: &str| {
        page.ops
            .iter()
            .find_map(|op| match op {
                edgedoc_display::Op::Glyphs(g) if g.text == text => Some(g),
                _ => None,
            })
            .unwrap()
    };
    assert_eq!(glyph("●").face, glyph("Normal").face);
    assert_ne!(glyph("●").face, glyph("Arial bullet").face);
}
#[test]
fn extra_multiple_spacing_may_extend_into_the_bottom_margin() {
    let body = r#"<w:p><w:pPr><w:spacing w:after="0"/></w:pPr><w:r><w:t>Anchor</w:t></w:r></w:p><w:p><w:pPr><w:spacing w:before="2100" w:line="480" w:lineRule="auto"/></w:pPr><w:r><w:t>Fits above the margin</w:t></w:r></w:p>"#;
    let section = r#"<w:pgSz w:w="12240" w:h="5760"/><w:pgMar w:top="1440" w:bottom="1440" w:left="1440" w:right="1440"/>"#;
    let doc = render(body, section, "", &[]);
    assert_eq!(doc.pages().len(), 1);
    assert!(doc.page_text(0).unwrap().contains("Fits above the margin"));
}

#[test]
fn exact_line_spacing_must_fit_above_the_bottom_margin() {
    let body = r#"<w:p><w:pPr><w:spacing w:after="0"/></w:pPr><w:r><w:t>Anchor</w:t></w:r></w:p><w:p><w:pPr><w:spacing w:before="2100" w:line="480" w:lineRule="exact"/></w:pPr><w:r><w:t>Requires its full line box</w:t></w:r></w:p>"#;
    let section = r#"<w:pgSz w:w="12240" w:h="5760"/><w:pgMar w:top="1440" w:bottom="1440" w:left="1440" w:right="1440"/>"#;
    let doc = render(body, section, "", &[]);
    assert_eq!(doc.pages().len(), 2);
    assert!((paragraphs(&doc, 1)[0].rect.y - doc.pages()[1].body.y).abs() < 0.01);
    assert!(doc
        .page_text(1)
        .unwrap()
        .contains("Requires its full line box"));
}

#[test]
fn special_hyphens_render_and_survive_an_edited_save() {
    let body = r#"<w:p><w:r><w:t>alpha</w:t><w:softHyphen/><w:t>beta</w:t></w:r></w:p><w:p><w:r><w:t>non</w:t><w:noBreakHyphen/><w:t>breaking</w:t></w:r></w:p>"#;
    let bytes = package(body, "", "", &[]);
    let mut display = open_document(&bytes, &OpenOptions::default()).unwrap();
    display.layout().unwrap();
    let ps = paragraphs(&display, 0);
    assert_eq!(ps[0].text.as_ref(), "alpha\u{00AD}beta");
    assert_eq!(ps[1].text.as_ref(), "non\u{2011}breaking");
    assert!(display.page(0).unwrap().ops.iter().all(|op| match op {
        edgedoc_display::Op::Glyphs(run) => !run.text.contains('\u{00AD}'),
        _ => true,
    }));
    let page = display.page(0).unwrap();
    let hyphen = page.ops.iter().find_map(|op| match op {
        edgedoc_display::Op::Glyphs(run) if run.text.contains('\u{2011}') => Some(run),
        _ => None,
    });
    let hyphen = hyphen.expect("the nonbreaking hyphen paints");
    assert!(hyphen.glyphs.iter().all(|glyph| *glyph != 0));
    assert_eq!(
        display.fonts().face(hyphen.face).unwrap().family,
        "Source Sans 3"
    );

    let mut doc = edgedoc::Document::read_bytes(&bytes).unwrap();
    doc.edit(edgedoc::EditOptions::default(), |edit| {
        edit.body().paragraph_at(0, |p| p.text(" added"))?;
        Ok(())
    })
    .unwrap();
    let saved = doc.write_bytes().unwrap();
    let mut zip = zip::ZipArchive::new(Cursor::new(&saved)).unwrap();
    let mut xml = String::new();
    zip.by_name("word/document.xml")
        .unwrap()
        .read_to_string(&mut xml)
        .unwrap();
    assert!(xml.contains("<w:softHyphen/>") && xml.contains("<w:noBreakHyphen/>"));
    let mut display = open_document(&saved, &OpenOptions::default()).unwrap();
    display.layout().unwrap();
    assert!(paragraphs(&display, 0)[0]
        .text
        .contains("alpha\u{00AD}beta"));
    assert_eq!(
        paragraphs(&display, 0)[1].text.as_ref(),
        "non\u{2011}breaking"
    );
}

#[test]
fn soft_hyphen_can_break_a_word_without_a_placeholder() {
    let body = r#"<w:p><w:r><w:t>alpha</w:t><w:softHyphen/><w:t>beta</w:t></w:r></w:p>"#;
    let section = r#"<w:pgSz w:w="2600" w:h="15840"/><w:pgMar w:left="1000" w:right="1000"/>"#;
    let doc = render(body, section, "", &[]);
    let para = paragraphs(&doc, 0)[0];
    assert_eq!(para.text.as_ref(), "alpha\u{00AD}beta");
    assert!(para.layout.lines.len() > 1);
}

#[test]
fn note_references_count_each_kind_in_document_order() {
    let body = r#"<w:p><w:r><w:t>A</w:t><w:footnoteReference w:id="7"/><w:t>B</w:t><w:footnoteReference w:id="3"/><w:t>C</w:t><w:endnoteReference w:id="4"/><w:t>D</w:t><w:endnoteReference w:id="2"/></w:r></w:p>"#;
    let original = package(body, "", "", &[]);
    let related = replace_part(&original, "word/_rels/document.xml.rels", |xml| {
        xml.replace("</Relationships>", &format!(r#"<Relationship Id="footnotes" Type="{R}/footnotes" Target="footnotes.xml"/><Relationship Id="endnotes" Type="{R}/endnotes" Target="endnotes.xml"/></Relationships>"#))
    });
    let mut input = zip::ZipArchive::new(Cursor::new(&related)).unwrap();
    let mut output = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for i in 0..input.len() {
        let mut entry = input.by_index(i).unwrap();
        output
            .start_file(entry.name(), zip::write::FileOptions::default())
            .unwrap();
        std::io::copy(&mut entry, &mut output).unwrap();
    }
    for (name, xml) in [
        (
            "word/footnotes.xml",
            format!(
                r#"<w:footnotes xmlns:w="{W}"><w:footnote w:id="7"><w:p><w:r><w:footnoteRef/><w:t> first</w:t></w:r></w:p></w:footnote><w:footnote w:id="3"><w:p><w:r><w:footnoteRef/><w:t> second</w:t></w:r></w:p></w:footnote></w:footnotes>"#
            ),
        ),
        (
            "word/endnotes.xml",
            format!(
                r#"<w:endnotes xmlns:w="{W}"><w:endnote w:id="4"><w:p><w:r><w:endnoteRef/><w:t> first end</w:t></w:r></w:p></w:endnote><w:endnote w:id="2"><w:p><w:r><w:endnoteRef/><w:t> second end</w:t></w:r></w:p></w:endnote></w:endnotes>"#
            ),
        ),
    ] {
        output
            .start_file(name, zip::write::FileOptions::default())
            .unwrap();
        output.write_all(xml.as_bytes()).unwrap();
    }
    let bytes = output.finish().unwrap().into_inner();
    let mut doc = open_document(&bytes, &OpenOptions::default()).unwrap();
    doc.layout().unwrap();
    let paragraph = paragraphs(&doc, 0)[0];
    assert_eq!(paragraph.text.as_ref(), "A1B2CiDii");
    assert!(doc.page_text(0).unwrap().contains("first"));
    assert!(doc.page_text(0).unwrap().contains("second"));
    let endnotes = paragraphs(&doc, 0)
        .into_iter()
        .filter(|para| para.path.starts_with("endnote["))
        .collect::<Vec<_>>();
    assert_eq!(
        endnotes.iter().map(|p| p.text.as_ref()).collect::<Vec<_>>(),
        ["i first end", "ii second end"]
    );
    let notes = paragraphs(&doc, 0)
        .into_iter()
        .filter(|para| para.path.starts_with("footnote["))
        .collect::<Vec<_>>();
    assert_eq!(notes.len(), 2);
    assert!(notes[0].rect.y >= paragraph.rect.bottom());
    assert!(notes[1].rect.y >= notes[0].rect.bottom());
    let body_size = paragraph.run_styles[0].size;
    assert!(paragraph.run_styles.iter().any(|style| {
        (style.size - body_size * 0.65).abs() < 0.01
            && (style.baseline_shift - body_size * 0.33).abs() < 0.01
    }));

    let section_endnote_body = r#"<w:p><w:pPr><w:sectPr><w:endnotePr><w:pos w:val="sectEnd"/></w:endnotePr></w:sectPr></w:pPr><w:r><w:t>First section</w:t><w:endnoteReference w:id="4"/></w:r></w:p><w:p><w:r><w:t>Second section</w:t><w:endnoteReference w:id="2"/></w:r></w:p>"#;
    let section_endnote = replace_part(&bytes, "word/document.xml", |xml| {
        xml.replace(body, section_endnote_body)
    });
    let mut section_endnote_doc = open_document(&section_endnote, &OpenOptions::default()).unwrap();
    section_endnote_doc.layout().unwrap();
    assert!(section_endnote_doc.pages().len() >= 2);
    assert!(section_endnote_doc
        .page_text(0)
        .unwrap()
        .contains("first end"));
    assert!(section_endnote_doc
        .page_text(1)
        .unwrap()
        .contains("second end"));

    let custom_endnote = replace_part(&bytes, "word/endnotes.xml", |xml| {
        xml.replace("<w:endnote w:id=\"4\">", r#"<w:endnote w:type="separator" w:id="0"><w:p><w:pPr><w:spacing w:after="0" w:line="240" w:lineRule="auto"/></w:pPr><w:r><w:separator/></w:r></w:p><w:p><w:r><w:t>Endnote separator text</w:t></w:r></w:p></w:endnote><w:endnote w:id="4">"#)
    });
    let mut custom_endnote_doc = open_document(&custom_endnote, &OpenOptions::default()).unwrap();
    custom_endnote_doc.layout().unwrap();
    assert!(custom_endnote_doc
        .page_text(0)
        .unwrap()
        .contains("Endnote separator text"));

    let column_endnote = replace_part(&custom_endnote, "word/document.xml", |xml| {
        xml.replace(
            "<w:sectPr></w:sectPr>",
            r#"<w:sectPr><w:cols w:num="2"/></w:sectPr>"#,
        )
    });
    let mut column_endnote_doc = open_document(&column_endnote, &OpenOptions::default()).unwrap();
    column_endnote_doc.layout().unwrap();
    assert!(column_endnote_doc
        .page_text(0)
        .unwrap()
        .contains("Endnote separator text"));

    let long_endnote = replace_part(&bytes, "word/endnotes.xml", |xml| {
        xml.replace(" first end", &" endnote continuation".repeat(500))
    });
    let mut long_endnote_doc = open_document(&long_endnote, &OpenOptions::default()).unwrap();
    long_endnote_doc.layout().unwrap();
    assert!(long_endnote_doc.pages().len() > 1);
    let endnote_text = (0..long_endnote_doc.pages().len())
        .map(|page| long_endnote_doc.page_text(page).unwrap())
        .collect::<Vec<_>>()
        .join(" ");
    assert_eq!(endnote_text.matches("continuation").count(), 500);

    let configured = replace_part(&bytes, "word/settings.xml", |xml| {
        xml.replace("</w:settings>", r#"<w:footnotePr><w:numStart w:val="5"/><w:numFmt w:val="lowerRoman"/></w:footnotePr></w:settings>"#)
    });
    let mut configured_doc = open_document(&configured, &OpenOptions::default()).unwrap();
    configured_doc.layout().unwrap();
    assert_eq!(
        paragraphs(&configured_doc, 0)[0].text.as_ref(),
        "AvBviCiDii"
    );

    let per_page_body = r#"<w:p><w:r><w:t>A</w:t><w:footnoteReference w:id="7"/></w:r></w:p><w:p><w:pPr><w:pageBreakBefore/></w:pPr><w:r><w:t>B</w:t><w:footnoteReference w:id="3"/></w:r></w:p>"#;
    let per_page = replace_part(&bytes, "word/document.xml", |xml| {
        xml.replace(body, per_page_body)
    });
    let per_page = replace_part(&per_page, "word/settings.xml", |xml| {
        xml.replace("</w:settings>", r#"<w:footnotePr><w:numStart w:val="3"/><w:numRestart w:val="eachPage"/></w:footnotePr></w:settings>"#)
    });
    let mut per_page_doc = open_document(&per_page, &OpenOptions::default()).unwrap();
    per_page_doc.layout().unwrap();
    assert_eq!(paragraphs(&per_page_doc, 0)[0].text.as_ref(), "A3");
    assert_eq!(paragraphs(&per_page_doc, 1)[0].text.as_ref(), "B3");
    assert!(per_page_doc.page_text(0).unwrap().contains("3 first"));
    assert!(per_page_doc.page_text(1).unwrap().contains("3 second"));

    let crowded_body = (0..30)
        .map(|index| {
            if index == 24 {
                r#"<w:p><w:r><w:t>Anchored line</w:t><w:footnoteReference w:id="7"/></w:r></w:p>"#
                    .to_string()
            } else {
                p(&format!("Body line {index}"))
            }
        })
        .collect::<String>();
    let crowded = replace_part(&bytes, "word/document.xml", |xml| {
        xml.replace(body, &crowded_body)
    });
    let mut crowded_doc = open_document(&crowded, &OpenOptions::default()).unwrap();
    crowded_doc.layout().unwrap();
    let note_page = (0..crowded_doc.pages().len())
        .find(|index| {
            paragraphs(&crowded_doc, *index)
                .iter()
                .any(|para| para.path.starts_with("footnote["))
        })
        .expect("the note is placed");
    let page_paragraphs = paragraphs(&crowded_doc, note_page);
    assert!(page_paragraphs
        .iter()
        .any(|para| para.text.contains("Anchored line")));
    let note_top = page_paragraphs
        .iter()
        .find(|para| para.path.starts_with("footnote["))
        .unwrap()
        .rect
        .y;
    let body_bottom = page_paragraphs
        .iter()
        .filter(|para| para.path.starts_with("body["))
        .map(|para| para.rect.bottom())
        .fold(0.0f32, f32::max);
    assert!(
        body_bottom <= note_top,
        "body and footnote must not overlap"
    );

    let long_note = replace_part(&bytes, "word/footnotes.xml", |xml| {
        xml.replace(" first", &" note continuation content".repeat(500))
            .replace(
                "<w:footnote w:id=\"7\">",
                r#"<w:footnote w:type="continuationSeparator" w:id="0"><w:p><w:pPr><w:spacing w:after="0" w:line="240" w:lineRule="auto"/></w:pPr><w:r><w:continuationSeparator/></w:r></w:p></w:footnote><w:footnote w:type="continuationNotice" w:id="2"><w:p><w:r><w:t>Continued below</w:t></w:r></w:p></w:footnote><w:footnote w:id="7">"#,
            )
    });
    let long_note = replace_part(&long_note, "word/document.xml", |xml| {
        xml.replace(
            body,
            r#"<w:p><w:r><w:t>Anchor</w:t><w:footnoteReference w:id="7"/></w:r></w:p>"#,
        )
    });
    let mut long_doc = open_document(&long_note, &OpenOptions::default()).unwrap();
    long_doc.layout().unwrap();
    assert!(
        long_doc.pages().len() > 1,
        "a long note must continue onto more pages"
    );
    for page in 0..long_doc.pages().len() {
        for para in paragraphs(&long_doc, page)
            .into_iter()
            .filter(|para| para.path.starts_with("footnote["))
        {
            assert!(para.rect.y >= long_doc.pages()[page].body.y - 0.01);
            assert!(para.rect.bottom() <= long_doc.pages()[page].body.bottom() + 0.01);
        }
    }
    let all_text = (0..long_doc.pages().len())
        .map(|page| long_doc.page_text(page).unwrap())
        .collect::<Vec<_>>()
        .join(" ");
    assert_eq!(all_text.matches("continuation").count(), 500);
    assert_eq!(all_text.matches("content").count(), 500);
    assert!(all_text.contains("Continued below"));

    let restart =
        r#"<w:footnotePr><w:numRestart w:val="eachSect"/><w:numStart w:val="3"/></w:footnotePr>"#;
    let sectioned = format!(
        r#"<w:p><w:pPr><w:sectPr>{restart}</w:sectPr></w:pPr><w:r><w:t>A</w:t><w:footnoteReference w:id="7"/></w:r></w:p><w:p><w:r><w:t>B</w:t><w:footnoteReference w:id="3"/></w:r></w:p>"#
    );
    let bytes = replace_part(&bytes, "word/document.xml", |xml| {
        xml.replace(body, &sectioned).replace(
            "<w:sectPr></w:sectPr>",
            &format!("<w:sectPr>{restart}</w:sectPr>"),
        )
    });
    let mut doc = open_document(&bytes, &OpenOptions::default()).unwrap();
    doc.layout().unwrap();
    assert_eq!(paragraphs(&doc, 0)[0].text.as_ref(), "A3");
    assert_eq!(paragraphs(&doc, 1)[0].text.as_ref(), "B3");
}

#[test]
fn references_to_missing_note_stories_do_not_paint_numbers() {
    let body =
        r#"<w:p><w:r><w:t>Before</w:t><w:footnoteReference w:id="7"/><w:t>After</w:t></w:r></w:p>"#;
    let doc = render(body, "", "", &[]);
    assert_eq!(paragraphs(&doc, 0)[0].text.as_ref(), "BeforeAfter");
}

#[test]
fn comment_reference_does_not_paint_a_note_number() {
    let body =
        r#"<w:p><w:r><w:t>Before</w:t><w:commentReference w:id="7"/><w:t>After</w:t></w:r></w:p>"#;
    let doc = render(body, "", "", &[]);
    assert_eq!(paragraphs(&doc, 0)[0].text.as_ref(), "BeforeAfter");
}

#[test]
fn superscript_and_subscript_use_wordcraft_glyph_sizes() {
    let body = r#"<w:p><w:r><w:t>A</w:t></w:r><w:r><w:rPr><w:vertAlign w:val="superscript"/></w:rPr><w:t>super</w:t></w:r><w:r><w:rPr><w:vertAlign w:val="subscript"/></w:rPr><w:t>sub</w:t></w:r></w:p>"#;
    let doc = render(body, "", "", &[]);
    let paragraph = paragraphs(&doc, 0)[0];
    let normal = paragraph.run_styles[0].size;
    let super_style = &paragraph.run_styles[1];
    let sub_style = &paragraph.run_styles[2];
    assert!((super_style.size - normal * 0.65).abs() < 0.01);
    assert!((sub_style.size - normal * 0.65).abs() < 0.01);
    assert!((super_style.baseline_shift - normal * 0.33).abs() < 0.01);
    assert!((sub_style.baseline_shift + normal * 0.14).abs() < 0.01);
}

#[test]
fn a_paragraph_with_only_superscript_uses_its_shifted_line_metrics() {
    let body = r#"<w:p><w:r><w:rPr><w:vertAlign w:val="superscript"/></w:rPr><w:t>Only raised text</w:t></w:r></w:p>"#;
    let doc = render(body, "", "", &[]);
    assert_eq!(paragraphs(&doc, 0)[0].text.as_ref(), "Only raised text");
}

#[test]
fn inline_content_controls_render_their_runs_in_body_and_header() {
    let control = r#"<w:sdt><w:sdtPr><w:alias w:val="Label"/></w:sdtPr><w:sdtContent><w:proofErr w:type="spellStart"/><w:r><w:rPr><w:b/></w:rPr><w:t>Visible</w:t></w:r><w:proofErr w:type="spellEnd"/></w:sdtContent></w:sdt>"#;
    let body =
        format!(r#"<w:p><w:r><w:t>Before </w:t></w:r>{control}<w:r><w:t> After</w:t></w:r></w:p>"#);
    let header = format!(r#"<w:p><w:r><w:t>Header </w:t></w:r>{control}</w:p>"#);
    let doc = render(
        &body,
        r#"<w:headerReference w:type="default" r:id="header1"/>"#,
        "",
        &[("header1", &header)],
    );
    let text = doc.page_text(0).unwrap();
    assert!(text.contains("Before Visible After"), "{text}");
    assert!(text.contains("Header Visible"), "{text}");
}

#[test]
fn cell_content_control_renders_as_a_table_cell() {
    let body = r#"<w:tbl><w:tblGrid><w:gridCol w:w="2000"/><w:gridCol w:w="2000"/></w:tblGrid><w:tr><w:tc><w:p><w:r><w:t>Left</w:t></w:r></w:p></w:tc><w:sdt><w:sdtPr><w:alias w:val="Cell"/></w:sdtPr><w:sdtContent><w:tc><w:p><w:r><w:t>Inside</w:t></w:r></w:p></w:tc></w:sdtContent></w:sdt></w:tr></w:tbl>"#;
    let doc = render(body, "", "", &[]);
    let text = doc.page_text(0).unwrap();
    assert!(text.contains("Left") && text.contains("Inside"), "{text}");
    let table = doc.pages()[0]
        .fragments
        .iter()
        .find_map(|fragment| match fragment {
            Fragment::Table(table) => Some(table),
            _ => None,
        })
        .expect("table is rendered");
    assert!(
        table.cells[0].rect.w < 40.0,
        "the grid must shrink to the first cell's content"
    );
}

#[test]
fn table_style_supplies_cell_spacing_and_borders() {
    let styles = format!(
        r#"<w:styles xmlns:w="{W}"><w:docDefaults><w:pPrDefault><w:pPr><w:spacing w:after="160"/></w:pPr></w:pPrDefault></w:docDefaults><w:style w:type="table" w:styleId="Base"><w:tblPr><w:tblBorders><w:top w:val="single" w:sz="4"/><w:left w:val="single" w:sz="4"/><w:bottom w:val="single" w:sz="4"/><w:right w:val="single" w:sz="4"/><w:insideH w:val="single" w:sz="4"/><w:insideV w:val="single" w:sz="4"/></w:tblBorders></w:tblPr></w:style><w:style w:type="table" w:styleId="Grid"><w:basedOn w:val="Base"/><w:pPr><w:spacing w:after="0"/></w:pPr></w:style></w:styles>"#
    );
    let body = r#"<w:tbl><w:tblPr><w:tblStyle w:val="Grid"/><w:tblLayout w:type="fixed"/></w:tblPr><w:tblGrid><w:gridCol w:w="2000"/><w:gridCol w:w="2000"/></w:tblGrid><w:tr><w:tc><w:p><w:r><w:t>A</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>B</w:t></w:r></w:p></w:tc></w:tr><w:tr><w:tc><w:p><w:r><w:t>C</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>D</w:t></w:r></w:p></w:tc></w:tr></w:tbl>"#;
    let mut doc = open_document(&styled_package(body, &styles), &OpenOptions::default()).unwrap();
    doc.layout().unwrap();
    let table = doc.pages()[0]
        .fragments
        .iter()
        .find_map(|fragment| match fragment {
            Fragment::Table(table) => Some(table),
            _ => None,
        })
        .expect("table");
    assert!(!table.borders.is_empty(), "style borders must be painted");
    assert!(
        table.cells[0].rect.h < 20.0,
        "table style must override document spacing"
    );
    let direct = body.replacen(
        "<w:p><w:r><w:t>A",
        r#"<w:p><w:pPr><w:spacing w:after="160"/></w:pPr><w:r><w:t>A"#,
        1,
    );
    let mut overridden =
        open_document(&styled_package(&direct, &styles), &OpenOptions::default()).unwrap();
    overridden.layout().unwrap();
    let table = overridden.pages()[0]
        .fragments
        .iter()
        .find_map(|fragment| match fragment {
            Fragment::Table(table) => Some(table),
            _ => None,
        })
        .unwrap();
    assert!(
        table.cells[0].rect.h > 20.0,
        "direct paragraph spacing must override the table style"
    );
}

#[test]
fn paragraph_spacing_precedes_a_table_with_a_wide_preferred_grid() {
    let body = r#"<w:p><w:pPr><w:spacing w:after="200"/></w:pPr><w:r><w:t>Before</w:t></w:r></w:p><w:tbl><w:tblPr><w:tblW w:w="0" w:type="auto"/></w:tblPr><w:tblGrid><w:gridCol w:w="4800"/><w:gridCol w:w="4800"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:w="4800" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>Left</w:t></w:r></w:p></w:tc><w:tc><w:tcPr><w:tcW w:w="4800" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>Right</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p><w:r><w:t>After</w:t></w:r></w:p>"#;
    let doc = render(body, "", "", &[]);
    let before = paragraphs(&doc, 0)[0];
    let table = doc.pages()[0]
        .fragments
        .iter()
        .find_map(|fragment| match fragment {
            Fragment::Table(table) => Some(table),
            _ => None,
        })
        .expect("table");
    assert!((table.rect.y - before.rect.bottom() - 10.0).abs() < 0.01);
    assert!(
        table.rect.w > doc.pages()[0].body.w,
        "the requested grid can extend beyond the text margin"
    );
}

#[test]
fn edited_table_style_retains_its_borders_and_cell_margins() {
    let styles = format!(
        r#"<w:styles xmlns:w="{W}"><w:style w:type="table" w:styleId="Grid"><w:pPr><w:spacing w:after="0"/></w:pPr><w:tblPr><w:tblBorders><w:top w:val="single" w:sz="8"/></w:tblBorders><w:tblCellMar><w:left w:w="120" w:type="dxa"/></w:tblCellMar></w:tblPr></w:style></w:styles>"#
    );
    let bytes = styled_package(&p("Body"), &styles);
    let mut model = edgedoc::Document::read_bytes(&bytes).unwrap();
    let style = model.styles().by_id("Grid").unwrap().clone();
    model.add_style(style);
    let reopened = edgedoc::Document::read_bytes(&model.write_bytes().unwrap()).unwrap();
    let table = reopened
        .styles()
        .by_id("Grid")
        .unwrap()
        .table_props
        .as_ref()
        .unwrap();
    assert_eq!(table.borders.top.as_ref().unwrap().size, Some(8));
    assert_eq!(table.cell_margins.left.unwrap().0, 120);
}

#[test]
fn conditional_table_styles_apply_first_row_and_corner_precedence() {
    let styles = format!(
        r#"<w:styles xmlns:w="{W}"><w:style w:type="table" w:styleId="Fancy"><w:tblStylePr w:type="firstRow"><w:rPr><w:b/></w:rPr><w:tcPr><w:shd w:val="clear" w:fill="DDEEFF"/></w:tcPr></w:tblStylePr><w:tblStylePr w:type="nwCell"><w:tcPr><w:shd w:val="clear" w:fill="FFCCAA"/></w:tcPr></w:tblStylePr></w:style></w:styles>"#
    );
    let body = r#"<w:tbl><w:tblPr><w:tblStyle w:val="Fancy"/><w:tblLook w:firstRow="1" w:firstColumn="1"/><w:tblLayout w:type="fixed"/></w:tblPr><w:tblGrid><w:gridCol w:w="2000"/><w:gridCol w:w="2000"/></w:tblGrid><w:tr><w:tc><w:p><w:r><w:t>One</w:t></w:r></w:p></w:tc><w:tc><w:tcPr><w:shd w:val="clear" w:fill="auto"/></w:tcPr><w:p><w:r><w:t>Two</w:t></w:r></w:p></w:tc></w:tr><w:tr><w:tc><w:p><w:r><w:t>Three</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>Four</w:t></w:r></w:p></w:tc></w:tr></w:tbl>"#;
    let mut doc = open_document(&styled_package(body, &styles), &OpenOptions::default()).unwrap();
    doc.layout().unwrap();
    let table = doc.pages()[0]
        .fragments
        .iter()
        .find_map(|fragment| match fragment {
            Fragment::Table(table) => Some(table),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        table.cells[0].shading,
        edgedoc_geom::Rgba::parse_hex("FFCCAA")
    );
    assert_eq!(
        table.cells[1].shading,
        edgedoc_geom::Rgba::parse_hex("DDEEFF")
    );
    assert_eq!(table.cells[2].shading, None);
    let first = table
        .content
        .iter()
        .find_map(|fragment| match fragment {
            Fragment::Paragraph(para) => Some(para),
            _ => None,
        })
        .unwrap();
    assert!(
        doc.fonts().face(first.run_styles[0].face).unwrap().bold || first.run_styles[0].synth_bold
    );
}

#[test]
fn conditional_table_bands_edges_and_direct_cell_properties_cascade() {
    let styles = format!(
        r#"<w:styles xmlns:w="{W}"><w:style w:type="table" w:styleId="Base"><w:tblPr><w:tblStyleRowBandSize w:val="1"/><w:tblStyleColBandSize w:val="1"/></w:tblPr><w:tblStylePr w:type="band1Horz"><w:tcPr><w:shd w:val="clear" w:fill="EEEEAA"/></w:tcPr></w:tblStylePr><w:tblStylePr w:type="firstCol"><w:tcPr><w:shd w:val="clear" w:fill="AAEEAA"/></w:tcPr></w:tblStylePr><w:tblStylePr w:type="firstRow"><w:tcPr><w:shd w:val="clear" w:fill="AAAABB"/><w:tcBorders><w:bottom w:val="single" w:sz="8" w:color="FF0000"/></w:tcBorders></w:tcPr></w:tblStylePr></w:style><w:style w:type="table" w:styleId="Child"><w:basedOn w:val="Base"/><w:tblStylePr w:type="nwCell"><w:tcPr><w:shd w:val="clear" w:fill="FFCCAA"/></w:tcPr></w:tblStylePr></w:style></w:styles>"#
    );
    let body = r#"<w:tbl><w:tblPr><w:tblStyle w:val="Child"/><w:tblLook w:firstRow="1" w:firstColumn="1" w:noHBand="0"/><w:tblLayout w:type="fixed"/></w:tblPr><w:tblGrid><w:gridCol w:w="1800"/><w:gridCol w:w="1800"/></w:tblGrid><w:tr><w:tc><w:p><w:r><w:t>A</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>B</w:t></w:r></w:p></w:tc></w:tr><w:tr><w:tc><w:p><w:r><w:t>C</w:t></w:r></w:p></w:tc><w:tc><w:tcPr><w:shd w:val="clear" w:fill="112233"/></w:tcPr><w:p><w:r><w:t>D</w:t></w:r></w:p></w:tc></w:tr><w:tr><w:tc><w:p><w:r><w:t>E</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>F</w:t></w:r></w:p></w:tc></w:tr></w:tbl>"#;
    let bytes = styled_package(body, &styles);
    let model = edgedoc::Document::read_bytes(&bytes).unwrap();
    let mut doc = open_document(&bytes, &OpenOptions::default()).unwrap();
    doc.layout().unwrap();
    let table = doc.pages()[0]
        .fragments
        .iter()
        .find_map(|fragment| match fragment {
            Fragment::Table(table) => Some(table),
            _ => None,
        })
        .unwrap();
    for (index, color) in [
        (0, "FFCCAA"),
        (1, "AAAABB"),
        (2, "AAEEAA"),
        (3, "112233"),
        (4, "AAEEAA"),
    ] {
        assert_eq!(
            table.cells[index].shading,
            edgedoc_geom::Rgba::parse_hex(color)
        );
    }
    assert!(!table.borders.is_empty());
    let mut edited = model;
    let style = edited.styles().by_id("Child").unwrap().clone();
    edited.add_style(style);
    let reopened = edgedoc::Document::read_bytes(&edited.write_bytes().unwrap()).unwrap();
    assert_eq!(
        reopened.styles().by_id("Child").unwrap().table_regions[0].kind,
        "nwCell"
    );
    let saved = edited.write_bytes().unwrap();
    let mut archive = zip::ZipArchive::new(Cursor::new(&saved)).unwrap();
    let mut xml = String::new();
    archive
        .by_name("word/document.xml")
        .unwrap()
        .read_to_string(&mut xml)
        .unwrap();
    assert!(xml.contains(r#"w:firstRow="1""#));
    assert!(xml.contains(r#"w:noHBand="0""#));
}

#[test]
fn conditional_table_last_edges_and_column_bands_follow_look_switches() {
    let parts = [
        ("band1Vert", "AABBCC"),
        ("band2Vert", "CCDDEE"),
        ("band1Horz", "EEEEDD"),
        ("lastRow", "DDCCCC"),
        ("firstCol", "CCDDCC"),
        ("lastCol", "CCCCDD"),
        ("swCell", "FFEEAA"),
        ("seCell", "AAEEFF"),
    ];
    let regions = parts.iter().map(|(name, color)| format!(r#"<w:tblStylePr w:type="{name}"><w:tcPr><w:shd w:val="clear" w:fill="{color}"/></w:tcPr></w:tblStylePr>"#)).collect::<String>();
    let styles = format!(
        r#"<w:styles xmlns:w="{W}"><w:style w:type="table" w:styleId="Grid"><w:tblPr><w:tblStyleRowBandSize w:val="1"/><w:tblStyleColBandSize w:val="1"/></w:tblPr>{regions}</w:style></w:styles>"#
    );
    let row = |r| {
        format!(
            "<w:tr>{}</w:tr>",
            (0..4)
                .map(|c| format!("<w:tc><w:p><w:r><w:t>{r}{c}</w:t></w:r></w:p></w:tc>"))
                .collect::<String>()
        )
    };
    let body = format!(
        r#"<w:tbl><w:tblPr><w:tblStyle w:val="Grid"/><w:tblLook w:firstRow="0" w:lastRow="1" w:firstColumn="1" w:lastColumn="1" w:noHBand="0" w:noVBand="0"/><w:tblLayout w:type="fixed"/></w:tblPr><w:tblGrid>{}</w:tblGrid>{}{}</w:tbl>"#,
        r#"<w:gridCol w:w="1200"/>"#.repeat(4),
        row(0),
        row(1)
    );
    let mut doc = open_document(&styled_package(&body, &styles), &OpenOptions::default()).unwrap();
    doc.layout().unwrap();
    let table = doc.pages()[0]
        .fragments
        .iter()
        .find_map(|fragment| match fragment {
            Fragment::Table(table) => Some(table),
            _ => None,
        })
        .unwrap();
    for (index, color) in [
        (0, "CCDDCC"),
        (1, "EEEEDD"),
        (2, "EEEEDD"),
        (3, "CCCCDD"),
        (4, "FFEEAA"),
        (5, "DDCCCC"),
        (6, "DDCCCC"),
        (7, "AAEEFF"),
    ] {
        assert_eq!(
            table.cells[index].shading,
            edgedoc_geom::Rgba::parse_hex(color),
            "cell {index}"
        );
    }
}

#[test]
fn whole_table_region_overrides_base_shading_and_borders() {
    let styles = format!(
        r#"<w:styles xmlns:w="{W}"><w:style w:type="table" w:styleId="Grid"><w:tblPr><w:shd w:val="clear" w:fill="AABBCC"/><w:tblBorders><w:top w:val="single" w:sz="4" w:color="000000"/></w:tblBorders></w:tblPr><w:tblStylePr w:type="wholeTable"><w:pPr><w:spacing w:after="0"/></w:pPr><w:tblPr><w:tblBorders><w:top w:val="single" w:sz="8" w:color="FF0000"/></w:tblBorders></w:tblPr><w:tcPr><w:shd w:val="clear" w:fill="DDEEFF"/></w:tcPr></w:tblStylePr></w:style></w:styles>"#
    );
    let body = r#"<w:tbl><w:tblPr><w:tblStyle w:val="Grid"/><w:tblLayout w:type="fixed"/></w:tblPr><w:tblGrid><w:gridCol w:w="2000"/></w:tblGrid><w:tr><w:tc><w:p><w:r><w:t>Whole</w:t></w:r></w:p></w:tc></w:tr></w:tbl>"#;
    let bytes = styled_package(body, &styles);
    let mut doc = open_document(&bytes, &OpenOptions::default()).unwrap();
    doc.layout().unwrap();
    let table = doc.pages()[0]
        .fragments
        .iter()
        .find_map(|fragment| match fragment {
            Fragment::Table(table) => Some(table),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        table.cells[0].shading,
        edgedoc_geom::Rgba::parse_hex("DDEEFF")
    );
    assert!(
        table.cells[0].rect.h < 20.0,
        "whole-table paragraph spacing must apply"
    );
    assert!(table
        .borders
        .iter()
        .any(|border| (border.border.width - 1.0).abs() < 0.01));
    let mut edited = edgedoc::Document::read_bytes(&bytes).unwrap();
    let style = edited.styles().by_id("Grid").unwrap().clone();
    edited.add_style(style);
    let reopened = edgedoc::Document::read_bytes(&edited.write_bytes().unwrap()).unwrap();
    assert_eq!(
        reopened.styles().by_id("Grid").unwrap().table_regions[0]
            .table_borders
            .top
            .as_ref()
            .unwrap()
            .size,
        Some(8)
    );
}

#[test]
fn table_row_height_includes_cell_insets_and_border_bands() {
    let body = r#"<w:tbl><w:tblPr><w:tblLayout w:type="fixed"/><w:tblBorders><w:top w:val="single" w:sz="4"/><w:bottom w:val="single" w:sz="4"/><w:insideH w:val="single" w:sz="4"/></w:tblBorders><w:tblCellMar><w:bottom w:w="40" w:type="dxa"/></w:tblCellMar></w:tblPr><w:tblGrid><w:gridCol w:w="2000"/></w:tblGrid><w:tr><w:trPr><w:trHeight w:val="341" w:hRule="atLeast"/></w:trPr><w:tc><w:p><w:pPr><w:spacing w:after="0"/></w:pPr><w:r><w:t>Minimum</w:t></w:r></w:p></w:tc></w:tr><w:tr><w:trPr><w:trHeight w:val="400" w:hRule="exact"/></w:trPr><w:tc><w:p><w:pPr><w:spacing w:after="0"/></w:pPr><w:r><w:t>Exact</w:t></w:r></w:p></w:tc></w:tr></w:tbl>"#;
    let doc = render(body, "", "", &[]);
    let table = doc.pages()[0]
        .fragments
        .iter()
        .find_map(|fragment| match fragment {
            Fragment::Table(table) => Some(table),
            _ => None,
        })
        .unwrap();
    assert!(
        (table.cells[0].rect.h - 19.55).abs() < 0.01,
        "minimum height includes bottom margin and top border"
    );
    assert!(
        (table.cells[1].rect.h - 22.0).abs() < 0.01,
        "exact height includes bottom margin"
    );
}

#[test]
fn cell_paragraph_spacing_and_zero_row_height_follow_wordcraft() {
    let body = r#"<w:tbl><w:tblPr><w:tblLayout w:type="fixed"/></w:tblPr><w:tblGrid><w:gridCol w:w="4000"/></w:tblGrid><w:tr><w:trPr><w:trHeight w:val="0" w:hRule="exact"/></w:trPr><w:tc><w:p><w:pPr><w:spacing w:before="200" w:after="400"/></w:pPr><w:r><w:t>First</w:t></w:r></w:p><w:p><w:pPr><w:spacing w:before="600" w:after="0"/></w:pPr><w:r><w:t>Second</w:t></w:r></w:p></w:tc></w:tr></w:tbl>"#;
    let doc = render(body, "", "", &[]);
    let table = doc.pages()[0]
        .fragments
        .iter()
        .find_map(|fragment| match fragment {
            Fragment::Table(table) => Some(table),
            _ => None,
        })
        .unwrap();
    let paragraphs = table
        .content
        .iter()
        .filter_map(|fragment| match fragment {
            Fragment::Paragraph(para) => Some(para),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!((paragraphs[0].rect.y - table.cells[0].rect.y - 10.0).abs() < 0.01);
    assert!((paragraphs[1].rect.y - paragraphs[0].rect.bottom() - 30.0).abs() < 0.01);
    assert!(table.cells[0].rect.h > 50.0);
}
