#!/usr/bin/env python3
"""Differential renderer gate against the unmodified sibling Wordcraft checkout.

prepare OUT creates original fixtures. run OUT --font STYLE=PATH renders both engines and
compares source hashes, fonts, text, page counts, glyph origins, and raster ink without alignment.
Missing evidence, stale builds, unsupported rendering, or mismatches fail the gate.
"""
import argparse
import base64
import concurrent.futures
import hashlib
import io
import json
import os
import re
import shutil
import subprocess
import sys
import unicodedata
import zipfile
from pathlib import Path
from xml.sax.saxutils import escape

ROOT = Path(__file__).resolve().parents[1]
W = "http://schemas.openxmlformats.org/wordprocessingml/2006/main"
R = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
POSITION_PT = 0.5
SIZE_PT = 0.05
UNMATCHED_INK_FRACTION = 0.01
MEAN_INK_COLOR_ERROR = 0.05
DPI = 144


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def save_json(path, value):
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n")


def build_digest(root=ROOT):
    inputs = sorted([*(root / "crates").rglob("*.rs"), *(root / "crates").rglob("Cargo.toml")]) + [root / "Cargo.lock", root / "Cargo.toml"]
    # Embedded font bytes are part of the renderer even when no external font was supplied.
    inputs.extend(sorted((root / "assets/fonts/subset").glob("*.ttf")))
    fallback_font = root / "assets/fonts/SourceSans3-Regular.ttf"
    if fallback_font.exists():
        inputs.append(fallback_font)
    if root == ROOT:
        inputs.append(ROOT / "site/display/edgedoc-viewer.js")
    h = hashlib.sha256()
    for path in inputs:
        h.update(str(path.relative_to(root)).encode())
        h.update(path.read_bytes())
    return h.hexdigest()


def renderer_command(command, engine, **kwargs):
    result = subprocess.run(command, capture_output=True, text=True, timeout=180, **kwargs)
    if result.stdout:
        print(result.stdout.strip(), flush=True)
    if result.returncode:
        raise RuntimeError(f"{engine} exited {result.returncode}: {result.stderr[-4000:].strip()}")


def render(out, case, fonts):
    folder = out / case
    source = folder / "input.docx"
    source_hash, code_hash = digest(source), build_digest()
    command = ["cargo", "run", "--quiet", "-p", "edgedoc-display", "--features", "serde", "--example", "parity_dump", "--", str(source), str(folder / "edgedoc")]
    options = {"regular": "--font", "bold": "--font-bold", "italic": "--font-italic", "bold-italic": "--font-bold-italic"}
    font_hashes = {}
    for font in fonts:
        style, path = font.split("=", 1)
        style, _, index = style.partition(":")
        index = int(index or 0)
        if style not in options:
            raise ValueError(f"Unknown font style {style}")
        if index < 0:
            raise ValueError("Font collection index must be nonnegative")
        path = str(Path(path).resolve())
        font_hashes[path] = digest(Path(path))
        command.extend([options[style], path])
        if index:
            command.extend(["--font-index", str(index)])
    if not font_hashes:
        raise ValueError("Supply --font regular=/path/to/font.ttf and any required bold/italic faces")
    renderer_command(command, "EdgeDoc", cwd=ROOT, env={**os.environ, "CARGO_TARGET_DIR": str(ROOT / "target")})
    if source_hash != digest(source) or code_hash != build_digest():
        raise ValueError("Source/build inputs changed while rendering")
    path = folder / "edgedoc/layout.json"
    result = json.loads(path.read_text())
    result.update({"source_sha256": source_hash, "build_inputs_sha256": code_hash})
    for font in result["fonts"]:
        font["sha256"] = font_hashes[font["path"]]
    for page in result["pages"]:
        for char in page["chars"]:
            char["font_sha256"] = font_hashes.get(char.get("font_path"))
    save_json(path, result)


def para(text, props="", run=""):
    return f'<w:p><w:pPr>{props}<w:rPr><w:rFonts w:ascii="Arial" w:hAnsi="Arial"/><w:sz w:val="22"/></w:rPr></w:pPr><w:r><w:rPr><w:rFonts w:ascii="Arial" w:hAnsi="Arial"/><w:sz w:val="22"/>{run}</w:rPr><w:t xml:space="preserve">{escape(text)}</w:t></w:r></w:p>'


