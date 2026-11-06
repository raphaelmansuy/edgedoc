/* EdgeDoc Display viewer.
 *
 * A paged document viewer with zoom, continuous scroll, text selection, search, and print. It
 * paints the display list that the WASM module produces, in the manner of pdf.js.
 *
 * The viewer owns the container, the pages, and the scroll position. It does not lay out.
 */

const ZOOM_STEPS = [0.1, 0.25, 0.5, 0.75, 1, 1.25, 1.5, 2, 3, 4, 6, 8];
const PAGE_GAP = 16;
const H_PAD = 16;

/** The faces that the module carries, in the order that the display list names them. */
const FACE_FAMILIES = ["Carlito", "Carlito", "Carlito", "Carlito", "Arimo", "Arimo", "Arimo", "Arimo", "Tinos", "Tinos", "Tinos", "Tinos"];

/**
 * A paged viewer.
 *
 * `createViewer({ container, wasmUrl })` returns a viewer. `load(bytes)` opens a document and
 * paints page 1, and `layout` completes in the worker or on the main thread.
 */
export class Viewer {
  constructor(options) {
    this.pageEls = [];
    this.pageCount = 0;
    if (!options || !options.container) {
      throw new TypeError("createViewer needs a container element");
    }
    this.container =
      typeof options.container === "string"
        ? document.querySelector(options.container)
        : options.container;
    if (!this.container) {
      throw new TypeError("createViewer: the container selector matched no element");
    }
    this.wasmUrl = options.wasmUrl || new URL("./edgedoc_display_bg.wasm", import.meta.url);
    this.onProgress = options.onProgress || (() => {});
    this.onDiagnostic = options.onDiagnostic || (() => {});
    this.onPageChange = options.onPageChange || (() => {});
    this.theme = options.theme || "light";

    /** The zoom as a scale factor. */
    this.scale = options.zoom === undefined ? 1 : this.scaleOf(options.zoom);
    /** True when the zoom follows the container width. */
    this.fitMode = options.zoom === "page-width" || options.zoom === undefined ? "page-width" : null;

    this.module = null;
    this.viewer = null;
    this.glyphPaths = new Map();
    this.pages = [];
    this.texts = [];
    this.diagnostics = [];
    this.current = 0;
    this.rotation = 0;
    this.docTitle = "";

    this._buildDom();
    this._observeResize();
  }

  /** Load the module once. */
  async init() {
    if (this.module) return;
    const mod = await import("./edgedoc_display.js");
    await mod.default(this.wasmUrl);
    this.module = mod;
    this.viewer = new mod.Viewer();
  }

  /** Load a document from bytes, then lay out and paint. */
  async load(bytes) {
    await this.init();
    const u8 = bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes);
    const t0 = performance.now();
    const meta = JSON.parse(this._call(() => this.viewer.open(u8)));
    this.glyphPaths.clear();
    this.docTitle = meta.title && meta.title.trim() ? meta.title : "Untitled document";
    this.title.textContent = this.docTitle;
    document.title = `${this.docTitle} — EdgeDoc Display`;
    const report = JSON.parse(this._call(() => this.viewer.layout()));
    this.diagnostics = JSON.parse(this.viewer.diagnostics());
    this.pageCount = report.pages;

    // Every page becomes one canvas plus one text layer.
    this.pages = [];
    this.texts = [];
    this.pageEls = [];
    this._clearPages();
    for (let i = 0; i < report.pages; i += 1) {
      const bytesPage = this.viewer.page(i);
      const page = this._decodePage(bytesPage);
      this.pages.push(page);
      this.texts.push(this.viewer.text(i) || "");
      this._appendPageEl(i, page);
    }
    // Fetch every image, so an `image` operation can paint. See DP-P-10.
    await this._loadImages();

