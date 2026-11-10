//! The handle that the module exposes, with no `wasm-bindgen` dependency.
//!
//! Every method returns a plain JSON string, so the same code runs in a native test and in the
//! browser. The `bind` module in `lib.rs` wraps it. See `DP-B-04` to `DP-B-06`.

use edgedoc_display::{
    DisplayDocument, Editor, EditorState, LayoutOptions, OpenOptions, SearchOptions, Severity,
};

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
    /// Present while the Word control is editing. Read methods prefer it over `doc`.
    editor: Option<Editor>,
    /// The encoded pages, which `page` returns.
    pages: Vec<Vec<u8>>,
    meta: Option<ViewerMeta>,
    /// How many matches the last `edit_replace` changed, so the surface can say "Replaced 3".
    replaced: usize,
}

impl DisplayHandle {
    pub fn new() -> Self {
        Self::default()
    }

    /// The document a read method should use. An editing session wins.
    fn view(&self) -> Option<&DisplayDocument> {
        if let Some(editor) = &self.editor {
            Some(editor.display())
        } else {
            self.doc.as_ref()
        }
    }

    /// Open a package for reading. See `DP-P-03`.
    pub fn open(&mut self, bytes: &[u8]) -> Result<String, (String, String)> {
        self.editor = None;
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
        if self.editor.is_some() {
            return self.layout_editor();
        }
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
        let doc = self.view()?;
        doc.pages().get(index).map(|p| (p.w, p.h))
    }

    /// A loaded glyph outline for the browser canvas.
    pub fn glyph_outline_json(&self, face: u32, glyph: u16) -> Option<String> {
        self.view()?.glyph_outline_json(face, glyph)
    }

    /// One page as an SVG document.
    pub fn svg(&self, index: usize) -> Option<String> {
        self.view()?.to_svg(index)
    }

    /// The visible text of one page.
    pub fn text(&self, index: usize) -> Option<String> {
        self.view()?.page_text(index)
    }

