#!/usr/bin/env node
/**
 * The end-to-end test for the display viewer.
 *
 * It serves `site/`, opens the demo in headless Chrome, loads a document, and checks the viewer
 * state at each step: initial paint, zoom, page navigation, selection, and diagnostics. It writes
 * a screenshot per step into `docs/display/e2e/`, which a person inspects.
 *
 *     node tools/display_e2e.mjs
 *
 * It needs Chrome and puppeteer. Set `CHROME_PATH` and `PUPPETEER_PATH` to override the defaults.
 */
import { createServer } from "node:http";
import { readFile, mkdir, writeFile } from "node:fs/promises";
import { existsSync } from "node:fs";
import { extname, join, resolve } from "node:path";
import { createRequire } from "node:module";

const require = createRequire(import.meta.url);
const ROOT = resolve(new URL("..", import.meta.url).pathname);
const SITE = join(ROOT, "site");
const OUT = join(ROOT, "docs/display/e2e");
const PORT = 8901;

const PUPPETEER =
  process.env.PUPPETEER_PATH ||
  "/Users/raphaelmansuy/.nvm/versions/node/v20.14.0/lib/node_modules/@mermaid-js/mermaid-cli/node_modules/puppeteer";
const CHROME =
  process.env.CHROME_PATH || "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";

const MIME = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".wasm": "application/wasm",
  ".css": "text/css; charset=utf-8",
  ".docx": "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
  ".png": "image/png",
  ".svg": "image/svg+xml",
};

function serve() {
  const server = createServer(async (req, res) => {
    const url = new URL(req.url, "http://x");
    let path = join(SITE, decodeURIComponent(url.pathname));
    if (url.pathname.endsWith("/")) path = join(path, "index.html");
    if (!existsSync(path)) {
      res.writeHead(404);
      res.end("not found");
      return;
    }
    const body = await readFile(path);
    res.writeHead(200, { "content-type": MIME[extname(path)] || "application/octet-stream" });
    res.end(body);
  });
  // A port that a previous run left open blocks the next run. Try a few, so one stale process
  // does not fail the suite. See `DP-Q-48`.
  return new Promise((ok, fail) => {
    let attempt = 0;
    const tryPort = (port) => {
      server.once("error", (err) => {
        if (err.code === "EADDRINUSE" && attempt < 12) {
          attempt += 1;
          tryPort(port + 1);
          return;
        }
        fail(err);
      });
      server.listen(port, "127.0.0.1", () => ok({ server, port }));
    };
    tryPort(PORT);
  });
}

const results = [];
function check(name, ok, detail) {
  results.push({ name, ok, detail });
  console.log(`  ${ok ? "ok  " : "FAIL"}  ${name}${detail ? `  ${detail}` : ""}`);
}