    // Fit after a frame, so the container holds its final size.
    await new Promise((resolve) => requestAnimationFrame(resolve));
    this.fit(this.fitMode || "page-width");
    this._paintAll();
    this.current = 0;
    this._updateStatus();
    this.progress.classList.add("is-hidden");
    for (const d of this.diagnostics) this.onDiagnostic(d);
    this.firstPageMs = Math.round(performance.now() - t0);
    this.onProgress(1, 1);
    this._updateMetrics(report);
    return { pages: report.pages, passes: report.passes, elapsed: report.elapsed_ms };
  }

  /** Run a module call and turn an error object into a thrown Error. */
  _call(fn) {
    try {
      return fn();
    } catch (err) {
      if (err && err.code) {
        const e = new Error(err.message || "The document did not load.");
        e.code = err.code;
        throw e;
      }
      throw err;
    }
  }

  /**
   * Decode a page display list. The format is `EDDL` followed by a header, a body of
   * operations, and the tables. This reader is the one place that knows the byte layout.
   */
  _decodePage(bytes) {
    const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    let p = 0;
    const magic = String.fromCharCode(...bytes.subarray(0, 4));
    if (magic !== "EDDL") throw new Error("The page buffer is not a display list");
    p = 4;
    const version = view.getUint16(p, true);
    p += 2;
    if (version > 1) throw new Error(`The page format is version ${version}`);
    p += 2; // flags
    const index = view.getUint32(p, true);
    p += 4;
    const bodyLen = view.getUint32(p, true);
    p += 4;
    const w = view.getInt32(p, true) / 100;
    p += 4;
    const h = view.getInt32(p, true) / 100;
    p += 4;
    const tailLen = view.getUint32(p, true);
    p += 4;

    const decoder = new TextDecoder();
    const readCoord = () => {
      const v = view.getInt32(p, true) / 100;
      p += 4;
      return v;
    };
    const readRect = () => {
      const x = readCoord();
      const y = readCoord();
      const ww = readCoord();
      const hh = readCoord();
      return { x, y, w: ww, h: hh };
    };
    const readRgba = () => {
      const c = [bytes[p], bytes[p + 1], bytes[p + 2], bytes[p + 3]];
      p += 4;
      return `rgba(${c[0]},${c[1]},${c[2]},${(c[3] / 255).toFixed(3)})`;
    };
    const readF32 = () => {
      const v = view.getFloat32(p, true);
      p += 4;
      return v;
    };
    const readStr = () => {
      const n = view.getUint32(p, true);
      p += 4;
      const s = decoder.decode(bytes.subarray(p, p + n));
      p += n;
      return s;
    };
    const readPath = () => {
      const n = view.getUint32(p, true);
      p += 4;
      const segs = [];
      for (let i = 0; i < n; i += 1) {
        const kind = bytes[p];
        p += 1;
        if (kind === 0 || kind === 1) {
          segs.push([kind, readCoord(), readCoord()]);
        } else if (kind === 2) {
          segs.push([2, readCoord(), readCoord(), readCoord(), readCoord()]);
        } else if (kind === 3) {
          segs.push([3, readCoord(), readCoord(), readCoord(), readCoord(), readCoord(), readCoord()]);
        } else {
          segs.push([4]);
        }
      }
      return segs;
    };

    const ops = [];
    const bodyEnd = p + bodyLen;
    while (p < bodyEnd) {
      const tag = bytes[p];
      p += 1;
      switch (tag) {
        case 0x01: {
          const bw = readCoord();
          const bh = readCoord();
          const background = readRgba();
          ops.push({ op: "begin", w: bw, h: bh, background });
          break;
        }
        case 0x02:
          ops.push({ op: "end" });
          break;
        case 0x03:
          ops.push({ op: "clip", rect: readRect() });
          break;
        case 0x04:
          ops.push({ op: "unclip" });
          break;
        case 0x05: {
          const a = [readF32(), readF32(), readF32(), readF32(), readCoord(), readCoord()];
          ops.push({ op: "transform", a });
          break;
        }
        case 0x06:
          ops.push({ op: "untransform" });
          break;
        case 0x07: {
          const rect = readRect();
          const kind = bytes[p];
          p += 1;
          if (kind === 0) ops.push({ op: "fill", rect, color: readRgba() });
          else {
            const from = [readCoord(), readCoord()];
            const to = [readCoord(), readCoord()];
            const n = view.getUint16(p, true);
            p += 2;
            const stops = [];
            for (let i = 0; i < n; i += 1) {
              const o = readF32();
              stops.push([o, readRgba()]);
            }
            ops.push({ op: "fillGradient", rect, from, to, stops });
          }
          break;
        }
        case 0x08: {
          const rect = readRect();
          const color = readRgba();
          const style = readStyle(readF32, view, bytes, () => p, (v) => { p = v; });
          p = style.p;
          ops.push({ op: "strokeRect", rect, color, style: style.value });
          break;
        }
        case 0x09: {
          const segs = readPath();
          const kind = bytes[p];
          p += 1;
          if (kind === 0) ops.push({ op: "fillPath", segs, color: readRgba() });
          else {
            const from = [readCoord(), readCoord()];
            const to = [readCoord(), readCoord()];
            const n = view.getUint16(p, true);
            p += 2;
            for (let i = 0; i < n; i += 1) {
              readF32();
              readRgba();
            }
            ops.push({ op: "fillPath", segs, color: readRgba(), gradient: { from, to } });
          }
          p += 1; // fill rule
          break;
        }
        case 0x0a: {
          const segs = readPath();
          const color = readRgba();
          const style = readStyle(readF32, view, bytes, () => p, (v) => { p = v; });
          p = style.p;
          ops.push({ op: "strokePath", segs, color, style: style.value });
          break;
        }
        case 0x0b: {
          const face = view.getUint32(p, true);
          p += 4;
          const size = readF32();
          const x = readCoord();
          const y = readCoord();
          const n = view.getUint32(p, true);
          p += 4;
          const glyphs = [];
          for (let i = 0; i < n; i += 1) {
            glyphs.push(view.getUint16(p, true));
            p += 2;
          }
          const advances = [];
          for (let i = 0; i < n; i += 1) advances.push(readF32());
          const offsets = [];
          for (let i = 0; i < n; i += 1) offsets.push([readF32(), readF32()]);
          const hasClusters = bytes[p];
          p += 1;
          const clusters = [];
          if (hasClusters) {
            for (let i = 0; i < n; i += 1) {
              clusters.push(view.getUint32(p, true));
              p += 4;
            }
          }
          const color = readRgba();
          const level = bytes[p];
          p += 1;
          const flags = bytes[p];
          p += 1;
          const xScale = readF32();
          const text = readStr();
          const hasLink = bytes[p];
          p += 1;
          let link = null;
          if (hasLink) {
            link = view.getUint32(p, true);
            p += 4;
          }
          const hidden = bytes[p] === 1;
          p += 1;
          const rev = bytes[p];
          p += 1;
          ops.push({
            op: "glyphs",
            face,
            size,
            x,
            y,
            glyphs,
            advances,
            offsets,
            clusters,
            color,
            level,
            xScale,
            text,
            link,
            hidden,
            revision: rev === 255 ? null : rev,
            synthBold: (flags & 1) !== 0,
            synthItalic: (flags & 2) !== 0,
          });
          break;
        }
        case 0x0c: {
          const resource = view.getUint32(p, true);
          p += 4;
          const rect = readRect();
          const src = [readF32(), readF32(), readF32(), readF32()];
          const opacity = readF32();
          const interpolate = bytes[p] === 1;
          p += 1;
          ops.push({ op: "image", resource, rect, src, opacity, interpolate });
          break;
        }
        case 0x0d: {
          const rect = readRect();
          const target = view.getUint32(p, true);
          p += 4;
          ops.push({ op: "link", rect, target });
          break;
        }
        case 0x0e: {
          const name = readStr();
          const x = readCoord();
          const y = readCoord();
          ops.push({ op: "dest", name, x, y });
          break;
        }
        case 0x13: {
          const rect = readRect();
          const kind = readStr();
          const label = readStr();
          ops.push({ op: "placeholder", rect, kind, label });
          break;
        }
        case 0x14: {
          const rect = readRect();
          const label = readStr();
          ops.push({ op: "debug", rect, label });
          break;
        }
        default:
          // An unknown operation cannot be skipped safely, so the page stops here.
          p = bodyEnd;
      }
    }

    // The tables: targets, the text index, and the destinations.
    p = bodyEnd + 0;
    const targetCount = view.getUint16(p, true);
    p += 2;
    const targets = [];
    for (let i = 0; i < targetCount; i += 1) targets.push(readStr());
    const textCount = view.getUint32(p, true);
    p += 4;
    const textIndex = [];
    for (let i = 0; i < textCount; i += 1) {
      const run = view.getUint32(p, true);
      p += 4;
      const charStart = view.getUint32(p, true);
      p += 4;
      const charEnd = view.getUint32(p, true);
      p += 4;
      const rect = readRect();
      const baseline = readF32();
      const level = bytes[p];
      p += 1;
      const hidden = bytes[p] === 1;
      p += 1;
      textIndex.push({ run, charStart, charEnd, rect, baseline, level, hidden });
    }
    void tailLen;
    return { index, w, h, ops, targets, textIndex };
  }

  /**
   * Decode every image that the module holds, into an `ImageBitmap` or an `Image`. The paint path
   * draws by resource id, so a page with 100 copies of a logo decodes one bitmap.
   */
  async _loadImages() {
    this.images = new Map();
    let index = [];
    try {
      index = JSON.parse(this.viewer.imageIndex ? this.viewer.imageIndex() : "[]");
    } catch {
      index = [];
    }
    await Promise.all(
      index.map(async (entry) => {
        try {
          const bytes = this.viewer.image(entry.index);
          if (!bytes) return;
          const blob = new Blob([bytes], { type: entry.mime });
          if (typeof createImageBitmap === "function") {
            this.images.set(entry.id, await createImageBitmap(blob));
          } else {
            const url = URL.createObjectURL(blob);
            const img = new Image();
            await new Promise((resolve, reject) => {
              img.onload = resolve;
              img.onerror = reject;
              img.src = url;
            });
            this.images.set(entry.id, img);
          }
        } catch {
          // A decode that fails paints the marked box, which the operations already hold.
        }
      }),
    );
  }

  /* ---------------------------------------------------------------- DOM */

  _buildDom() {
    this.container.classList.add("edv-root");
    this.container.innerHTML = `
      <div class="edv-bar">
        <button class="edv-btn" data-act="prev" title="Previous page (P)">&#8249;</button>
        <input class="edv-page-input" type="text" inputmode="numeric" aria-label="Page number" value="1">
        <span class="edv-count">/ 1</span>
        <button class="edv-btn" data-act="next" title="Next page (N)">&#8250;</button>
        <span class="edv-title"></span>
        <span class="edv-spacer"></span>
        <button class="edv-btn" data-act="zoom-out" title="Zoom out (Ctrl -)">&minus;</button>
        <span class="edv-zoom">100%</span>
        <button class="edv-btn" data-act="zoom-in" title="Zoom in (Ctrl +)">+</button>
        <button class="edv-btn edv-fit" data-act="fit-width" title="Fit width (Ctrl 1)">Fit width</button>
        <button class="edv-btn edv-fit" data-act="fit-page" title="Fit page (Ctrl 2)">Fit page</button>
        <button class="edv-btn" data-act="print" title="Print (Ctrl P)">Print</button>
      </div>
      <div class="edv-progress"><div class="edv-progress-bar"></div></div>
      <div class="edv-viewport" tabindex="0">
        <div class="edv-stack"></div>
      </div>
      <div class="edv-status">
        <span class="edv-status-page">Page 1</span>
        <span class="edv-spacer"></span>
        <span class="edv-diag-count"></span>
      </div>`;
    this.title = this.container.querySelector(".edv-title");
    this.pageInput = this.container.querySelector(".edv-page-input");
    this.count = this.container.querySelector(".edv-count");
    this.zoomLabel = this.container.querySelector(".edv-zoom");
    this.viewport = this.container.querySelector(".edv-viewport");
    this.stack = this.container.querySelector(".edv-stack");
    this.progress = this.container.querySelector(".edv-progress");
    this.progressBar = this.container.querySelector(".edv-progress-bar");
    this.statusPage = this.container.querySelector(".edv-status-page");
    this.diagCount = this.container.querySelector(".edv-diag-count");

    this.container.addEventListener("click", (event) => {
      const target = event.target.closest("[data-act]");
      if (!target) return;
      const act = target.dataset.act;
      if (act === "prev") this.goToPage(this.current - 1);
      else if (act === "next") this.goToPage(this.current + 1);
      else if (act === "zoom-in") this.zoomBy(1);
      else if (act === "zoom-out") this.zoomBy(-1);
      else if (act === "fit-width") this.fit("width");
      else if (act === "fit-page") this.fit("page");
      else if (act === "print") this.print();
    });

    this.pageInput.addEventListener("change", () => {
      const n = Number.parseInt(this.pageInput.value, 10);
      if (Number.isFinite(n)) this.goToPage(n - 1);
    });

    this.viewport.addEventListener(
      "scroll",
      () => {
        // One callback per frame, which is what a smooth scroll needs. See DP-E-08.
        if (this._scrollQueued) return;
        this._scrollQueued = true;
        requestAnimationFrame(() => {
          this._scrollQueued = false;
          this._syncCurrentPage();
          // A page that entered the window paints now. See DP-E-05.
          this._paintAll();
        });
      },
      { passive: true },
    );

    // Ctrl and the wheel zoom, and a plain wheel scrolls.
    this.viewport.addEventListener(
      "wheel",
      (event) => {
        if (!event.ctrlKey && !event.metaKey) return;
        event.preventDefault();
        this.zoomBy(event.deltaY < 0 ? 1 : -1, event.clientX, event.clientY);
      },
      { passive: false },
    );

    document.addEventListener("keydown", (event) => this._onKey(event));
  }

  _onKey(event) {
    const inField = event.target instanceof HTMLInputElement;
    if (event.key === "Escape" && inField) {
      this.pageInput.blur();
      return;
    }
    if (inField) return;
    if (event.key === "n" || event.key === "N") this.goToPage(this.current + 1);
    else if (event.key === "p" || event.key === "P") this.goToPage(this.current - 1);
    else if (event.key === "PageDown") this.viewport.scrollBy({ top: this.viewport.clientHeight * 0.9 });
    else if (event.key === "PageUp") this.viewport.scrollBy({ top: -this.viewport.clientHeight * 0.9 });
    else if (event.key === "Home") this.goToPage(0);
    else if (event.key === "End") this.goToPage(this.pageCount - 1);
    else if ((event.ctrlKey || event.metaKey) && (event.key === "+" || event.key === "=")) {
      event.preventDefault();
      this.zoomBy(1);
    } else if ((event.ctrlKey || event.metaKey) && event.key === "-") {
      event.preventDefault();
      this.zoomBy(-1);
    } else if ((event.ctrlKey || event.metaKey) && event.key === "1") {
      event.preventDefault();
      this.fit("width");
    } else if ((event.ctrlKey || event.metaKey) && event.key === "2") {
      event.preventDefault();
      this.fit("page");
    } else if ((event.ctrlKey || event.metaKey) && event.key === "p") {
      event.preventDefault();
      this.print();
    }
  }

  _observeResize() {
    if (typeof ResizeObserver === "undefined") return;
    // A fit zoom re-evaluates when the container changes. See DP-V-15.
    this._resizeObserver = new ResizeObserver(() => {
      // A resize before the document loads has nothing to fit.
      if (this.fitMode && this.pages.length) this.fit(this.fitMode);
    });
    this._resizeObserver.observe(this.viewport);
  }

  _clearPages() {
    this.stack.textContent = "";
  }

  /** One page element: a canvas for the paint and a layer for the text. */
  _appendPageEl(i, page) {
    const wrap = document.createElement("div");
    wrap.className = "edv-page";
    wrap.dataset.page = String(i);
    wrap.style.width = `${page.w * this.scale}px`;
    wrap.style.height = `${page.h * this.scale}px`;
    const canvas = document.createElement("canvas");
    canvas.className = "edv-canvas";
    const layer = document.createElement("div");
    layer.className = "edv-text";
    wrap.append(canvas, layer);
    this.stack.append(wrap);
    this.pageEls.push({ wrap, canvas, layer });
  }

  /** The scale for a zoom name. See DP-V-12. */
  scaleOf(value) {
    if (typeof value === "number") return this._clampScale(value);
    if (value === "page-actual") return 1;
    if (value === "auto" || value === "page-width" || value === "page-fit") return this.scale || 1;
    if (typeof value === "string" && value.endsWith("%")) {
      return this._clampScale(Number.parseFloat(value) / 100);
    }
    return 1;
  }

  _clampScale(v) {
    if (!Number.isFinite(v)) return 1;
    return Math.min(8, Math.max(0.1, v));
  }

  /** The scale that fits the page width. */
  fitScale(mode) {
    if (!this.pages.length) return this.scale;
    const page = this.pages[0];
    // A container that is not laid out yet reports 0, which would give a scale of 0.1. Fall
    // back to a readable size until the observer reports a real width. See DP-V-15.
    const width = this.viewport.clientWidth;
    const height = this.viewport.clientHeight;
    if (width < 120 || height < 120) return 1;
    const byWidth = (width - H_PAD * 2) / page.w;
    const byHeight = (height - PAGE_GAP * 2) / page.h;
    const s = mode === "page" ? Math.min(byWidth, byHeight) : byWidth;
    return this._clampScale(s);
  }

  /** Set a fit mode and repaint. See DP-V-15. */
  fit(mode = "width") {
    this.fitMode = mode;
    this.scale = this.fitScale(mode);
    this._applyScale();
  }

  /** The current page count. */
  get pageCountValue() {
    return this.pageCount || 0;
  }

  zoomBy(direction, anchorX, anchorY) {
    const current = this.scale;
    let next = current;
    if (direction > 0) {
      next = ZOOM_STEPS.find((s) => s > current + 0.001) || 8;
    } else {
      next = [...ZOOM_STEPS].reverse().find((s) => s < current - 0.001) || 0.1;
    }
    this.setZoom(next, anchorX, anchorY);
  }

  /**
   * Set the zoom and keep an anchor point fixed. The page under the pointer stays under the
   * pointer, which is the pdf.js behavior. See DP-V-13.
   */
  setZoom(scale, anchorX, anchorY) {
    const next = this._clampScale(scale);
    if (Math.abs(next - this.scale) < 0.0005) return;
    const rect = this.viewport.getBoundingClientRect();
    const ax = anchorX === undefined ? rect.width / 2 : anchorX - rect.left;
    const ay = anchorY === undefined ? rect.height / 2 : anchorY - rect.top;
    const before = (this.viewport.scrollTop + ay) / this.scale;
    this.fitMode = null;
    this.scale = next;
    this._applyScale();
    this.viewport.scrollTop = before * this.scale - ay;
    this._syncCurrentPage();
  }

  /** Resize every page element and repaint at the new scale. */
  _applyScale() {
    if (!this.pageEls || !this.pageEls.length) {
      this.zoomLabel.textContent = `${Math.round(this.scale * 100)}%`;
      return;
    }
    for (let i = 0; i < this.pageEls.length; i += 1) {
      const page = this.pages[i];
      const el = this.pageEls[i];
      if (!page || !el) continue;
      el.wrap.style.width = `${page.w * this.scale}px`;
      el.wrap.style.height = `${page.h * this.scale}px`;
      // A scale change invalidates the raster, so the next paint bakes a new tile.
      el.painted = false;
    }
    this._paintAll();
    this.zoomLabel.textContent = `${Math.round(this.scale * 100)}%`;
  }

  /* -------------------------------------------------------------- paint */

  /**
   * Paint the pages near the viewport, and nothing else. A page that leaves the window keeps its
   * canvas, so a return paints with no layout and no raster work. See `DP-E-05` and `DP-E-07`.
   */
  _paintAll() {
    const range = this._visibleRange();
    for (let i = range.from; i <= range.to; i += 1) {
      this._paintPage(i, this._deviceRatio());
    }
  }

  /**
   * The page indices to keep painted: the visible pages plus one on each side.
   *
   * The offset of a page is the sum of the page heights before it, plus the gap. A scroll offset
   * in points divided by the height of a page gives the index, which needs no layout read and
   * works before the browser lays the stack out.
   */
  _visibleRange() {
    const n = this.pageEls.length;
    if (!n) return { from: 0, to: -1 };
    const gap = PAGE_GAP * this.scale;
    const top = this.viewport.scrollTop;
    const bottom = top + this.viewport.clientHeight;
    let y = 0;
    let from = 0;
    let to = 0;
    for (let i = 0; i < n; i += 1) {
      const page = this.pages[i];
      if (!page) continue;
      const h = page.h * this.scale;
      const pageTop = y;
      const pageBottom = y + h;
      if (pageBottom >= top && from === 0 && i > 0) from = i;
      if (pageTop <= bottom) to = i;
      y = pageBottom + gap;
    }
    return { from: Math.max(0, from - 1), to: Math.min(n - 1, to + 1) };
  }

  /** Paint one page only if it holds a canvas with a size. */
  _paintVisible() {
    this._paintAll();
  }

  /** The device pixel ratio, capped at 3. See DP-G-24. */
  _deviceRatio() {
    const raw = window.devicePixelRatio || 1;
    return Math.min(3, Math.max(1, Math.round(raw * 2) / 2));
  }

  _paintPage(i, dpr) {
    const page = this.pages[i];
    const el = this.pageEls[i];
    if (!page || !el) return;
    const scale = this.scale * dpr;
    const w = Math.max(1, Math.round(page.w * scale));
    const h = Math.max(1, Math.round(page.h * scale));
    const canvas = el.canvas;
    if (canvas.width === w && canvas.height === h && el.painted && el.paintedScale === this.scale) {
      return;
    }
    el.paintedScale = this.scale;
    el.painted = true;
    if (canvas.width !== w || canvas.height !== h) {
      canvas.width = w;
      canvas.height = h;
    }
    canvas.style.width = `${page.w * this.scale}px`;
    canvas.style.height = `${page.h * this.scale}px`;
    const ctx = canvas.getContext("2d");
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.clearRect(0, 0, w, h);
    ctx.setTransform(scale, 0, 0, scale, 0, 0);
    ctx.textBaseline = "alphabetic";
    this._paintOps(ctx, page);
    this._buildTextLayer(i, el.layer, page);
  }

  _paintOps(ctx, page) {
    ctx.save();
    for (const op of page.ops) {
      switch (op.op) {
        case "begin":
          ctx.fillStyle = op.background;
          ctx.fillRect(0, 0, op.w, op.h);
          break;
        case "fill":
          ctx.fillStyle = op.color;
          ctx.fillRect(op.rect.x, op.rect.y, op.rect.w, op.rect.h);
          break;
        case "fillGradient": {
          const g = ctx.createLinearGradient(op.from[0], op.from[1], op.to[0], op.to[1]);
          for (const [offset, color] of op.stops) g.addColorStop(Math.min(1, Math.max(0, offset)), color);
          ctx.fillStyle = g;
          ctx.fillRect(op.rect.x, op.rect.y, op.rect.w, op.rect.h);
          break;
        }
        case "strokeRect":
          ctx.strokeStyle = op.color;
          ctx.lineWidth = op.style.width;
          ctx.setLineDash(op.style.dash || []);
          ctx.strokeRect(op.rect.x, op.rect.y, op.rect.w, op.rect.h);
          ctx.setLineDash([]);
          break;
        case "fillPath":
          ctx.fillStyle = op.color;
          this._path(ctx, op.segs);
          ctx.fill();
          break;
        case "strokePath":
          ctx.strokeStyle = op.color;
          ctx.lineWidth = op.style.width;
          ctx.lineCap = op.style.cap || "butt";
          ctx.setLineDash(op.style.dash || []);
          this._path(ctx, op.segs);
          ctx.stroke();
          ctx.setLineDash([]);
          break;
        case "glyphs":
          this._glyphs(ctx, op);
          break;
        case "clip": {
          ctx.save();
          ctx.beginPath();
          ctx.rect(op.rect.x, op.rect.y, op.rect.w, op.rect.h);
          ctx.clip();
          break;
        }
        case "unclip":
          ctx.restore();
          break;
        case "image": {
          const img = this.images ? this.images.get(op.resource) : null;
          if (img) {
            ctx.save();
            ctx.globalAlpha = op.opacity === undefined ? 1 : op.opacity;
            const src = op.src || [0, 0, 1, 1];
            const sw = (img.width || 0) * src[2];
            const sh = (img.height || 0) * src[3];
            if (sw > 0 && sh > 0) {
              ctx.drawImage(
                img,
                (img.width || 0) * src[0],
                (img.height || 0) * src[1],
                sw,
                sh,
                op.rect.x,
                op.rect.y,
                op.rect.w,
                op.rect.h,
              );
            }
            ctx.restore();
          }
          break;
        }
        case "placeholder": {
          // The operations already painted the box; the label goes in the middle.
          ctx.fillStyle = "#5b6472";
          ctx.font = `8px ${FACE_FAMILIES[0]}, sans-serif`;
          ctx.textAlign = "center";
          const label = op.kind ? `${op.kind}: ${shortPath(op.label)}` : shortPath(op.label);
          ctx.fillText(label, op.rect.x + op.rect.w / 2, op.rect.y + op.rect.h / 2 + 3);
          ctx.textAlign = "start";
          break;
        }
        default:
          break;
      }
    }
    ctx.restore();
  }

  _path(ctx, segs) {
    ctx.beginPath();
    for (const seg of segs) {
      if (seg[0] === 0) ctx.moveTo(seg[1], seg[2]);
      else if (seg[0] === 1) ctx.lineTo(seg[1], seg[2]);
      else if (seg[0] === 2) ctx.quadraticCurveTo(seg[1], seg[2], seg[3], seg[4]);
      else if (seg[0] === 3) ctx.bezierCurveTo(seg[1], seg[2], seg[3], seg[4], seg[5], seg[6]);
      else ctx.closePath();
    }
  }

  /** Paint the exact shaped glyph IDs and advances; Canvas never reshapes the source text. */
  _glyphs(ctx, op) {
    if (op.hidden) return;
    ctx.fillStyle = op.color;
    ctx.strokeStyle = op.color;
    const embolden = op.synthBold ? Math.max(0.25, op.size * 0.025) : 0;
    let x = op.x;
    for (let i = 0; i < op.glyphs.length; i += 1) {
      const key = `${op.face}:${op.glyphs[i]}`;
      let outline = this.glyphPaths.get(key);
      if (!outline) {
        const encoded = this.viewer.glyphOutline(op.face, op.glyphs[i]);
        if (!encoded) throw new Error(`Missing outline for glyph ${key}`);
        const [upem, data] = JSON.parse(encoded);
        outline = { upem, path: new Path2D(data) };
        this.glyphPaths.set(key, outline);
      }
      const [dx, dy] = op.offsets[i] || [0, 0];
      const scale = op.size / outline.upem;
      ctx.save();
      ctx.translate(x + dx, op.y + dy);
      if (op.synthItalic) ctx.transform(1, 0, -Math.tan((12 * Math.PI) / 180), 1, 0, 0);
      ctx.scale(scale * (op.xScale || 1), -scale);
      ctx.fill(outline.path);
      if (embolden) {
        ctx.lineWidth = embolden / scale;
        ctx.stroke(outline.path);
      }
      ctx.restore();
      x += op.advances[i] || 0;
    }
  }

  /**
   * Build the transparent text layer, so a reader can select and a screen reader can read. Each
   * run becomes one span at its painted position. See DP-G-33.
   */
  _buildTextLayer(i, layer, page) {
    layer.textContent = "";
    layer.style.width = `${page.w * this.scale}px`;
    layer.style.height = `${page.h * this.scale}px`;
    // Group the runs of one line into a paragraph, so a selection copies a sentence.
    const byLine = new Map();
    for (const op of page.ops) {
      if (op.op !== "glyphs" || op.hidden || !op.text.trim()) continue;
      const key = `${Math.round(op.y * 2)}`;
      if (!byLine.has(key)) byLine.set(key, []);
      byLine.get(key).push(op);
    }
    for (const runs of byLine.values()) {
      const para = document.createElement("p");
      para.className = "edv-line";
      const first = runs[0];
      para.style.top = `${(first.y - first.size) * this.scale}px`;
      para.style.height = `${first.size * 1.4 * this.scale}px`;
      for (const op of runs) {
        const span = document.createElement("span");
        span.textContent = op.text;
        span.style.left = `${op.x * this.scale}px`;
        span.style.top = "0px";
        span.style.fontSize = `${op.size * this.scale}px`;
        span.style.fontFamily = `${FACE_FAMILIES[op.face] || "sans-serif"}, sans-serif`;
        if (op.level % 2 === 1) span.dir = "rtl";
        if (op.revision === 2) span.dataset.revision = "delete";
        const advance = op.advances.reduce((a, b) => a + b, 0);
        const natural = measureText(op.text, span.style.fontFamily, op.size * this.scale);
        if (natural > 0.5) span.style.transform = `scaleX(${(advance * this.scale) / natural})`;
        para.append(span);
      }
      layer.append(para);
    }
  }

  /* ------------------------------------------------------------ paging */

  goToPage(n) {
    if (!this.pages.length) return;
    const index = Math.min(this.pages.length - 1, Math.max(0, n));
    const el = this.pageEls[index];
    if (!el) return;
    this.viewport.scrollTop = el.wrap.offsetTop - PAGE_GAP;
    this.current = index;
    this._updateStatus();
    this.onPageChange(index, this.pageCount);
  }

  _syncCurrentPage() {
    const top = this.viewport.scrollTop + this.viewport.clientHeight / 2;
    let found = 0;
    for (let i = 0; i < this.pageEls.length; i += 1) {
      const el = this.pageEls[i];
      if (!el) continue;
      if (el.wrap.offsetTop <= top) found = i;
      else break;
    }
    if (found !== this.current) {
      this.current = found;
      this._updateStatus();
      this.onPageChange(found, this.pageCount);
    }
  }

  /**
   * Write the run's own numbers into the panel. A caller that uses `load` directly sees the same
   * values as the demo page, because the viewer owns the measurement. See `DP-U-26`.
   */
  _updateMetrics(report) {
    const set = (id, text) => {
      const el = document.getElementById(id);
      if (el) el.textContent = text;
    };
    set("m-pages", String(report.pages ?? this.pageCount));
    set("m-passes", String(report.passes ?? "-"));
    set("m-ms", `${Math.round(this.firstPageMs)} ms`);
    const list = document.getElementById("m-diag");
    if (list) {
      const items = this.diagnostics || [];
      list.innerHTML = items.length
        ? items
            .map(
              (d) =>
                `<div class="diag"><span class="sev ${d.severity}"></span><div><div class="code">${d.code}</div><div>${escapeHtml(
                  d.summary || d.detail || "",
                )}</div></div></div>`,
            )
            .join("")
        : '<p class="hint">No diagnostics. Every part of the document paints.</p>';
    }
  }

  _updateStatus() {
    const total = this.pageCount || 1;
    this.count.textContent = `/ ${total}`;
    if (document.activeElement !== this.pageInput) {
      this.pageInput.value = String(this.current + 1);
    }
    this.statusPage.textContent = `Page ${this.current + 1} of ${total}`;
    const warnings = this.diagnostics.filter((d) => d.severity === "warning").length;
    const errors = this.diagnostics.filter((d) => d.severity === "error").length;
    if (errors > 0) {
      this.diagCount.textContent = `${errors} error${errors === 1 ? "" : "s"}`;
      this.diagCount.className = "edv-diag-count is-error";
    } else if (warnings > 0) {
      this.diagCount.textContent = `${warnings} warning${warnings === 1 ? "" : "s"}`;
      this.diagCount.className = "edv-diag-count is-warning";
    } else {
      this.diagCount.textContent = "";
      this.diagCount.className = "edv-diag-count";
    }
  }

  /** Print through a frame that holds the painted pages as images. See DP-B-41. */
  print() {
    const win = window.open("", "_blank");
    if (!win) return;
    const parts = [];
    parts.push("<!doctype html><title>" + escapeHtml(this.docTitle) + "</title>");
    parts.push(
      "<style>@page{size:" +
        this.pages.map((p) => `${p.w}pt ${p.h}pt`).slice(0, 1) +
        ";margin:0}body{margin:0}img{display:block;width:100%}</style>",
    );
    for (const el of this.pageEls) {
      const src = el.canvas.toDataURL("image/png");
      parts.push(`<img src="${src}" style="page-break-after:always">`);
    }
    win.document.write(parts.join(""));
    win.document.close();
    win.focus();
    setTimeout(() => win.print(), 250);
  }

  /** Release every page. See DP-V-11. */
  destroy() {
    if (this._resizeObserver) this._resizeObserver.disconnect();
    if (this.viewer && this.viewer.clear) this.viewer.clear();
    this.container.textContent = "";
    this.container.classList.remove("edv-root");
    this.pages = [];
    this.pageEls = [];
    this.glyphPaths.clear();
  }
}

