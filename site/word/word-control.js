import { createViewer } from "../display/edgedoc-viewer.js";

const COLORS = ["1F4E79", "201F1E", "C00000", "2E7D32", "6B4C9A", "B45309", "FFFFFF", "000000"];
const PARA_SHADES = [
  ["", "No Color"],
  ["F3F2F1", "Gray"],
  ["FFF3A0", "Yellow"],
  ["DEEAF6", "Blue"],
  ["E2EFDA", "Green"],
];
const HIGHLIGHTS = [
  ["yellow", "#fff3a0"],
  ["green", "#b7f0b0"],
  ["cyan", "#b8f2f2"],
  ["magenta", "#f6c2ea"],
  ["red", "#ffc7c2"],
  ["blue", "#c5dcff"],
  ["darkYellow", "#e6d36a"],
  ["none", "#ffffff"],
];

const docName = document.querySelector("#doc-name");
const fieldList = document.querySelector("#field-list");
const previewForm = document.querySelector("#preview-form");
const fieldHint = document.querySelector("#field-hint");
const banner = document.querySelector("#banner");
const statusError = document.querySelector("#status-error");
const ruler = document.querySelector("#ruler");
const zoom = document.querySelector("#zoom");
const zoomReadout = document.querySelector("#zoom-readout");

const viewer = createViewer({
  container: document.querySelector("#doc"),
  chrome: false,
  editing: true,
  zoom: 1,
  onViewChange: () => {
    paintCaret();
    paintFind();
    placeMini();
    syncRuler();
    syncZoom();
  },
});

const host = {
  ready: false,
  state: null,
  viewer,
  error: "",
};
window.__word = host;

let state = null;
let queue = Promise.resolve();
let fieldsKey = "";
let rememberedFields = [];
let dragging = false;
let dragFrame = 0;
let finds = [];
let findAt = -1;
const previewValues = new Map();

const colorPop = document.querySelector("#color-pop");
const highlightPop = document.querySelector("#highlight-pop");
const numberPop = document.querySelector("#number-pop");
const selectPop = document.querySelector("#select-pop");
const multilevelPop = document.querySelector("#multilevel-pop");
const bulletPop = document.querySelector("#bullet-pop");
const spacingPop = document.querySelector("#spacing-pop");
const paraShadingPop = document.querySelector("#para-shading-pop");
const paraBorderPop = document.querySelector("#para-border-pop");
const findPop = document.querySelector("#find-pop");
if (paraShadingPop) {
  paraShadingPop.replaceChildren();
  for (const [hex, label] of PARA_SHADES) {
    const button = document.createElement("button");
    button.type = "button";
    button.role = "menuitem";
    button.dataset.shading = hex;
    button.textContent = label;
    if (hex) button.style.background = `#${hex}`;
    paraShadingPop.append(button);
  }
}
let navDocKey = "";
let navAutoDismissed = false;

function closeMenus(keep) {
  if (keep !== "color") colorPop.hidden = true;
  if (keep !== "highlight") highlightPop.hidden = true;
  if (keep !== "number" && numberPop) numberPop.hidden = true;
  if (keep !== "select" && selectPop) selectPop.hidden = true;
  if (keep !== "multilevel" && multilevelPop) multilevelPop.hidden = true;
  if (keep !== "bullet" && bulletPop) bulletPop.hidden = true;
  if (keep !== "spacing" && spacingPop) spacingPop.hidden = true;
  if (keep !== "shading" && paraShadingPop) paraShadingPop.hidden = true;
  if (keep !== "border" && paraBorderPop) paraBorderPop.hidden = true;
  if (keep !== "find" && findPop) findPop.hidden = true;
}

function docNavKey() {
  if (!state) return "";
  return `${state.title || ""}\0${state.pages || 0}\0${(state.fields || []).join("\0")}`;
}

function maybeAutoNavPane() {
  const key = docNavKey();
  if (key !== navDocKey) {
    navDocKey = key;
    navAutoDismissed = false;
  }
  if (navAutoDismissed || document.body.classList.contains("nav-on")) return;
  let outline = [];
  try {
    outline = JSON.parse(viewer.viewer.outline?.() || "[]");
  } catch {
    outline = [];
  }
  if (!outline.length) return;
  document.body.classList.add("nav-on");
  const toggle = document.querySelector("#toggle-nav-pane");
  toggle?.classList.add("is-on");
  toggle?.setAttribute("aria-pressed", "true");
  syncNav();
}
for (const hex of COLORS) {
  const button = document.createElement("button");
  button.type = "button";
  button.style.background = `#${hex}`;
  button.title = `#${hex}`;
  button.addEventListener("click", () => {
    colorPop.hidden = true;
    document.querySelector("#color-bar").style.background = `#${hex}`;
    run(() => viewer.viewer.editColor(hex));
  });
  colorPop.append(button);
}
for (const [name, paint] of HIGHLIGHTS) {
  const button = document.createElement("button");
  button.type = "button";
  button.style.background = paint;
  button.title = name;
  button.addEventListener("click", () => {
    highlightPop.hidden = true;
    document.querySelector("#highlight-bar").style.background = paint;
    run(() => viewer.viewer.editHighlight(name));
  });
  highlightPop.append(button);
}

function showError(message) {
  host.error = message || "";
  statusError.textContent = host.error;
}

function run(invoke, options) {
  const job = queue.then(async () => {
    state = await viewer.commit(invoke, options);
    host.state = state;
    render();
    showError("");
  });
  queue = job.catch((err) => {
    showError(err && err.message ? err.message : "The edit did not apply.");
  });
  return job;
}

function render() {
  if (!state) return;
  host.state = state;
  const title = state.title || "Untitled";
  docName.textContent = state.dirty ? `${title} — Not saved` : title;
  document.title = `${title} — EdgeDoc Word`;
  refreshFind();
  paintCaret();
  paintFind();
  if (document.body.classList.contains("highlight-fields")) {
    paintFieldHits(state.fields || rememberedFields);
  }
  placeMini();
  syncRibbon();
  syncFields();
  syncNav();
  maybeAutoNavPane();
  syncMarks();
  syncRuler();
  syncStatus();
  document.body.classList.toggle("is-preview", Boolean(state.preview));
  banner.hidden = !state.preview;
  document.querySelector("#act-end-preview").hidden = !state.preview;
  host.ready = true;
}

