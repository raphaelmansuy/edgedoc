#!/usr/bin/env python3
"""Render selected, provenance-checked page pairs for manual image inspection.

This prepares images; it does not make a visual acceptance judgment.
"""
import argparse
import json
import subprocess
from pathlib import Path

import wordcraft_parity as parity


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("pages", nargs="+", help="CASE:PAGE (one-based page numbers)")
    args = parser.parse_args()
    run, out = args.directory.resolve(), args.output.resolve()
    entries = {entry["id"]: entry for entry in json.loads((run / "manifest.json").read_text())["cases"]}
    out.mkdir(parents=True, exist_ok=True)
    records_path = out / "images.json"
    records = json.loads(records_path.read_text()) if records_path.exists() else {}
    for selection in args.pages:
        name, number = selection.rsplit(":", 1)
        index = int(number)
        if name not in entries or index < 1:
            raise ValueError(f"Unknown case/page: {selection}")
        folder = run / name
        edge = json.loads((folder / "edgedoc/layout.json").read_text())
        reference = json.loads((folder / "wordcraft/layout.json").read_text())
        source_hash = parity.digest(folder / "input.docx")
        if edge.get("build_inputs_sha256") != parity.build_digest() or reference.get("reference") != parity.reference_identity():
            raise ValueError(f"Stale engine evidence: {selection}")
        if any(record.get("source_sha256") != source_hash for record in [edge, reference]) or entries[name]["input_sha256"] != source_hash:
            raise ValueError(f"Source identity mismatch: {selection}")
        output = out / name
        output.mkdir(exist_ok=True)
        record = {"case": name, "page": index, "source_sha256": source_hash,
                  "edgedoc_build_inputs_sha256": edge["build_inputs_sha256"], "reference": reference["reference"],
                  "pages": {"edgedoc": len(edge["pages"]), "wordcraft": len(reference["pages"])}, "images": {}}
        for engine, data in [("wordcraft", reference), ("edgedoc", edge)]:
            page = data["pages"][index - 1]
            svg = folder / engine / f"page-{index:03}.svg"
            png = output / f"{engine}-page-{index:03}.png"
            fingerprint = parity.digest(svg)
            subprocess.run(["rsvg-convert", "-w", str(round(page["width"] * parity.DPI / 72)),
                            "-h", str(round(page["height"] * parity.DPI / 72)), str(svg), "-o", str(png)],
                           check=True, timeout=45)
            if fingerprint != parity.digest(svg):
                raise ValueError("Source SVG changed during inspection rendering")
            record["images"][engine] = {"path": str(png), "svg_sha256": fingerprint, "png_sha256": parity.digest(png)}
        records[selection] = record
        print(f"Prepared {selection}: {record['pages']}")
    parity.save_json(records_path, records)


if __name__ == "__main__":
    main()
