# Your code is quietly destroying the document it was asked to fill in.

> LinkedIn post — EdgeDoc 0.1.0 announcement.
> Body is plain text, no Markdown, no code fences. Copy from the line after `---` to the hashtags.
> Length: 2,900 characters, under the 3,000 limit.

---

Your code is quietly destroying the document it was asked to fill in.

It is not a bug in your code. It is the library underneath.

The file opens. Word does not complain. So nobody looks.

But on save, the library throws away every part it does not understand:

- the header lost its logo
- the numbering restarted
- the chart is gone
- the macros vanished

That file is no longer the template your brand and legal teams approved. It is a lookalike.

WHY IT HAPPENS

A .docx is not a list of paragraphs. It is a ZIP package of parts: styles, themes, numbering, media, charts, macros, settings.

Most Word libraries read a .docx into a paragraph list, then rebuild the file from that list.

A paragraph list cannot hold a package. So the rest is dropped, silently, on save.

For one report, nobody notices. Repeat it for every invoice you send, and it reaches your customers.

THE RULE I BUILT ON

The package is the document.

Every part has one of three states:

- Modeled: read into a typed model, and an edit can change it
- Kept: not modeled, copied byte for byte
- Refused: cannot be read safely, so the call returns a stable error code

Nothing is dropped in silence. Save a file with no edit and you get the original bytes back, byte for byte. A test enforces that promise on every commit.

Mail merge follows the same rule. Give it CSV records, add filters and sort keys, repeat one table row per record. A price list grows a row per product while the header row stays once. A small value function handles currency and formatting, and the template stays plain text.

WHAT YOU GET

If your code generates invoices, letters, statements, contracts or reports:

- your approved layout survives the merge
- one template serves every region and every currency
- the file you save is the file Word expects to open

One core, three runtimes. The same Rust library ships as a crate and a PyPI wheel, and builds to WebAssembly from the same source. One field parser, one set of limits, one set of error codes. Your Python batch job and your browser form cannot drift apart.

Apache 2.0. Zero unsafe blocks. Every parser runs on a memory budget, so a hostile file fails before it allocates past it.

TEST IT IN ONE MINUTE

Take any template. Read it. Save it with no edit. Compare the two files.

If your current library returns anything other than the original bytes, it is rewriting your template.

If you want to skip the experiment:

pip install edgedoc
cargo add edgedoc

The repo holds 12 real documents: a branded two-page invoice, a board report with a chart, personalised letters, a label sheet, a price list. Open one in Word. Check the header, the numbering, the footer.

github.com/raphaelmansuy/edgedoc

I built this because I was tired of patching .docx files by hand.

If you have done the same, tell me what broke. I read every comment.

#Rust #Python #WebAssembly #DocumentAutomation #OpenSource

---

## Notes for the operator (do not paste this section)

### Why this structure

| Block | Job |
|---|---|
| Line 1 | The hook. A claim about the reader's own code, not about the library. |
| Line 2 | Removes blame. Keeps the reader reading. |
| "Word does not complain" | Names the reason nobody caught it. This is the tension. |
| Bullet list of losses | Four concrete, visual failures. Concrete beats abstract. |
| WHY IT HAPPENS | The payoff for the WHY opening. `.docx` is a package, not a paragraph list. |
| THE RULE | The product, stated as a principle. Modeled / Kept / Refused. |
| WHAT YOU GET | Value in the reader's terms: invoices, letters, contracts. |
| TEST IT IN ONE MINUTE | The proof. A falsifiable experiment the reader can run against their current library. |
| Close | The founder line, then a question that invites a story, not a like. |

### Why the hook works

It accuses the code, not the person. The reader's instinct is "not my code" — so they
open the post to check. The word "quietly" does the work: it implies a fault they were
never warned about.

### Alternative hooks, if you want to A/B test

1. `A .docx is not a list of paragraphs. Most libraries think it is.`
2. `Your invoice template survived design review. It does not survive your code.`
3. `I read a .docx, saved it unchanged, and got a different file back. That was the bug.`

Post 3 is the strongest for a technical audience: it is a first-person discovery, and the
claim is verifiable.

### Formatting rules that matter on LinkedIn

- The body has no Markdown. LinkedIn does not render `**bold**` or backticks; it shows
  the asterisks. Section headers are plain capitals for that reason.
- Plain capitals read as a header in the feed and survive "see more".
- Blank line between every block. Walls of text lose readers at 3 lines.
- The link sits near the end. A link in line 1 suppresses reach on most accounts.
- The first 210 characters are the feed preview. The hook and the de-blame line both
  fit inside that window.

### Before you post

1. Publish the WASM module, or keep the text as written (it says "builds to
   WebAssembly", which is true: `make wasm`).
2. Confirm `pip install edgedoc` resolves to 0.1.0. Checked: PyPI serves 0.1.0.
3. Confirm `cargo add edgedoc` resolves. Checked: crates.io serves 0.1.0.
4. Attach one image. The strongest candidate is
   `site/assets/previews/invoice-p1.png` — a real document, not a diagram. A
   two-image carousel of `invoice-template.png` then `invoice-p1.png` tells the
   before/after in one swipe.
5. Post between 08:00 and 10:00 on a Tuesday, Wednesday or Thursday, in the timezone
   of your main audience.
6. Reply to every comment in the first 90 minutes. The comment thread carries more
   reach than the post.

### Numbers used in the body

| Claim | Source |
|---|---|
| A save with no edit returns the original bytes | `pkg-clean` in `crates/edgedoc/tests/format_coverage.rs` |
| Zero unsafe blocks | `unsafe_code = "forbid"` in `Cargo.toml` |
| 12 real documents | `make examples`, listed in `README.md` |
| Memory budget on every parser | `docs/08-edge-cases.md`, `edgedoc-limit` |

The claim "a test enforces that promise on every commit" is true because
`.github/workflows/ci.yml` runs the whole test suite on every push and pull request.

