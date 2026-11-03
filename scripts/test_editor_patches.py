"""Regression checks for checked, repeatable upstream source patches."""
import tempfile
import unittest
from pathlib import Path

from patch_editor_baseline import replace_once


class CheckedPatchTests(unittest.TestCase):
    def test_shorter_symbol_cannot_masquerade_as_applied_patch(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "imports.rs"
            original = "use api::{D3DCompileFromFile};\n"
            path.write_text(original, encoding="utf-8")
            with self.assertRaises(ValueError):
                replace_once(path, "D3DCompileFromFile", "D3DCompile")
            self.assertEqual(path.read_text(encoding="utf-8"), original)

            # Include the delimiter so the old and new imports are distinct.
            replace_once(path, "D3DCompileFromFile}", "D3DCompile}")
            updated = path.read_text(encoding="utf-8")
            self.assertEqual(updated, "use api::{D3DCompile};\n")
            replace_once(path, "D3DCompileFromFile}", "D3DCompile}")
            self.assertEqual(path.read_text(encoding="utf-8"), updated)

    def test_changed_upstream_anchor_is_reported(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "upstream.rs"
            path.write_text("upstream changed\n", encoding="utf-8")
            with self.assertRaises(RuntimeError):
                replace_once(path, "old anchor", "new anchor")


if __name__ == "__main__":
    unittest.main()