function paintCaret() {
  if (!state) return;
  for (const el of viewer.pageEls) {
    el.wrap.querySelectorAll(".wc-caret, .wc-sel").forEach((node) => node.remove());
  }
  const scale = viewer.scale || 1;
  for (const mark of state.marks || []) {
    const page = viewer.pageEls[mark.page];
    if (!page) continue;
    const box = document.createElement("div");
    box.className = "wc-sel";
    box.style.left = `${mark.x * scale}px`;
    box.style.top = `${mark.y * scale}px`;
    box.style.width = `${Math.max(1, mark.w * scale)}px`;
    box.style.height = `${Math.max(1, mark.h * scale)}px`;
    page.wrap.append(box);
  }
  if ((state.marks || []).length > 0) return;
  const page = viewer.pageEls[state.page];
  if (!page) return;
  const caret = document.createElement("div");
  caret.className = "wc-caret";
  caret.style.left = `${state.x * scale}px`;
  caret.style.top = `${state.y * scale}px`;
  caret.style.height = `${Math.max(12, state.h * scale)}px`;
  page.wrap.append(caret);
}

function placeMini() {
  const mini = document.querySelector("#mini");
  if (!mini) return;
  const mark = state && !dragging ? (state.marks || [])[0] : null;
  const page = mark && viewer.pageEls[mark.page];
  if (!page) {
    mini.hidden = true;
    return;
  }
  const box = page.wrap.getBoundingClientRect();
  const scale = viewer.scale || 1;
  mini.hidden = false;
  mini.style.left = `${Math.max(8, box.left + mark.x * scale)}px`;
  mini.style.top = `${Math.max(8, box.top + mark.y * scale - 42)}px`;
}

function refreshFind() {
  const query = document.querySelector("#find").value.trim();
  if (!query || !viewer.viewer || !viewer.viewer.search) {
    finds = [];
    findAt = -1;
    return;
  }
  try {
    const result = JSON.parse(viewer.viewer.search(query));
    finds = result.hits || [];
  } catch {
    finds = [];
  }
  if (findAt >= finds.length) findAt = finds.length ? 0 : -1;
}

function paintFind() {
  for (const el of viewer.pageEls || []) {
    el.wrap.querySelectorAll(".wc-hit").forEach((node) => node.remove());
  }
  const scale = viewer.scale || 1;
  finds.forEach((hit, index) => {
    const page = viewer.pageEls[hit.page];
    if (!page) return;
    for (const rect of hit.rects || []) {
      const box = document.createElement("div");
      box.className = index === findAt ? "wc-hit is-current" : "wc-hit";
      box.style.left = `${rect[0] * scale}px`;
      box.style.top = `${rect[1] * scale}px`;
      box.style.width = `${Math.max(2, rect[2] * scale)}px`;
      box.style.height = `${Math.max(2, rect[3] * scale)}px`;
      page.wrap.append(box);
    }
  });
}

function revealFind() {
  const hit = finds[findAt];
  const page = hit && viewer.pageEls[hit.page];
  if (page) page.wrap.scrollIntoView({ block: "center", inline: "nearest" });
}

function syncRibbon() {
  const press = (selector, on) => {
    document.querySelectorAll(selector).forEach((node) => node.classList.toggle("is-on", on));
  };
  press('[data-act="bold"]', state.bold);
  press('[data-act="italic"]', state.italic);
  press('[data-act="underline"]', state.underline);
  press('[data-act="strike"]', state.strike);
  press('[data-act="subscript"]', state.subscript);
  press('[data-act="superscript"]', state.superscript);
  press('[data-act="painter"]', Boolean(state.painter));
  press('[data-act="marks"]', document.body.classList.contains("show-marks"));
  document.querySelector("#act-highlight-fields")?.classList.toggle(
    "is-on",
    document.body.classList.contains("highlight-fields"),
  );
  const align = state.align === "both" ? "justify" : state.align || "left";
  document.querySelectorAll("[data-align]").forEach((node) => {
    node.classList.toggle("is-on", node.dataset.align === align);
  });
  const rawList = state.list || "";
  const numbering = new Set(["number", "decimal", "lowerroman", "upperroman", "lowerletter", "upperletter"]);
  document.querySelectorAll("[data-list]").forEach((node) => {
    const key = node.dataset.list;
    let on = false;
    if (key === "bullet") on = rawList === "bullet";
    else if (key === "number") on = rawList === "decimal" || rawList === "number";
    else if (numbering.has(key)) on = rawList === key;
    node.classList.toggle("is-on", on);
  });
  document.querySelector('[data-act="number-menu"]')?.classList.toggle(
    "is-on",
    numbering.has(rawList) || rawList === "decimal",
  );
  document.querySelector('[data-act="bullet-menu"]')?.classList.toggle("is-on", rawList === "bullet");
  const line = state.line || 0;
  const spacingPick =
    line >= 2.75 ? "3"
    : line >= 2.25 ? "2.5"
    : line >= 1.9 ? "2"
    : line >= 1.4 ? "1.5"
    : line >= 1.2 ? "1.15"
    : line >= 0.9 ? "1"
    : "";
  document.querySelector('[data-act="spacing-menu"]')?.classList.toggle("is-on", Boolean(spacingPick));
  const spacing = document.querySelector("#spacing");
  if (spacing) spacing.value = spacingPick;
  const shade = (state.paraShading || "").toUpperCase();
  document.querySelector('[data-act="shading-menu"]')?.classList.toggle("is-on", Boolean(shade));
  document.querySelector('[data-act="border-menu"]')?.classList.toggle(
    "is-on",
    Boolean(state.paraBorderBottom),
  );
  document.querySelectorAll("[data-look]").forEach((node) => {
    if (node.classList.contains("wc-style")) {
      node.classList.toggle("is-on", node.dataset.look === (state.look || "normal"));
    }
  });
  document.querySelectorAll('[data-act="undo"]').forEach((node) => {
    node.disabled = !state.canUndo;
  });
  document.querySelectorAll('[data-act="redo"]').forEach((node) => {
    node.disabled = !state.canRedo;
  });
  const size = document.querySelector("#size");
  const sizeText = String(Math.round(state.fontSize || 11));
  if (document.activeElement !== size && [...size.options].some((option) => option.value === sizeText)) {
    size.value = sizeText;
  }
}

