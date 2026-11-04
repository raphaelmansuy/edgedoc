"""Type stubs for `edgedoc`.

Every function takes and returns bytes. An error raises `RuntimeError` with a message
that starts with a stable code, such as `package.not_zip`. Use `error_code` on the
message to branch on the code.
"""

def merge(template: bytes, csv: bytes) -> bytes:
    """Merge every CSV record into the template and return one joined document."""

merge_csv = merge

def plain_text(document: bytes) -> str:
    """The plain text of the body. A field contributes its cached result."""

def doc_title(document: bytes) -> str:
    """The title from the core properties. An absent title is an empty string."""

def merge_field_names(document: bytes) -> list[str]:
    """The merge field names in the body, the headers, and the footers, in order."""

def part_names(document: bytes) -> list[str]:
    """The package part names, so a caller can see what stays in the file."""

def text_of_story(document: bytes, story: str) -> str:
    """The text of one story.

    `story` is `body`, `header`, `footer`, `footnote`, `endnote`, or `comment`.
    An unknown name raises `RuntimeError`.
    """

def story_summary(document: bytes) -> list[tuple[str, int]]:
    """The number of stories of each kind, as `(name, count)` pairs."""

def merge_record(template: bytes, csv: bytes, record: int, missing: str = "error") -> bytes:
    """Replace every field with one record and return that document.

    `record` is a zero-based row index. `missing` is `error`, `empty`, or `placeholder`.
    """

def merge_records(
    template: bytes,
    csv: bytes,
    kind: str = "letters",
    join: str = "auto",
    missing: str = "error",
) -> bytes:
    """Merge every record and join the result.

    `kind` is `letters`, `directory`, `labels`, or `emails`.
    `join` is `auto`, `page-break`, or `none`.
    `missing` is `error`, `empty`, or `placeholder`.
    """

def write_as_flavor(document: bytes, flavor: str) -> bytes:
    """Write the package in another flavour: `docx`, `docm`, `dotx`, or `dotm`."""

def is_password_protected(document: bytes) -> bool:
    """True when the bytes hold a package that a password protects."""

def document_summary(document: bytes) -> list[tuple[str, str]]:
    """A summary of the package as `(key, value)` pairs, with the values as text."""

def is_edited(document: bytes) -> bool:
    """True when the document has an edit that a save would write."""

def read_path(path: str) -> bytes:
    """Read a document from a path."""

def write_path(path: str, data: bytes) -> None:
    """Write a document to a path."""

def read_and_write(path: str) -> bytes:
    """Read a document and write it again in the same flavour."""

def error_code(message: str) -> str:
    """The first word of a message, which is the stable error code."""