def prepare_retained(out, previous, fixtures=None):
    """Carry every old case forward; append new regressions without duplicating bytes."""
    if (out / "manifest.json").exists():
        raise FileExistsError(f"Refusing to replace {out / 'manifest.json'}")
    manifest = json.loads(previous.read_text())
    prior_count = len(manifest["cases"])
    seen = {}
    ids = set()
    # Validate the entire retained set before writing anything.
    for entry in manifest["cases"]:
        name = entry["id"]
        if not re.fullmatch(r"[A-Za-z0-9_.-]+", name) or name in {".", ".."} or name in ids:
            raise ValueError(f"Invalid or duplicate retained case identity {name}")
        source = previous.parent / name / "input.docx"
        if digest(source) != entry["input_sha256"]:
            raise ValueError(f"Retained source hash mismatch: {name}")
        if (out / name).exists():
            raise FileExistsError(f"Refusing to replace {out / name}")
        ids.add(name)
    out.mkdir(parents=True, exist_ok=True)
    for entry in manifest["cases"]:
        folder = out / entry["id"]
        folder.mkdir()
        shutil.copyfile(previous.parent / entry["id"] / "input.docx", folder / "input.docx")
        seen.setdefault(entry["input_sha256"], entry)
    if fixtures:
        for source in sorted(fixtures.glob("*.docx")):
            fingerprint = digest(source)
            if fingerprint in seen:
                seen[fingerprint].setdefault("regression_contracts", []).append(source.stem)
                continue
            name = "regression-" + source.stem
            if name in ids:
                raise ValueError(f"Duplicate regression identity {name}")
            folder = out / name
            folder.mkdir()
            shutil.copyfile(source, folder / "input.docx")
            entry = {"id": name, "input_sha256": fingerprint, "features": [source.stem.rsplit("-", 1)[0]]}
            manifest["cases"].append(entry)
            seen[fingerprint] = entry
            ids.add(name)
    manifest["retained_manifest_sha256"] = digest(previous)
    manifest["scope"] = f"{len(manifest['cases'])} fixture cases retaining every prior input; not exhaustive Wordcraft parity"
    save_json(out / "manifest.json", manifest)
    print(f"Prepared {len(manifest['cases'])} cases, retaining all {prior_count} prior cases")