function syncFields() {
  let names = state.fields || [];
  if (names.length) rememberedFields = names.slice();
  else if (state.preview && rememberedFields.length) names = rememberedFields;
  const key = names.join("\0");
  fieldList.replaceChildren();
  const suggestions = document.querySelector("#field-suggestions");
  if (suggestions) suggestions.replaceChildren();
  if (!names.length) {
    const item = document.createElement("li");
    item.className = "wc-empty";
    item.textContent = "No merge fields yet.";
    fieldList.append(item);
  }
  for (const name of names) {
    const item = document.createElement("li");
    const button = document.createElement("button");
    button.type = "button";
    button.textContent = name;
    button.addEventListener("click", () => {
      document.querySelector("#field-name").value = name;
      activate("mailings");
      run(() => viewer.viewer.editMergeField(name));
    });
    item.append(button);
    fieldList.append(item);
    if (suggestions) {
      const option = document.createElement("option");
      option.value = name;
      suggestions.append(option);
    }
  }
  if (document.body.classList.contains("highlight-fields")) paintFieldHits(names);
  if (key === fieldsKey) return;
  fieldsKey = key;
  const focus = document.activeElement;
  previewForm.replaceChildren();
  for (const name of names) {
    const label = document.createElement("label");
    label.textContent = name;
    const input = document.createElement("input");
    input.type = "text";
    input.dataset.field = name;
    input.value = previewValues.get(name) || "";
    input.addEventListener("input", () => previewValues.set(name, input.value));
    label.append(input);
    previewForm.append(label);
  }
  if (focus && previewForm.contains(focus)) focus.focus();
}

function paintFieldHits(names) {
  for (const el of viewer.pageEls || []) {
    el.wrap.querySelectorAll(".wc-field-hit").forEach((node) => node.remove());
  }
  if (!document.body.classList.contains("highlight-fields") || !viewer.viewer?.search) return;
  const scale = viewer.scale || 1;
  for (const name of names || []) {
    let hits = [];
    try {
      hits = JSON.parse(viewer.viewer.search(`«${name}»`)).hits || [];
    } catch {
      hits = [];
    }
    for (const hit of hits) {
      const page = viewer.pageEls[hit.page];
      if (!page) continue;
      for (const rect of hit.rects || []) {
        const box = document.createElement("div");
        box.className = "wc-hit wc-field-hit";
        box.style.left = `${rect[0] * scale}px`;
        box.style.top = `${rect[1] * scale}px`;
        box.style.width = `${Math.max(2, rect[2] * scale)}px`;
        box.style.height = `${Math.max(2, rect[3] * scale)}px`;
        page.wrap.append(box);
      }
    }
  }
}

function syncStatus() {
  const page = (state.page || 0) + 1;
  const pages = state.pages || viewer.pageCount || 1;
  document.querySelector("#status-page").textContent = `Page ${page} of ${pages}`;
  const words = state.words || 0;
  const selected = (state.selection || "").trim().split(/\s+/).filter(Boolean).length;
  document.querySelector("#status-words").textContent =
    selected > 0 ? `${selected} of ${words} ${words === 1 ? "word" : "words"}` : `${words} ${words === 1 ? "word" : "words"}`;
  document.querySelector("#status-field").textContent = state.field ? `Field ${state.field}` : "";
  document.querySelector("#layout-pages").textContent = `${pages} ${pages === 1 ? "page" : "pages"}`;
  syncZoom();
}

function syncZoom() {
  const percent = Math.round((viewer.scale || 1) * 100);
  zoomReadout.textContent = `${percent}%`;
  if (document.activeElement !== zoom) zoom.value = String(Math.min(200, Math.max(50, percent)));
}

function syncRuler() {
  const page = viewer.pages[0];
  const el = viewer.pageEls && viewer.pageEls[0];
  if (!page || !el) return;
  const holder = ruler.parentElement.getBoundingClientRect();
  const box = el.wrap.getBoundingClientRect();
  const cssWidth = Math.max(1, Math.round(box.width));
  const scale = cssWidth / page.w;
  ruler.style.left = `${box.left - holder.left}px`;
  ruler.style.width = `${cssWidth}px`;
  const dpr = Math.min(2, window.devicePixelRatio || 1);
  ruler.style.width = `${cssWidth}px`;
  ruler.width = Math.round(cssWidth * dpr);
  ruler.height = Math.round(22 * dpr);
  const ctx = ruler.getContext("2d");
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.clearRect(0, 0, cssWidth, 22);
  ctx.fillStyle = "#f7f8fa";
  ctx.fillRect(0, 0, cssWidth, 22);
  const margin = 72 * scale;
  ctx.fillStyle = "#eceff3";
  ctx.fillRect(0, 0, Math.min(margin, cssWidth), 22);
  ctx.fillRect(Math.max(0, cssWidth - margin), 0, margin, 22);
  ctx.strokeStyle = "#8a919c";
  ctx.fillStyle = "#5c6570";
  ctx.font = "10px Segoe UI, sans-serif";
  ctx.beginPath();
  const inch = 72 * scale;
  for (let x = 0, n = 0; x <= cssWidth + 1; x += inch / 4, n += 1) {
    const major = n % 4 === 0;
    const h = major ? 10 : n % 2 === 0 ? 6 : 4;
    ctx.moveTo(x + 0.5, 22);
    ctx.lineTo(x + 0.5, 22 - h);
    if (major && n > 0) ctx.fillText(String(n / 4), x + 3, 10);
  }
  ctx.stroke();
  const indent = Math.max(0, (state && state.indentLeft) || 0) * scale;
  const handle = margin + indent;
  ctx.fillStyle = varAccent();
  ctx.beginPath();
  ctx.moveTo(handle, 14);
  ctx.lineTo(handle - 5, 21);
  ctx.lineTo(handle + 5, 21);
  ctx.closePath();
  ctx.fill();
  syncVRuler();
}

