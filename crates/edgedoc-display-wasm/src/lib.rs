//! WebAssembly adapter for the display library.
//!
//! The module runs inside a dedicated worker. It exports one class, so a caller never sees the
//! crate graph. See `DP-B-01` to `DP-B-13`.

#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![deny(clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod protocol;
pub mod surface;

pub use protocol::{Request, Response, PROTOCOL_VERSION};
pub use surface::{DisplayHandle, PageMeta, ViewerMeta};

/// The version of this module, which a viewer checks. See `DP-B-13`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(feature = "wasm")]
mod bind {
    use wasm_bindgen::prelude::*;

    use super::surface::DisplayHandle;

    /// The protocol version, so a viewer can refuse a stale worker.
    #[wasm_bindgen(js_name = protocolVersion)]
    pub fn protocol_version() -> u32 {
        super::PROTOCOL_VERSION
    }

    /// The module version.
    #[wasm_bindgen(js_name = moduleVersion)]
    pub fn module_version() -> String {
        super::VERSION.to_string()
    }

    /// A document handle. One worker owns one handle.
    #[wasm_bindgen]
    pub struct Viewer {
        inner: DisplayHandle,
    }

    #[wasm_bindgen]
    impl Viewer {
        /// Create a handle. The optional face bytes register extra faces.
        #[wasm_bindgen(constructor)]
        pub fn new() -> Viewer {
            Viewer {
                inner: DisplayHandle::new(),
            }
        }

        /// Open a package. Returns a JSON string with the document meta, or an error object.
        pub fn open(&mut self, bytes: &[u8]) -> Result<String, JsValue> {
            self.inner.open(bytes).map_err(js_error)
        }

        /// Lay out and encode every page. Returns a JSON string with the report.
        pub fn layout(&mut self) -> Result<String, JsValue> {
            self.inner.layout().map_err(js_error)
        }

        /// Every page display list, as one array of `Uint8Array`.
        pub fn pages(&mut self) -> Result<js_sys::Array, JsValue> {
            let list = self.inner.all_page_bytes().map_err(js_error)?;
            let out = js_sys::Array::new();
            for bytes in list {
                out.push(&js_sys::Uint8Array::from(bytes.as_slice()));
            }
            Ok(out)
        }

        /// One page display list as bytes.
        pub fn page(&mut self, index: u32) -> Result<Vec<u8>, JsValue> {
            self.inner
                .page_bytes(index as usize)
                .ok_or_else(|| js_error(("display.bad_state".into(), "No such page".into())))
        }

        /// The page count after a layout.
        #[wasm_bindgen(js_name = pageCount)]
        pub fn page_count(&self) -> u32 {
            self.inner.page_count()
        }

        /// The document outline, as a JSON string.
        pub fn outline(&self) -> String {
            self.inner.outline_json()
        }

        /// The diagnostics, as a JSON string.
        pub fn diagnostics(&self) -> String {
            self.inner.diagnostics_json()
        }

        /// A hit test at a point on a page, as a JSON string.
        #[wasm_bindgen(js_name = hitTest)]
        pub fn hit_test(&self, page: u32, x: f32, y: f32) -> Option<String> {
            self.inner.hit_json(page as usize, x, y)
        }

        /// Search for a query, as a JSON string.
        pub fn search(&self, query: &str) -> String {
            self.inner.search_json(query)
        }

        /// One page as an SVG document, which a caller can set as `innerHTML`.
        pub fn svg(&self, index: u32) -> Option<String> {
            self.inner.svg(index as usize)
        }

        /// The visible text of one page.
        pub fn text(&self, index: u32) -> Option<String> {
            self.inner.text(index as usize)
        }

        /// The page size in points, as `[w, h]`.
        #[wasm_bindgen(js_name = pageSize)]
        pub fn page_size(&self, index: u32) -> Option<Box<[f32]>> {
            self.inner
                .page_size(index as usize)
                .map(|(w, h)| Box::new([w, h]) as Box<[f32]>)
        }

        /// The actual font outline, used by the viewer for shaped glyph IDs.
        #[wasm_bindgen(js_name = glyphOutline)]
        pub fn glyph_outline(&self, face: u32, glyph: u16) -> Option<String> {
            self.inner.glyph_outline_json(face, glyph)
        }

