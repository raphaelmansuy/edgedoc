//! The handle that the module exposes, with no `wasm-bindgen` dependency.
//!
//! Every method returns a plain JSON string, so the same code runs in a native test and in the
//! browser. The `bind` module in `lib.rs` wraps it. See `DP-B-04` to `DP-B-06`.

use edgedoc_display::{DisplayDocument, LayoutOptions, OpenOptions, SearchOptions, Severity};

/// One page's metadata, which a viewer uses to size a placeholder.
#[derive(Debug, Clone, PartialEq)]
pub struct PageMeta {
    pub index: u32,
    pub w: f32,
    pub h: f32,
    pub number: u32,
    pub label: String,
}

/// The document metadata that `open` and `layout` return.
#[derive(Debug, Clone, PartialEq)]
pub struct ViewerMeta {
    pub title: String,
    pub pages: u32,
    /// The page size of page 1, which sizes a placeholder before layout. See `DP-V-20`.
    pub page_w: f32,
    pub page_h: f32,
}

/// A document handle. One worker owns one handle.
#[derive(Default)]
pub struct DisplayHandle {
    doc: Option<DisplayDocument>,
    /// The encoded pages, which `page` returns.
    pages: Vec<Vec<u8>>,
    meta: Option<ViewerMeta>,
}

impl DisplayHandle {
    pub fn new() -> Self {
        Self::default()
    }

    /// Open a package. See `DP-P-03`.
    pub fn open(&mut self, bytes: &[u8]) -> Result<String, (String, String)> {
        let doc = DisplayDocument::open(bytes, &OpenOptions::default())
            .map_err(|e| (e.code.clone(), e.message.clone()))?;
        let title = doc.title().to_string();
        self.doc = Some(doc);
        self.pages.clear();
        self.meta = Some(ViewerMeta {
            title: title.clone(),
            pages: 0,
            page_w: 595.28,
            page_h: 841.89,
        });
        Ok(format!("{{\"title\":{},\"pages\":0}}", json_string(&title)))
    }

    /// Lay out and encode every page. See `DP-V-01`.
    pub fn layout(&mut self) -> Result<String, (String, String)> {
        let Some(doc) = self.doc.as_mut() else {
            return Err((
                "display.bad_state".into(),
                "Call open before layout.".into(),
            ));
        };
        let report = doc
            .layout_with(&LayoutOptions::default())
            .map_err(|e| (e.code.clone(), e.message.clone()))?;
        self.pages.clear();
        for i in 0..doc.pages().len() {
            if let Some(bytes) = doc.page_bytes(i) {
                self.pages.push(bytes);
            }
        }
        let (w, h) = doc.pages().first().map_or((595.28, 841.89), |p| (p.w, p.h));
        if let Some(meta) = self.meta.as_mut() {
            meta.pages = report.pages;
            meta.page_w = w;
            meta.page_h = h;
        }
        Ok(format!(
            "{{\"pages\":{},\"passes\":{},\"elapsed_ms\":{:.1}}}",
            report.pages, report.passes, report.elapsed_ms
        ))
    }

    pub fn page_count(&self) -> u32 {
        self.meta.as_ref().map_or(0, |m| m.pages)
    }

    pub fn page_bytes(&self, index: usize) -> Option<Vec<u8>> {
        self.pages.get(index).cloned()
    }

    pub fn all_page_bytes(&self) -> Result<Vec<Vec<u8>>, (String, String)> {
        if self.pages.is_empty() {
            return Err((
                "display.bad_state".into(),
                "Call layout before pages.".into(),
            ));
        }
        Ok(self.pages.clone())
    }

    pub fn page_size(&self, index: usize) -> Option<(f32, f32)> {
        let doc = self.doc.as_ref()?;
        doc.pages().get(index).map(|p| (p.w, p.h))
    }

    /// One page as an SVG document.
    pub fn svg(&self, index: usize) -> Option<String> {
        self.doc.as_ref()?.to_svg(index)
    }

    /// The visible text of one page.
    pub fn text(&self, index: usize) -> Option<String> {
        self.doc.as_ref()?.page_text(index)
    }

    /// The outline as JSON. See `DP-V-37`.
    pub fn outline_json(&self) -> String {
        let Some(doc) = self.doc.as_ref() else {
            return "[]".into();
        };
        let items: Vec<String> = doc
            .outline()
            .iter()
            .map(|i| {
                format!(
                    "{{\"title\":{},\"level\":{},\"path\":{}}}",
                    json_string(&i.title),
                    i.level,
                    json_string(&i.path)
                )
            })
            .collect();
        format!("[{}]", items.join(","))
    }