function varAccent() {
  return getComputedStyle(document.documentElement).getPropertyValue("--accent").trim() || "#1f4e79";
}

function syncVRuler() {
  const vruler = document.querySelector("#vruler");
  const page = viewer.pages[0];
  const el = viewer.pageEls && viewer.pageEls[0];
  if (!vruler || !page || !el) return;
  const sheet = vruler.parentElement.getBoundingClientRect();
  const box = el.wrap.getBoundingClientRect();
  const cssHeight = Math.max(1, Math.round(box.height));
  const scale = cssHeight / page.h;
  vruler.style.top = `${box.top - sheet.top}px`;
  vruler.style.left = `${Math.max(0, box.left - sheet.left - 18)}px`;
  vruler.style.height = `${cssHeight}px`;
  const dpr = Math.min(2, window.devicePixelRatio || 1);
  vruler.width = Math.round(16 * dpr);
  vruler.height = Math.round(cssHeight * dpr);
  const ctx = vruler.getContext("2d");
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.clearRect(0, 0, 16, cssHeight);
  ctx.fillStyle = "#f7f8fa";
  ctx.fillRect(0, 0, 16, cssHeight);
  const margin = 72 * scale;
  ctx.fillStyle = "#eceff3";
  ctx.fillRect(0, 0, 16, Math.min(margin, cssHeight));
  ctx.fillRect(0, Math.max(0, cssHeight - margin), 16, margin);
  ctx.strokeStyle = "#8a919c";
  ctx.beginPath();
  const inch = 72 * scale;
  for (let y = 0, n = 0; y <= cssHeight + 1; y += inch / 4, n += 1) {
    const major = n % 4 === 0;
    const w = major ? 8 : n % 2 === 0 ? 5 : 3;
    ctx.moveTo(16, y + 0.5);
    ctx.lineTo(16 - w, y + 0.5);
  }
  ctx.stroke();
}

function pointOf(event) {
  const hit = document.elementFromPoint(event.clientX, event.clientY);
  const wrap = hit && hit.closest ? hit.closest(".edv-page") : null;
  if (!wrap) return null;
  const rect = wrap.getBoundingClientRect();
  const scale = viewer.scale || 1;
  return {
    page: Number(wrap.dataset.page || 0),
    x: (event.clientX - rect.left) / scale,
    y: (event.clientY - rect.top) / scale,
  };
}

function typingTarget(target) {
  return target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement || target instanceof HTMLSelectElement;
}

function activate(name) {
  document.querySelectorAll(".wc-tab").forEach((tab) => {
    tab.setAttribute("aria-selected", tab.dataset.tab === name ? "true" : "false");
  });
  document.querySelectorAll(".wc-panel").forEach((panel) => {
    panel.hidden = panel.dataset.panel !== name;
  });
}

document.querySelectorAll(".wc-tab").forEach((tab) => {
  tab.addEventListener("click", () => activate(tab.dataset.tab));
});

document.querySelector(".wc-ribbon").addEventListener("mousedown", (event) => {
  if (event.target.closest("button")) event.preventDefault();
});
document.querySelector('[data-act="painter"]')?.addEventListener("dblclick", (event) => {
  event.preventDefault();
  run(() => viewer.viewer.editPainter(true));
});

document.querySelector("#act-new").addEventListener("click", () => {
  fieldsKey = "";
  rememberedFields = [];
  navDocKey = "";
  navAutoDismissed = false;
  run(() => viewer.viewer.editBlank(), { keepView: false });
});
document.querySelector("#act-open").addEventListener("click", () => document.querySelector("#file").click());
document.querySelector("#file").addEventListener("change", async () => {
  const file = document.querySelector("#file").files[0];
  document.querySelector("#file").value = "";
  if (!file) return;
  const bytes = new Uint8Array(await file.arrayBuffer());
  fieldsKey = "";
  rememberedFields = [];
  navDocKey = "";
  navAutoDismissed = false;
  run(() => viewer.viewer.editOpen(bytes), { keepView: false });
});
document.querySelector("#sample").addEventListener("change", async (event) => {
  const url = event.target.value;
  event.target.value = "";
  if (!url) return;
  const response = await fetch(url);
  if (!response.ok) {
    showError("The sample did not load.");
    return;
  }
  const bytes = new Uint8Array(await response.arrayBuffer());
  fieldsKey = "";
  rememberedFields = [];
  navDocKey = "";
  navAutoDismissed = false;
  run(() => viewer.viewer.editOpen(bytes), { keepView: false });
});
document.querySelector("#act-save").addEventListener("click", () => {
  try {
    const bytes = viewer.viewer.documentBytes();
    const blob = new Blob([bytes], {
      type: "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
    });
    const link = document.createElement("a");
    const title = (state && state.title) || "Untitled";
    link.href = URL.createObjectURL(blob);
    link.download = `${title.replace(/[^\w.-]+/g, "-")}.docx`;
    link.click();
    URL.revokeObjectURL(link.href);
  } catch (err) {
    showError(err.message || "The document could not be saved.");
  }
});