def prepare(out, fixtures=None):
    out.mkdir(parents=True, exist_ok=True)
    exact = '<w:spacing w:line="480" w:lineRule="exact"/>'
    core = para("EDGEDOC RENDERING PARITY - ORIGINAL SYNTHETIC INPUT", exact)
    core += para("Left aligned: 0123456789 ABCDEFG abcdefg", exact)
    core += para("Centered: 0123456789", exact + '<w:jc w:val="center"/>')
    core += para("Right aligned: 0123456789", exact + '<w:jc w:val="right"/>')
    core += para("Bold text: 0123456789", exact, "<w:b/>")
    core += para("Italic text: 0123456789", exact, "<w:i/>")
    core += para("Colored text: 0123456789", exact, '<w:color w:val="1E40AF"/>')
    core += para("Second page", exact + "<w:pageBreakBefore/>")
    core += para("Natural line spacing: Alpha Beta Gamma")
    core += para("Multiple line spacing: Alpha Beta Gamma", '<w:spacing w:line="480" w:lineRule="auto"/>')
    core += para("At least line spacing: Alpha Beta Gamma", '<w:spacing w:line="600" w:lineRule="atLeast"/>')
    core += para("Final exact line: 0123456789", exact)
    notes = para("NOTE AND DROP CAP COVERAGE - ORIGINAL SYNTHETIC INPUT", exact)
    notes += '<w:p><w:r><w:rPr><w:rFonts w:ascii="Arial"/><w:sz w:val="22"/></w:rPr><w:t>Body reference</w:t><w:footnoteReference w:id="1"/></w:r></w:p>'
    notes += para("Drop cap paragraph with several original words to flow beside the initial letter.", '<w:framePr w:dropCap="drop" w:lines="3"/>')
    cases = [("core-layout", core, None), ("notes-dropcap", notes, "Original footnote text that must appear below the body.")]
    entries = []
    for name, body, note in cases:
        folder = out / name
        folder.mkdir(exist_ok=True)
        source = folder / "input.docx"
        if source.exists():
            raise FileExistsError(f"Refusing to replace existing fixture {source}")
        sect = '<w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:bottom="1440" w:left="1440" w:right="1440" w:header="720" w:footer="720"/>'
        rels = f'<Relationship Id="styles" Type="{R}/styles" Target="styles.xml"/>'
        types = '<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>'
        if note:
            rels += f'<Relationship Id="notes" Type="{R}/footnotes" Target="footnotes.xml"/>'
            types += '<Override PartName="/word/footnotes.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.footnotes+xml"/>'
        with zipfile.ZipFile(source, "w", compression=zipfile.ZIP_DEFLATED) as z:
            z.writestr("[Content_Types].xml", f'<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>{types}</Types>')
            z.writestr("_rels/.rels", f'<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="document" Type="{R}/officeDocument" Target="word/document.xml"/></Relationships>')
            z.writestr("word/document.xml", f'<w:document xmlns:w="{W}" xmlns:r="{R}"><w:body>{body}<w:sectPr>{sect}</w:sectPr></w:body></w:document>')
            z.writestr("word/_rels/document.xml.rels", f'<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">{rels}</Relationships>')
            z.writestr("word/styles.xml", f'<w:styles xmlns:w="{W}"><w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="Arial" w:hAnsi="Arial"/><w:sz w:val="22"/></w:rPr></w:rPrDefault><w:pPrDefault><w:pPr><w:spacing w:before="0" w:after="0"/></w:pPr></w:pPrDefault></w:docDefaults></w:styles>')
            if note:
                z.writestr("word/footnotes.xml", f'<w:footnotes xmlns:w="{W}"><w:footnote w:type="separator" w:id="-1"><w:p><w:r><w:separator/></w:r></w:p></w:footnote><w:footnote w:id="1">{para(note)}</w:footnote></w:footnotes>')
        entries.append({"id": name, "input_sha256": digest(source), "features": ["font", "alignment", "spacing", "page-break", "bold", "italic"] if not note else ["footnotes", "drop-caps"]})
    if fixtures:
        for source in sorted(fixtures.glob("*.docx")):
            name = source.stem
            folder = out / name
            folder.mkdir(exist_ok=True)
            target = folder / "input.docx"
            if target.exists(): raise FileExistsError(f"Refusing to replace {target}")
            shutil.copyfile(source, target)
            entries.append({"id": name, "input_sha256": digest(target), "features": [name.rsplit("-", 1)[0]]})
    save_json(out / "manifest.json", {"schema": 1, "cases": entries, "scope": f"{len(entries)} original synthetic fixtures; not exhaustive Wordcraft parity"})
    print(f"Prepared {len(entries)} cases in {out}")


def add_external_corpus(out, manifest_path, directory):
    from fetch_external_corpus import validate
    external = json.loads(manifest_path.read_text())
    path = out / "manifest.json"
    manifest = json.loads(path.read_text())
    ids = {entry["id"] for entry in manifest["cases"]}
    for entry in external["documents"]:
        name = entry["id"]
        if name in ids:
            raise ValueError(f"Duplicate corpus identity {name}")
        content = (directory / (name + ".docx")).read_bytes()
        validate(entry, content)
        folder = out / name
        folder.mkdir()
        (folder / "input.docx").write_bytes(content)
        manifest["cases"].append({"id": name, "input_sha256": entry["sha256"], "features": entry["features"],
            "source": {key: entry[key] for key in ["repository", "revision", "path", "url", "git_blob_sha1"]}})
        ids.add(name)
    manifest.setdefault("external_manifest_sha256s", []).append(digest(manifest_path))
    manifest["scope"] = f"{len(manifest['cases'])} fixture cases, including {len(external['documents'])} pinned upstream DOCX documents; not exhaustive Wordcraft parity"
    save_json(path, manifest)
    print(f"Added {len(external['documents'])} pinned external documents")


def font_family(name):
    name = re.sub(r"^[A-Z]{6}\+", "", name).lower()
    for suffix in ["bolditalicmt", "boldmt", "italicmt", "regular", "psmt", "mt"]:
        if name.endswith(suffix):
            name = name[:-len(suffix)]
            break
    return re.sub(r"[^a-z0-9]", "", name)