    /// The outline as JSON. See `DP-V-37`.
    pub fn outline_json(&self) -> String {
        let Some(doc) = self.view() else {
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
        let Some(doc) = self.view() else {
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
        let doc = self.view()?;
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
        let Some(doc) = self.view() else {
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
        let Some(doc) = self.view() else {
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
        self.editor = None;
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
        let Some(doc) = self.view() else {
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

    /// Open a blank page for editing and return the caret state.
    pub fn edit_blank(&mut self) -> Result<String, (String, String)> {
        let editor = Editor::blank().map_err(edit_err)?;
        self.doc = None;
        self.editor = Some(editor);
        self.finish_edit()
    }

    /// Open a package for editing. The caret starts in the first paragraph.
    pub fn edit_open(&mut self, bytes: &[u8]) -> Result<String, (String, String)> {
        let editor = Editor::open(bytes).map_err(edit_err)?;
        self.doc = None;
        self.editor = Some(editor);
        self.finish_edit()
    }

    /// The caret, the selection, and the ribbon flags.
    pub fn edit_state(&self) -> Result<String, (String, String)> {
        self.editor
            .as_ref()
            .map(|_| self.state_json())
            .ok_or(editor_closed())
    }

    /// The current package, including edits.
    pub fn document_bytes(&self) -> Result<Vec<u8>, (String, String)> {
        self.editor
            .as_ref()
            .ok_or_else(editor_closed)?
            .package()
            .map_err(edit_err)
    }

    pub fn edit_insert(&mut self, text: &str) -> Result<String, (String, String)> {
        self.editor
            .as_mut()
            .ok_or_else(editor_closed)?
            .insert_text(text)
            .map_err(edit_err)?;
        self.finish_edit()
    }

    /// Replace the next match, or every match (specs/144 DOC2-39). One call, one undo step.
    ///
    /// Returns the state, with the count of matches replaced in `replaced`, so the surface can say
    /// "Replaced 3" without a second query.
    pub fn edit_replace(&mut self, query: &str, text: &str, all: bool) -> Result<String, (String, String)> {
        let count = self
            .editor
            .as_mut()
            .ok_or_else(editor_closed)?
            .replace_text(query, text, all)
            .map_err(edit_err)?;
        self.replaced = count;
        self.finish_edit()
    }
    pub fn edit_delete(&mut self, backward: bool) -> Result<String, (String, String)> {
        self.editor
            .as_mut()
            .ok_or_else(editor_closed)?
            .delete(backward)
            .map_err(edit_err)?;
        self.finish_edit()
    }

    pub fn edit_enter(&mut self, soft: bool) -> Result<String, (String, String)> {
        self.editor
            .as_mut()
            .ok_or_else(editor_closed)?
            .insert_newline(soft)
            .map_err(edit_err)?;
        self.finish_edit()
    }

    pub fn edit_field(&mut self, name: &str) -> Result<String, (String, String)> {
        self.editor
            .as_mut()
            .ok_or_else(editor_closed)?
            .insert_field(name)
            .map_err(edit_err)?;
        self.finish_edit()
    }

    pub fn edit_table(&mut self, rows: u32, cols: u32) -> Result<String, (String, String)> {
        self.editor
            .as_mut()
            .ok_or_else(editor_closed)?
            .insert_table(rows as usize, cols as usize)
            .map_err(edit_err)?;
        self.finish_edit()
    }

    pub fn edit_picture(
        &mut self,
        bytes: &[u8],
        width_pt: f32,
        height_pt: f32,
    ) -> Result<String, (String, String)> {
        self.editor
            .as_mut()
            .ok_or_else(editor_closed)?
            .insert_picture(bytes.to_vec(), width_pt, height_pt)
            .map_err(edit_err)?;
        self.finish_edit()
    }

    pub fn edit_mark(&mut self, mark: &str) -> Result<String, (String, String)> {
        self.editor
            .as_mut()
            .ok_or_else(editor_closed)?
            .toggle_mark(mark)
            .map_err(edit_err)?;
        self.finish_edit()
    }

    pub fn edit_align(&mut self, align: &str) -> Result<String, (String, String)> {
        self.editor
            .as_mut()
            .ok_or_else(editor_closed)?
            .set_align(align)
            .map_err(edit_err)?;
        self.finish_edit()
    }

    pub fn edit_list(&mut self, kind: &str) -> Result<String, (String, String)> {
        self.editor
            .as_mut()
            .ok_or_else(editor_closed)?
            .toggle_list(kind)
            .map_err(edit_err)?;
        self.finish_edit()
    }

    pub fn edit_font(&mut self, name: &str) -> Result<String, (String, String)> {
        self.editor
            .as_mut()
            .ok_or_else(editor_closed)?
            .set_font(name)
            .map_err(edit_err)?;
        self.finish_edit()
    }

    pub fn edit_font_size(&mut self, size: f32) -> Result<String, (String, String)> {
        self.editor
            .as_mut()
            .ok_or_else(editor_closed)?
            .set_font_size(size)
            .map_err(edit_err)?;
        self.finish_edit()
    }

    pub fn edit_color(&mut self, hex: &str) -> Result<String, (String, String)> {
        self.editor
            .as_mut()
            .ok_or_else(editor_closed)?
            .set_color(hex)
            .map_err(edit_err)?;
        self.finish_edit()
    }

    pub fn edit_highlight(&mut self, name: &str) -> Result<String, (String, String)> {
        self.editor
            .as_mut()
            .ok_or_else(editor_closed)?
            .set_highlight(name)
            .map_err(edit_err)?;
        self.finish_edit()
    }

    pub fn edit_undo(&mut self) -> Result<String, (String, String)> {
        self.editor
            .as_mut()
            .ok_or_else(editor_closed)?
            .undo()
            .map_err(edit_err)?;
        self.finish_edit()
    }

    pub fn edit_redo(&mut self) -> Result<String, (String, String)> {
        self.editor
            .as_mut()
            .ok_or_else(editor_closed)?
            .redo()
            .map_err(edit_err)?;
        self.finish_edit()
    }

    pub fn edit_click(
        &mut self,
        page: u32,
        x: f32,
        y: f32,
        extend: bool,
    ) -> Result<String, (String, String)> {
        self.editor
            .as_mut()
            .ok_or_else(editor_closed)?
            .click(page as usize, x, y, extend);
        self.finish_edit()
    }

    pub fn edit_move(&mut self, key: &str, extend: bool) -> Result<String, (String, String)> {
        self.editor
            .as_mut()
            .ok_or_else(editor_closed)?
            .move_caret(key, extend);
        self.finish_edit()
    }

    pub fn edit_select_all(&mut self) -> Result<String, (String, String)> {
        self.editor.as_mut().ok_or_else(editor_closed)?.select_all();
        self.finish_edit()
    }

    pub fn edit_select_word(&mut self) -> Result<String, (String, String)> {
        self.editor
            .as_mut()
            .ok_or_else(editor_closed)?
            .select_word();
        self.finish_edit()
    }

    pub fn edit_select_sentence(&mut self) -> Result<String, (String, String)> {
        self.editor
            .as_mut()
            .ok_or_else(editor_closed)?
            .select_sentence();
        self.finish_edit()
    }

    pub fn edit_look(&mut self, name: &str) -> Result<String, (String, String)> {
        self.editor
            .as_mut()
            .ok_or_else(editor_closed)?
            .apply_look(name)
            .map_err(edit_err)?;
        self.finish_edit()
    }

    pub fn edit_case(&mut self, mode: &str) -> Result<String, (String, String)> {
        self.editor
            .as_mut()
            .ok_or_else(editor_closed)?
            .change_case(mode)
            .map_err(edit_err)?;
        self.finish_edit()
    }

    pub fn edit_painter(&mut self, sticky: bool) -> Result<String, (String, String)> {
        self.editor
            .as_mut()
            .ok_or_else(editor_closed)?
            .arm_painter(sticky);
        self.finish_edit()
    }

    pub fn edit_indent(&mut self, delta: i32) -> Result<String, (String, String)> {
        self.editor
            .as_mut()
            .ok_or_else(editor_closed)?
            .bump_indent(delta)
            .map_err(edit_err)?;
        self.finish_edit()
    }

    pub fn edit_line_spacing(&mut self, multiple: f32) -> Result<String, (String, String)> {
        self.editor
            .as_mut()
            .ok_or_else(editor_closed)?
            .set_line_spacing(multiple)
            .map_err(edit_err)?;
        self.finish_edit()
    }

    pub fn edit_para_spacing(&mut self, action: &str) -> Result<String, (String, String)> {
        self.editor
            .as_mut()
            .ok_or_else(editor_closed)?
            .para_spacing(action)
            .map_err(edit_err)?;
        self.finish_edit()
    }

    pub fn edit_para_shading(&mut self, fill: Option<String>) -> Result<String, (String, String)> {
        self.editor
            .as_mut()
            .ok_or_else(editor_closed)?
            .set_para_shading(fill.as_deref())
            .map_err(edit_err)?;
        self.finish_edit()
    }

    pub fn edit_para_border(&mut self, kind: &str) -> Result<String, (String, String)> {
        self.editor
            .as_mut()
            .ok_or_else(editor_closed)?
            .set_para_border(kind)
            .map_err(edit_err)?;
        self.finish_edit()
    }

    pub fn edit_preview(
        &mut self,
        values: &[(String, String)],
    ) -> Result<String, (String, String)> {
        self.editor
            .as_mut()
            .ok_or_else(editor_closed)?
            .preview(values)
            .map_err(edit_err)?;
        self.finish_edit()
    }

    pub fn edit_end_preview(&mut self) -> Result<String, (String, String)> {
        self.editor
            .as_mut()
            .ok_or_else(editor_closed)?
            .end_preview()
            .map_err(edit_err)?;
        self.finish_edit()
    }

    fn layout_editor(&mut self) -> Result<String, (String, String)> {
        let (pages, w, h, title, count, passes, elapsed) = {
            let editor = self.editor.as_mut().ok_or_else(editor_closed)?;
            let report = editor.layout().map_err(edit_err)?;
            let packed = pack_pages(editor.display());
            (
                packed.pages,
                packed.w,
                packed.h,
                packed.title,
                report.pages,
                report.passes,
                report.elapsed_ms,
            )
        };
        self.store_view(pages, w, h, title, count);
        Ok(format!(
            "{{\"pages\":{count},\"passes\":{passes},\"elapsed_ms\":{elapsed:.1}}}"
        ))
    }

    fn finish_edit(&mut self) -> Result<String, (String, String)> {
        let packed = {
            let doc = self.editor.as_ref().ok_or_else(editor_closed)?.display();
            pack_pages(doc)
        };
        let count = packed.count;
        self.store_view(packed.pages, packed.w, packed.h, packed.title, count);
        // The count belongs to the call that produced it: an unrelated keystroke must not keep
        // saying "Replaced 3". Only `edit_replace` sets it, and only for the state it returns.
        let state = self.state_json();
        self.replaced = 0;
        Ok(state)
    }

    fn store_view(&mut self, pages: Vec<Vec<u8>>, w: f32, h: f32, title: String, count: u32) {
        self.pages = pages;
        match self.meta.as_mut() {
            Some(meta) => {
                meta.title = title;
                meta.pages = count;
                meta.page_w = w;
                meta.page_h = h;
            }
            None => {
                self.meta = Some(ViewerMeta {
                    title,
                    pages: count,
                    page_w: w,
                    page_h: h,
                });
            }
        }
    }

    fn state_json(&self) -> String {
        let Some(state) = self.editor.as_ref().map(Editor::state) else {
            return "{}".to_string();
        };
        encode_state(&state, self.replaced)
    }
}

struct PackedPages {
    pages: Vec<Vec<u8>>,
    w: f32,
    h: f32,
    title: String,
    count: u32,
}

fn pack_pages(doc: &DisplayDocument) -> PackedPages {
    let mut pages = Vec::new();
    for i in 0..doc.pages().len() {
        if let Some(bytes) = doc.page_bytes(i) {
            pages.push(bytes);
        }
    }
    let (w, h) = doc
        .pages()
        .first()
        .map_or((595.28, 841.89), |page| (page.w, page.h));
    let count = doc
        .report()
        .map_or(pages.len() as u32, |report| report.pages);
    PackedPages {
        pages,
        w,
        h,
        title: doc.title().to_string(),
        count,
    }
}

fn encode_state(state: &EditorState, replaced: usize) -> String {
    let marks: Vec<String> = state
        .marks
        .iter()
        .map(|mark| {
            format!(
                "{{\"page\":{},\"x\":{:.2},\"y\":{:.2},\"w\":{:.2},\"h\":{:.2}}}",
                mark.page, mark.x, mark.y, mark.w, mark.h
            )
        })
        .collect();
    let fields: Vec<String> = state.fields.iter().map(|name| json_string(name)).collect();
    let anchor_path = state
        .anchor_path
        .as_deref()
        .map_or_else(|| "null".to_string(), json_string);
    let anchor_offset = state
        .anchor_offset
        .map_or_else(|| "null".to_string(), |offset| offset.to_string());
    let field = state
        .field
        .as_deref()
        .map_or_else(|| "null".to_string(), json_string);
    format!(
        "{{\"path\":{},\"offset\":{},\"page\":{},\"x\":{:.2},\"y\":{:.2},\"h\":{:.2},\"anchorPath\":{},\"anchorOffset\":{},\"marks\":[{}],\"bold\":{},\"italic\":{},\"underline\":{},\"strike\":{},\"subscript\":{},\"superscript\":{},\"fontSize\":{:.2},\"align\":{},\"list\":{},\"field\":{},\"canUndo\":{},\"canRedo\":{},\"preview\":{},\"dirty\":{},\"words\":{},\"pages\":{},\"fields\":[{}],\"title\":{},\"revision\":{},\"selection\":{},\"indentLeft\":{:.2},\"line\":{:.2},\"look\":{},\"painter\":{},\"paraShading\":{},\"paraBorderBottom\":{},\"replaced\":{}}}",
        json_string(&state.path),
        state.offset,
        state.page,
        state.x,
        state.y,
        state.h,
        anchor_path,
        anchor_offset,
        marks.join(","),
        state.bold,
        state.italic,
        state.underline,
        state.strike,
        state.subscript,
        state.superscript,
        state.font_size,
        json_string(&state.align),
        json_string(&state.list),
        field,
        state.can_undo,
        state.can_redo,
        state.preview,
        state.dirty,
        state.words,
        state.pages,
        fields.join(","),
        json_string(&state.title),
        state.revision,
        json_string(&state.selection),
        state.indent_left,
        state.line,
        json_string(&state.look),
        state.painter,
        json_string(&state.para_shading),
        state.para_border_bottom,
        replaced
    )
}

fn edit_err(err: edgedoc_display::Error) -> (String, String) {
    (err.code, err.message)
}

fn editor_closed() -> (String, String) {
    (
        "editor.closed".into(),
        "Open a document for editing first.".into(),
    )
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
        let run = page
            .ops
            .iter()
            .find_map(|op| match op {
                edgedoc_paint::Op::Glyphs(run) => Some(run),
                _ => None,
            })
            .expect("glyph run");
        let outline = h
            .glyph_outline_json(run.face, run.glyphs[0])
            .expect("loaded font outline");
        assert!(outline.contains(",\"M"), "{outline}");
        assert!(h.glyph_outline_json(u32::MAX, 0).is_none());
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

    /// specs/144 DOC2-39: the surface carries the replace count for exactly one state.
    ///
    /// "Replaced 3" is what the surface says after a replace-all, and it must not survive into the
    /// next state: a person who then types a letter is not told a third match was replaced.
    #[test]
    fn a_replace_reports_what_it_changed_in_one_state_only() {
        let mut h = DisplayHandle::new();
        h.edit_blank().expect("blank");
        h.edit_insert("Pay soon. Pay now.").expect("type");
        h.edit_move("doc-start", false).expect("caret");

        let json = h.edit_replace("Pay", "Send", true).expect("replace");
        assert!(json.contains("\"replaced\":2"), "{json}");

        let after = h.edit_insert("x").expect("type");
        assert!(
            after.contains("\"replaced\":0"),
            "the count belongs to the replace's own state: {after}"
        );
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

    #[test]
    fn the_invoice_template_lists_every_merge_field() {
        let bytes = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../site/assets/documents/invoice-template.docx"
        ))
        .expect("invoice template");
        let mut handle = DisplayHandle::new();
        let state = handle.edit_open(&bytes).expect("open");
        for name in [
            "InvoiceNumber",
            "Customer",
            "City",
            "Subtotal",
            "VAT",
            "Total",
        ] {
            assert!(state.contains(&format!("\"{name}\"")), "{state}");
        }
    }

    #[test]
    fn an_edit_session_types_and_keeps_the_read_only_open_path() {
        let mut h = DisplayHandle::new();
        let state = h.edit_blank().expect("blank");
        assert!(state.contains("\"path\":\"body[0]\""));
        h.edit_insert("Hello").expect("type");
        let painted = h.text(0).unwrap_or_default();
        assert!(painted.contains("Hello"), "{painted}");
        assert!(h.page_count() >= 1);
        h.open(&report_doc()).expect("open");
        assert!(h.edit_insert("x").is_err());
    }
}
