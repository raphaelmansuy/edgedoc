#!/usr/bin/env python3
"""Validate every generated document against the ECMA-376 schemas.

Word is stricter than most readers. A package can be well-formed XML, open in
python-docx, and still fail in Word with "Word experienced an error trying to open
the file". That is what happens when a mandatory child element is missing.

This script validates each part against the official `wml.xsd`, so the same defect
fails here instead of in front of a user.

    python3 tools/validate_ooxml.py [path ...]

Without a path, it checks `site/assets/documents/` and `target/examples/`.

Exit codes:

    0  every part is valid
    1  a part is invalid
    2  lxml or the schema is not available, so nothing was checked
"""

import glob
import os
import sys
import urllib.request
import zipfile

ROOT = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
SCHEMA_DIR = os.path.join(ROOT, "target", "ooxml-schema")
SCHEMA_URL = "https://raw.githubusercontent.com/python-openxml/python-docx/master/ref/xsd/"

# `wml.xsd` imports these. The XML namespace one is not published, so the script writes it.
PARTS = [
    "wml.xsd",
    "shared-commonSimpleTypes.xsd",
    "shared-math.xsd",
    "shared-customXmlSchemaProperties.xsd",
    "shared-relationshipReference.xsd",
    "dml-main.xsd",
    "dml-picture.xsd",
    "dml-wordprocessingDrawing.xsd",
    "dml-chart.xsd",
    "dml-chartDrawing.xsd",
    "dml-diagram.xsd",
]

XML_XSD = """<?xml version="1.0" encoding="utf-8"?>
<xsd:schema xmlns:xsd="http://www.w3.org/2001/XMLSchema"
  targetNamespace="http://www.w3.org/XML/1998/namespace"
  xmlns:xml="http://www.w3.org/XML/1998/namespace">
  <xsd:attribute name="space">
    <xsd:simpleType>
      <xsd:restriction base="xsd:NCName">
        <xsd:enumeration value="default"/>
        <xsd:enumeration value="preserve"/>
      </xsd:restriction>
    </xsd:simpleType>
  </xsd:attribute>
  <xsd:attribute name="lang" type="xsd:language"/>
</xsd:schema>
"""


def fetch_schema():
    """Download the schemas once, then reuse them from `target/`."""
    os.makedirs(SCHEMA_DIR, exist_ok=True)
    for name in PARTS:
        path = os.path.join(SCHEMA_DIR, name)
        if os.path.exists(path) and os.path.getsize(path) > 1000:
            continue
        try:
            with urllib.request.urlopen(SCHEMA_URL + name, timeout=30) as response:
                data = response.read()
        except Exception as error:
            print(f"  cannot fetch {name}: {error}")
            return False
        with open(path, "wb") as handle:
            handle.write(data)

    xml_path = os.path.join(SCHEMA_DIR, "xml.xsd")
    if not os.path.exists(xml_path):
        with open(xml_path, "w", encoding="utf8") as handle:
            handle.write(XML_XSD)

    # Point the XML namespace import at the local file.
    wml = os.path.join(SCHEMA_DIR, "wml.xsd")
    with open(wml, encoding="utf8") as handle:
        text = handle.read()
    if 'namespace="http://www.w3.org/XML/1998/namespace"/>' in text:
        text = text.replace(
            'namespace="http://www.w3.org/XML/1998/namespace"/>',
            'namespace="http://www.w3.org/XML/1998/namespace" schemaLocation="xml.xsd"/>',
        )
        with open(wml, "w", encoding="utf8") as handle:
            handle.write(text)
    return True


def targets(arguments):
    if arguments:
        found = []
        for argument in arguments:
            path = os.path.abspath(argument)
            if os.path.isdir(path):
                found += sorted(glob.glob(os.path.join(path, "*.docx")))
            else:
                found.append(path)
        return found
    found = sorted(glob.glob(os.path.join(ROOT, "site", "assets", "documents", "*.docx")))
    found += sorted(glob.glob(os.path.join(ROOT, "target", "examples", "**", "*.docx"), recursive=True))
    return found


def main(argv):
    try:
        from lxml import etree
    except ImportError:
        print("  lxml is not installed. Run: pip install lxml")
        return 2

    if not fetch_schema():
        print("  the schema is not available, so nothing was checked")
        return 2

    try:
        schema = etree.XMLSchema(etree.parse(os.path.join(SCHEMA_DIR, "wml.xsd")))
    except Exception as error:
        print(f"  the schema does not load: {str(error)[:200]}")
        return 2

    files = targets(argv[1:])
    if not files:
        print("  no document to check. Run `make examples` first.")
        return 2

    invalid = 0
    for path in files:
        try:
            package = zipfile.ZipFile(path)
        except zipfile.BadZipFile:
            print(f"  {os.path.basename(path)}: not a ZIP package")
            invalid += 1
            continue
        for name in package.namelist():
            if not name.startswith("word/") or not name.endswith(".xml") or name.endswith(".rels"):
                continue
            try:
                document = etree.fromstring(package.read(name))
            except Exception as error:
                print(f"  {os.path.basename(path)} :: {name}: {str(error)[:120]}")
                invalid += 1
                continue
            if not schema.validate(etree.ElementTree(document)):
                first = schema.error_log[0] if len(schema.error_log) else None
                detail = f"line {first.line}: {first.message[:130]}" if first else "invalid"
                print(f"  {os.path.basename(path)} :: {name}: {detail}")
                invalid += 1

    if invalid:
        print(f"\n  {invalid} invalid part(s) in {len(files)} document(s).")
        return 1
    print(f"  {len(files)} documents valid against the ECMA-376 schema.")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