def lines(chars):
    rows = []
    for ch in sorted(chars, key=lambda c: (c["baseline"], c["x"])):
        row = next((r for r in rows if abs(r["baseline"] - ch["baseline"]) <= 0.25), None)
        if row is None:
            row = {"baseline": ch["baseline"], "chars": []}
            rows.append(row)
        row["chars"].append(ch)
    for row in rows:
        row["chars"].sort(key=lambda c: c["x"])
        row["text"] = "".join(unicodedata.normalize("NFC", c["text"]) for c in row["chars"])
    return rows


def reference_root():
    return ROOT.parent / "wordcraft"


def reference_identity():
    root = reference_root()
    head = subprocess.check_output(["git", "-C", str(root), "rev-parse", "HEAD"], text=True).strip()
    dirty = subprocess.check_output(["git", "-C", str(root), "status", "--porcelain"], text=True).strip()
    return {"engine": "Wordcraft", "checkout": str(root), "commit": head, "dirty": bool(dirty), "source_sha256": build_digest(root),
        "adapter_sha256": digest(ROOT / "tools/wordcraft-parity/src/main.rs"),
        "adapter_lock_sha256": digest(ROOT / "tools/wordcraft-parity/Cargo.lock"),
        "adapter_manifest_sha256": digest(ROOT / "tools/wordcraft-parity/Cargo.toml")}


def build_reference():
    before = reference_identity()
    subprocess.run(["cargo", "build", "--locked", "--manifest-path", str(ROOT / "tools/wordcraft-parity/Cargo.toml")], cwd=ROOT,
        env={**os.environ, "CARGO_TARGET_DIR": str(ROOT / "target/wordcraft-reference")}, check=True)
    if before != reference_identity():
        raise ValueError("Reference source changed during compilation")
    binary = ROOT / "target/wordcraft-reference/debug/edgedoc-wordcraft-reference"
    save_json(binary.with_suffix(".provenance.json"), {"reference": before, "binary_sha256": digest(binary)})


def render_reference(out, case, fonts):
    before = reference_identity()
    binary = ROOT / "target/wordcraft-reference/debug/edgedoc-wordcraft-reference"
    provenance = binary.with_suffix(".provenance.json")
    expected = {"reference": before, "binary_sha256": digest(binary)} if binary.exists() else None
    if not provenance.exists() or json.loads(provenance.read_text()) != expected:
        build_reference()
    folder = out / case
    paths = [font.split("=", 1)[1] for font in fonts]
    binary = ROOT / "target/wordcraft-reference/debug/edgedoc-wordcraft-reference"
    renderer_command([str(binary), str(folder / "input.docx"), str(folder / "wordcraft"), *paths], "Wordcraft")
    if before != reference_identity():
        raise ValueError("Reference checkout changed while rendering")
    path = folder / "wordcraft/layout.json"
    data = json.loads(path.read_text())
    data["reference"] = before
    save_json(path, data)


def reusable_reference(out, case, fonts):
    """An explicit reuse requires the same input, reference source and full font profile."""
    folder = out / case
    try:
        reference = json.loads((folder / "wordcraft/layout.json").read_text())
        prior_edge = json.loads((folder / "edgedoc/layout.json").read_text())
        source_hash = digest(folder / "input.docx")
        if reference.get("source_sha256") != source_hash or prior_edge.get("source_sha256") != source_hash:
            return False
        if reference.get("reference") != reference_identity():
            return False
        styles = {"regular": (False, False), "bold": (True, False), "italic": (False, True), "bold-italic": (True, True)}
        expected = []
        for argument in fonts:
            style, name = argument.split("=", 1)
            style, _, index = style.partition(":")
            path = Path(name).resolve()
            expected.append((str(path), digest(path), *styles[style], int(index or 0)))
        actual = [(str(Path(font["path"]).resolve()), font["sha256"], font["bold"], font["italic"], font.get("index", 0)) for font in prior_edge["fonts"]]
        return bool(expected) and sorted(actual) == sorted(expected) and bool(reference["pages"]) and all(
            (folder / "wordcraft" / f"page-{index:03}.svg").exists()
            for index in range(1, len(reference["pages"]) + 1))
    except (OSError, ValueError, KeyError, TypeError):
        return False


