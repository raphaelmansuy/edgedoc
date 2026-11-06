#!/usr/bin/env node
/**
 * Build the WebAssembly module for Node, then run one example of each operation.
 *
 *     node examples/wasm/run.mjs
 *
 * The script builds the `wasm32-unknown-unknown` crate, converts it with
 * `wasm-bindgen`, and calls the result. It needs `wasm-bindgen` 0.2.100 on `PATH`
 * (`cargo install wasm-bindgen-cli --version 0.2.100`).
 *
 * The examples read the documents that `make examples` writes, so run that first.
 */

import { execFileSync } from "node:child_process";
import { mkdirSync, readFileSync, writeFileSync, existsSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, "..", "..");
const out = join(root, "target", "wasm-node");
const artifacts = join(root, "target", "examples");

function step(message) {
  process.stdout.write(`\n  ${message}\n`);
}

function build() {
  step("building the module for wasm32-unknown-unknown");
  execFileSync(
    "cargo",
    ["build", "--target", "wasm32-unknown-unknown", "-p", "edgedoc-wasm", "--features", "wasm", "--release"],
    { cwd: root, stdio: "inherit" },
  );

  step("converting the module with wasm-bindgen");
  mkdirSync(out, { recursive: true });
  execFileSync(
    "wasm-bindgen",
    [
      join(root, "target", "wasm32-unknown-unknown", "release", "edgedoc_wasm.wasm"),
      "--out-dir",
      out,
      "--target",
      "nodejs",
      "--out-name",
      "edgedoc",
    ],
    { stdio: "inherit" },
  );
}

function read(name) {
  const path = join(artifacts, name);
  if (!existsSync(path)) {
    throw new Error(`${name} is missing. Run \`make examples\` first.`);
  }
  return new Uint8Array(readFileSync(path));
}

async function main() {
  if (!existsSync(join(out, "edgedoc.js"))) {
    build();
  }
  const edgedoc = await import(join(out, "edgedoc.js"));

  step("1. read a document");
  const invoice = read("invoices/INV-2026-0418.docx");
  const text = edgedoc.readDocument(invoice);
  const firstLine = text.split("\n")[0];
  console.log(`     ${firstLine}`);
  console.log(`     ${text.length} characters of body text`);

  step("2. summary of the package");
  for (const line of edgedoc.documentSummary(invoice).split("\n")) {
    console.log(`     ${line}`);
  }

  step("3. merge field names");
  const template = read("invoice-template.docx");
  console.log(`     ${edgedoc.mergeFieldNames(template).join(", ")}`);

  step("4. read one story");
  const header = edgedoc.textOf(template, "header");
  console.log(`     header: ${header.replace(/\s+/g, " ").trim()}`);
  const footer = edgedoc.textOf(template, "footer");
  console.log(`     footer: ${footer.replace(/\s+/g, " ").trim()}`);

  step("5. preview one record, then merge every record");
  const csv = new TextEncoder().encode(
    [
      "InvoiceNumber,InvoiceDate,DueDate,Reference,Customer,Contact,Address,City,PostalCode,Country,Terms,Notes,Lines",
      "INV-9001,1 November 2026,1 December 2026,PO-1,Contoso,Ada Lovelace,1 Main Street,Leeds,LS1 1AA,United Kingdom,Net 30,First,Consulting:2:900.00",
      "INV-9002,2 November 2026,2 December 2026,PO-2,Fabrikam,Grace Hopper,2 Market Road,Oslo,0150,Norway,Net 15,Second,Support:1:450.00",
    ].join("\n"),
  );
  const previewText = edgedoc.preview(template, csv, "empty");
  console.log(`     preview: ${previewText.split("\n").slice(0, 2).join(" | ")}`);

  const merged = edgedoc.mergeRecords(template, csv, "letters", "auto", "empty");
  const mergedText = edgedoc.readDocument(merged);
  const invoices = (mergedText.match(/INV-9001/g) ?? []).length;
  console.log(`     merged ${merged.length} bytes, first invoice appears ${invoices} time(s)`);

  step("6. write in another flavour");
  const dotx = edgedoc.writeAs(template, "dotx");
  console.log(`     dotx ${dotx.length} bytes, docx ${template.length} bytes`);

  step("7. a round trip through read and write");
  const rewritten = edgedoc.writeDocument(template);
  console.log(`     unedited write returns ${rewritten.length} bytes, same as input: ${rewritten.length === template.length}`);

  step("8. an error carries a stable code");
  try {
    edgedoc.readDocument(new TextEncoder().encode("not a document"));
  } catch (error) {
    console.log(`     ${String(error).split("\n")[0]}`);
  }

  step("9. read markdown into a document");
  const fromMarkdown = edgedoc.markdownToDocument("# Hello\n\n**Ada**\n");
  const fromText = edgedoc.readDocument(fromMarkdown);
  console.log(`     ${fromMarkdown.length} bytes, text: ${fromText.replace(/\n/g, " | ")}`);
  if (!fromText.includes("Hello") || !fromText.includes("Ada")) {
    throw new Error("markdownToDocument dropped the heading or the paragraph");
  }
  const image = new TextEncoder().encode("PNG-BYTES");
  const withImage = edgedoc.markdownToDocumentWithImages(
    "![Dot](images/dot.png)\n",
    ["images/dot.png"],
    image,
    [image.length],
  );
  if (withImage[0] !== 0x50 || withImage[1] !== 0x4b) {
    throw new Error("markdownToDocumentWithImages did not return a package");
  }
  console.log(`     image markdown became ${withImage.length} bytes`);

  step("10. password detection");
  // A protected Word file is a compound file that holds an `EncryptionInfo` stream.
  const cfbSignature = [0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1];
  const streamName = Array.from(new TextEncoder().encode("EncryptionInfo")).flatMap((byte) => [byte, 0]);
  const protectedFile = new Uint8Array([...cfbSignature, ...streamName]);
  console.log(`     a compound file with EncryptionInfo is protected: ${edgedoc.isPasswordProtected(protectedFile)}`);
  console.log(`     a plain ZIP package is protected: ${edgedoc.isPasswordProtected(template)}`);

  step("writing a merged document to target/examples/wasm-merged.docx");
  writeFileSync(join(artifacts, "wasm-merged.docx"), merged);
  console.log("     done");
}

main().catch((error) => {
  console.error(`\n  failed: ${error.message}`);
  process.exitCode = 1;
});