document.querySelector("#font").addEventListener("change", (event) => {
  run(() => viewer.viewer.editFont(event.target.value));
});
document.querySelector("#size").addEventListener("change", (event) => {
  run(() => viewer.viewer.editFontSize(Number(event.target.value)));
});
document.querySelector("#case").addEventListener("change", (event) => {
  const mode = event.target.value;
  event.target.value = "";
  if (mode) run(() => viewer.viewer.editCase(mode));
});
document.querySelector("#spacing").addEventListener("change", (event) => {
  const value = Number(event.target.value);
  if (value) run(() => viewer.viewer.editLineSpacing(value));
});
document.querySelector("#color-pop").parentElement.querySelector('[data-act="color-menu"]').addEventListener("click", (event) => {
  event.stopPropagation();
  closeMenus("color");
  colorPop.hidden = !colorPop.hidden;
});
document.querySelector('[data-act="highlight-menu"]').addEventListener("click", (event) => {
  event.stopPropagation();
  closeMenus("highlight");
  highlightPop.hidden = !highlightPop.hidden;
});
document.querySelector('[data-act="number-menu"]')?.addEventListener("click", (event) => {
  event.stopPropagation();
  closeMenus("number");
  if (numberPop) numberPop.hidden = !numberPop.hidden;
});
document.querySelector('[data-act="select-menu"]')?.addEventListener("click", (event) => {
  event.stopPropagation();
  closeMenus("select");
  if (selectPop) selectPop.hidden = !selectPop.hidden;
});
document.querySelector('[data-act="multilevel-menu"]')?.addEventListener("click", (event) => {
  event.stopPropagation();
  closeMenus("multilevel");
  if (multilevelPop) multilevelPop.hidden = !multilevelPop.hidden;
});
document.querySelector('[data-act="bullet-menu"]')?.addEventListener("click", (event) => {
  event.stopPropagation();
  closeMenus("bullet");
  if (bulletPop) bulletPop.hidden = !bulletPop.hidden;
});
document.querySelector('[data-act="spacing-menu"]')?.addEventListener("click", (event) => {
  event.stopPropagation();
  closeMenus("spacing");
  if (spacingPop) spacingPop.hidden = !spacingPop.hidden;
});
document.querySelector('[data-act="shading-menu"]')?.addEventListener("click", (event) => {
  event.stopPropagation();
  closeMenus("shading");
  if (paraShadingPop) paraShadingPop.hidden = !paraShadingPop.hidden;
});
document.querySelector('[data-act="border-menu"]')?.addEventListener("click", (event) => {
  event.stopPropagation();
  closeMenus("border");
  if (paraBorderPop) paraBorderPop.hidden = !paraBorderPop.hidden;
});
document.querySelector('[data-act="find-menu"]')?.addEventListener("click", (event) => {
  event.stopPropagation();
  closeMenus("find");
  if (findPop) findPop.hidden = !findPop.hidden;
});

document.body.addEventListener("click", (event) => {
  const findNav = event.target.closest("[data-find]");
  if (findNav) {
    closeMenus();
    document.body.classList.add("nav-on");
    document.querySelector("#toggle-nav-pane")?.classList.add("is-on");
    document.querySelector("#toggle-nav-pane")?.setAttribute("aria-pressed", "true");
    navMode = "results";
    document.querySelectorAll("[data-nav]").forEach((node) => {
      const on = node.dataset.nav === "results";
      node.classList.toggle("is-on", on);
      node.setAttribute("aria-selected", String(on));
    });
    syncNav();
    findBox.focus();
    return;
  }
  const shade = event.target.closest("[data-shading]");
  if (shade) {
    closeMenus();
    const fill = shade.dataset.shading;
    run(() => viewer.viewer.editParaShading(fill || null));
    return;
  }
  const border = event.target.closest("[data-para-border]");
  if (border) {
    closeMenus();
    run(() => viewer.viewer.editParaBorder(border.dataset.paraBorder));
    return;
  }
  const space = event.target.closest("[data-para-space]");
  if (space) {
    closeMenus();
    run(() => viewer.viewer.editParaSpacing(space.dataset.paraSpace));
    return;
  }
  const gap = event.target.closest("[data-spacing]");
  if (gap) {
    closeMenus();
    run(() => viewer.viewer.editLineSpacing(Number(gap.dataset.spacing)));
    return;
  }
  const pick = event.target.closest("[data-select]");
  if (pick) {
    closeMenus();
    const mode = pick.dataset.select;
    if (mode === "word") run(() => viewer.viewer.editSelectWord());
    else if (mode === "sentence") run(() => viewer.viewer.editSelectSentence());
    else run(() => viewer.viewer.editSelectAll());
    return;
  }
  const align = event.target.closest("[data-align]");
  if (align) {
    run(() => viewer.viewer.editAlign(align.dataset.align));
    return;
  }
  const list = event.target.closest("[data-list]");
  if (list) {
    closeMenus();
    run(() => viewer.viewer.editList(list.dataset.list));
    return;
  }
  const look = event.target.closest("[data-look]");
  if (look) {
    run(() => viewer.viewer.editLook(look.dataset.look));
    return;
  }
  const act = event.target.closest("[data-act]");
  if (
    !act ||
    act.dataset.act === "color-menu" ||
    act.dataset.act === "highlight-menu" ||
    act.dataset.act === "number-menu" ||
    act.dataset.act === "select-menu" ||
    act.dataset.act === "multilevel-menu" ||
    act.dataset.act === "bullet-menu" ||
    act.dataset.act === "spacing-menu" ||
    act.dataset.act === "shading-menu" ||
    act.dataset.act === "border-menu" ||
    act.dataset.act === "find-menu"
  ) {
    if (!event.target.closest(".wc-pop")) closeMenus();
    return;
  }
  const name = act.dataset.act;
  if (name === "undo") run(() => viewer.viewer.editUndo());
  else if (name === "redo") run(() => viewer.viewer.editRedo());
  else if (
    name === "bold" ||
    name === "italic" ||
    name === "underline" ||
    name === "strike" ||
    name === "subscript" ||
    name === "superscript"
  ) {
    run(() => viewer.viewer.editMark(name));
  } else if (name === "cut") cutSelection();
  else if (name === "copy") copySelection();
  else if (name === "painter") run(() => viewer.viewer.editPainter(false));
  else if (name === "grow") nudgeSize(1);
  else if (name === "shrink") nudgeSize(-1);
  else if (name === "indent-more") run(() => viewer.viewer.editIndent(1));
  else if (name === "indent-less") run(() => viewer.viewer.editIndent(-1));
  else if (name === "marks") {
    document.body.classList.toggle("show-marks");
    event.target.closest("[data-act]")?.classList.toggle(
      "is-on",
      document.body.classList.contains("show-marks"),
    );
    syncMarks();
  }
});