def raster_pair(folder, index, width, height):
    import numpy as np
    from PIL import Image, ImageFilter
    images = []
    for engine in ["wordcraft", "edgedoc"]:
        svg = folder / engine / f"page-{index:03}.svg"
        png = svg.with_suffix(".png")
        subprocess.run(["rsvg-convert", "-w", str(round(width * DPI / 72)), "-h", str(round(height * DPI / 72)), str(svg), "-o", str(png)], check=True, capture_output=True, timeout=45)
        images.append(Image.open(png).convert("RGB"))
    ref, edge = images
    if ref.size != edge.size:
        return {"error": "Raster dimensions differ", "unmatched_ink_fraction": 1.0}
    rm, em = np.min(np.asarray(ref), axis=2) < 200, np.min(np.asarray(edge), axis=2) < 200
    rd = np.asarray(Image.fromarray(rm.astype("uint8") * 255).filter(ImageFilter.MaxFilter(3))) > 0
    ed = np.asarray(Image.fromarray(em.astype("uint8") * 255).filter(ImageFilter.MaxFilter(3))) > 0
    unmatched = float((rm & ~ed).sum() + (em & ~rd).sum()) / max(1, int(rm.sum() + em.sum()))
    common = rm & em
    color_error = float(np.abs(np.asarray(ref, dtype="float32") - np.asarray(edge, dtype="float32"))[common].mean() / 255) if common.any() else (0.0 if not rm.any() and not em.any() else 1.0)
    diff = np.full((*rm.shape, 3), 255, dtype="uint8")
    diff[rm & em] = [80, 80, 80]
    diff[rm & ~em] = [220, 40, 40]
    diff[em & ~rm] = [30, 80, 220]
    Image.fromarray(diff).save(folder / f"diff-{index:03}.png")
    return {"unmatched_ink_fraction": unmatched, "mean_ink_color_error": color_error,
            "reference_ink_pixels": int(rm.sum()), "edgedoc_ink_pixels": int(em.sum())}


def image_evidence(folder, index, raster):
    """Recognize a visibly painted, shared embedded raster image on an otherwise textless page."""
    from PIL import Image
    if min(raster.get("reference_ink_pixels", 0), raster.get("edgedoc_ink_pixels", 0)) < 64:
        return False

    def visible_images(engine):
        try:
            svg = (folder / engine / f"page-{index:03}.svg").read_text()
        except (OSError, UnicodeError):
            return set()
        images = set()
        pattern = r'<image\b[^>]*\bhref=["\']data:image/(?:png|jpeg|gif|webp);base64,([^"\']+)["\']'
        for payload in re.findall(pattern, svg, flags=re.IGNORECASE):
            try:
                data = base64.b64decode(payload, validate=True)
                with Image.open(io.BytesIO(data)) as image:
                    if image.width * image.height > 40_000_000:
                        continue
                    image.thumbnail((128, 128))
                    pixels = image.convert("RGBA")
                    if any(alpha >= 64 and min(red, green, blue) < 200
                           for red, green, blue, alpha in pixels.getdata()):
                        images.add(hashlib.sha256(data).hexdigest())
            except (ValueError, OSError, Image.DecompressionBombError):
                continue
        return images

    return bool(visible_images("wordcraft") & visible_images("edgedoc"))


def missing_reference_faces(reference_fonts, supplied_fonts):
    """Find Wordcraft layout faces absent from EdgeDoc's supplied fonts."""
    supplied = {(f["sha256"], f.get("index", 0)) for f in supplied_fonts}
    return [f for f in reference_fonts if (f["sha256"], f.get("index")) not in supplied]


