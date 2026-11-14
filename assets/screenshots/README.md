# Site screenshots

These images are end-to-end captures of the live site, taken from
<https://raphaelmansuy.github.io/edgedoc/> at a 1440 px viewport. They let a reviewer
see the rendered page without a browser.

| File | What it shows |
|---|---|
| `e2e-hero.png` | The hero: the headline, the lede, and the four facts |
| `e2e-gallery-1.png` | The first gallery card, with a real invoice inside it |
| `e2e-table-merge.png` | The card that explains the table merge, with the price list |

The captures use the Chrome DevTools Protocol with an explicit clip, because a scaled
preview hides layout faults. `tools/capture_site.py` records the method.
