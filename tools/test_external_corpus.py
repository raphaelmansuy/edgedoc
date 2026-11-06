"""Integrity checks for the pinned external corpus downloader, without network access."""
import hashlib
import io
import unittest
import zipfile
from pathlib import Path
from tempfile import TemporaryDirectory

import fetch_external_corpus as corpus


class CorpusIntegrityTests(unittest.TestCase):
    def fixture(self):
        output = io.BytesIO()
        with zipfile.ZipFile(output, 'w') as archive:
            archive.writestr('word/document.xml', f'<w:document xmlns:w="{corpus.W[1:-1]}"><w:body><w:p><w:r><w:t>Original fixture</w:t></w:r></w:p></w:body></w:document>')
        data = output.getvalue()
        entry = {'id': 'fixture', 'bytes': len(data), 'features': ['paragraphs'],
                 'sha256': hashlib.sha256(data).hexdigest(),
                 'git_blob_sha1': hashlib.sha1(b'blob ' + str(len(data)).encode() + b'\0' + data).hexdigest()}
        return entry, data

    def test_exact_bytes_and_structure_are_verified(self):
        entry, data = self.fixture()
        corpus.validate(entry, data)
        with TemporaryDirectory() as folder:
            path = Path(folder) / 'fixture.docx'
            path.write_bytes(data)
            result = corpus.inventory(entry, path)
            self.assertEqual(result['structure']['p'], 1)
            self.assertEqual(result['text_characters'], len('Original fixture'))

    def test_changed_or_truncated_download_is_rejected(self):
        entry, data = self.fixture()
        for changed in [data[:-1], b'x' + data[1:]]:
            with self.assertRaises(ValueError):
                corpus.validate(entry, changed)

    def test_git_blob_identity_is_independently_checked(self):
        entry, data = self.fixture()
        entry['git_blob_sha1'] = '0' * 40
        with self.assertRaises(ValueError):
            corpus.validate(entry, data)

    def test_unpinned_or_redirected_source_is_rejected(self):
        entry = {'repository': 'apache/poi', 'revision': 'main', 'path': 'test.docx', 'url': 'https://example.com/test.docx'}
        with self.assertRaises(ValueError):
            corpus.source_url(entry)
        entry['revision'] = 'a' * 40
        with self.assertRaises(ValueError):
            corpus.source_url(entry)

    def test_corrupt_existing_file_is_not_overwritten(self):
        entry, _ = self.fixture()
        with TemporaryDirectory() as folder:
            path = Path(folder) / 'fixture.docx'
            path.write_bytes(b'preserve this invalid local file')
            with self.assertRaises(ValueError):
                corpus.fetch(entry, Path(folder))
            self.assertEqual(path.read_bytes(), b'preserve this invalid local file')

    def test_verified_upstream_negative_fixture_is_preserved_and_inventoried(self):
        data = b'not a DOCX archive; negative fixture'
        entry = {'id': 'negative', 'bytes': len(data), 'features': ['negative'],
                 'valid_docx_archive': False, 'sha256': hashlib.sha256(data).hexdigest(),
                 'git_blob_sha1': hashlib.sha1(b'blob ' + str(len(data)).encode() + b'\0' + data).hexdigest()}
        with TemporaryDirectory() as folder:
            path = Path(folder) / 'negative.docx'
            path.write_bytes(data)
            result = corpus.inventory(entry, path)
            self.assertIn('negative', result['archive_status'])
            with self.assertRaises(ValueError):
                corpus.validate(entry, data + b'changed')


if __name__ == '__main__':
    unittest.main()