def compare_case(out, entry):
    folder = out / entry["id"]
    evidence = [folder / p for p in ["input.docx", "wordcraft/layout.json", "edgedoc/layout.json"]]
    missing = [str(p) for p in evidence if not p.exists()]
    if missing:
        return {"id": entry["id"], "status": "blocked", "missing_evidence": missing}
    reference = json.loads((folder / "wordcraft/layout.json").read_text())
    edge = json.loads((folder / "edgedoc/layout.json").read_text())
    source_hash = digest(folder / "input.docx")
    if any(d.get("source_sha256") != source_hash for d in [edge, reference]) or source_hash != entry["input_sha256"]:
        return {"id": entry["id"], "status": "blocked", "errors": ["Input/render hash mismatch"]}
    if edge.get("build_inputs_sha256") != build_digest() or reference.get("reference") != reference_identity():
        return {"id": entry["id"], "status": "blocked", "errors": ["Engine source changed since rendering"]}
    errors = []
    if reference["reference"].get("dirty"):
        errors.append("Reference checkout contains local changes")
    # A known unsupported request cannot become a pass through an accidental visual match.
    unsupported = [d for d in edge["diagnostics"] if d["code"] != "display.font_substituted"]
    if unsupported:
        errors.append({"edgedoc_diagnostics": unsupported})
    if reference.get("unsupported_draws"):
        errors.append({"reference_adapter_limit": reference["unsupported_draws"]})
    fonts = edge["fonts"]
    if not fonts or any(f.get("sha256") != digest(Path(f["path"])) for f in fonts):
        errors.append("Comparison fonts missing or changed since rendering")
    # Compare collection indices as well as bytes, including paragraph-mark faces
    # that emitted no glyphs.
    missing_faces = missing_reference_faces(reference["fonts"], fonts)
    if missing_faces:
        errors.append({"reference_used_other_font_data": missing_faces})
    if len(reference["pages"]) != len(edge["pages"]):
        errors.append({"page_count": {"wordcraft": len(reference["pages"]), "edgedoc": len(edge["pages"])}})
    text_evidence = bool(reference["pages"] and edge["pages"] and
                         any(page["chars"] for page in reference["pages"]) and
                         any(page["chars"] for page in edge["pages"]))
    image_evidence_pages = []
    pages = []
    for i, (rp, ep) in enumerate(zip(reference["pages"], edge["pages"]), 1):
        page_errors = []
        if abs(rp["width"] - ep["width"]) > SIZE_PT or abs(rp["height"] - ep["height"]) > SIZE_PT:
            page_errors.append("Page dimensions differ")
        if bool(rp["chars"]) != bool(ep["chars"]):
            page_errors.append("Only one renderer omitted this page's text")
        rl, el = lines(rp["chars"]), lines(ep["chars"])
        reference_lines, edge_lines = [r["text"] for r in rl], [r["text"] for r in el]
        if reference_lines != edge_lines:
            page_errors.append({"line_breaks_or_text": {"wordcraft": reference_lines, "edgedoc": edge_lines}})
        max_position = 0.0
        if reference_lines == edge_lines:
            for rr, er in zip(rl, el):
                if len(rr["chars"]) != len(er["chars"]):
                    page_errors.append("Glyph clustering differs")
                    continue
                for rc, ec in zip(rr["chars"], er["chars"]):
                    delta = max(abs(rc["x"] - ec["x"]), abs(rc["baseline"] - ec["baseline"]))
                    max_position = max(max_position, delta)
                    if rc["font_sha256"] != ec.get("font_sha256") or rc.get("font_index") != ec.get("font_index") or rc["gid"] != ec.get("gid") or font_family(rc["font"]) != font_family(ec["font"]) or rc["bold"] != ec["bold"] or rc["italic"] != ec["italic"]:
                        page_errors.append({"font_or_style_mismatch": [rc, ec]})
                        break
                    if abs(rc["size"] - ec["size"]) > SIZE_PT:
                        page_errors.append("Font sizes differ")
                        break
            if max_position > POSITION_PT:
                page_errors.append({"glyph_origin_error_pt": max_position})
        if ep["unmapped_glyphs"]:
            page_errors.append("EdgeDoc paint has unmapped glyphs")
        raster = raster_pair(folder, i, rp["width"], rp["height"])
        if not text_evidence and not rp["chars"] and not ep["chars"]:
            if image_evidence(folder, i, raster):
                image_evidence_pages.append(i)
        if raster["unmatched_ink_fraction"] > UNMATCHED_INK_FRACTION or raster.get("mean_ink_color_error", 1.0) > MEAN_INK_COLOR_ERROR:
            page_errors.append(raster)
        pages.append({"page": i, "passed": not page_errors, "max_glyph_origin_error_pt": max_position, **raster, "errors": page_errors})
    if not text_evidence and not image_evidence_pages:
        errors.append("Empty text rendering is not parity evidence for these fixtures")
    return {"id": entry["id"], "status": "passed" if not errors and all(p["passed"] for p in pages) else "failed", "reference": reference["reference"], "diagnostics": edge["diagnostics"], "pages": pages, "image_evidence_pages": image_evidence_pages, "errors": errors}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=["prepare", "render", "reference", "compare", "run"])
    parser.add_argument("directory", type=Path)
    parser.add_argument("case", nargs="?")
    parser.add_argument("--font", action="append", default=[], help="STYLE=/absolute/font/path; styles: regular, bold, italic, bold-italic")
    parser.add_argument("--fixtures", type=Path, help="Include OOXML fixtures exported by the Rust regression suite")
    parser.add_argument("--retain-manifest", type=Path, help="Retain every original input from an earlier run; append distinct --fixtures")
    parser.add_argument("--corpus-manifest", type=Path, help="Pinned external corpus manifest to include during prepare")
    parser.add_argument("--corpus-directory", type=Path, default=ROOT / "target/external-docx")
    parser.add_argument("--font-config", type=Path, help="JSON list of STYLE=PATH font arguments")
    parser.add_argument("--reuse-reference", action="store_true", help="Reuse reference evidence only when input, reference source/adapter and complete font profile match")
    parser.add_argument("--jobs", type=int, default=1, help="Independent document render workers (1-4; default 1)")
    args = parser.parse_args()
    if not 1 <= args.jobs <= 4:
        parser.error("--jobs must be between 1 and 4")
    if args.font_config:
        args.font.extend(json.loads(args.font_config.read_text()))
    out = args.directory.resolve()
    if args.command == "prepare":
        if args.retain_manifest:
            prepare_retained(out, args.retain_manifest, args.fixtures)
        else:
            prepare(out, args.fixtures)
        if args.corpus_manifest:
            add_external_corpus(out, args.corpus_manifest, args.corpus_directory)
        return 0
    if args.command in ["render", "reference"]:
        if not args.case: parser.error("render/reference requires CASE")
        (render if args.command == "render" else render_reference)(out, args.case, args.font)
        return 0
    manifest = json.loads((out / "manifest.json").read_text())
    render_failures = {}
    reused = []
    if args.command == "run":
        build_reference()
        def render_case(entry):
            errors = []
            reuse = args.reuse_reference and reusable_reference(out, entry["id"], args.font)
            for engine, operation in [("edgedoc", render), ("wordcraft", render_reference)]:
                if engine == "wordcraft" and reuse:
                    continue
                try:
                    operation(out, entry["id"], args.font)
                except Exception as exc:
                    errors.append({"engine": engine, "error": str(exc)})
            return entry["id"], reuse, errors
        with concurrent.futures.ThreadPoolExecutor(max_workers=args.jobs) as pool:
            rendered = list(pool.map(render_case, manifest["cases"]))
        for case_id, reuse, errors in rendered:
            if reuse:
                reused.append(case_id)
            if errors:
                render_failures[case_id] = {"id": case_id, "status": "blocked", "render_errors": errors}
    cases = []
    for entry in manifest["cases"]:
        if entry["id"] in render_failures:
            cases.append(render_failures[entry["id"]])
            continue
        try: cases.append(compare_case(out, entry))
        except Exception as exc: cases.append({"id": entry["id"], "status": "blocked", "error": str(exc)})
    passed = bool(cases) and all(c["status"] == "passed" for c in cases)
    save_json(out / "comparison.json", {"schema": 1, "passed": passed, "comparison_tool_sha256": digest(Path(__file__)), "reference": reference_identity(), "edgedoc_build_inputs_sha256": build_digest(), "scope": manifest["scope"],
        "reference_reused_cases": reused,
        "thresholds": {"position_pt": POSITION_PT, "font_size_pt": SIZE_PT, "unmatched_ink_fraction": UNMATCHED_INK_FRACTION, "mean_ink_color_error": MEAN_INK_COLOR_ERROR, "dpi": DPI, "pixel_neighborhood": 1}, "cases": cases})
    for case in cases: print(f'{case["status"].upper()}: {case["id"]}')
    print(f"Report: {out / 'comparison.json'}")
    return 0 if passed else 1


if __name__ == "__main__":
    sys.exit(main())
