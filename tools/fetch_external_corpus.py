#!/usr/bin/env python3
"""Fetch and inventory the commit-pinned external DOCX rendering corpus."""
import argparse
import concurrent.futures
import hashlib
import io
import json
import re
import urllib.parse
import urllib.request
import zipfile
from collections import Counter
from pathlib import Path
from xml.etree import ElementTree as ET

ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / 'tests/corpus/external-manifest.json'
W = '{http://schemas.openxmlformats.org/wordprocessingml/2006/main}'
MAX_DOWNLOAD = 16 * 1024 * 1024
MAX_UNCOMPRESSED = 64 * 1024 * 1024


def validate(entry, content):
    if len(content) != entry['bytes'] or len(content) > MAX_DOWNLOAD:
        raise ValueError(f"Unexpected size: {entry['id']}")
    if hashlib.sha256(content).hexdigest() != entry['sha256']:
        raise ValueError(f"SHA-256 mismatch: {entry['id']}")
    blob = b'blob ' + str(len(content)).encode() + b'\0' + content
    if hashlib.sha1(blob).hexdigest() != entry['git_blob_sha1']:
        raise ValueError(f"Upstream Git blob mismatch: {entry['id']}")
    # Upstream fuzz and password fixtures may deliberately not be DOCX ZIP archives.
    # Preserve their verified bytes as negative inputs, without decompressing them.
    if entry.get('valid_docx_archive') is False:
        return
    with zipfile.ZipFile(io.BytesIO(content)) as archive:
        files = archive.infolist()
        if len(files) > 2048 or sum(f.file_size for f in files) > MAX_UNCOMPRESSED:
            raise ValueError(f"Archive budget exceeded: {entry['id']}")
        if 'word/document.xml' not in archive.namelist():
            raise ValueError(f"Not a DOCX document: {entry['id']}")


def source_url(entry):
    if entry['repository'] not in ['apache/poi', 'plutext/docx4j'] or not re.fullmatch(r'[0-9a-f]{40}', entry['revision']):
        raise ValueError('Unrecognized upstream identity')
    expected = f"https://raw.githubusercontent.com/{entry['repository']}/{entry['revision']}/" + urllib.parse.quote(entry['path'], safe='/')
    if entry['url'] != expected:
        raise ValueError('Download URL does not match pinned source')
    return expected


def fetch(entry, directory):
    path = directory / (entry['id'] + '.docx')
    if path.exists():
        validate(entry, path.read_bytes())
        return path
    with urllib.request.urlopen(source_url(entry), timeout=45) as response:
        content = response.read(MAX_DOWNLOAD + 1)
    validate(entry, content)
    temporary = path.with_suffix('.docx.part')
    temporary.write_bytes(content)
    temporary.replace(path)
    return path


def inventory(entry, path):
    content = path.read_bytes()
    validate(entry, content)
    if entry.get('valid_docx_archive') is False:
        return {'id': entry['id'], 'sha256': entry['sha256'], 'bytes': len(content),
                'text_characters': 0, 'features': entry['features'],
                'archive_status': 'upstream negative or unsupported archive fixture'}
    with zipfile.ZipFile(io.BytesIO(content)) as archive:
        trees = []
        fonts = set()
        for name in archive.namelist():
            if name.startswith('word/') and name.endswith('.xml'):
                tree = ET.fromstring(archive.read(name))
                trees.append(tree)
                for el in tree.iter(W + 'rFonts'):
                    fonts.update(v for k, v in el.attrib.items() if k in [W + 'ascii', W + 'hAnsi', W + 'eastAsia', W + 'cs'])
        counts = Counter(el.tag.removeprefix(W) for tree in trees for el in tree.iter() if el.tag.startswith(W))
        text = ''.join(el.text or '' for tree in trees for el in tree.iter(W + 't'))
        props = ET.fromstring(archive.read('docProps/app.xml')) if 'docProps/app.xml' in archive.namelist() else None
        page_hint = next((e.text for e in props.iter() if e.tag.endswith('}Pages')), None) if props is not None else None
        return {'id': entry['id'], 'sha256': entry['sha256'], 'bytes': len(content), 'text_characters': len(text),
                'authored_page_hint': page_hint, 'fonts_requested': sorted(fonts), 'features': entry['features'],
                'structure': {key: counts[key] for key in ['p', 'tbl', 'tr', 'tc', 'sectPr', 'footnote', 'endnote', 'footnoteReference', 'endnoteReference', 'drawing', 'pict', 'fldSimple', 'instrText', 'numPr', 'tab', 'vMerge', 'framePr']}}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--directory', type=Path, default=ROOT / 'target/external-docx')
    parser.add_argument('--manifest', type=Path, default=MANIFEST)
    parser.add_argument('--verify-only', action='store_true')
    args = parser.parse_args()
    manifest = json.loads(args.manifest.read_text())
    args.directory.mkdir(parents=True, exist_ok=True)
    if not args.verify_only:
        with concurrent.futures.ThreadPoolExecutor(max_workers=6) as pool:
            list(pool.map(lambda entry: fetch(entry, args.directory), manifest['documents']))
    rows = []
    for entry in manifest['documents']:
        try:
            rows.append(inventory(entry, args.directory / (entry['id'] + '.docx')))
        except (ET.ParseError, KeyError, zipfile.BadZipFile, RuntimeError) as error:
            # Malformed XML and encrypted archive entries are upstream negative fixtures.
            rows.append({'id': entry['id'], 'sha256': entry['sha256'], 'bytes': entry['bytes'],
                         'text_characters': 0, 'features': entry['features'], 'inventory_error': str(error)})
    summary = {'schema': 1, 'manifest_sha256': hashlib.sha256(args.manifest.read_bytes()).hexdigest(), 'documents': rows,
               'total_documents': len(rows), 'total_text_characters': sum(row['text_characters'] for row in rows),
               'feature_tags': dict(Counter(feature for row in rows for feature in row['features']))}
    (args.directory / 'inventory.json').write_text(json.dumps(summary, indent=2) + '\n')
    print(f"Verified {len(rows)} DOCX documents, {summary['total_text_characters']:,} text characters")
    print(f"Inventory: {args.directory / 'inventory.json'}")


if __name__ == '__main__':
    main()
