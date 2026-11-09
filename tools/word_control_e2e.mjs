#!/usr/bin/env node
/**
 * End-to-end test for the Word control.
 *
 * Serves `site/`, drives the page the way a person would, and writes one screenshot per step to
 * `specs/001-word-control/e2e/screenshots`.
 *
 *     node tools/word_control_e2e.mjs
 */
import { createServer } from "node:http";
import { readFile, mkdir, writeFile, stat } from "node:fs/promises";
import { existsSync } from "node:fs";
import { extname, join, resolve } from "node:path";
import { createRequire } from "node:module";

const require = createRequire(import.meta.url);
const ROOT = resolve(new URL("..", import.meta.url).pathname);
const SITE = join(ROOT, "site");
const OUT = resolve(process.env.WORD_E2E_OUT || join(ROOT, "specs/001-word-control/e2e/screenshots"));
const PORT = Number(process.env.WORD_E2E_PORT || 8911);

const PUPPETEER_CANDIDATES = [
  process.env.PUPPETEER_PATH,
  "/tmp/pptest/node_modules/puppeteer-core",
  "/Users/raphaelmansuy/.nvm/versions/node/v20.14.0/lib/node_modules/@mermaid-js/mermaid-cli/node_modules/puppeteer",
].filter(Boolean);
const CHROME =
  process.env.CHROME_PATH !== undefined
    ? process.env.CHROME_PATH || undefined
    : "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";

const MIME = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".wasm": "application/wasm",
  ".css": "text/css; charset=utf-8",
  ".docx": "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
  ".png": "image/png",
  ".jpg": "image/jpeg",
  ".jpeg": "image/jpeg",
};

