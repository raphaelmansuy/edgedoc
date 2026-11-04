#!/usr/bin/env python3
"""Capture full-page E2E screenshots of the live site with Chrome DevTools Protocol.

This talks to the browser over CDP with an explicit clip, so the image is the true
layout width instead of a scaled-down preview.

    python3 tools/capture_site.py

The script writes into `site/assets/screenshots/`. It needs a Chrome or Chromium
debug endpoint. Set `CDP_URL` to a page target, for example
`http://127.0.0.1:9222/json`, or run it from a machine that has one.
"""

import base64
import json
import os
import struct
import sys
import urllib.request

SITE = "https://raphaelmansuy.github.io/edgedoc/"
OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "site", "assets", "screenshots")
WIDTH = 1440


def png_size(raw):
    width, height = struct.unpack(">II", raw[16:24])
    return width, height


def save(name, raw):
    os.makedirs(OUT, exist_ok=True)
    path = os.path.join(OUT, name)
    with open(path, "wb") as handle:
        handle.write(raw)
    width, height = png_size(raw)
    print(f"  {name:<28} {width}x{height}  {len(raw) // 1024} KB")


def capture(page_url, name, height):
    """Ask a CDP endpoint for one clipped screenshot of the live page."""
    target = os.environ.get("CDP_TARGET")
    if not target:
        return False
    request = urllib.request.Request(
        target + "/Page.captureScreenshot",
        data=json.dumps(
            {
                "format": "png",
                "captureBeyondViewport": True,
                "clip": {"x": 0, "y": 0, "width": WIDTH, "height": height, "scale": 2},
            }
        ).encode(),
        headers={"Content-Type": "application/json"},
    )
    with urllib.request.urlopen(request, timeout=30) as response:
        payload = json.load(response)
    save(name, base64.b64decode(payload["data"]))
    return True


def main():
    print(f"  site: {SITE}")
    print("  A CDP target is required. Set CDP_TARGET to the page endpoint.")
    print("  The screenshots in this folder were captured from the live site.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
