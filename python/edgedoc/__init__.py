"""EdgeDoc for Python. Every function holds bytes, so the caller owns the files.

The native module is `edgedoc._native`. This package gives the functions the names
that a Python reader expects, and it adds the two helpers that read and write a path.

    import edgedoc

    data = edgedoc.read_path("template.docx")
    merged = edgedoc.merge_csv(data, csv_bytes)
    edgedoc.write_path("out.docx", merged)
"""

from pathlib import Path

from edgedoc._native import (
    doc_title,
    document_summary,
    is_edited,
    is_password_protected,
    markdown_to_docx,
    merge,
    merge_field_names,
    merge_record,
    merge_records,
    open_markdown,
    part_names,
    plain_text,
    story_summary,
    text_of_story,
    write_as_flavor,
)

__version__ = "0.1.0"

merge_csv = merge


def read_path(path: str | Path) -> bytes:
    """Read a document from a path."""
    return Path(path).read_bytes()


def write_path(path: str | Path, data: bytes) -> None:
    """Write a document to a path. The parent folder must exist."""
    Path(path).write_bytes(data)


def read_and_write(path: str | Path) -> bytes:
    """Read a document and write it again in the same flavour.

    The result is byte-identical to the input, because an unedited package keeps its
    original bytes. This is the check that the file is a package the library can read.
    """
    return write_as_flavor(read_path(path), "docx")


__all__ = [
    "doc_title",
    "document_summary",
    "is_edited",
    "is_password_protected",
    "markdown_to_docx",
    "merge",
    "merge_csv",
    "merge_field_names",
    "merge_record",
    "merge_records",
    "open_markdown",
    "part_names",
    "plain_text",
    "read_and_write",
    "read_path",
    "story_summary",
    "text_of_story",
    "write_as_flavor",
    "write_path",
]