        /// Every image, as a JSON list of `{ id, mime, length }` in the order of `images()`.
        #[wasm_bindgen(js_name = imageIndex)]
        pub fn image_index(&self) -> String {
            self.inner.image_index_json()
        }

        /// One image's bytes, by its index in the resource list.
        pub fn image(&self, index: u32) -> Option<Vec<u8>> {
            self.inner.image_bytes(index as usize)
        }

        /// Release every page, so a viewer can load another document.
        pub fn clear(&mut self) {
            self.inner.clear();
        }

        /// Open a blank page for editing. Returns the caret state as JSON.
        #[wasm_bindgen(js_name = editBlank)]
        pub fn edit_blank(&mut self) -> Result<String, JsValue> {
            self.inner.edit_blank().map_err(js_error)
        }

        /// Open a package for editing. Returns the caret state as JSON.
        #[wasm_bindgen(js_name = editOpen)]
        pub fn edit_open(&mut self, bytes: &[u8]) -> Result<String, JsValue> {
            self.inner.edit_open(bytes).map_err(js_error)
        }

        /// The caret, the selection, and the ribbon flags, as JSON.
        #[wasm_bindgen(js_name = editState)]
        pub fn edit_state(&self) -> Result<String, JsValue> {
            self.inner.edit_state().map_err(js_error)
        }

        /// Record the current bytes as saved, so the unsaved marker clears.
        #[wasm_bindgen(js_name = markSaved)]
        pub fn mark_saved(&mut self) -> Result<String, JsValue> {
            self.inner.mark_saved().map_err(js_error)
        }

        /// The current package, including edits.
        #[wasm_bindgen(js_name = documentBytes)]
        pub fn document_bytes(&self) -> Result<Vec<u8>, JsValue> {
            self.inner.document_bytes().map_err(js_error)
        }

        #[wasm_bindgen(js_name = editInsert)]
        pub fn edit_insert(&mut self, text: &str) -> Result<String, JsValue> {
            self.inner.edit_insert(text).map_err(js_error)
        }

        #[wasm_bindgen(js_name = editDelete)]
        pub fn edit_delete(&mut self, backward: bool) -> Result<String, JsValue> {
            self.inner.edit_delete(backward).map_err(js_error)
        }

        /// Replace the next match, or every match (specs/144 DOC2-39). One call, one undo step.
        #[wasm_bindgen(js_name = editReplace)]
        pub fn edit_replace(
            &mut self,
            query: &str,
            text: &str,
            all: bool,
        ) -> Result<String, JsValue> {
            self.inner.edit_replace(query, text, all).map_err(js_error)
        }

        #[wasm_bindgen(js_name = editEnter)]
        pub fn edit_enter(&mut self, soft: bool) -> Result<String, JsValue> {
            self.inner.edit_enter(soft).map_err(js_error)
        }

        #[wasm_bindgen(js_name = editMergeField)]
        pub fn edit_merge_field(&mut self, name: &str) -> Result<String, JsValue> {
            self.inner.edit_field(name).map_err(js_error)
        }

        /// Insert a row below the caret's row (specs/144 DOC2-41).
        #[wasm_bindgen(js_name = editTableRowInsert)]
        pub fn edit_table_row_insert(&mut self) -> Result<String, JsValue> {
            self.inner.edit_table_row_insert().map_err(js_error)
        }

        /// Merge the caret's cell with another in the same row (specs/144 DOC2-43).
        #[wasm_bindgen(js_name = editTableMerge)]
        pub fn edit_table_merge(&mut self, to: &str) -> Result<String, JsValue> {
            self.inner.edit_table_merge(to).map_err(js_error)
        }

        /// Split the merged cell the caret is in.
        #[wasm_bindgen(js_name = editTableSplit)]
        pub fn edit_table_split(&mut self) -> Result<String, JsValue> {
            self.inner.edit_table_split().map_err(js_error)
        }

        /// Set the caret's column width in twips.
        #[wasm_bindgen(js_name = editTableColumnWidth)]
        pub fn edit_table_column_width(&mut self, width: i32) -> Result<String, JsValue> {
            self.inner.edit_table_column_width(width).map_err(js_error)
        }

        /// Delete the caret's row.
        #[wasm_bindgen(js_name = editTableRowDelete)]
        pub fn edit_table_row_delete(&mut self) -> Result<String, JsValue> {
            self.inner.edit_table_row_delete().map_err(js_error)
        }