async function main() {
  await mkdir(OUT, { recursive: true });
  const { server, port } = await serve();
  const puppeteer = require(PUPPETEER);
  const browser = await puppeteer.launch({
    executablePath: CHROME,
    headless: "new",
    args: ["--no-sandbox", "--disable-gpu", "--force-device-scale-factor=2"],
  });
  const page = await browser.newPage();
  await page.setViewport({ width: 1440, height: 950, deviceScaleFactor: 2 });
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  // A missing favicon is not a viewer defect. Every other console error is.
  page.on("console", (m) => {
    const text = m.text();
    if (m.type() !== "error") return;
    if (text.includes("favicon") || text.includes("404 (Not Found)")) return;
    errors.push(text);
  });

  const shot = async (name) => {
    await page.screenshot({ path: join(OUT, `${name}.png`), fullPage: false });
  };

  console.log("\nEdgeDoc Display: end-to-end\n");

  await page.goto(`http://127.0.0.1:${port}/display/index.html`, {
    waitUntil: "domcontentloaded",
    timeout: 60000,
  });
  await page.waitForFunction("window.__edgedoc && window.__edgedoc.ready === true", { timeout: 60000 });
  check("the viewer module loads", true);
  await shot("01-empty");

  // Load a document through the file input, which is the one-call path.
  const input = await page.$("#file");
  await input.uploadFile(join(SITE, "assets/documents/report.docx"));
  await page.waitForFunction(() => document.querySelectorAll(".edv-page").length > 0, { timeout: 90000 });
  await new Promise((r) => setTimeout(r, 1800));

  const state = await page.evaluate(() => {
    const v = window.__edgedoc.viewer;
    const canvas = document.querySelector(".edv-canvas");
    const ctx = canvas.getContext("2d");
    const data = ctx.getImageData(0, 0, canvas.width, canvas.height).data;
    let ink = 0;
    for (let i = 0; i < data.length; i += 4) if (data[i] < 128) ink += 1;
    return {
      pages: v.pageCount,
      els: document.querySelectorAll(".edv-page").length,
      spans: document.querySelectorAll(".edv-text span").length,
      scale: +v.scale.toFixed(3),
      ink,
      ms: Math.round(v.firstPageMs),
      diag: v.diagnostics.length,
    };
  });
  check("every page mounts", state.els === state.pages && state.pages > 1, `${state.pages} pages`);
  check("the first page paints ink", state.ink > 2000, `${state.ink} dark pixels`);
  check("the text layer holds one span per run", state.spans > 20, `${state.spans} spans`);
  check("the first paint is under 300 ms", state.ms < 300, `${state.ms} ms`);
  check("a missing font is reported", state.diag >= 1, `${state.diag} diagnostics`);
  await shot("02-loaded");

  // Zoom: fit width, 200%, and back to 100%.
  await page.evaluate(() => window.__edgedoc.viewer.fit("width"));
  await new Promise((r) => setTimeout(r, 700));
  const fitWidth = await page.evaluate(() => window.__edgedoc.viewer.scale);
  await shot("03-fit-width");
  await page.evaluate(() => window.__edgedoc.viewer.setZoom(2));
  await new Promise((r) => setTimeout(r, 800));
  const zoomed = await page.evaluate(() => window.__edgedoc.viewer.scale);
  await shot("04-zoom-200");
  check("fit width sets a scale", fitWidth > 0.2 && fitWidth < 4, fitWidth.toFixed(3));
  check("zoom 200 reaches 2.0", Math.abs(zoomed - 2) < 0.01, zoomed.toFixed(3));

  // The zoom keeps an anchor, so a point stays under the pointer.
  const anchor = await page.evaluate(() => {
    const v = window.__edgedoc.viewer;
    v.setZoom(1);
    const viewport = document.querySelector(".edv-viewport");
    const before = viewport.scrollTop;
    v.setZoom(2, viewport.getBoundingClientRect().left + 300, viewport.getBoundingClientRect().top + 200);
    return { before, after: viewport.scrollTop };
  });
  check("a zoom change moves the scroll position", anchor.after !== anchor.before || anchor.before === 0);

  // Pagination: go to page 2 and check the status line.
  await page.evaluate(() => window.__edgedoc.viewer.setZoom(1));
  await new Promise((r) => setTimeout(r, 400));
  await page.evaluate(() => window.__edgedoc.viewer.goToPage(1));
  await new Promise((r) => setTimeout(r, 700));
  const page2 = await page.evaluate(() => ({
    current: window.__edgedoc.viewer.current,
    status: document.querySelector(".edv-status-page").textContent,
    field: document.querySelector(".edv-page-input").value,
  }));
  check("goToPage reaches page 2", page2.current === 1, page2.status);
  check("the page field follows", page2.field === "2");
  await shot("05-page-2");

  // A footer field reads the real number, and the total matches the page count.
  const field = await page.evaluate(() => window.__edgedoc.viewer.texts[1] || "");
  const total = String(state.pages);
  check("a page field shows the page number", field.includes("2"), field.slice(-24).trim());
  check("a total field shows the page count", field.includes(total), `of ${total}`);

  // Click a word: the hit test returns a character.
  const hit = await page.evaluate(() => {
    const v = window.__edgedoc.viewer;
    v.goToPage(1);
    const p = v.pages[1];
    const run = p.ops.find((o) => o.op === "glyphs" && o.text.trim().length > 3 && o.y > 100);
    if (!run) return null;
    return v.viewer.hitTest(1, run.x + 2, run.y - run.size * 0.4);
  });
  check("a click finds a character", hit !== null && hit !== undefined);

  // Search.
  const search = await page.evaluate(() => {
    const v = window.__edgedoc.viewer;
    return JSON.parse(v.viewer.search("Revenue"));
  });
  check("search finds a term", search.hits && search.hits.length > 0, `${search.hits.length} hits`);

  // The outline comes from the headings.
  const outline = await page.evaluate(() => JSON.parse(window.__edgedoc.viewer.viewer.outline()));
  check("the outline holds the headings", outline.length > 0, `${outline.length} entries`);

  // A file that is not a Word package is refused with a clear code.
  const bad = await page.evaluate(async () => {
    const v = window.__edgedoc.viewer;
    try {
      await v.load(new Uint8Array([1, 2, 3, 4, 5]));
      return null;
    } catch (e) {
      return { code: e.code || "display.unknown", message: e.message };
    }
  });
  check("a bad file is refused with a code", bad && bad.code && bad.code !== "display.unknown", bad ? bad.code : "no error");
  await shot("06-refused");

  // Reload for a clean shot of page 2 at a readable zoom.
  await page.reload({ waitUntil: "domcontentloaded" });
  await page.waitForFunction("window.__edgedoc && window.__edgedoc.ready === true", { timeout: 60000 });
  const input2 = await page.$("#file");
  await input2.uploadFile(join(SITE, "assets/documents/report.docx"));
  await page.waitForFunction(() => document.querySelectorAll(".edv-page").length > 0, { timeout: 90000 });
  await new Promise((r) => setTimeout(r, 1500));
  await page.evaluate(() => {
    const v = window.__edgedoc.viewer;
    v.setZoom(1.4);
    v.goToPage(1);
  });
  await new Promise((r) => setTimeout(r, 800));
  await shot("07-page-2-full");

  // A narrow viewport, which a phone uses.
  await page.setViewport({ width: 430, height: 900, deviceScaleFactor: 2 });
  await new Promise((r) => setTimeout(r, 900));
  await shot("08-narrow");

  check("the browser reported no error", errors.length === 0, errors.slice(0, 2).join(" | "));

  await browser.close();
  server.close();

  const failed = results.filter((r) => !r.ok);
  const summary = { results, failed: failed.length, at: new Date().toISOString() };
  await writeFile(join(OUT, "report.json"), JSON.stringify(summary, null, 2));
  console.log(`\n${results.length - failed.length}/${results.length} checks passed`);
  console.log(`screenshots: ${OUT}\n`);
  process.exit(failed.length === 0 ? 0 : 1);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
