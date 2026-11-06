#!/usr/bin/env python3
"""Expand the first note in the exported crowded-note regression fixture.

Example: python tools/generate_note_stress.py input.docx output.docx --repeat 1800
The output must not already exist. This deliberately leaves the source package untouched.
"""

import argparse
from pathlib import Path
from zipfile import ZIP_DEFLATED, ZipFile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--repeat", type=int, default=1800)
    args = parser.parse_args()
    if not 1 <= args.repeat <= 10000:
        parser.error("--repeat must be between 1 and 10000")
    marker = b"<w:t> first</w:t>"
    with ZipFile(args.source) as source:
        note = source.read("word/footnotes.xml")
        if note.count(marker) != 1:
            parser.error("source does not contain exactly one expected note marker")
        replacement = b'<w:t xml:space="preserve">' + b" note continuation content" * args.repeat + b"</w:t>"
        with args.output.open("xb") as output, ZipFile(output, "w", ZIP_DEFLATED) as target:
            for info in source.infolist():
                data = note.replace(marker, replacement) if info.filename == "word/footnotes.xml" else source.read(info.filename)
                target.writestr(info, data)


if __name__ == "__main__":
    main()