function syncMarks() {
  for (const line of document.querySelectorAll(".edv-line")) {
    line.querySelectorAll(".wc-pilcrow").forEach((node) => node.remove());
    if (!document.body.classList.contains("show-marks")) continue;
    const mark = document.createElement("span");
    mark.className = "wc-pilcrow";
    mark.textContent = "¶";
    const spans = line.querySelectorAll("span");
    const last = spans[spans.length - 1];
    if (last) {
      mark.style.left = `${parseFloat(last.style.left || "0") + (last.offsetWidth || 0)}px`;
      mark.style.top = last.style.top || "0";
    }
    line.append(mark);
  }
}

let navMode = "headings";

function syncNav() {
  const list = document.querySelector("#nav-list");
  if (!list || !document.body.classList.contains("nav-on")) return;
  list.replaceChildren();
  if (navMode === "pages") {
    const count = viewer.pageCount || viewer.pageEls.length || 1;
    for (let i = 0; i < count; i += 1) {
      const item = document.createElement("li");
      const button = document.createElement("button");
      button.type = "button";
      button.textContent = `Page ${i + 1}`;
      button.addEventListener("click", () => {
        viewer.pageEls[i]?.wrap.scrollIntoView({ block: "start", behavior: "smooth" });
      });
      item.append(button);
      list.append(item);
    }
    return;
  }
  if (navMode === "results") {
    if (!finds.length) {
      const empty = document.createElement("li");
      empty.className = "wc-empty";
      empty.textContent = "No results. Use Find.";
      list.append(empty);
      return;
    }
    finds.forEach((hit, index) => {
      const item = document.createElement("li");
      const button = document.createElement("button");
      button.type = "button";
      button.textContent = hit.text || `Result ${index + 1}`;
      button.addEventListener("click", () => {
        findAt = index;
        paintFind();
        revealFind();
      });
      item.append(button);
      list.append(item);
    });
    return;
  }
  let outline = [];
  try {
    outline = JSON.parse(viewer.viewer.outline?.() || "[]");
  } catch {
    outline = [];
  }
  const query = (document.querySelector("#nav-search")?.value || "").trim().toLowerCase();
  const items = outline.filter((entry) => !query || (entry.title || "").toLowerCase().includes(query));
  if (!items.length) {
    const empty = document.createElement("li");
    empty.className = "wc-empty";
    empty.textContent = outline.length ? "No matching headings." : "No headings in this document.";
    list.append(empty);
    return;
  }
  for (const entry of items) {
    const item = document.createElement("li");
    item.dataset.level = String(entry.level || 1);
    const button = document.createElement("button");
    button.type = "button";
    button.textContent = entry.title || "Heading";
    button.addEventListener("click", () => {
      const title = entry.title || "";
      if (!title || !viewer.viewer.search) return;
      try {
        const result = JSON.parse(viewer.viewer.search(title));
        const hit = (result.hits || [])[0];
        if (!hit) return;
        findAt = 0;
        finds = result.hits || [];
        paintFind();
        revealFind();
      } catch {
        /* ignore */
      }
    });
    item.append(button);
    list.append(item);
  }
}

function nudgeSize(delta) {
  run(() => viewer.viewer.editFontStep(delta > 0));
}

function copySelection() {
  if (!state || !state.selection) return;
  navigator.clipboard.writeText(state.selection).catch(() => {});
}

function cutSelection() {
  if (!state || !state.selection) return;
  const text = state.selection;
  run(() => viewer.viewer.editDelete(false)).then(() => {
    navigator.clipboard.writeText(text).catch(() => {});
  });
}

document.querySelector("#act-paste").addEventListener("click", async () => {
  try {
    const text = await navigator.clipboard.readText();
    if (text) run(() => viewer.viewer.editInsert(text));
  } catch {
    showError("Press Ctrl+V to paste.");
  }
});

const tablePick = document.querySelector("#table-pick");
const tableSize = document.querySelector("#table-size");
if (tablePick) {
  for (let r = 1; r <= 8; r += 1) {
    for (let c = 1; c <= 8; c += 1) {
      const cell = document.createElement("span");
      cell.dataset.r = String(r);
      cell.dataset.c = String(c);
      cell.addEventListener("mouseenter", () => {
        tablePick.querySelectorAll("span").forEach((node) => {
          node.classList.toggle("is-on", Number(node.dataset.r) <= r && Number(node.dataset.c) <= c);
        });
        tableSize.textContent = `${r} × ${c}`;
        document.querySelector("#table-rows").value = String(r);
        document.querySelector("#table-cols").value = String(c);
      });
      cell.addEventListener("click", () => {
        run(() => viewer.viewer.editTable(r, c));
      });
      tablePick.append(cell);
    }
  }
  tablePick.addEventListener("mouseleave", () => {
    const rows = Number(document.querySelector("#table-rows").value) || 3;
    const cols = Number(document.querySelector("#table-cols").value) || 3;
    tablePick.querySelectorAll("span").forEach((node) => {
      node.classList.toggle(
        "is-on",
        Number(node.dataset.r) <= rows && Number(node.dataset.c) <= cols,
      );
    });
    tableSize.textContent = `${rows} × ${cols}`;
  });
}
document.querySelector("#act-table").addEventListener("click", () => {
  const rows = Number(document.querySelector("#table-rows").value);
  const cols = Number(document.querySelector("#table-cols").value);
  run(() => viewer.viewer.editTable(rows, cols));
});
document.querySelector("#act-blank-page").addEventListener("click", () => {
  run(() => viewer.viewer.editInsert("\f"));
});
document.querySelector("#act-picture").addEventListener("click", () => document.querySelector("#picture-file").click());
document.querySelector("#act-address-block").addEventListener("click", async () => {
  for (const name of ["Customer", "Address", "City", "Country"]) {
    await run(() => viewer.viewer.editMergeField(name));
    await run(() => viewer.viewer.editEnter(false));
  }
});
document.querySelector("#act-greeting").addEventListener("click", async () => {
  await run(() => viewer.viewer.editInsert("Dear "));
  await run(() => viewer.viewer.editMergeField("Customer"));
  await run(() => viewer.viewer.editInsert(","));
});
document.querySelector("#act-highlight-fields").addEventListener("click", (event) => {
  document.body.classList.toggle("highlight-fields");
  const on = document.body.classList.contains("highlight-fields");
  event.currentTarget.classList.toggle("is-on", on);
  event.currentTarget.setAttribute("aria-pressed", String(on));
  if (on) paintFieldHits(state?.fields || rememberedFields);
  else {
    for (const el of viewer.pageEls || []) {
      el.wrap.querySelectorAll(".wc-field-hit").forEach((node) => node.remove());
    }
  }
});
document.querySelector("#toggle-fields-pane").addEventListener("click", (event) => {
  document.body.classList.toggle("fields-off");
  const on = !document.body.classList.contains("fields-off");
  event.currentTarget.classList.toggle("is-on", on);
  event.currentTarget.setAttribute("aria-pressed", String(on));
  syncRuler();
});
document.querySelector("#toggle-nav-pane").addEventListener("click", (event) => {
  document.body.classList.toggle("nav-on");
  const on = document.body.classList.contains("nav-on");
  if (!on) navAutoDismissed = true;
  event.currentTarget.classList.toggle("is-on", on);
  event.currentTarget.setAttribute("aria-pressed", String(on));
  syncNav();
  syncRuler();
});
document.querySelectorAll("[data-nav]").forEach((button) => {
  button.addEventListener("click", () => {
    navMode = button.dataset.nav;
    document.querySelectorAll("[data-nav]").forEach((node) => {
      const selected = node === button;
      node.classList.toggle("is-on", selected);
      node.setAttribute("aria-selected", String(selected));
    });
    syncNav();
  });
});
document.querySelector("#nav-search")?.addEventListener("input", () => syncNav());

