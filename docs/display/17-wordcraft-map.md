# The WordCraft map: what to copy, what to change, what not to copy.

> Part of the EdgeDoc Display specification. Date: 2026-11-04. Requirement ids are normative.
> See [the index](README.md).

## 117. The reference checkout

WordCraft is the reference implementation for this library. It is a Rust word processor at
version `0.14.0`, and it already computes Word page breaks, shapes text, exports PDF, and
runs in a browser. This clause maps each display crate to the WordCraft module that proves
the behavior, and it says what to change.

Read a cited file before you write the module that it maps to. Copy the algorithm. Do not
copy the editor around it.

| Field | Value |
|---|---|
| Checkout | `/Users/raphaelmansuy/Github/03-working/wordcraft` |
| Version | `0.14.0` |
| Edition | 2024, `rust-version = 1.95` |
| `crates/layout` | 28 modules, 37,327 lines including tests |
| Parity document | `docs/layout-parity.md` |
| Layer rule | `xtask/src/layers.rs` |
| Measurement tools | `xtask/src/visual_diff.rs`, `xtask/src/stats.rs` |
| Licence of copied ideas | MIT OR Apache-2.0, the same as EdgeDoc |

`DP-N-07`. A feature that WordCraft proves with a test in `docs/layout-parity.md` is a
feature we can port with confidence. A row marked ❌ or a gap in that document is a feature
we must design ourselves, and the row becomes a risk in [15-plan.md](15-plan.md).

## 118. The layer rule, ported

WordCraft enforces layering with a table and a check:

```42:58:/Users/raphaelmansuy/Github/03-working/wordcraft/xtask/src/layers.rs
pub const TABLE: &[(&str, Class)] = &[
    ("geom", Class::Layer(0)),
    ("control-key", Class::Standalone),
    ("doc", Class::Layer(1)),
    ("fonts", Class::Layer(1)),
    ("metafile", Class::Layer(1)),
    ("proof", Class::Layer(1)),
    ("layout", Class::Layer(2)),
    ("docx", Class::Layer(2)),
    ("docbin", Class::Layer(2)),
    ("formats", Class::Layer(2)),
    ("render", Class::Layer(3)),
    ("pdf", Class::Layer(3)),
    ("engine", Class::Layer(4)),
    ...
];
```

`DP-A-28`. EdgeDoc Display copies this mechanism. `xtask/src/layers.rs` in this repository
holds a `TABLE` of the display crates, and `cargo xtask layers` fails on a reverse edge. The
table below states the EdgeDoc form of it. The levels match the WordCraft levels, so a reader
can compare the two graphs side by side.

| EdgeDoc Display crate | WordCraft crate | Level |
|---|---|---|
| `edgedoc-geom` | `wordcraft-geom` | L0 |
| `edgedoc-fonts` | `wordcraft-fonts` | L1 |
| `edgedoc-text` | part of `wordcraft-layout::para` | L1 |
| `edgedoc-style` | part of `wordcraft-doc::resolve` | L1 |
| `edgedoc-flow` | `wordcraft-layout` | L2 |
| `edgedoc-paint` | `wordcraft-layout::display` plus `wordcraft-render` | L2 |
| `edgedoc-display` | `wordcraft-engine` (the session that owns layout) | L3 |
| `edgedoc-display-wasm` | `apps/wordcraft-web` | L4 |
| `edgedoc-display-pdf` | `wordcraft-pdf` | L3 |

`DP-A-29`. The split differs in one place, on purpose. WordCraft keeps line breaking inside
`crates/layout/src/para`, and the paragraph layout type is shared by the flow code. EdgeDoc
Display separates `edgedoc-text` from `edgedoc-flow`, because the line breaker has one caller
contract (`DP-A-20`) and the flow crate has another. The split lets a test break a line with
no page.

## 119. Crate by crate

### 119.1 `edgedoc-geom` from `wordcraft-geom`

```1:14:/Users/raphaelmansuy/Github/03-working/wordcraft/crates/geom/src/lib.rs
//! WordCraft geometry and units.
//!
//! Layout works in **points** (1/72 inch, `f32`). File formats use other units: OOXML uses
//! twentieths of a point (twips) for most lengths, half-points for font sizes, eighths of a point
//! for borders and English Metric Units (EMU, 914 400 per inch) for drawings. The conversions live
//! here so every crate agrees on them.
```

| Copy | Change |
|---|---|
| Points as the layout unit, `f32` | Nothing. This decision is settled and measured |
| `TWIPS_PER_PT = 20`, `EMU_PER_PT = 12_700` in one place | Nothing |
| `Rect`, `Point`, `Region`, `Spin` | Re-export `kurbo` instead of a second rectangle type, if `kurbo` fits the WASM budget |
| `pixels.rs` decode budgets | Keep. See 119.7 |

`DP-A-30`. The pixel budgets are part of geometry, not of the image decoder:

