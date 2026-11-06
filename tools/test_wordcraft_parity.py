"""Negative tests for the parity gate; these synthetic records are never reference evidence."""
import copy
import base64
import io
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import wordcraft_parity as parity


class ParityGateTests(unittest.TestCase):
    def test_build_digest_changes_with_embedded_font_bytes(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "crates").mkdir()
            (root / "Cargo.toml").write_text("[workspace]\n")
            (root / "Cargo.lock").write_text("version = 3\n")
            font_dir = root / "assets/fonts/subset"
            font_dir.mkdir(parents=True)
            font = font_dir / "face.ttf"
            font.write_bytes(b"first font")
            before = parity.build_digest(root)
            font.write_bytes(b"changed font")
            self.assertNotEqual(before, parity.build_digest(root))
            fallback = root / "assets/fonts/SourceSans3-Regular.ttf"
            fallback.write_bytes(b"fallback font")
            with_fallback = parity.build_digest(root)
            fallback.write_bytes(b"changed fallback")
            self.assertNotEqual(with_fallback, parity.build_digest(root))

    def test_reference_reuse_rejects_changed_fonts_source_adapter_or_missing_pages(self):
        with tempfile.TemporaryDirectory() as temporary:
            out = Path(temporary)
            folder = out / "case"
            (folder / "wordcraft").mkdir(parents=True)
            (folder / "edgedoc").mkdir()
            source, font = folder / "input.docx", folder / "font.ttf"
            source.write_bytes(b"test source")
            font.write_bytes(b"test font identity")
            reference = {"source_sha256": parity.digest(source), "reference": {"adapter": "original"}, "pages": [{}]}
            edge = {"source_sha256": parity.digest(source), "fonts": [{"path": str(font), "sha256": parity.digest(font), "bold": False, "italic": False}]}
            parity.save_json(folder / "wordcraft/layout.json", reference)
            parity.save_json(folder / "edgedoc/layout.json", edge)
            page = folder / "wordcraft/page-001.svg"
            page.write_text("<svg/>")
            fonts = ["regular=" + str(font)]
            with patch.object(parity, "reference_identity", return_value={"adapter": "original"}):
                self.assertTrue(parity.reusable_reference(out, "case", fonts))
                self.assertFalse(parity.reusable_reference(out, "case", ["bold=" + str(font)]))
                font.write_bytes(b"changed font")
                self.assertFalse(parity.reusable_reference(out, "case", fonts))
                font.write_bytes(b"test font identity")
                source.write_bytes(b"changed source")
                self.assertFalse(parity.reusable_reference(out, "case", fonts))
                source.write_bytes(b"test source")
                page.unlink()
                self.assertFalse(parity.reusable_reference(out, "case", fonts))
                page.write_text("<svg/>")
            with patch.object(parity, "reference_identity", return_value={"adapter": "changed"}):
                self.assertFalse(parity.reusable_reference(out, "case", fonts))

    def test_retained_inputs_keep_failures_and_deduplicate_only_new_fixtures(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            prior, fresh, out = root / "prior", root / "fresh", root / "out"
            fresh.mkdir()
            entries = []
            for name in ["passed", "known-failure"]:
                folder = prior / name
                folder.mkdir(parents=True)
                source = folder / "input.docx"
                source.write_bytes(b"same original bytes")
                entries.append({"id": name, "input_sha256": parity.digest(source)})
            parity.save_json(prior / "manifest.json", {"schema": 1, "cases": entries})
            (fresh / "duplicate.docx").write_bytes(b"same original bytes")
            (fresh / "new.docx").write_bytes(b"new regression")
            parity.prepare_retained(out, prior / "manifest.json", fresh)
            import json
            manifest = json.loads((out / "manifest.json").read_text())
            self.assertEqual([e["id"] for e in manifest["cases"]], ["passed", "known-failure", "regression-new"])
            self.assertEqual((out / "known-failure/input.docx").read_bytes(), b"same original bytes")
            self.assertEqual(manifest["cases"][0]["regression_contracts"], ["duplicate"])

    def test_changed_retained_input_is_rejected_before_writing(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / "case"
            source.mkdir()
            (source / "input.docx").write_bytes(b"changed")
            parity.save_json(root / "manifest.json", {"cases": [{"id": "case", "input_sha256": "original"}]})
            with self.assertRaisesRegex(ValueError, "hash mismatch"):
                parity.prepare_retained(root / "out", root / "manifest.json")
            self.assertFalse((root / "out").exists())

    def test_missing_reference_is_blocked(self):
        with tempfile.TemporaryDirectory() as folder:
            result = parity.compare_case(Path(folder), {"id": "case"})
            self.assertEqual(result["status"], "blocked")

    def compare(self, mutate=None):
        with tempfile.TemporaryDirectory() as temporary:
            out = Path(temporary)
            case = out / "case"
            (case / "edgedoc").mkdir(parents=True)
            (case / "wordcraft").mkdir()
            (case / "input.docx").write_bytes(b"test input, not a reference document")
            font = case / "font.ttf"
            font.write_bytes(b"test font identifier, not a font")
            source_hash = parity.digest(case / "input.docx")
            font_hash = parity.digest(font)
            char = {"text": "a", "x": 72, "baseline": 84, "size": 12, "font": "Test",
                    "bold": False, "italic": False, "gid": 1, "font_sha256": font_hash}
            page = {"width": 612, "height": 792, "chars": [char], "unmapped_glyphs": 0}
            edge = {"source_sha256": source_hash, "build_inputs_sha256": "build", "diagnostics": [],
                    "fonts": [{"path": str(font), "sha256": font_hash}], "pages": [copy.deepcopy(page)]}
            reference = {"source_sha256": source_hash, "reference": {"test": True},
                         "fonts": [{"sha256": font_hash}], "pages": [copy.deepcopy(page)]}
            if mutate:
                mutate(edge, reference)
            parity.save_json(case / "edgedoc/layout.json", edge)
            parity.save_json(case / "wordcraft/layout.json", reference)
            # Pixel comparison is deliberately stubbed here to isolate the metadata/text gate.
            with patch.object(parity, "build_digest", return_value="build"), \
                    patch.object(parity, "reference_identity", return_value={"test": True}), \
                    patch.object(parity, "raster_pair", return_value={"unmatched_ink_fraction": 0, "mean_ink_color_error": 0}):
                return parity.compare_case(out, {"id": "case", "input_sha256": source_hash})

    def test_matching_metadata_reaches_the_pixel_gate(self):
        self.assertEqual(self.compare()["status"], "passed")

    def test_changed_build_is_blocked(self):
        result = self.compare(lambda edge, _: edge.update(build_inputs_sha256="stale"))
        self.assertEqual(result["status"], "blocked")

    def test_shifted_glyph_fails_even_with_matching_pixels(self):
        result = self.compare(lambda edge, _: edge["pages"][0]["chars"][0].update(x=73))
        self.assertEqual(result["status"], "failed")

    def test_changed_glyph_or_font_data_fails(self):
        for change in [{"gid": 2}, {"font_sha256": "other font"}]:
            result = self.compare(lambda edge, _: edge["pages"][0]["chars"][0].update(change))
            self.assertEqual(result["status"], "failed")

    def test_unsupported_feature_cannot_become_a_pass(self):
        result = self.compare(lambda edge, _: edge["diagnostics"].append({"code": "display.unsupported_layout"}))
        self.assertEqual(result["status"], "failed")

    def test_empty_page_is_not_evidence(self):
        result = self.compare(lambda edge, _: edge["pages"][0].update(chars=[]))
        self.assertEqual(result["status"], "failed")

    def test_matching_blank_pages_are_allowed_only_with_document_text(self):
        def add_blank(edge, reference):
            blank = {"width": 612, "height": 792, "chars": [], "unmapped_glyphs": 0}
            edge["pages"].append(copy.deepcopy(blank))
            reference["pages"].append(copy.deepcopy(blank))
        self.assertEqual(self.compare(add_blank)["status"], "passed")

    def test_two_entirely_empty_text_renderings_are_not_proof(self):
        def empty(edge, reference):
            edge["pages"][0]["chars"] = []
            reference["pages"][0]["chars"] = []
        self.assertEqual(self.compare(empty)["status"], "failed")

    def test_textless_document_needs_verified_image_evidence(self):
        def empty(edge, reference):
            edge["pages"][0]["chars"] = []
            reference["pages"][0]["chars"] = []
        with patch.object(parity, "image_evidence", return_value=True):
            result = self.compare(empty)
        self.assertEqual(result["status"], "passed")
        self.assertEqual(result["image_evidence_pages"], [1])
        def empty_with_unsupported(edge, reference):
            empty(edge, reference)
            edge["diagnostics"].append({"code": "display.unsupported_layout"})
        with patch.object(parity, "image_evidence", return_value=True):
            self.assertEqual(self.compare(empty_with_unsupported)["status"], "failed")

    def test_image_evidence_requires_shared_visible_media_and_painted_ink(self):
        from PIL import Image

        def payload(color):
            data = io.BytesIO()
            Image.new("RGBA", (16, 16), color).save(data, format="PNG")
            return base64.b64encode(data.getvalue()).decode("ascii")

        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for engine in ("wordcraft", "edgedoc"):
                (root / engine).mkdir()
            def write(engine, data):
                (root / engine / "page-001.svg").write_text(f'<svg><image href="data:image/png;base64,{data}"/></svg>')

            black, white = payload((0, 0, 0, 255)), payload((255, 255, 255, 255))
            raster = {"reference_ink_pixels": 100, "edgedoc_ink_pixels": 100}
            write("wordcraft", black)
            write("edgedoc", black)
            self.assertTrue(parity.image_evidence(root, 1, raster))
            self.assertFalse(parity.image_evidence(root, 1, {**raster, "edgedoc_ink_pixels": 0}))
            write("edgedoc", payload((80, 0, 0, 255)))
            self.assertFalse(parity.image_evidence(root, 1, raster))
            write("edgedoc", white)
            self.assertFalse(parity.image_evidence(root, 1, raster))
            write("wordcraft", white)
            self.assertFalse(parity.image_evidence(root, 1, raster))
            transparent = payload((0, 0, 0, 0))
            write("wordcraft", transparent)
            write("edgedoc", transparent)
            self.assertFalse(parity.image_evidence(root, 1, raster))

    def test_whitespace_changes_are_retained(self):
        char = lambda text, x: {"text": text, "x": x, "baseline": 10}
        self.assertNotEqual(parity.lines([char("ab", 0)])[0]["text"],
                            parity.lines([char("a", 0), char(" ", 1), char("b", 2)])[0]["text"])


if __name__ == "__main__":
    unittest.main()