    /// The diagnostics as JSON. See `DP-P-25`.
    pub fn diagnostics_json(&self) -> String {
        let Some(doc) = self.doc.as_ref() else {
            return "[]".into();
        };
        let items: Vec<String> = doc
            .diagnostics()
            .iter()
            .map(|d| {
                format!(
                    "{{\"code\":{},\"severity\":{},\"page\":{},\"detail\":{},\"summary\":{}}}",
                    json_string(d.code),
                    json_string(d.severity.as_str()),
                    d.page.map_or_else(|| "null".to_string(), |p| p.to_string()),
                    json_string(&d.detail),
                    json_string(edgedoc_display::diagnose::summary_for(d.code))
                )
            })
            .collect();
        format!("[{}]", items.join(","))
    }

    /// A hit test, as JSON. See `DP-G-27`.
    pub fn hit_json(&self, page: usize, x: f32, y: f32) -> Option<String> {
        let doc = self.doc.as_ref()?;
        match doc.hit_test(page, edgedoc_display::Point::new(x, y))? {
            edgedoc_display::Hit::Glyph {
                rect,
                baseline,
                text_offset,
                run,
            } => Some(format!(
                "{{\"kind\":\"glyph\",\"x\":{:.2},\"y\":{:.2},\"w\":{:.2},\"h\":{:.2},\"baseline\":{:.2},\"offset\":{},\"text\":{}}}",
                rect.x, rect.y, rect.w, rect.h, baseline, text_offset, json_string(&run)
            )),
            edgedoc_display::Hit::Image { rect, resource } => Some(format!(
                "{{\"kind\":\"image\",\"x\":{:.2},\"y\":{:.2},\"w\":{:.2},\"h\":{:.2},\"resource\":{}}}",
                rect.x, rect.y, rect.w, rect.h, resource
            )),
            edgedoc_display::Hit::Link { rect, target } => Some(format!(
                "{{\"kind\":\"link\",\"x\":{:.2},\"y\":{:.2},\"w\":{:.2},\"h\":{:.2},\"target\":{}}}",
                rect.x,
                rect.y,
                rect.w,
                rect.h,
                target.map_or_else(|| "null".to_string(), |t| json_string(&t))
            )),
            edgedoc_display::Hit::Page { page } => Some(format!("{{\"kind\":\"page\",\"page\":{page}}}")),
        }
    }

    /// A search, as JSON. See `DP-V-26`.
    pub fn search_json(&self, query: &str) -> String {
        let Some(doc) = self.doc.as_ref() else {
            return "{\"hits\":[],\"query\":\"\"}".into();
        };
        let results = doc.search(query, &SearchOptions::default());
        let hits: Vec<String> = results
            .hits
            .iter()
            .map(|h| {
                let rects: Vec<String> = h
                    .rects
                    .iter()
                    .map(|r| format!("[{:.2},{:.2},{:.2},{:.2}]", r.x, r.y, r.w, r.h))
                    .collect();
                format!(
                    "{{\"page\":{},\"rects\":[{}],\"text\":{}}}",
                    h.page,
                    rects.join(","),
                    json_string(&h.text)
                )
            })
            .collect();
        format!(
            "{{\"hits\":[{}],\"query\":{}}}",
            hits.join(","),
            json_string(&results.query)
        )
    }

    /// Every page's metadata.
    pub fn page_meta(&self) -> Vec<PageMeta> {
        let Some(doc) = self.doc.as_ref() else {
            return Vec::new();
        };
        doc.pages()
            .iter()
            .map(|p| PageMeta {
                index: p.index,
                w: p.w,
                h: p.h,
                number: p.number,
                label: p.label.clone(),
            })
            .collect()
    }

    pub fn clear(&mut self) {
        self.doc = None;
        self.pages.clear();
        self.meta = None;
    }

    /// Every image resource, as `(id, mime, bytes)`, so a viewer can paint an image.
    /// See `DP-P-10`.
    pub fn image_resources(&self) -> Vec<(u32, String, Vec<u8>)> {
        edgedoc_display::resources()
            .into_iter()
            .filter_map(|(id, _name, bytes)| {
                edgedoc_display::sniff_mime(&bytes).map(|mime| (id, mime.to_string(), bytes))
            })
            .collect()
    }

    /// The image resources, cached so an index and a byte read agree.
    fn images(&self) -> Vec<(u32, String, Vec<u8>)> {
        self.image_resources()
    }

    /// A JSON index of the images, so a viewer knows what to fetch.
    pub fn image_index_json(&self) -> String {
        let items: Vec<String> = self
            .images()
            .iter()
            .enumerate()
            .map(|(i, (id, mime, bytes))| {
                format!(
                    "{{\"index\":{i},\"id\":{id},\"mime\":{},\"length\":{}}}",
                    json_string(mime),
                    bytes.len()
                )
            })
            .collect();
        format!("[{}]", items.join(","))
    }