```5:9:/Users/raphaelmansuy/Github/03-working/wordcraft/crates/geom/src/pixels.rs
/// Bytes a decoded picture may take as RGBA8 (what the `image` crate's default decode budget is).
pub const MAX_DECODED_BYTES: u64 = 512 * 1024 * 1024;

/// Most pixels one page raster may hold (400 MB of RGBA8); larger requests are scaled down.
pub const MAX_RASTER_PIXELS: u64 = 100_000_000;
```

EdgeDoc Display uses 256 MiB for `MAX_DECODED_BYTES`, because the browser tab must hold the
model, the pages, and the tiles at the same time. The `fit_pixels` shrink rule is copied as
it stands: over the cap the picture draws smaller, and it is not cropped.

### 119.2 `edgedoc-fonts` from `wordcraft-fonts`

WordCraft answers the shaping question for us, and the answer is not `rustybuzz`:

| Crate | Role | Copy |
|---|---|---|
| `harfrust` 0.13 | Shaping, the Rust port of HarfBuzz | Yes |
| `skrifa` 0.47 | Font parsing, metrics, outlines, variations | Yes |
| `wuff` 0.2.9 | WOFF and WOFF2 decode | Yes, when a document embeds a web font |
| `unicode-bidi`, `unicode-linebreak`, `unicode-segmentation` | Unicode algorithms | Yes |

```172:186:/Users/raphaelmansuy/Github/03-working/wordcraft/crates/fonts/src/lib.rs
pub fn shape(face: &FontFace, text: &str, features: &[Feature], map: impl Fn(char) -> char) -> Vec<ShapedGlyph> {
    if !text.chars().any(is_rtl) {
        return shape_dir(face, text, features, &map, false);
    }
    let mut out = Vec::with_capacity(text.len());
    for (r, rtl) in direction_runs(text) {
        let Some(sub) = text.get(r.clone()) else { continue };
        let mut g = shape_run(face, sub, features, &map, rtl);
        for x in &mut g {
            x.cluster += r.start;
        }
        out.extend(g);
    }
    out
}
```

`DP-F-45`. `Shaper` in EdgeDoc Display wraps `harfrust` with the WordCraft call shape above.
It holds a fast path for a run with no right-to-left character, and a per-direction-run path
for every other run. The `map` closure is where the symbol-font substitution of `DP-F-35` and
the small-caps mapping sit, so the shaper never learns about Word.

WordCraft also settles font substitution with a table and a test suite:

```385:385:/Users/raphaelmansuy/Github/03-working/wordcraft/crates/fonts/src/word.rs
pub fn resolve(family: &str, bold: bool, italic: bool) -> Resolved {
```

| Copy from `wordcraft-fonts` | Change |
|---|---|
| `word::resolve` and its named substitutes: Calibri→Carlito, Cambria→Caladea, Arial→Liberation Sans | Keep the table. Add: a document font that no face provides keeps the substitute metrics for line breaking (`DP-F-04`) |
| `symbol_font_metrics` and `symbol_line`: a symbol font keeps its own line height | Keep. It is the difference between one wrong line height on every bullet line and a correct page |
| One substitute face for a whole Thai run, so the run keeps one font at one size | Keep the rule. It matters for any complex script, not only Thai |
| `MAX_FONT_FILE_BYTES` at 50 MiB | Lower to 8 MiB in the browser, because a face fetch has a 4 MiB limit (`DP-B-21`) |
| The `ui_fallback` and `script` modules | Drop. A viewer has no user interface font |

`DP-F-46`. WordCraft records the exact defect that it fixed in `docs/layout-parity.md`. Without
`w:kern`, Word must not kern. WordCraft kerned every run, so its lines held more text than
Word's. EdgeDoc Display therefore keys kerning on `w:kern` and its size threshold, and the
test `kerning_follows_w_kern` locks it. A port must carry this class of defect across. A
substituted font with the right metrics is still wrong when the feature flags differ.

### 119.3 `edgedoc-text` from `wordcraft-layout::para`

This is the most valuable module to port, and the most delicate.

```1:2:/Users/raphaelmansuy/Github/03-working/wordcraft/crates/layout/src/para.rs
//! Paragraph layout: resolve runs, shape them into clusters, break lines (first-fit, as word
//! processors do), place tabs, list labels and alignment.
```

`DP-F-47`. **The line breaker is first-fit, not Knuth–Plass.** This corrects an earlier
draft of this specification. WordCraft measured the behavior against Word and records the
result in the module comment above. A word processor fills the line with the next item that
fits and breaks there. A global optimum would place a break at a different word on a
justified paragraph, and the page break would then differ.

`DP-F-48`. `DP-F-25` is withdrawn. The text crate uses a forward scanner with a
`break_after` flag per cluster, exactly as WordCraft does. The cost of raggedness is not
computed, and the paragraph is not re-broken to balance the lines.

The data model to port:

```96:101:/Users/raphaelmansuy/Github/03-working/wordcraft/crates/layout/src/para/model.rs
pub struct Glyph {
    pub gid: u32,
    /// Offset from the cluster's x, points.
    pub dx: f32,
    pub dy: f32,
}
```

