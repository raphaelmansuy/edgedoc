"""Every EdgeDoc task from Python, one example for each.

Run it from the repository root after `make examples`:

    /tmp/edgedoc-venv/bin/python examples/python/basic.py

The script reads the documents that `make examples` writes, so run that first. Each
example prints what it did, and the last one writes a file into `target/examples/`.
"""

from __future__ import annotations

import sys
from pathlib import Path

import edgedoc

ROOT = Path(__file__).resolve().parents[2]
EXAMPLES = ROOT / "target" / "examples"

CSV = (
    b"InvoiceNumber,InvoiceDate,DueDate,Reference,Customer,Contact,Address,City,"
    b"PostalCode,Country,Terms,Notes,Lines\n"
    b"INV-9001,1 November 2026,1 December 2026,PO-1,Contoso,Ada Lovelace,"
    b"1 Main Street,Leeds,LS1 1AA,United Kingdom,Net 30,First,Consulting:2:900.00\n"
    b"INV-9002,2 November 2026,2 December 2026,PO-2,Fabrikam,Grace Hopper,"
    b"2 Market Road,Oslo,0150,Norway,Net 15,Second,Support:1:450.00\n"
)


def heading(number: int, text: str) -> None:
    print(f"\n{number}. {text}")


def read(document: bytes, name: str) -> Path:
    path = EXAMPLES / name
    if not path.exists():
        raise SystemExit(f"{path} is missing. Run `make examples` first.")
    return path


def main() -> int:
    template = read(edgedoc.merge, "invoice-template.docx").read_bytes()
    invoice = (EXAMPLES / "invoices" / "INV-2026-0418.docx").read_bytes()

    heading(1, "read the body text")
    text = edgedoc.plain_text(invoice)
    print(f"   first line: {text.splitlines()[0]}")
    print(f"   {len(text)} characters")

    heading(2, "title and a summary")
    print(f"   title: {edgedoc.doc_title(invoice)}")
    for key, value in edgedoc.document_summary(invoice):
        print(f"   {key}: {value}")

    heading(3, "merge fields in the template")
    names = edgedoc.merge_field_names(template)
    print(f"   {len(names)} fields: {', '.join(names)}")

    heading(4, "stories: body, header, footer, and notes")
    for name, count in edgedoc.story_summary(template):
        if count:
            print(f"   {name}: {count}")
    header = edgedoc.text_of_story(template, "header")
    print(f"   header text: {' '.join(header.split())}")
    footer = edgedoc.text_of_story(template, "footer")
    print(f"   footer text: {' '.join(footer.split())}")

    heading(5, "one record, then every record")
    single = edgedoc.merge_record(template, CSV, 0, "empty")
    print(f"   one record: {len(single)} bytes")
    single_text = edgedoc.plain_text(single)
    print(f"   {single_text.splitlines()[1]}")

    all_records = edgedoc.merge_records(template, CSV, kind="letters", join="auto", missing="empty")
    all_text = edgedoc.plain_text(all_records)
    print(f"   all records: {len(all_records)} bytes")
    print(f"   both invoices present: {'INV-9001' in all_text and 'INV-9002' in all_text}")

    heading(6, "the one-call merge")
    joined = edgedoc.merge_csv(template, CSV)
    print(f"   merge_csv: {len(joined)} bytes")

    heading(7, "write another flavour")
    dotx = edgedoc.write_as_flavor(template, "dotx")
    print(f"   dotx: {len(dotx)} bytes, docx: {len(template)} bytes")

    heading(8, "a path helper, and the same bytes back")
    rewritten = edgedoc.read_and_write(EXAMPLES / "letter-template.docx")
    original = (EXAMPLES / "letter-template.docx").read_bytes()
    print(f"   unedited write is byte-identical: {rewritten == original}")

    heading(9, "an error carries a stable code")
    try:
        edgedoc.plain_text(b"not a document")
    except RuntimeError as error:
        code = str(error).split()[0]
        print(f"   code: {code}")

    heading(10, "password detection")
    protected = bytes([0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1]) + "EncryptionInfo".encode(
        "utf-16-le"
    )
    print(f"   a compound file with EncryptionInfo: {edgedoc.is_password_protected(protected)}")
    print(f"   a plain package: {edgedoc.is_password_protected(template)}")

    heading(11, "write the merged invoices")
    out = EXAMPLES / "python-merged.docx"
    edgedoc.write_path(out, all_records)
    print(f"   wrote {out.relative_to(ROOT)}")

    return 0


if __name__ == "__main__":
    sys.exit(main())
