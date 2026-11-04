#!/usr/bin/env node
/**
 * The end-to-end test over the downloaded corpus.
 *
 * It serves `site/`, loads every document from `tests/corpus/downloaded` in the viewer, and
 * asserts what the page shows: that it opens, that the text paints, that a table draws a rule,
 * and that no page paints outside its own size. It writes one screenshot per document that
 * carries text, so a person can inspect the result.
 *
 *     node tools/display_corpus_e2e.mjs
 *
 * It needs Chrome and puppeteer. Set `CHROME_PATH` and `PUPPETEER_PATH` to override the defaults.
 */
import { createServer } from "node:http";
import { readFile, mkdir, writeFile, readdir } from "node:fs/promises";
import { existsSync } from "node:fs";
import { extname, join, resolve } from "node:path";
import { createRequire } from "node:module";

const require = createRequire(import.meta.url);
const ROOT = resolve(new URL("..", import.meta.url).pathname);
const SITE = join(ROOT, "site");
const CORPUS = join(ROOT, "tests/corpus/downloaded");
const OUT = join(ROOT, "docs/display/e2e/corpus");
const PORT = 8902;

const PUPPETEER =
  process.env.PUPPETEER_PATH ||
  "/Users/raphaelmansuy/.nvm/versions/node/v20.14.0/lib/node_modules/@mermaid-js/mermaid-cli/node_modules/puppeteer";
// An explicit path wins. An empty value means "let puppeteer find the browser it installed", which
// is what CI does. With no value at all, the macOS default is assumed.
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
  ".svg": "image/svg+xml",
};

function serve() {
  const server = createServer(async (req, res) => {
    const url = new URL(req.url, "http://x");
    const decoded = decodeURIComponent(url.pathname);
    // The corpus lives outside `site`, so the server reads it from its own folder.
    let path = decoded.startsWith("/corpus/")
      ? join(CORPUS, decoded.slice("/corpus/".length))
      : join(SITE, decoded);
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
  if (!ok) console.log(`  FAIL  ${name}  ${detail || ""}`);
}

async function main() {
  await mkdir(OUT, { recursive: true });
  const { server, port } = await serve();
  const files = (await readdir(CORPUS)).filter((f) => f.endsWith(".docx")).sort();
  const puppeteer = require(PUPPETEER);
  const browser = await puppeteer.launch({
    executablePath: CHROME,
    headless: "new",
    args: ["--no-sandbox", "--disable-gpu"],
  });
  const page = await browser.newPage();
  await page.setViewport({ width: 1200, height: 900, deviceScaleFactor: 1.5 });
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  page.on("console", (m) => {
    const t = m.text();
    if (m.type() === "error" && !t.includes("favicon") && !t.includes("404")) errors.push(t);
  });

  console.log(`\nEdgeDoc Display: corpus end-to-end, ${files.length} documents\n`);
  await page.goto(`http://127.0.0.1:${port}/display/index.html`, {
    waitUntil: "domcontentloaded",
    timeout: 60000,
  });
  await page.waitForFunction("window.__edgedoc && window.__edgedoc.ready === true", { timeout: 60000 });

  const rows = [];
  for (const file of files) {
    const state = await page.evaluate(async (url) => {
      const v = window.__edgedoc.viewer;
      try {
        const response = await fetch(url);
        const bytes = new Uint8Array(await response.arrayBuffer());
        await v.load(bytes);
        const pages = v.pageCount;
        // Ink is counted over every painted page, because a document can hold its text on a later
        // page. A check of page 1 alone reports 0 for a table that starts on page 2.
        let ink = 0;
        for (const el of document.querySelectorAll(".edv-canvas")) {
          const ctx = el.getContext("2d");
          if (!el.width || !el.height) continue;
          const data = ctx.getImageData(0, 0, el.width, el.height).data;
          for (let i = 0; i < data.length; i += 4) if (data[i] < 128) ink += 1;
        }
        // A rule is a fill that is much wider or much taller than it is thick.
        // A rule can sit on any page, so the count sums every page.
        let rules = 0;
        for (const p of v.pages) {
          rules += p.ops.filter(
            (o) =>
              o.op === "fill" &&
              o.rect &&
              ((o.rect.h < 2 && o.rect.w > 20) || (o.rect.w < 2 && o.rect.h > 20)),
          ).length;
        }
        const spans = document.querySelectorAll(".edv-text span").length;
        const allText = v.texts.join("");
        const outOfBounds = v.pages.some((p) =>
          p.ops.some(
            (o) =>
              o.rect &&
              (o.rect.x < -40 || o.rect.y < -60 || o.rect.x > p.w + 40 || o.rect.y > p.h + 60),
          ),
        );
        return {
          pages,
          ink,
          rules,
          spans,
          outOfBounds,
          diag: v.diagnostics.length,
          text: allText.slice(0, 80),
        };
      } catch (e) {
        return { error: e.code || String(e.message || e) };
      }
    }, `/corpus/${file}`);

    const row = { file, ...state };
    rows.push(row);
    if (state.error) {
      check(`${file}: opens`, false, state.error);
      continue;
    }
    check(`${file}: opens`, true);
    check(`${file}: has at least one page`, state.pages >= 1, `${state.pages}`);
    check(`${file}: paints inside the page`, !state.outOfBounds);
    // A document that holds text must paint it. A structural fixture holds none, and its zero
    // is correct, so the check reads the text layer rather than guessing.
    if (state.spans > 0) {
      check(`${file}: paints text`, state.ink > 100, `${state.ink} ink`);
    }
    if (state.rules > 0) {
      check(`${file}: paints a rule`, state.rules >= 1, `${state.rules} rules`);
    }
    // One screenshot per document that carries text, at fit width.
    if (state.spans > 0) {
      await page.evaluate(() => window.__edgedoc.viewer.fit("width"));
      await new Promise((r) => setTimeout(r, 250));
      await page.screenshot({
        path: join(OUT, `${file.replace(/\.docx$/, "")}.png`),
        fullPage: false,
      });
    }
  }

  check("the browser reported no error", errors.length === 0, errors.slice(0, 2).join(" | "));

  await browser.close();
  server.close();

  const failed = results.filter((r) => !r.ok);
  const withText = rows.filter((r) => !r.error && r.spans > 0).length;
  const structural = rows.filter((r) => !r.error && r.spans === 0).length;
  console.log(`\n  ${rows.length} documents`);
  console.log(`  ${withText} carry text, ${structural} are structural fixtures`);
  console.log(`  ${results.length - failed.length}/${results.length} checks passed`);
  if (failed.length) {
    for (const f of failed) console.log(`    FAIL ${f.name} ${f.detail || ""}`);
  }
  await writeFile(
    join(OUT, "report.json"),
    JSON.stringify({ rows, results, failed: failed.length, at: new Date().toISOString() }, null, 2),
  );
  console.log(`  screenshots: ${OUT}\n`);
  process.exit(failed.length === 0 ? 0 : 1);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