```120:138:/Users/raphaelmansuy/Github/03-working/wordcraft/crates/layout/src/para/model.rs
pub struct Cluster {
    pub start: usize,
    pub end: usize,
    pub adv: f32,
    pub kind: ClKind,
    /// Index into `ParaLayout::styles`.
    pub style: u32,
    /// Glyph range in `ParaLayout::glyphs`.
    pub g0: u32,
    pub g1: u32,
    /// A line may break after this cluster.
    pub break_after: bool,
    /// Height above the baseline for objects (images), points.
    pub obj_h: f32,
    /// Depth below the baseline for objects (equations), points.
    pub obj_d: f32,
    /// The cluster is a decimal separator (decimal tabs align on it).
    pub dot: bool,
}
```

| WordCraft decision | Why it is right | Requirement |
|---|---|---|
| A `Cluster` is a grapheme, never a glyph, and it owns the glyph range | A ligature is one cluster with 3 glyphs, so a caret never lands inside `ffi` | `DP-F-09`, `DP-F-10` |
| `adv` lives on the cluster, `dx` on the glyph | Justification moves clusters, not glyphs | `DP-F-28` |
| `break_after` is a flag on the cluster, not a list of positions | One array, one pass, no second lookup | `DP-F-22` |
| `ClKind` distinguishes `Space`, `Tab`, `LineBreak`, `PageBreak`, `ColumnBreak`, `Object`, `Marker` | Justification, tabs, and pagination each need one of these and must not guess from the text | `DP-F-28`, `DP-F-32`, `DP-L-06` |
| `obj_h` and `obj_d` sit on the cluster | An inline picture takes line height with no second pass | `DP-L-33` |
| `dot` marks a decimal separator | A decimal tab aligns on it | `DP-F-32` |

```154:179:/Users/raphaelmansuy/Github/03-working/wordcraft/crates/layout/src/para/model.rs
pub struct Line {
    pub top: f32,
    pub height: f32,
    pub baseline: f32,
    /// The text's own ascent and descent on this line, without line-spacing leading (the caret's extent).
    pub text_asc: f32,
    pub text_desc: f32,
    /// Cluster index range.
    pub c0: usize,
    pub c1: usize,
    /// x of each cluster in the line, plus the end x (len = c1 - c0 + 1).
    pub xs: Vec<f32>,
    /// For tab clusters: the leader to draw across them.
    pub leaders: Vec<(usize, TabLeader)>,
    pub end: LineEnd,
    /// Byte range of the paragraph covered by the line.
    pub start: usize,
    pub stop: usize,
    /// Left and right edges available to the line.
    pub left: f32,
    pub right: f32,
    /// The line ends at a hyphenation point: (style, glyph, advance) of the hyphen drawn there.
    pub hyphen: Option<(u32, u32, f32)>,
    /// The text continues the previous line's row on the far side of a floating object (same top
    /// and height): one row, two lines. Row counts (line numbers, keeping rows on one page) skip it.
    pub beside: bool,
```

`DP-F-49`. Three fields in `Line` answer three requirements that this specification would
otherwise have had to invent:

- `xs` holds one x per cluster, so a hit test and a selection rectangle are a binary search.
  This replaces the "glyph bounds" path of `DP-G-28`.
- `beside` marks a line that continues a row on the other side of a float. Without it, a
  line number and a widow rule count one row twice. This is the fix for `DP-X-119` and
  `DP-L-29`.
- `hyphen` carries the style, glyph, and advance of the hyphen that paints at the break, so
  the painter needs no rule (`DP-F-36`).

`DP-F-50`. The break loop is a state machine over one paragraph, not a loop of nested
conditions:

```148:166:/Users/raphaelmansuy/Github/03-working/wordcraft/crates/layout/src/para/lines.rs
pub(super) fn break_lines(pl: &mut ParaLayout, env: &ParaEnv, mark_style: u32, level: Option<&Level>) {
    let width = env.width.max(MIN_LINE_WIDTH);
    let mirrored = pl.rp.bidi.then(|| mirrored_exclusions(env.exclusions, width));
    let exclusions = mirrored.as_deref().unwrap_or(env.exclusions);
    let mut bk = Breaker::new(pl, (env, exclusions), mark_style, level);
    loop {
        let mut sc = bk.begin_line(pl);
        sc.scan(&mut bk, pl);
        if sc.skip {
            bk.skip_row(&sc);
            continue;
        }
        let end = bk.close_line(pl, sc);
        if bk.next_line(pl, end) {
            break;
        }
    }
    bk.finish(pl);
}
```

`DP-F-51`. `MIN_LINE_WIDTH` is 1.0 pt, and every width clamps to it. A zero-width column
therefore makes progress instead of looping. This is the guard for `DP-X-34`.

`DP-F-52`. Exclusions (the wrap areas of floating objects) arrive as a slice that the
breaker reads per line. A right-to-left paragraph mirrors them once, before the loop, so the
scanner has no direction branch. EdgeDoc Display keeps both decisions.

### 119.4 `edgedoc-style` from `wordcraft-doc::resolve`

