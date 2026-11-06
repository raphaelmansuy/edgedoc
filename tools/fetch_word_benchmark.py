#!/usr/bin/env python3
"""Download and verify the pinned Word-rendered benchmark pairs for local testing."""
import argparse
import concurrent.futures
import hashlib
import json
import os
import tempfile
import urllib.request
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
LIMIT = 32 * 1024 * 1024


def digest(path):
    h = hashlib.sha256()
    with path.open('rb') as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b''):
            h.update(chunk)
    return h.hexdigest()


def fetch(entry, kind, directory):
    name = entry['id'] + ('.docx' if kind == 'docx' else '.pdf')
    path = directory / name
    expected = entry[kind + '_sha256']
    if path.exists():
        if digest(path) != expected:
            raise ValueError(f'Refusing to replace {path}: checksum differs')
        return {'id': entry['id'], 'kind': kind, 'status': 'verified', 'bytes': path.stat().st_size}
    request = urllib.request.Request(entry[kind + '_url'], headers={'User-Agent': 'EdgeDoc-parity-inspection/1.0'})
    fd, temporary = tempfile.mkstemp(prefix='.download-', dir=directory)
    temp = Path(temporary)
    try:
        total = 0
        with os.fdopen(fd, 'wb') as output, urllib.request.urlopen(request, timeout=60) as response:
            while True:
                chunk = response.read(1024 * 1024)
                if not chunk:
                    break
                total += len(chunk)
                if total > LIMIT:
                    raise ValueError(f'{name} exceeds the {LIMIT}-byte download limit')
                output.write(chunk)
        if digest(temp) != expected:
            raise ValueError(f'{name} failed SHA-256 verification')
        if kind == 'docx':
            with zipfile.ZipFile(temp) as archive:
                if not {'[Content_Types].xml', 'word/document.xml'}.issubset(archive.namelist()):
                    raise ValueError(f'{name} is not a conventional DOCX package')
        else:
            with temp.open('rb') as source:
                if source.read(5) != b'%PDF-':
                    raise ValueError(f'{name} is not a PDF')
        os.link(temp, path)  # Fails rather than replacing a concurrently created file.
        return {'id': entry['id'], 'kind': kind, 'status': 'verified', 'bytes': total}
    finally:
        temp.unlink(missing_ok=True)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--manifest', type=Path, default=ROOT / 'tests/corpus/word-benchmark-manifest.json')
    parser.add_argument('--out', type=Path, default=ROOT / 'target/word-benchmark-corpus')
    args = parser.parse_args()
    manifest = json.loads(args.manifest.read_text())
    args.out.mkdir(parents=True, exist_ok=True)
    jobs = [(entry, kind) for entry in manifest['documents'] for kind in ('docx', 'pdf')]
    results = []
    with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
        futures = {pool.submit(fetch, entry, kind, args.out): (entry['id'], kind) for entry, kind in jobs}
        for future in concurrent.futures.as_completed(futures):
            ident, kind = futures[future]
            try:
                result = future.result()
            except Exception as error:
                result = {'id': ident, 'kind': kind, 'status': 'failed', 'error': str(error)}
            results.append(result)
            if len(results) % 10 == 0:
                print(f'Verified {sum(r["status"] == "verified" for r in results)}/{len(jobs)} files ({len(results)} attempted)', flush=True)
    report = {'revision': manifest['revision'], 'manifest_sha256': digest(args.manifest), 'files': sorted(results, key=lambda r: (r['id'], r['kind']))}
    (args.out / 'download-report.json').write_text(json.dumps(report, indent=2) + '\n')
    failures = [r for r in results if r['status'] != 'verified']
    print(f'{len(results) - len(failures)}/{len(jobs)} files verified; {len(failures)} failed', flush=True)
    if failures:
        raise SystemExit(1)


if __name__ == '__main__':
    main()