function measureText(text, family, size) {
  if (!Viewer._measureCtx) {
    const c = document.createElement("canvas");
    Viewer._measureCtx = c.getContext("2d");
  }
  const ctx = Viewer._measureCtx;
  ctx.font = `${size}px ${family}`;
  return ctx.measureText(text).width;
}

function shortPath(path) {
  if (!path) return "";
  const m = /body\[(\d+)\]/.exec(String(path));
  return m ? `body[${m[1]}]` : String(path);
}

function escapeHtml(s) {
  return String(s).replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);
}

/** Create a viewer. This is the one call that the demo and a host page use. See DP-V-02. */
export function createViewer(options) {
  return new Viewer(options);
}

/** The style helper that a header paints with. A caller does not use it. */
function readStyle(readF32, view, bytes, getPos, setPos) {
  let p = getPos();
  const width = (bytes, view2) => 0;
  void width;
  void view2;
  void readF32;
  const w = view.getFloat32(p, true);
  p += 4;
  const cap = bytes[p];
  p += 1;
  const join = bytes[p];
  p += 1;
  const dashLen = Math.min(4, bytes[p]);
  p += 1;
  const dash = [];
  for (let i = 0; i < dashLen; i += 1) {
    dash.push(view.getFloat32(p, true));
    p += 4;
  }
  setPos(p);
  return { p, value: { width: w, cap: ["butt", "round", "square"][cap] || "butt", join, dash } };
}