| WordCraft location | Copy | Change |
|---|---|---|
| `crates/doc/src/resolve.rs`, `ResolvedPara` and `ResolvedChar` | The cascade order and the resolved record | Nothing |
| `crates/doc/src/numbering.rs`, `Counters`, `Level` | Counters keyed by `numId` | Nothing (`DP-S-14`) |
| `crates/doc/src/cjk_numbers.rs` | The CJK counting formats | Nothing |
| `crates/doc/src/symbol_fonts.rs` | The Symbol and Wingdings tables | Nothing |
| `crates/doc/src/section.rs` | Section properties and note options | Keep, and add `Section` with its block range (`DP-S-23`) |
| `styles.resolve_char_in` (style, table style, direct) | The table-cell layering | Nothing (`DP-S-05`) |

`DP-S-29`. The resolved record is a plain value, not a reference chain: `ResolvedPara` and
`ResolvedChar` in WordCraft hold numbers and small enums. EdgeDoc Display clones them into
every fragment, as `DP-S-09` requires.

`DP-S-30`. WordCraft measures several numbering rules that this specification listed only as
"a test locks it", and the measurements go into the port:

| Measured behavior | Source |
|---|---|
| A level that has not counted yet takes its first number at the first deeper item, so after 10.1–10.4 the next level-1 item is 11 | `doc::numbering::tests::a_deeper_item_first_counts_the_levels_above_it` |
| `arabicAlpha` and `arabicAbjad` draw the 28 Arabic letters, then double (أأ, بب) | `doc::section::tests::arabic_letters_count_in_both_orders` |
| Russian letters skip ё, й, ъ and ь, then double | `doc::section::tests::russian_letters_follow_word` |
| A label mixing faces draws run by run | `layout::tests::chinese_counting_list_labels_and_their_font_runs` |
| A bullet that no font can draw shows the level's default bullet, not another font's private-use glyph | `layout::tests::private_use_bullet_no_font_draws_shows_the_levels_default_bullet` |
| An empty paragraph with a hidden mark draws nothing, and its number is still counted | `layout::tests::empty_list_paragraph_with_a_hidden_mark_counts_but_shows_nothing` |

### 119.5 `edgedoc-flow` from `wordcraft-layout`

WordCraft produces the whole document in one call, and the caller keeps the result:

```309:309:/Users/raphaelmansuy/Github/03-working/wordcraft/crates/layout/src/lib.rs
pub fn layout(doc: &Document, cache: &mut LayoutCache, opts: &LayoutOptions) -> DocLayout {
```

```267:294:/Users/raphaelmansuy/Github/03-working/wordcraft/crates/layout/src/lib.rs
pub struct Page {
    pub w: f32,
    pub h: f32,
    /// Index into `Document::sections()`.
    pub section: usize,
    /// Page number as displayed (after section restarts).
    pub number: u32,
    pub items: Vec<Placed>,
    /// Header / footer items (drawn dimmed while editing the body).
    pub header: Vec<Placed>,
    pub footer: Vec<Placed>,
    /// The body text area (margins).
    pub body: Rect,
    pub header_story: Option<u32>,
    pub footer_story: Option<u32>,
    ...
```

`DP-L-51`. **This corrects `DP-A-07`.** WordCraft's layout is a whole-document function. It
must work that way, because a page total and a note number are not known until the last page.
The EdgeDoc facade therefore does not promise a page before the document is laid out. It
promises the opposite, and the promise is honest:

1. `open` is fast (model and styles only).
2. `layout` runs the whole pass in the worker.
3. Each page leaves through the sink as the pass produces it.
4. A later pass (page totals, note renumbering, float rewrapping) re-emits a page only when
   its geometry changed.

The viewer paints page 1 as soon as pass 1 produces it, which is what `DP-I-16` measures. A
document with no page-total field and no per-page note restart takes one pass, and the
measurement applies. A document with `NUMPAGES` takes up to three passes, and the report says
so.

`DP-L-52`. The multi-pass loop is the reference for the state machine in
[04-pipeline.md](04-pipeline.md):

```311:327:/Users/raphaelmansuy/Github/03-working/wordcraft/crates/layout/src/lib.rs
    let per_page = opts.view == ViewMode::Print
        && doc.sections().iter().any(|(_, s)| doc.note_options(s, false).num_restart == wordcraft_doc::section::NoteRestart::EachPage);
    let mut note_pages = HashMap::new();
    let mut note_passes = 1;
    let mut totals = Totals::first_guess(doc.sections().len());
    let mut passes = 0;
    let (mut early, mut rewraps) = (EarlyFloats::new(), 0);
    let mut l = loop {
        match layout_pass(doc, cache, opts, &note_pages, &totals, passes < MAX_TOTAL_PASSES, (&early, rewraps < MAX_REWRAP_PASSES)) {
            Pass::Again(t) => {
                totals = t;
                passes += 1;
            }
            Pass::Rewrap(found) => {
```