        #[wasm_bindgen(js_name = editTable)]
        pub fn edit_table(&mut self, rows: u32, cols: u32) -> Result<String, JsValue> {
            self.inner.edit_table(rows, cols).map_err(js_error)
        }

        #[wasm_bindgen(js_name = editPicture)]
        pub fn edit_picture(
            &mut self,
            bytes: &[u8],
            width_pt: f32,
            height_pt: f32,
        ) -> Result<String, JsValue> {
            self.inner
                .edit_picture(bytes, width_pt, height_pt)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = pageSheet)]
        pub fn page_sheet(&self) -> Option<Box<[f32]>> {
            self.inner
                .page_sheet()
                .map(|sheet| Box::new(sheet) as Box<[f32]>)
        }

        #[wasm_bindgen(js_name = editPageSetup)]
        pub fn edit_page_setup(
            &mut self,
            paper: &str,
            landscape: bool,
            margin: i32,
        ) -> Result<String, JsValue> {
            self.inner
                .edit_page_setup(paper, landscape, margin)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = editMark)]
        pub fn edit_mark(&mut self, mark: &str) -> Result<String, JsValue> {
            self.inner.edit_mark(mark).map_err(js_error)
        }

        #[wasm_bindgen(js_name = editAlign)]
        pub fn edit_align(&mut self, align: &str) -> Result<String, JsValue> {
            self.inner.edit_align(align).map_err(js_error)
        }

        #[wasm_bindgen(js_name = editList)]
        pub fn edit_list(&mut self, kind: &str) -> Result<String, JsValue> {
            self.inner.edit_list(kind).map_err(js_error)
        }

        #[wasm_bindgen(js_name = editFont)]
        pub fn edit_font(&mut self, name: &str) -> Result<String, JsValue> {
            self.inner.edit_font(name).map_err(js_error)
        }

        #[wasm_bindgen(js_name = editFontSize)]
        pub fn edit_font_size(&mut self, size: f32) -> Result<String, JsValue> {
            self.inner.edit_font_size(size).map_err(js_error)
        }

        #[wasm_bindgen(js_name = editFontStep)]
        pub fn edit_font_step(&mut self, grow: bool) -> Result<String, JsValue> {
            self.inner.edit_font_step(grow).map_err(js_error)
        }

        #[wasm_bindgen(js_name = editClearCharacterFormatting)]
        pub fn edit_clear_character_formatting(&mut self) -> Result<String, JsValue> {
            self.inner
                .edit_clear_character_formatting()
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = editColor)]
        pub fn edit_color(&mut self, hex: &str) -> Result<String, JsValue> {
            self.inner.edit_color(hex).map_err(js_error)
        }

        #[wasm_bindgen(js_name = editHighlight)]
        pub fn edit_highlight(&mut self, name: &str) -> Result<String, JsValue> {
            self.inner.edit_highlight(name).map_err(js_error)
        }

        #[wasm_bindgen(js_name = editUndo)]
        pub fn edit_undo(&mut self) -> Result<String, JsValue> {
            self.inner.edit_undo().map_err(js_error)
        }

        #[wasm_bindgen(js_name = editRedo)]
        pub fn edit_redo(&mut self) -> Result<String, JsValue> {
            self.inner.edit_redo().map_err(js_error)
        }

        #[wasm_bindgen(js_name = editCaret)]
        pub fn edit_caret(
            &mut self,
            page: u32,
            x: f32,
            y: f32,
            extend: bool,
        ) -> Result<String, JsValue> {
            self.inner.edit_click(page, x, y, extend).map_err(js_error)
        }

        #[wasm_bindgen(js_name = editMove)]
        pub fn edit_move(&mut self, key: &str, extend: bool) -> Result<String, JsValue> {
            self.inner.edit_move(key, extend).map_err(js_error)
        }

        #[wasm_bindgen(js_name = editSelectParagraph)]
        pub fn edit_select_paragraph(&mut self) -> Result<String, JsValue> {
            self.inner.edit_select_paragraph().map_err(js_error)
        }

        #[wasm_bindgen(js_name = editSelectAll)]
        pub fn edit_select_all(&mut self) -> Result<String, JsValue> {
            self.inner.edit_select_all().map_err(js_error)
        }

        #[wasm_bindgen(js_name = editSelectWord)]
        pub fn edit_select_word(&mut self) -> Result<String, JsValue> {
            self.inner.edit_select_word().map_err(js_error)
        }

        #[wasm_bindgen(js_name = editSelectSentence)]
        pub fn edit_select_sentence(&mut self) -> Result<String, JsValue> {
            self.inner.edit_select_sentence().map_err(js_error)
        }

        #[wasm_bindgen(js_name = editLook)]
        pub fn edit_look(&mut self, name: &str) -> Result<String, JsValue> {
            self.inner.edit_look(name).map_err(js_error)
        }

        #[wasm_bindgen(js_name = editCase)]
        pub fn edit_case(&mut self, mode: &str) -> Result<String, JsValue> {
            self.inner.edit_case(mode).map_err(js_error)
        }

        #[wasm_bindgen(js_name = editPainter)]
        pub fn edit_painter(&mut self, sticky: bool) -> Result<String, JsValue> {
            self.inner.edit_painter(sticky).map_err(js_error)
        }

        #[wasm_bindgen(js_name = editIndent)]
        pub fn edit_indent(&mut self, delta: i32) -> Result<String, JsValue> {
            self.inner.edit_indent(delta).map_err(js_error)
        }

        #[wasm_bindgen(js_name = editLineSpacing)]
        pub fn edit_line_spacing(&mut self, multiple: f32) -> Result<String, JsValue> {
            self.inner.edit_line_spacing(multiple).map_err(js_error)
        }

        #[wasm_bindgen(js_name = editParaSpacing)]
        pub fn edit_para_spacing(&mut self, action: &str) -> Result<String, JsValue> {
            self.inner.edit_para_spacing(action).map_err(js_error)
        }

        #[wasm_bindgen(js_name = editParaShading)]
        pub fn edit_para_shading(&mut self, fill: Option<String>) -> Result<String, JsValue> {
            self.inner.edit_para_shading(fill).map_err(js_error)
        }

        #[wasm_bindgen(js_name = editParaBorder)]
        pub fn edit_para_border(&mut self, kind: &str) -> Result<String, JsValue> {
            self.inner.edit_para_border(kind).map_err(js_error)
        }

        /// Fill merge fields for one record. `values` is a plain object of strings.
        #[wasm_bindgen(js_name = editPreview)]
        pub fn edit_preview(&mut self, values: JsValue) -> Result<String, JsValue> {
            self.inner
                .edit_preview(&record_pairs(&values)?)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = editEndPreview)]
        pub fn edit_end_preview(&mut self) -> Result<String, JsValue> {
            self.inner.edit_end_preview().map_err(js_error)
        }
    }

    /// Read `{ name: value }` into the pairs a preview expects.
    fn record_pairs(values: &JsValue) -> Result<Vec<(String, String)>, JsValue> {
        if !values.is_object() {
            return Err(js_error((
                "editor.preview".into(),
                "The preview record must be an object.".into(),
            )));
        }
        let obj = js_sys::Object::from(values.clone());
        let keys = js_sys::Object::keys(&obj);
        let mut pairs = Vec::new();
        for key in keys.iter() {
            let Some(name) = key.as_string() else {
                continue;
            };
            let raw = js_sys::Reflect::get(&obj, &key).map_err(|_| {
                js_error((
                    "editor.preview".into(),
                    "The preview record must be an object.".into(),
                ))
            })?;
            let value = if let Some(text) = raw.as_string() {
                text
            } else if let Some(number) = raw.as_f64() {
                number.to_string()
            } else {
                String::new()
            };
            pairs.push((name, value));
        }
        Ok(pairs)
    }

    impl Default for Viewer {
        fn default() -> Self {
            Self::new()
        }
    }

    /// Map a `(code, message)` pair from the surface to a JavaScript error object.
    fn js_error(pair: (String, String)) -> JsValue {
        let (code, message) = pair;
        let obj = js_sys::Object::new();
        let _ = &code;
        let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("code"), &JsValue::from_str(&code));
        let _ = js_sys::Reflect::set(
            &obj,
            &JsValue::from_str("message"),
            &JsValue::from_str(&message),
        );
        obj.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_protocol_version_is_one() {
        assert_eq!(PROTOCOL_VERSION, 1);
    }

    #[test]
    fn the_module_version_is_not_empty() {
        assert!(!VERSION.is_empty());
    }
}
