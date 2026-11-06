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