    /// One image's bytes by its index.
    pub fn image_bytes(&self, index: usize) -> Option<Vec<u8>> {
        self.images().get(index).map(|(_, _, bytes)| bytes.clone())
    }

    /// The count of each severity, which a badge shows. See `DP-U-18`.
    pub fn severity_counts(&self) -> (usize, usize) {
        let Some(doc) = self.doc.as_ref() else {
            return (0, 0);
        };
        let warnings = doc
            .diagnostics()
            .iter()
            .filter(|d| d.severity == Severity::Warning)
            .count();
        let errors = doc
            .diagnostics()
            .iter()
            .filter(|d| d.severity >= Severity::Error)
            .count();
        (warnings, errors)
    }
}

/// Escape a string for JSON. See `DP-B-05`.
pub fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report_doc() -> Vec<u8> {
        let files = edgedoc_demos::report::render().expect("render");
        files
            .into_iter()
            .find(|r| r.name == "report.docx")
            .map(|r| r.bytes)
            .expect("report.docx")
    }

    #[test]
    fn a_handle_opens_lays_out_and_returns_pages() {
        let mut h = DisplayHandle::new();
        h.open(&report_doc()).expect("open");
        let report = h.layout().expect("layout");
        assert!(report.contains("\"pages\""));
        assert!(h.page_count() > 1);
        assert_eq!(
            h.all_page_bytes().expect("bytes").len(),
            h.page_count() as usize
        );
    }

    #[test]
    fn a_page_is_decodable() {
        let mut h = DisplayHandle::new();
        h.open(&report_doc()).expect("open");
        h.layout().expect("layout");
        let bytes = h.page_bytes(0).expect("page");
        let page = edgedoc_paint::decode(&bytes).expect("decode");
        assert_eq!(page.index, 0);
    }

    #[test]
    fn a_page_has_a_size() {
        let mut h = DisplayHandle::new();
        h.open(&report_doc()).expect("open");
        h.layout().expect("layout");
        let (w, hh) = h.page_size(0).expect("size");
        assert!((w - 595.28).abs() < 2.0);
        assert!(hh > 700.0);
    }

    #[test]
    fn layout_before_open_is_refused() {
        let mut h = DisplayHandle::new();
        let err = h.layout().unwrap_err();
        assert_eq!(err.0, "display.bad_state");
    }

    #[test]
    fn pages_before_layout_are_refused() {
        let mut h = DisplayHandle::new();
        h.open(&report_doc()).expect("open");
        assert!(h.all_page_bytes().is_err());
    }

    #[test]
    fn the_outline_is_json() {
        let mut h = DisplayHandle::new();
        h.open(&report_doc()).expect("open");
        let json = h.outline_json();
        assert!(json.starts_with('['));
        assert!(json.ends_with(']'));
    }

    #[test]
    fn the_diagnostics_are_json() {
        let mut h = DisplayHandle::new();
        h.open(&report_doc()).expect("open");
        h.layout().expect("layout");
        let json = h.diagnostics_json();
        assert!(json.starts_with('['));
        assert!(json.contains("display.font_substituted"), "{json}");
    }

    #[test]
    fn a_search_returns_json() {
        let mut h = DisplayHandle::new();
        h.open(&report_doc()).expect("open");
        h.layout().expect("layout");
        let json = h.search_json("Revenue");
        assert!(json.starts_with('{'));
        assert!(json.contains("\"hits\""));
    }

    #[test]
    fn a_hit_returns_json() {
        let mut h = DisplayHandle::new();
        h.open(&report_doc()).expect("open");
        h.layout().expect("layout");
        let json = h.hit_json(0, 100.0, 100.0);
        assert!(json.is_some());
    }

    #[test]
    fn an_svg_page_is_well_formed() {
        let mut h = DisplayHandle::new();
        h.open(&report_doc()).expect("open");
        h.layout().expect("layout");
        let svg = h.svg(0).expect("svg");
        assert!(svg.starts_with("<svg"));
    }

    #[test]
    fn json_escaping_handles_quotes_and_newlines() {
        assert_eq!(json_string("a\"b"), "\"a\\\"b\"");
        assert_eq!(json_string("a\nb"), "\"a\\nb\"");
    }

    #[test]
    fn clear_releases_the_document() {
        let mut h = DisplayHandle::new();
        h.open(&report_doc()).expect("open");
        h.layout().expect("layout");
        h.clear();
        assert_eq!(h.page_count(), 0);
        assert!(h.page_bytes(0).is_none());
    }
}
