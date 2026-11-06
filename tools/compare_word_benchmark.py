#!/usr/bin/env python3
"""Compare pinned Word-benchmark DOCX inputs with Wordcraft and inspect PDF page counts."""
import argparse
import concurrent.futures
import json
import os
import re
import shutil
import subprocess
from pathlib import Path

import wordcraft_parity as w

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--manifest', type=Path, default=ROOT / 'tests/corpus/word-benchmark-manifest.json')
    parser.add_argument('--corpus', type=Path, default=ROOT / 'target/word-benchmark-corpus')
    parser.add_argument('--out', type=Path, default=ROOT / 'target/wordcraft-parity/word-benchmark-48')
    parser.add_argument('--font-config', type=Path, required=True)
    parser.add_argument('--jobs', type=int, default=3, choices=range(1, 5))
    args = parser.parse_args()
    manifest = json.loads(args.manifest.read_text())
    fonts = json.loads(args.font_config.read_text())
    args.out.mkdir(parents=True, exist_ok=True)
    entries = []
    for record in manifest['documents']:
        docx = args.corpus / (record['id'] + '.docx')
        pdf = args.corpus / (record['id'] + '.pdf')
        if w.digest(docx) != record['docx_sha256'] or w.digest(pdf) != record['pdf_sha256']:
            raise ValueError(f'Input checksum mismatch for {record["id"]}')
        folder = args.out / ('wordbench-' + record['id'])
        folder.mkdir(exist_ok=True)
        target = folder / 'input.docx'
        if target.exists() and w.digest(target) != record['docx_sha256']:
            raise ValueError(f'Refusing to replace different input: {target}')
        if not target.exists():
            shutil.copyfile(docx, target)
        entries.append({'id': folder.name, 'input_sha256': record['docx_sha256'],
                        'features': ['word-benchmark'], 'word_pdf_sha256': record['pdf_sha256'],
                        'word_pdf': str(pdf.resolve()), 'source': record})
    w.save_json(args.out / 'manifest.json', {'schema': 1, 'scope': 'Pinned Word benchmark documents',
        'source_manifest_sha256': w.digest(args.manifest), 'cases': entries})
    w.build_reference()
    build_digest, reference_identity = w.build_digest, w.reference_identity
    build_hash, reference = build_digest(), reference_identity()
    tool_hash = w.digest(ROOT / 'tools/wordcraft_parity.py')
    w.build_digest = lambda *a, **k: build_hash if not a and not k else build_digest(*a, **k)
    w.reference_identity = lambda: reference
    original_raster = w.raster_pair

    def compact(folder, index, width, height):
        try:
            return original_raster(folder, index, width, height)
        finally:
            for path in (folder / 'edgedoc' / f'page-{index:03}.png',
                         folder / 'wordcraft' / f'page-{index:03}.png', folder / f'diff-{index:03}.png'):
                path.unlink(missing_ok=True)
    w.raster_pair = compact

    def compare(entry):
        name, errors = entry['id'], []
        pdf = Path(entry['word_pdf'])
        try:
            info = subprocess.run(['pdfinfo', str(pdf)], check=True, capture_output=True, text=True, timeout=30).stdout
            found = re.search(r'^Pages:\s+(\d+)\s*$', info, flags=re.MULTILINE)
            pdf_pages = int(found.group(1)) if found else None
        except (subprocess.SubprocessError, OSError, ValueError):
            pdf_pages = None
        for engine, operation in [('edgedoc', w.render), ('wordcraft', w.render_reference)]:
            try:
                operation(args.out, name, fonts)
            except Exception as exc:
                errors.append({'engine': engine, 'error': str(exc)})
        if errors:
            result = {'id': name, 'status': 'blocked', 'render_errors': errors}
        else:
            try:
                result = w.compare_case(args.out, entry)
            except Exception as exc:
                result = {'id': name, 'status': 'blocked', 'error': str(exc)}
        result['word_pdf'] = {'sha256': entry['word_pdf_sha256'], 'pdfinfo_pages': pdf_pages,
                              'dataset_pages': entry['source']['reference_pages'],
                              'dataset_attribution': 'Word for Mac print to PDF, as reported by the source corpus'}
        for engine in ('edgedoc', 'wordcraft'):
            path = args.out / name / engine / 'layout.json'
            try:
                result['word_pdf'][engine + '_pages'] = len(json.loads(path.read_text())['pages'])
            except (OSError, ValueError, KeyError):
                result['word_pdf'][engine + '_pages'] = None
        return result

    results = []
    with concurrent.futures.ThreadPoolExecutor(max_workers=args.jobs) as pool:
        futures = [pool.submit(compare, entry) for entry in entries]
        for future in concurrent.futures.as_completed(futures):
            results.append(future.result())
            if len(results) % 5 == 0:
                w.save_json(args.out / 'checkpoint.json', {'cases': results})
                print(f'Compared {len(results)}/{len(entries)} documents', flush=True)
    if build_digest() != build_hash or reference_identity() != reference or w.digest(ROOT / 'tools/wordcraft_parity.py') != tool_hash:
        raise RuntimeError('Renderer, reference, or comparison tool changed during the run')
    report = {'schema': 1, 'scope': 'DOCX comparison against Wordcraft; Word PDF page counts are supplementary, not visual parity verdicts',
              'source_manifest_sha256': w.digest(args.manifest), 'edgedoc_build_inputs_sha256': build_hash,
              'comparison_tool_sha256': tool_hash, 'reference': reference,
              'cases': sorted(results, key=lambda case: case['id'])}
    w.save_json(args.out / 'comparison.json', report)
    print('Finished', flush=True)


if __name__ == '__main__':
    main()