| Pass | Reason | Cap |
|---|---|---|
| Totals | `NUMPAGES` and `SECTIONPAGES` are known only after pagination | `MAX_TOTAL_PASSES = 3` |
| Note numbering | `numRestart="eachPage"` numbers from where the notes land | `MAX_NOTE_PASSES = 3` |
| Float rewrap | A float placed above its anchor changes the text above it | `MAX_REWRAP_PASSES = 3` |

`DP-L-53`. A cap exists for each loop, and the loop terminates on the cap with a diagnostic.
An uncapped fixpoint loop on a crafted document is a hang, and a hang in a browser tab is
worse than a wrong page break.

`DP-L-54`. The first guess for the total is 1 page per section:

```150:152:/Users/raphaelmansuy/Github/03-working/wordcraft/crates/layout/src/cache.rs
    pub(crate) fn first_guess(sections: usize) -> Totals {
        Totals { pages: 1, sections: vec![1; sections] }
    }
```

The guess is low on purpose. A page total that comes out lower than the guess shortens the
document, and a reader sees the scrollbar shrink once. A guess that is too high would make
the layout longer than the document and leave a blank tail.

The page model to port:

```3:12:/Users/raphaelmansuy/Github/03-working/wordcraft/crates/layout/src/placed.rs
/// Something placed on a page (page coordinates, points, y down).
#[derive(Clone, Debug)]
pub enum Placed {
    /// Lines `l0..l1` of a paragraph; `y` is the top of line `l0`; `x` the column's left edge.
    /// Turned lines (a table cell's text direction) run as `turn` says from the page point
    /// (`x`, `y`): see [`turn_point`].
    Lines {
        story: StoryRef,
        path: Path,
        para: Arc<ParaLayout>,
        l0: usize,
        l1: usize,
        x: f32,
        y: f32,
        turn: TextDirection,
    },
```

`DP-L-55`. `Placed::Lines` holds a range of lines in a shared `Arc<ParaLayout>` and a
position. It does not hold glyph arrays. A paragraph split over 20 pages therefore holds one
`ParaLayout` and 20 small ranges. EdgeDoc Display takes the same shape for
`ParagraphFragment`, and it is the reason a page geometry is cheap to clone (`DP-V-06`).

`DP-L-56`. The variant list is the fragment list of `DP-L-44` with two additions that the
port needs. `Cell` serves hit testing and cell selection. `Object` serves a drag handle and a
wrap area. EdgeDoc Display reads both as data and does not implement a drag.

`DP-L-57`. Section side selection is a solved problem in WordCraft. It is subtle, because an
odd-page break can add a blank page only when the document prints both sides:

```1:26:/Users/raphaelmansuy/Github/03-working/wordcraft/crates/layout/src/headers.rs
//! Whether a section that starts on a new page needs a blank page first so it lands on the right
//! side, and the page number it restarts at. `last` is the number of the page before it and
//! `pages` how many pages there are so far.
//! - An odd- or even-page break counts on from the previous section's numbers. A section that
//!   restarts its numbering gets no blank page unless the document prints both sides (different
//!   odd and even headers, or mirror margins); a restart at the wrong parity takes the next number.
//! - With different odd and even headers, a next-page break that restarts the numbers at an odd
//!   (even) number starts on an odd (even) sheet.
pub(crate) fn section_side(start: SectionStart, restart: Option<u32>, last: u32, pages: usize, even_odd: bool, mirror: bool) -> (bool, Option<u32>) {
```

`DP-L-58`. EdgeDoc Display ports `section_side` with its tests. This one function answers
`DP-X-27`, `DP-X-28`, and half of `DP-L-08`, and a hand-written rule gets it wrong on the
case where no blank page appears.

`DP-L-59`. Table fit is a single pure function in WordCraft, and it is the one to port:

```298:311:/Users/raphaelmansuy/Github/03-working/wordcraft/crates/layout/src/table.rs
pub fn autofit_widths(cols: &[(f32, f32)], avail: f32) -> Vec<f32> {
    let clean = |v: f32| if v.is_finite() { v.clamp(0.0, 1584.0) } else { 0.0 };
    let cols: Vec<(f32, f32)> = cols.iter().map(|&(a, b)| (clean(a), clean(b).max(clean(a)))).collect();
    let avail = clean(avail);
    let (sum_min, sum_max) = cols.iter().fold((0.0f32, 0.0f32), |(a, b), c| (a + c.0, b + c.1));
    if sum_max <= avail {
        return cols.iter().map(|c| c.1).collect();
    }
    if sum_min >= avail || sum_max <= sum_min {
        return cols.iter().map(|c| c.0).collect();
    }
    let k = (avail - sum_min) / (sum_max - sum_min);
    cols.iter().map(|c| c.0 + (c.1 - c.0) * k).collect()
}
```

`DP-L-60`. The three guards in `autofit_widths` cover three edge cases at once. A non-finite
width becomes 0. A width past 22 inches clamps. A maximum below the minimum takes the
minimum. This is the pattern for every numeric port: clean, clamp, then decide.