function setPrintLayout() {
  document.body.classList.remove("web-layout", "is-focus");
  document.querySelector("#view-print")?.classList.add("is-on");
  document.querySelector("#view-web")?.classList.remove("is-on");
  document.querySelector("#status-print")?.classList.add("is-on");
  document.querySelector("#status-web")?.classList.remove("is-on");
  document.querySelector("#status-focus")?.classList.remove("is-on");
  viewer.setZoom(1);
}
function setWebLayout() {
  document.body.classList.add("web-layout");
  document.body.classList.remove("is-focus");
  document.querySelector("#view-web")?.classList.add("is-on");
  document.querySelector("#view-print")?.classList.remove("is-on");
  document.querySelector("#status-web")?.classList.add("is-on");
  document.querySelector("#status-print")?.classList.remove("is-on");
  document.querySelector("#status-focus")?.classList.remove("is-on");
  viewer.fit("width");
}
function setFocusLayout() {
  document.body.classList.add("is-focus");
  document.body.classList.remove("web-layout");
  document.querySelector("#status-focus")?.classList.add("is-on");
  document.querySelector("#status-print")?.classList.remove("is-on");
  document.querySelector("#status-web")?.classList.remove("is-on");
  viewer.fit("width");
}
document.querySelector("#view-print").addEventListener("click", setPrintLayout);
document.querySelector("#view-web").addEventListener("click", setWebLayout);
document.querySelector("#status-print").addEventListener("click", setPrintLayout);
document.querySelector("#status-web").addEventListener("click", setWebLayout);
document.querySelector("#status-focus").addEventListener("click", setFocusLayout);
document.querySelector("#view-zoom-in").addEventListener("click", () => viewer.zoomBy(1));
document.querySelector("#view-zoom-out").addEventListener("click", () => viewer.zoomBy(-1));
document.querySelector("#picture-file").addEventListener("change", async () => {
  const file = document.querySelector("#picture-file").files[0];
  document.querySelector("#picture-file").value = "";
  if (!file) return;
  if (file.type !== "image/png" && file.type !== "image/jpeg") {
    showError("Choose a PNG or a JPEG.");
    return;
  }
  const bytes = new Uint8Array(await file.arrayBuffer());
  const image = new Image();
  image.src = URL.createObjectURL(file);
  try {
    await image.decode();
  } catch {
    showError("The picture could not be read.");
    return;
  }
  const width = Math.min(240, Math.max(48, image.naturalWidth * 0.75));
  const height = width * (image.naturalHeight / Math.max(1, image.naturalWidth));
  URL.revokeObjectURL(image.src);
  run(() => viewer.viewer.editPicture(bytes, width, height));
});
document.querySelector("#act-break").addEventListener("click", () => {
  run(() => viewer.viewer.editInsert("\f"));
});

document.querySelector("#field-form").addEventListener("submit", (event) => {
  event.preventDefault();
  const name = document.querySelector("#field-name").value.trim();
  if (!name) {
    showError("Type a field name first.");
    return;
  }
  run(() => viewer.viewer.editMergeField(name));
});

function previewRecord() {
  const values = {};
  previewForm.querySelectorAll("[data-field]").forEach((input) => {
    values[input.dataset.field] = input.value;
  });
  run(() => viewer.viewer.editPreview(values));
}
document.querySelector("#act-preview").addEventListener("click", previewRecord);
function endPreview() {
  run(() => viewer.viewer.editEndPreview());
}
document.querySelector("#act-end-preview").addEventListener("click", endPreview);
document.querySelector("#banner-end").addEventListener("click", endPreview);

viewer.viewport.addEventListener("scroll", () => syncRuler(), { passive: true });
zoom.addEventListener("input", () => {
  viewer.setZoom(Number(zoom.value) / 100);
  syncZoom();
  paintCaret();
});
document.querySelector("#fit-width").addEventListener("click", () => viewer.fit("width"));
document.querySelector("#fit-page").addEventListener("click", () => viewer.fit("page"));
document.querySelector("#zoom-100").addEventListener("click", () => viewer.setZoom(1));
document.querySelector("#zoom-readout").addEventListener("click", () => viewer.setZoom(1));
document.querySelector("#zoom-in").addEventListener("click", () => viewer.zoomBy(1));
document.querySelector("#zoom-out").addEventListener("click", () => viewer.zoomBy(-1));
document.querySelector("#toggle-rulers").addEventListener("click", (event) => {
  document.body.classList.toggle("rulers-off");
  const on = !document.body.classList.contains("rulers-off");
  event.currentTarget.classList.toggle("is-on", on);
  event.currentTarget.setAttribute("aria-pressed", String(on));
});
const findBox = document.querySelector("#find");
const replaceBox = document.querySelector("#replace");
findBox.addEventListener("input", () => {
  findAt = 0;
  refreshFind();
  paintFind();
  revealFind();
});
findBox.addEventListener("keydown", (event) => {
  if (event.key !== "Enter") return;
  event.preventDefault();
  if (!finds.length) return;
  if (event.altKey) {
    replaceCurrent();
    return;
  }
  const step = event.shiftKey ? -1 : 1;
  findAt = (findAt + step + finds.length) % finds.length;
  paintFind();
  revealFind();
});
replaceBox.addEventListener("keydown", (event) => {
  if (event.key !== "Enter") return;
  event.preventDefault();
  replaceCurrent();
});
document.querySelector("#act-find-focus").addEventListener("click", () => {
  closeMenus();
  findBox.focus();
  findBox.select();
});
document.querySelector("#act-replace").addEventListener("click", () => {
  closeMenus();
  document.body.classList.add("show-replace");
  replaceBox.focus();
});

