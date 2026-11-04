#!/usr/bin/env python3
"""Check the generated documents against an independent Word reader.

The strongest test of a Word writer is another Word reader. This script reads each
document with WordCraft and fails when the reader rejects a file, reports no page,
or gives a different text after it saves the file again.

    python3 tools/verify_with_wordcraft.py

Set `WORDCRAFT_CLI` to the `wordcraft-cli` binary. Without it, the script looks in the
WordCraft checkout next to this repository. When no binary is available, the script
prints what to run and exits with status 2, so a caller can tell "not checked" from
"checked and failed".

The checks need no network and no Word installation.
"""

import json
import os
import shutil
import subprocess
import sys
import tempfile

ROOT = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
EXAMPLES = os.path.join(ROOT, "target", "examples")
WORDCRAFT = os.path.abspath(os.path.join(ROOT, "..", "wordcraft"))

PASS, FAIL, SKIP = 0, 1, 2


def find_cli():
    env = os.environ.get("WORDCRAFT_CLI")
    if env:
        return env if os.access(env, os.X_OK) else None
    candidates = [
        os.path.join(WORDCRAFT, "target", "debug", "wordcraft-cli"),
        os.path.join(WORDCRAFT, "target", "release", "wordcraft-cli"),
    ]
    for path in candidates:
        if os.access(path, os.X_OK):
            return path
    found = shutil.which("wordcraft-cli")
    return found


def run(cli, *args):
    return subprocess.run([cli, *args], capture_output=True, text=True)


def documents():
    if not os.path.isdir(EXAMPLES):
        return []
    found = []
    for base, _, names in os.walk(EXAMPLES):
        for name in sorted(names):
            if name.endswith(".docx"):
                found.append(os.path.join(base, name))
    return sorted(found)


def main():
    cli = find_cli()
    if cli is None:
        print("  WordCraft CLI not found. Build it with:")
        print("    cargo +1.95.0 build -p wordcraft-cli")
        print("  in the WordCraft checkout, or set WORDCRAFT_CLI.")
        return SKIP

    files = documents()
    if not files:
        print("  No documents. Run `make examples` first.")
        return SKIP

    failures = 0
    with tempfile.TemporaryDirectory() as work:
        for path in files:
            rel = os.path.relpath(path, EXAMPLES)
            info = run(cli, "info", path)
            if info.returncode != 0 or '"pages"' not in info.stdout:
                print(f"  {rel:<44} reader rejected the file")
                if info.stderr.strip():
                    print(f"      {info.stderr.strip().splitlines()[0]}")
                failures += 1
                continue
            pages = json.loads(info.stdout)["pages"]
            if pages < 1:
                print(f"  {rel:<44} reader found no page")
                failures += 1
                continue

            out = os.path.join(work, rel)
            os.makedirs(os.path.dirname(out), exist_ok=True)
            saved = run(cli, "convert", path, out)
            if saved.returncode != 0:
                print(f"  {rel:<44} reader could not save the file")
                failures += 1
                continue

            before = run(cli, "text", path).stdout
            after = run(cli, "text", out).stdout
            if before != after:
                print(f"  {rel:<44} text changed after the reader saved it")
                failures += 1
                continue

            print(f"  {rel:<44} ok   pages={pages}  text survives a save")

    if failures:
        print(f"\n  {failures} document(s) failed.")
        return FAIL
    print(f"\n  {len(files)} documents verified with WordCraft.")
    return PASS


if __name__ == "__main__":
    sys.exit(main())