function serve() {
  const server = createServer(async (req, res) => {
    const url = new URL(req.url, "http://x");
    let path = join(SITE, decodeURIComponent(url.pathname));
    if (url.pathname.endsWith("/")) path = join(path, "index.html");
    if (!path.startsWith(SITE) || !existsSync(path)) {
      res.writeHead(404);
      res.end("not found");
      return;
    }
    const body = await readFile(path);
    res.writeHead(200, {
      "content-type": MIME[extname(path)] || "application/octet-stream",
      "cache-control": "no-store",
    });
    res.end(body);
  });
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

function loadPuppeteer() {
  for (const candidate of PUPPETEER_CANDIDATES) {
    if (existsSync(candidate)) return require(candidate);
  }
  throw new Error("puppeteer was not found. Set PUPPETEER_PATH.");
}

const results = [];
function check(name, ok, detail) {
  results.push({ name, ok: Boolean(ok), detail: detail || "" });
  console.log(`  ${ok ? "ok  " : "FAIL"}  ${name}${detail ? `  ${detail}` : ""}`);
}

async function main() {
  await mkdir(OUT, { recursive: true });
  const { server, port } = await serve();
  const puppeteer = loadPuppeteer();
  const browser = await puppeteer.launch({
    executablePath: CHROME,
    headless: "new",
    args: ["--no-sandbox", "--disable-gpu", "--force-device-scale-factor=1"],
  });
  const page = await browser.newPage();
  await page.setViewport({ width: 1440, height: 900, deviceScaleFactor: 1 });
  const errors = [];
  page.on("pageerror", (err) => errors.push(String(err)));
  page.on("console", (message) => {
    if (message.type() !== "error") return;
    const text = message.text();
    if (text.includes("favicon")) return;
    errors.push(text);
  });

  const shot = async (name) => {
    await page.screenshot({ path: join(OUT, `${name}.png`) });
  };
  const state = () => page.evaluate(() => window.__word.state);
  const textOf = () => page.evaluate(() => (window.__word.viewer.texts || []).join("\n"));

  console.log("\nEdgeDoc Word control: end-to-end\n");
  await page.goto(`http://127.0.0.1:${port}/word/`, { waitUntil: "domcontentloaded", timeout: 60000 });
  await page.waitForFunction(() => window.__word && window.__word.ready === true, { timeout: 60000 });
  await page.waitForSelector(".wc-caret");
  const blank = await page.evaluate(() => {
    const caret = document.querySelector(".wc-caret");
    const rect = caret.getBoundingClientRect();
    return {
      title: document.querySelector("#doc-name").textContent,
      h: rect.height,
      pages: document.querySelectorAll(".edv-page").length,
      error: window.__word.error,
    };
  });
  check("a blank document opens", blank.pages === 1 && blank.title === "Untitled" && !blank.error, blank.title);
  const homeChrome = await page.evaluate(() => ({
    voice: document.querySelector("#act-dictate") !== null,
    editor: document.querySelector("#act-editor") !== null,
    findMenu: document.querySelector('[data-act="find-menu"]') !== null,
  }));
  check(
    "the home ribbon matches wordcraft groups",
    homeChrome.voice && homeChrome.editor && homeChrome.findMenu,
  );
  check("the caret is visible", blank.h >= 10, `${blank.h.toFixed(1)} px`);
  const ruler = await page.evaluate(() => {
    const rule = document.querySelector("#ruler").getBoundingClientRect();
    const sheet = document.querySelector(".edv-page").getBoundingClientRect();
    return { left: rule.left - sheet.left, width: rule.width - sheet.width };
  });
  check("the ruler matches the page", Math.abs(ruler.left) < 2 && Math.abs(ruler.width) < 2, JSON.stringify(ruler));
  await shot("01-blank");

  await page.click(".edv-page");
  await page.keyboard.type("Invoice 1042");
  await page.waitForFunction(() => (window.__word.viewer.texts[0] || "").includes("Invoice 1042"), { timeout: 20000 });
  check("typing appears on the page", true);
  await shot("02-typed");
  await page.keyboard.type(". Pay on time.");
  await page.keyboard.press("Home");
  await page.click('[data-act="select-menu"]');
  await page.waitForSelector('[data-select="sentence"]');
  await page.click('[data-select="sentence"]');
  const sentence = await page.evaluate(() => window.__word.state?.selection || "");
  check("select sentence grabs one sentence", sentence.includes("Invoice 1042"), sentence);
  await page.keyboard.down("Shift");
  await page.keyboard.press("Enter");
  await page.keyboard.up("Shift");
  await page.keyboard.type("Detail line.");
  const soft = await page.evaluate(() => {
    const text = window.__word.viewer.texts[0] || "";
    const path = window.__word.state?.path || "";
    return text.includes("Detail line.") && !path.includes("body[1]");
  });
  check("soft line break stays in one paragraph", soft);

  await page.click(".edv-page");
  await page.click('[data-act="select-menu"]');
  await page.waitForSelector('[data-select="all"]');
  await page.click('[data-select="all"]');
  await page.waitForFunction(() => window.__word.state && window.__word.state.selection.length > 0);
  await page.click('[data-act="bold"]');
  await page.waitForFunction(() => window.__word.state && window.__word.state.bold === true);
  check("bold marks the selection", true);
  await page.click('[data-act="shading-menu"]');
  await page.click('[data-shading="F3F2F1"]');
  await page.waitForFunction(() => window.__word.state && window.__word.state.paraShading === "F3F2F1");
  check("paragraph shading applies", true);
  await shot("03-bold");

  await page.click("#act-new");
  await page.waitForFunction(
    () =>
      window.__word.state?.title === "Untitled" &&
      !(window.__word.viewer.texts[0] || "").trim(),
    { timeout: 15000 },
  );
  await page.click(".edv-page");
  await page.keyboard.type("Invoice 1042");
  await page.keyboard.press("Enter");
  await page.keyboard.type("Bill to ");
  await page.click('[data-tab="mailings"]');
  await page.type("#field-name", "Customer");
  await page.click('#field-form button[type="submit"]');
  await page.waitForFunction(() => window.__word.state && window.__word.state.fields.includes("Customer"));
  const fieldText = await textOf();
  check("a merge field paints its name", fieldText.includes("Customer"), fieldText.slice(0, 80));
  await shot("04-field");

  await page.click('[data-tab="insert"]');
  await page.click("#act-table");
  await page.waitForFunction(() => window.__word.state && window.__word.state.path.includes("row["));
  check("a table opens in its first cell", (await state()).path);
  await shot("05-table");

  await page.click('[data-tab="mailings"]');
  const previewInput = await page.$('#preview-form input[data-field="Customer"]');
  check("the side panel lists the field", previewInput !== null);
  if (previewInput) await previewInput.type("Ada Lovelace");
  await page.click("#act-preview");
  await page.waitForFunction(() => window.__word.state && window.__word.state.preview === true);
  const previewText = await textOf();
  check("preview fills the field", previewText.includes("Ada Lovelace"), previewText.slice(0, 120));
  await shot("06-preview");

  await page.click("#banner-end");
  await page.waitForFunction(() => window.__word.state && window.__word.state.preview === false);
  const restored = await textOf();
  check("leaving preview restores the template", restored.includes("Customer") && !restored.includes("Ada Lovelace"));
  await shot("07-template");

  await page.keyboard.down("Meta");
  await page.keyboard.press("KeyZ");
  await page.keyboard.up("Meta");
  await page.waitForFunction(() => window.__word.state && !window.__word.state.path.includes("row["));
  check("undo removes the table", true);
  await shot("08-undo");

  const input = await page.$("#file");
  await input.uploadFile(join(SITE, "assets/documents/invoice-template.docx"));
  await page.waitForFunction(
    () => window.__word.state && window.__word.state.title === "Invoice" && window.__word.state.fields.length > 5,
    { timeout: 30000 },
  );
  const opened = await page.evaluate(() => ({
    pages: window.__word.viewer.pageCount,
    fields: window.__word.state.fields.length,
    ink: (() => {
      const canvas = document.querySelector(".edv-canvas");
      const data = canvas.getContext("2d").getImageData(0, 0, canvas.width, canvas.height).data;
      let dark = 0;
      for (let i = 0; i < data.length; i += 16) if (data[i] < 80) dark += 1;
      return dark;
    })(),
  }));
  check("a template opens", opened.pages >= 1 && opened.fields > 0, `${opened.pages} pages, ${opened.fields} fields`);
  check("the template page paints ink", opened.ink > 40, `${opened.ink} dark samples`);
  const navOnOpen = await page.evaluate(() => ({
    nav: document.body.classList.contains("nav-on"),
    headings: [...document.querySelectorAll("#nav-list button")].map((node) => node.textContent.trim()),
  }));
  check(
    "structured documents open the navigation pane",
    navOnOpen.nav && navOnOpen.headings.length > 0,
    navOnOpen.headings.join(", "),
  );
  await shot("09-invoice");

  await page.evaluate(() => window.__word.viewer.setZoom(1.5));
  await new Promise((resolve) => setTimeout(resolve, 400));
  const zoomed = await page.evaluate(() => ({
    scale: window.__word.viewer.scale,
    caret: document.querySelector(".wc-caret") !== null,
  }));
  check("zoom keeps the caret", Math.abs(zoomed.scale - 1.5) < 0.02 && zoomed.caret, zoomed.scale.toFixed(2));
  const zoomRuler = await page.evaluate(() => {
    const rule = document.querySelector("#ruler").getBoundingClientRect();
    const sheet = document.querySelector(".edv-page").getBoundingClientRect();
    return { left: rule.left - sheet.left, width: rule.width - sheet.width };
  });
  check("the ruler still matches after zoom", Math.abs(zoomRuler.left) < 2 && Math.abs(zoomRuler.width) < 2, JSON.stringify(zoomRuler));
  await shot("10-zoom");

  await page.setViewport({ width: 390, height: 844, deviceScaleFactor: 1 });
  await page.evaluate(() => window.__word.viewer.fit("width"));
  await new Promise((resolve) => setTimeout(resolve, 500));
  await shot("11-narrow");
  await page.setViewport({ width: 1440, height: 900, deviceScaleFactor: 1 });
  await page.evaluate(() => window.__word.viewer.setZoom(1));
  await new Promise((resolve) => setTimeout(resolve, 300));

  await page.click('[data-tab="layout"]');
  await page.waitForSelector('.wc-panel[data-panel="layout"]:not([hidden])');
  const layout = await page.evaluate(() => ({
    margins: document.querySelector("#layout-margins")?.textContent || "",
    indent: document.querySelector('[data-panel="layout"] [data-act="indent-more"]') !== null,
  }));
  check("the layout ribbon shows page setup", layout.margins.includes("Normal") && layout.indent, layout.margins);
  await shot("12-layout");

  await page.click('[data-tab="view"]');
  await page.waitForSelector('.wc-panel[data-panel="view"]:not([hidden])');
  const view = await page.evaluate(() => ({
    print: document.querySelector("#view-print") !== null,
    fields: document.querySelector("#toggle-fields-pane") !== null,
    nav: document.querySelector("#toggle-nav-pane") !== null,
    tableGrid: document.querySelector("#table-pick span") !== null,
    address: document.querySelector("#act-address-block") !== null,
    sub: document.querySelector('[data-act="subscript"]') !== null,
    marks: document.querySelector('[data-act="marks"]') !== null,
    selectMenu: document.querySelector('[data-act="select-menu"]') !== null,
    numberingMenu: document.querySelector('[data-act="number-menu"]') !== null,
    multilevelMenu: document.querySelector('[data-act="multilevel-menu"]') !== null,
    bulletMenu: document.querySelector('[data-act="bullet-menu"]') !== null,
    spacingMenu: document.querySelector('[data-act="spacing-menu"]') !== null,
    shadingMenu: document.querySelector('[data-act="shading-menu"]') !== null,
    borderMenu: document.querySelector('[data-act="border-menu"]') !== null,
    findMenu: document.querySelector('[data-act="find-menu"]') !== null,
    dictate: document.querySelector("#act-dictate") !== null,
    editorBtn: document.querySelector("#act-editor") !== null,
    sort: document.querySelector('[data-act="sort"]') !== null,
    styles: document.querySelectorAll("[data-look]").length >= 6,
  }));
  check("the view ribbon exposes print layout and fields pane", view.print && view.fields && view.nav);
  check(
    "home and insert keep wordcraft controls",
    view.tableGrid &&
      view.address &&
      view.sub &&
      view.marks &&
      view.styles &&
      view.selectMenu &&
      view.numberingMenu &&
      view.multilevelMenu &&
      view.bulletMenu &&
      view.spacingMenu &&
      view.shadingMenu &&
      view.borderMenu &&
      view.findMenu &&
      view.dictate &&
      view.editorBtn &&
      view.sort,
  );
  await page.evaluate(() => {
    if (!document.body.classList.contains("nav-on")) document.querySelector("#toggle-nav-pane")?.click();
  });
  await page.waitForFunction(() => document.body.classList.contains("nav-on"));
  check("the navigation pane opens", await page.$("#nav-pane") !== null);
  const navHeadings = await page.evaluate(() =>
    [...document.querySelectorAll("#nav-list button")].map((node) => node.textContent.trim()),
  );
  check(
    "the navigation pane lists document headings",
    navHeadings.some((title) => /how to pay|notes/i.test(title)),
    navHeadings.join(", "),
  );
  await shot("13-view");

  await page.click('[data-tab="mailings"]');
  await page.click("#act-highlight-fields");
  await new Promise((resolve) => setTimeout(resolve, 300));
  const highlighted = await page.evaluate(() => document.body.classList.contains("highlight-fields"));
  check("highlight fields arms", highlighted);
  await shot("14-highlight");

  check("the browser reported no error", errors.length === 0, errors.slice(0, 3).join(" | "));

  const shotNames = [
    "01-blank",
    "02-typed",
    "03-bold",
    "04-field",
    "05-table",
    "06-preview",
    "07-template",
    "08-undo",
    "09-invoice",
    "10-zoom",
    "11-narrow",
    "12-layout",
    "13-view",
    "14-highlight",
  ];
  for (const name of shotNames) {
    const path = join(OUT, `${name}.png`);
    const info = await stat(path).catch(() => null);
    check(`screenshot ${name} is written`, info && info.size > 4096, info ? `${info.size} bytes` : "missing");
  }

  await browser.close();
  server.close();
  const failed = results.filter((item) => !item.ok);
  await writeFile(join(OUT, "report.json"), JSON.stringify({ results, failed: failed.length, at: new Date().toISOString() }, null, 2));
  console.log(`\n${results.length - failed.length}/${results.length} checks passed`);
  console.log(`screenshots: ${OUT}\n`);
  process.exit(failed.length === 0 ? 0 : 1);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