`DP-L-61`. WordCraft caps a text box inside a text box at depth 4, and a document that
expands a box inside itself returns without expanding:

```30:30:/Users/raphaelmansuy/Github/03-working/wordcraft/crates/doc/src/block.rs
pub const MAX_TEXT_BOX_DEPTH: usize = 4;
```

EdgeDoc Display uses 4 for text boxes and 24 for tables, as the two limits answer different
risks. The `BoxBudget` type guards both.

### 119.6 `edgedoc-paint` from `wordcraft-layout::display` and `wordcraft-render`

```1:1:/Users/raphaelmansuy/Github/03-working/wordcraft/crates/layout/src/display.rs
//! Page → draw items, shared by the raster renderer, the PDF exporter and thumbnails.
```

`DP-G-49`. This is the strongest confirmation of `DP-G-01`. WordCraft renders a page for
the screen, for a thumbnail, and for a PDF through the same `Draw` list. EdgeDoc Display has
the same split and the same reason, and it adds `RecordPainter` so a test can check the list
without a raster.

The `Draw` enum to study before you write the display list:

```36:110:/Users/raphaelmansuy/Github/03-working/wordcraft/crates/layout/src/display.rs
pub enum Draw {
    Glyphs {
        face: FaceRef,
        size: f32,
        /// Glyph id, x, baseline y (page coordinates).
        glyphs: Vec<(u32, f32, f32)>,
        color: Rgb,
        alpha: f32,
        synth_bold: bool,
        synth_italic: bool,
        /// The text these glyphs show (for PDF text extraction), and the hyperlink.
        text: String,
        link: Option<String>,
        /// Byte range of `text` each glyph shows (one per glyph; a ligature's range covers all
        /// its characters). Empty when unknown: the PDF writer then guesses from the font.
        ranges: Vec<std::ops::Range<usize>>,
        ...
```

| WordCraft field | Why EdgeDoc Display needs it | Requirement |
|---|---|---|
| `glyphs: Vec<(u32, f32, f32)>` | Glyph id with its pen x and baseline y. One run, no per glyph operations | `DP-P-20` |
| `text` | The PDF text layer and the clipboard need the characters | `DP-V-33` |
| `ranges` | A ligature's byte range covers all its characters, so a selection inside `ffi` maps back | `DP-F-10`, `DP-G-35` |
| `link` | A hyperlink is a property of a run, so no second overlay pass | `DP-V-36` |
| `synth_bold`, `synth_italic` | A synthesized face needs a different transform, and the flag is the only record | `DP-F-13` |
| `x_scale` | `w:w` at 90% stretches each glyph across from its pen position | `DP-S-08` |
| `text_bg` | A highlight chooses the text colour against the dimmed background | `DP-G-46` |

