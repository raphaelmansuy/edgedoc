"""The Python API, checked against documents that the Rust tests also read.

Run from the repository root after `make examples`:

    /tmp/edgedoc-venv/bin/python -m pytest examples/python -q

A skipped case means the example documents are not built yet.
"""

from __future__ import annotations

from pathlib import Path

import pytest

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


def example(name: str) -> bytes:
    path = EXAMPLES / name
    if not path.exists():
        pytest.skip(f"{name} is missing. Run `make examples` first.")
    return path.read_bytes()


@pytest.fixture(scope="module")
def template() -> bytes:
    return example("invoice-template.docx")


@pytest.fixture(scope="module")
def invoice() -> bytes:
    return example("invoices/INV-2026-0418.docx")


def test_bytes_in_bytes_out(template: bytes) -> None:
    """A writer returns `bytes`, not a list of integers."""
    out = edgedoc.merge_record(template, CSV, 0, "empty")
    assert isinstance(out, bytes)
    assert out[:2] == b"PK"


def test_plain_text(invoice: bytes) -> None:
    text = edgedoc.plain_text(invoice)
    assert text.startswith("Tax invoice")
    assert "Northwind Traders" in text


def test_title_and_summary(invoice: bytes) -> None:
    assert edgedoc.doc_title(invoice) == "Invoice"
    summary = dict(edgedoc.document_summary(invoice))
    assert summary["flavor"] == "docx"
    assert int(summary["parts"]) > 5


def test_merge_fields(template: bytes) -> None:
    names = edgedoc.merge_field_names(template)
    assert "InvoiceNumber" in names
    assert "Total" in names
    assert len(names) == len(set(names))


def test_stories(template: bytes) -> None:
    counts = dict(edgedoc.story_summary(template))
    assert counts["body"] == 1
    assert counts["header"] == 1
    assert counts["footer"] == 1
    header = edgedoc.text_of_story(template, "header")
    assert "EdgeDoc Studio" in header


def test_unknown_story_raises(template: bytes) -> None:
    with pytest.raises(RuntimeError) as caught:
        edgedoc.text_of_story(template, "sidebar")
    assert str(caught.value).startswith("unsupported")


def test_merge_one_record(template: bytes) -> None:
    out = edgedoc.merge_record(template, CSV, 1, "empty")
    text = edgedoc.plain_text(out)
    assert "INV-9002" in text
    assert "Fabrikam" in text
    assert "INV-9001" not in text


def test_merge_every_record(template: bytes) -> None:
    out = edgedoc.merge_records(template, CSV, "letters", "auto", "empty")
    text = edgedoc.plain_text(out)
    assert "INV-9001" in text
    assert "INV-9002" in text


def test_one_call_merge(template: bytes) -> None:
    out = edgedoc.merge_csv(template, CSV)
    assert edgedoc.plain_text(out).count("Invoice INV-") == 2


def test_flavour(template: bytes) -> None:
    dotx = edgedoc.write_as_flavor(template, "dotx")
    assert isinstance(dotx, bytes)
    assert len(dotx) != len(template) or dotx != template


def test_unknown_flavour_raises(template: bytes) -> None:
    with pytest.raises(RuntimeError) as caught:
        edgedoc.write_as_flavor(template, "rtf")
    assert str(caught.value).startswith("unsupported")


def test_missing_field_mode(template: bytes) -> None:
    """A field with no column follows the mode: error, placeholder, or empty."""
    # The template holds `Subtotal` and `VAT`, which have no CSV column.
    with pytest.raises(RuntimeError) as caught:
        edgedoc.merge_record(template, CSV, 0, "error")
    assert "Subtotal" in str(caught.value)
    assert str(caught.value).startswith("merge.unknown_field")

    placeholder = edgedoc.merge_record(template, CSV, 0, "placeholder")
    assert "«Subtotal»" in edgedoc.plain_text(placeholder)

    empty = edgedoc.merge_record(template, CSV, 0, "empty")
    assert "«Subtotal»" not in edgedoc.plain_text(empty)
    assert len(empty) > 1000


def test_error_code() -> None:
    with pytest.raises(RuntimeError) as caught:
        edgedoc.plain_text(b"not a document")
    assert str(caught.value).split()[0] == "package.not_zip"


def test_password_detection(template: bytes) -> None:
    protected = bytes([0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1]) + "EncryptionInfo".encode(
        "utf-16-le"
    )
    assert edgedoc.is_password_protected(protected) is True
    assert edgedoc.is_password_protected(template) is False


def test_part_names(template: bytes) -> None:
    names = edgedoc.part_names(template)
    assert any(name.endswith("document.xml") for name in names)


def test_edited_flag(template: bytes) -> None:
    assert edgedoc.is_edited(template) is False


def test_read_and_write_is_identical() -> None:
    path = EXAMPLES / "letter-template.docx"
    if not path.exists():
        pytest.skip("letter-template.docx is missing. Run `make examples` first.")
    original = path.read_bytes()
    assert edgedoc.read_and_write(path) == original