function replaceCurrent() {
  const query = findBox.value;
  const next = replaceBox.value;
  if (!query || findAt < 0 || !finds[findAt]) return;
  const hit = finds[findAt];
  const page = viewer.pageEls[hit.page];
  if (!page) return;
  const rect = (hit.rects && hit.rects[0]) || null;
  if (!rect) return;
  const x = rect[0] + rect[2] / 2;
  const y = rect[1] + rect[3] / 2;
  run(() => viewer.viewer.editCaret(hit.page, x, y, false))
    .then(() => run(() => viewer.viewer.editSelectWord()))
    .then(() => {
      if (!state || !state.selection) return;
      return run(() => viewer.viewer.editInsert(next));
    })
    .then(() => {
      findAt = 0;
      refreshFind();
      paintFind();
      revealFind();
    });
}

viewer.viewport.addEventListener("mousedown", (event) => {
  const point = pointOf(event);
  if (!point || event.button !== 0) return;
  event.preventDefault();
  viewer.viewport.focus({ preventScroll: true });
  dragging = true;
  closeMenus();
  const clicks = event.detail;
  run(() => viewer.viewer.editCaret(point.page, point.x, point.y, event.shiftKey)).then(() => {
    if (clicks >= 3) run(() => viewer.viewer.editSelectParagraph());
    else if (clicks === 2) run(() => viewer.viewer.editSelectWord());
  });
});
window.addEventListener("mousemove", (event) => {
  if (!dragging) return;
  const point = pointOf(event);
  if (!point) return;
  if (dragFrame) return;
  dragFrame = requestAnimationFrame(() => {
    dragFrame = 0;
    run(() => viewer.viewer.editCaret(point.page, point.x, point.y, true));
  });
});
window.addEventListener("mouseup", () => {
  dragging = false;
});

let composing = false;
viewer.viewport.addEventListener("compositionstart", () => {
  composing = true;
});
viewer.viewport.addEventListener("compositionend", (event) => {
  composing = false;
  if (event.data) run(() => viewer.viewer.editInsert(event.data));
});
viewer.viewport.addEventListener("paste", (event) => {
  const text = event.clipboardData && event.clipboardData.getData("text/plain");
  if (!text) return;
  event.preventDefault();
  run(() => viewer.viewer.editInsert(text));
});
viewer.viewport.addEventListener("copy", (event) => {
  if (!state || !state.selection) return;
  event.clipboardData.setData("text/plain", state.selection);
  event.preventDefault();
});

window.addEventListener("keydown", (event) => {
  if (typingTarget(event.target) || composing || event.isComposing) return;
  const meta = event.metaKey || event.ctrlKey;
  const key = event.key;
  if (meta && key.toLowerCase() === "b") return command(event, () => viewer.viewer.editMark("bold"));
  if (meta && key.toLowerCase() === "i") return command(event, () => viewer.viewer.editMark("italic"));
  if (meta && key.toLowerCase() === "u") return command(event, () => viewer.viewer.editMark("underline"));
  if (meta && key.toLowerCase() === "a") return command(event, () => viewer.viewer.editSelectAll());
  if (meta && key.toLowerCase() === "f") {
    event.preventDefault();
    findBox.focus();
    findBox.select();
    return;
  }
  if (meta && event.shiftKey && key.toLowerCase() === "z") return command(event, () => viewer.viewer.editRedo());
  if (meta && key.toLowerCase() === "z") return command(event, () => viewer.viewer.editUndo());
  if (meta && key.toLowerCase() === "y") return command(event, () => viewer.viewer.editRedo());
  if (meta && key.toLowerCase() === "s") {
    event.preventDefault();
    document.querySelector("#act-save").click();
    return;
  }
  if (key === "Enter") return command(event, () => viewer.viewer.editEnter(event.shiftKey));
  if (key === "Backspace") return command(event, () => viewer.viewer.editDelete(true));
  if (key === "Delete") return command(event, () => viewer.viewer.editDelete(false));
  const move = {
    ArrowLeft: meta ? "word-left" : "left",
    ArrowRight: meta ? "word-right" : "right",
    ArrowUp: "up",
    ArrowDown: "down",
    Home: meta ? "doc-start" : "home",
    End: meta ? "doc-end" : "end",
  }[key];
  if (move) return command(event, () => viewer.viewer.editMove(move, event.shiftKey));
  if (meta || event.altKey || key.length !== 1) return;
  command(event, () => viewer.viewer.editInsert(key));
});

function command(event, invoke) {
  event.preventDefault();
  run(invoke);
}

const requested = new URLSearchParams(location.search).get("doc");
const initial = requested
  ? fetch(requested.startsWith("/") ? requested : `../assets/documents/${requested}`).then(async (response) => {
      if (!response.ok) throw new Error("The sample did not load.");
      const bytes = new Uint8Array(await response.arrayBuffer());
      return viewer.editOpenBytes(bytes);
    })
  : viewer.editBlank();

queue = initial
  .then((opened) => {
    state = opened;
    render();
    viewer.viewport.focus({ preventScroll: true });
  })
  .catch((err) => {
    showError(err && err.message ? err.message : "The document did not open.");
  });