`DP-G-50`. `Draw` has two grouping variants that the encoded format must keep:
`Turned` (a table cell's text direction) and `Rotated` (a rotation about a point). A painter
that flattens them loses the vertical text of `DP-L-47` and the rotation of `DP-X-97`.
EdgeDoc Display encodes them as `PushTransform` and `PopTransform` around a group
(`DP-G-02`, 0x05 and 0x06).

`DP-G-51`. The rasterizer is a trait in WordCraft terms: `wordcraft-render` calls
`vello_cpu`, and the web build runs the same code with no threads:

```270:275:/Users/raphaelmansuy/Github/03-working/wordcraft/crates/render/src/lib.rs
pub fn render_page(doc: &Document, page: &Page, scale: f32, opts: &RenderOptions) -> Rendered {
    let scale = if scale.is_finite() && scale > 0.0 { scale } else { 1.0 };
    let w = ((page.w * scale).ceil() as u32).clamp(1, MAX_SIDE);
    let h = ((page.h.min(1e6) * scale).ceil() as u32).clamp(1, MAX_SIDE);
    render_region(doc, page, w, h, Affine::scale(scale as f64), opts)
}
```

| WordCraft decision | Copy |
|---|---|
| A bad scale becomes 1.0, and a size past `MAX_SIDE` (16,000 px) clamps | Yes. `DP-V-12` clamps the zoom, and this function clamps the raster |
| Over the pixel cap, the view scales down instead of cropping | Yes (`DP-B-38`) |
| A media cache of 128 MiB and 256 entries | Yes, as a basis. The browser uses 64 MiB (`DP-E-12`) |
| Workers = 0 on `wasm32`, because the browser has no threads | Yes (`DP-B-03`) |

`DP-G-52`. EdgeDoc Display uses the browser `Canvas2D` for the tile painter and keeps
`vello_cpu` or `tiny-skia` for the native test path. WordCraft proves that one display list
drives a CPU rasterizer, a GPU renderer, and a PDF writer, so the abstraction is sound. The
browser painter is the only new target.

### 119.7 The pixel and count guards

`DP-P-26`. Every limit in `DP-P-06` has a WordCraft counterpart, and the port keeps the
value where the reason is the same:

| Guard | WordCraft value | EdgeDoc Display value | Reason for the difference |
|---|---|---|---|
| Decoded image bytes | 512 MiB | 256 MiB | The tab holds model, pages, and tiles |
| Raster pixels per page | 100,000,000 | 33,554,432 | A tile never needs 400 MB |
| Raster side | 16,000 px | 16,000 px | Same |
| Font file bytes | 50 MiB | 8 MiB | A fetch has a 4 MiB limit |
| Metafile items | 200,000 | 20,000 | A viewer paints fewer vector objects |
| Text box depth | 4 | 4 | Same |
| Nested table depth | 24 | 24 | Same (`ED-T-11`) |
| Layout passes | 3 per loop | 3 per loop | Same |

### 119.8 `edgedoc-display` from `wordcraft-engine`

WordCraft's `Session` owns the document, the layout cache, and the command registry. EdgeDoc
Display takes only the ownership part: one `DisplayDocument` holds the model, the computed
styles, the font faces, and the layout cache.

```1:8:/Users/raphaelmansuy/Github/03-working/wordcraft/crates/layout/src/lib.rs
//! WordCraft layout: turns a [`Document`] into pages.
//!
//! - [`para`]: one paragraph → lines (shaping, first-fit line breaking, tabs, list labels,
//!   alignment). Results are memoised per paragraph revision in a [`LayoutCache`].
//! - [`layout`]: sections → pages and columns; tables; keep/widow rules; page breaks; headers
//!   and footers; page-number fields.
//! - [`display`]: a page → draw items (glyph runs, rules, fills, images) for the renderers.
//! - [`hit`]: point ↔ position, caret geometry, line navigation, selection rectangles.
```

`DP-V-44`. The cache key is the contract, and WordCraft's key is the list of everything that
can change a paragraph's lines:

```7:27:/Users/raphaelmansuy/Github/03-working/wordcraft/crates/layout/src/cache.rs
pub(crate) struct Key {
    pub(crate) rev: u64,
    pub(crate) width: u32,
    pub(crate) label: Option<String>,
    pub(crate) page: Option<(u32, u32, u32, u32, u64, wordcraft_doc::section::NumFormat)>,
    /// The pages a split paragraph's fields are on after its first (`FieldCtx::page_from`).
    pub(crate) page_from: u64,
    pub(crate) table: u64,
    pub(crate) notes: u64,
    pub(crate) excl: u64,
    pub(crate) eq: u32,
    pub(crate) read_spacing: Option<u32>,
    pub(crate) grid: (u32, u32, u32, bool),
    pub(crate) fit: bool,
    ...
```

EdgeDoc Display keys on the content hash and the width (`DP-E-21`). It also adds the wrap
areas (`excl`) and the note numbers, because a zoom change must never invalidate a paragraph
layout. A key that omits one input is a stale page, and a stale page is a wrong page break.

### 119.9 `edgedoc-display-pdf` from `wordcraft-pdf`

| WordCraft location | Copy |
|---|---|
| `crates/pdf/src/exporter.rs`, `Exporter` with its face, image, and vector maps | The structure. One exporter, one page loop, one subset per face |
| `crates/pdf/src/tags.rs`, `TagEntry` and `Role` | Tagged PDF with a paragraph per structure element, and a header as an artifact |
| `crates/pdf/src/exporter/text.rs`, cluster splitting and visual order | The rule that a glyph run splits at a cluster boundary |
| `crates/pdf/src/options.rs`, `PdfOptions` | `pages`, `tagged`, `links`, and `compress` |

`DP-V-45`. A viewer prints without links, because a printer driver cannot fit a page that
carries annotations. WordCraft records this in `PdfOptions::links`: "A print job leaves them
out". EdgeDoc Display does the same, and the print path of `DP-B-41` uses the same option.

### 119.10 `edgedoc-display-wasm` from `apps/wordcraft-web` and `packaging/web`

WordCraft proves the hosting facts, and this specification takes each one as a requirement:

| WordCraft evidence | Requirement |
|---|---|
| `packaging/web/README.md`: "WordCraft does not use `SharedArrayBuffer`, so it needs no `Cross-Origin-Opener-Policy` or `Cross-Origin-Embedder-Policy`" | `DP-B-02`, `DP-B-06` |
| The same file: serve `.wasm` as `application/wasm`, or "the app then loads slowly or not at all" | `DP-B-31` |
| `Trunk.toml`: `public_url = "./"`, `filehash = true`, so any path works and a hash lets a file cache forever | `DP-B-30` |
| The same file: "about 13 MB, or 5 MB with compression" | The size model of `DP-B-32` |
| `apps/wordcraft-web/src/main.rs`: URL flags `?webgl` and `?sample` | `DP-U-28` |
| The same file: `?host=parent` talks to a same-origin parent page | The embed mode of `DP-U-30` |

`DP-B-43`. **The size budget of `DP-B-32` needs a note.** WordCraft is 5 MB compressed. It
carries an editor, a ribbon, a spell checker, and an egui user interface. A read-only page
viewer carries the model, the shaper, the flow crate, and one painter. The 1.6 MiB gzip
budget is therefore plausible, but no build proves it yet. The first phase measures the real
size. The budget then becomes the measured value plus 15% (`DP-E-28`).

`DP-U-30`. The demo supports the embed mode that WordCraft proves. A host page passes the
document in and receives the page number out, through `postMessage` on a same-origin
`iframe`. The host uses this mode to show a document inside a documentation page.

## 120. Defects that this specification must not inherit

`DP-N-08`. WordCraft records its own gaps. Each row below is a behavior that version 1 of
EdgeDoc Display either fixes or states as a known limit. A port that copies the gap and does
not say so is a silent wrong page.

| WordCraft gap | Effect on a page | EdgeDoc Display decision |
|---|---|---|
| Aptos, Word's default font since 2024, has no metric-compatible free substitute | A new Word document wraps differently | `DP-F-03` maps Aptos to Carlito with a diagnostic at `warning`, and the fidelity report names it |
| Kashida justification for Arabic is missing | An Arabic justified line stretches spaces, not kashida | A known limit of T2. The line stays justified, and it is marked in the report |
| Kerning was applied to every run before a fix | Lines held more text than Word's | `DP-F-12` keys kerning on `w:kern`, and the test locks it |
| `w14:ligatures` is not read, so ligatures draw when Word draws none | A slightly shorter line | `DP-F-12` reads `w14:ligatures`. Without the property, no ligature draws |
| A picture's tight wrap uses a convex hull | Text clears a convex shape, not a concave one | `DP-X-133` and `DP-L-29`. A rectangle for a JPEG, a hull for a PNG with alpha |
| Complex-script scripts other than the ones with a dictionary wrap only at spaces | A Khmer or Lao line is loose | A known limit of T2, with `display.hyphenation_unavailable` |
| Most legacy `w:compat` options are ignored | An old document breaks differently | `DP-S-28` reads the listed set and logs each ignored setting |
| No page-by-page comparison against Word in CI | A regression is invisible | `DP-Q-10` and `DP-Q-11` make the corpus and the gate. This is the biggest change from the reference |

`DP-N-09`. The last row is the reason for [14-testing-and-quality.md](14-testing-and-quality.md).
WordCraft lists "Page-by-page comparison harness against Word" as ❌, and estimates 8–12
hours for it. EdgeDoc Display puts that harness in phase 2, not at the end, because a
layout engine without a gate drifts.

## 121. The reference tools to port

WordCraft already has the measurement tools that this specification needs in CI.

| Tool | WordCraft location | EdgeDoc Display use |
|---|---|---|
| Two-PDF page diff with SSIM, line matching, and per-line delta | `xtask/src/visual_diff.rs` | `cargo xtask display-diff` for the gate of `DP-Q-10` |
| Layer check | `xtask/src/layers.rs` | `cargo xtask layers` for `DP-A-01` |
| Linear-time step counter | `crates/layout/src/work.rs` | The guard for `DP-X-67` and `DP-X-158` |
| Parity checklist generator | `xtask/src/parity.rs` | The generated fidelity table on the demo site (`DP-U-29`) |

`DP-Q-42`. The step counter is the most valuable of the four, because a timing test on a
busy machine is not stable:

```1:6:/Users/raphaelmansuy/Github/03-working/wordcraft/crates/layout/src/work.rs
//! A count of the steps the layout's lookups take, for the linear-time tests (#3692): they check
//! that an input 8x the size costs about 8x the steps, which a busy machine can't skew the way it
//! skews a timing. Each walk that once ran again for every item it served (a line's clusters, a
//! page's items, the revision authors, the sections, a table's rows) counts the elements it
//! looks at, so a caller that walks it once per item again shows up as a quadratic count. Outside
//! tests the count is nothing.
```

EdgeDoc Display uses the same counter for the page lookups, the text index, and the search.

## 122. How to port a module

`DP-N-10`. Use this procedure for each module. The order prevents the common failure. That
failure is a port that compiles and paints, but breaks the page in a way that no test sees.

1. Read the WordCraft module and its tests. Write down each behavior in one sentence.
2. Read the matching row in `docs/layout-parity.md`. Note the state, ✅ or 🟡 or ❌.
3. Write the EdgeDoc Display signature first. It must satisfy the trait contract of
   [03-architecture.md](03-architecture.md). It must not take a `Document` reference where a
   computed record is enough.
4. Port the test before the code. A WordCraft test name becomes an EdgeDoc test name with the
   same words, so a reviewer can find both.
5. Port the guards: the clamp, the cap, the `finite` check, the depth limit.
6. Add the diagnostic code for each degraded path.
7. Run the corpus and the diff tool. A page that moves is a behavior that the port changed.

`DP-N-11`. A copied test carries a comment with the WordCraft path and the version:

```rust
// Ported from wordcraft 0.14.0, crates/layout/src/table.rs:298
// autofit_widths: max below min takes min.
#[test]
fn autofit_max_below_min_takes_min() { /* ... */ }
```

This comment is how a later reader checks a behavior against its source. Without it, a
correct port looks like a guess and a wrong port looks correct.
