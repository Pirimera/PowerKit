"""Regression tests for repository-link validation."""
import importlib.util
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("check_repo", Path(__file__).parents[1] / "check_repo.py")
check = importlib.util.module_from_spec(spec)
spec.loader.exec_module(check)

class LinkTests(unittest.TestCase):
    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory()
        self.addCleanup(self.scratch.cleanup)
        self.root = Path(self.scratch.name)
        (self.root / "docs").mkdir()
        self.source = self.root / "docs/guide.md"
        (self.root / "README.md").write_text("# Example", encoding="utf-8")

    def errors(self, text):
        return check.local_link_errors(self.root, self.source, text)

    def test_relative_anchor_and_external_links(self):
        self.assertEqual(self.errors("[root](../README.md#example) [web](https://example.org/a)"), [])

    def test_missing_target_and_escape_are_rejected(self):
        self.assertEqual(len(self.errors("[bad](missing.md) [escape](../../outside.md)")), 2)

    def test_examples_are_not_treated_as_links(self):
        self.assertEqual(self.errors("~~~md\n[example](missing.md)\n~~~\n"), [])

    def test_percent_encoded_spaces(self):
        (self.root / "docs/my guide.md").write_text("text", encoding="utf-8")
        self.assertEqual(self.errors("[space](my%20guide.md)"), [])

class MetadataTests(unittest.TestCase):
    def setUp(self):
        import shutil
        self.scratch = tempfile.TemporaryDirectory()
        self.addCleanup(self.scratch.cleanup)
        self.root = Path(self.scratch.name)
        self.version = check.tomllib.loads((check.ROOT / "Cargo.toml").read_text(encoding="utf-8"))["package"]["version"]
        for name in ("Cargo.toml", "Cargo.lock", "app.manifest", "rust-toolchain.toml",
                     ".github/workflows/release.yml", f"docs/releases/{self.version}.md"):
            target = self.root / name
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(check.ROOT / name, target)

    def test_current_metadata_matches(self):
        self.assertEqual(check.metadata_errors(self.root), [])

    def test_manifest_version_drift_is_rejected(self):
        f = self.root / "app.manifest"
        f.write_text(f.read_text(encoding="utf-8").replace(self.version + '.0', '9.9.9.0'), encoding="utf-8")
        self.assertTrue(any("app.manifest" in e for e in check.metadata_errors(self.root)))

    def test_mutable_action_reference_is_rejected(self):
        f = self.root / ".github/workflows/release.yml"
        text = f.read_text(encoding="utf-8")
        text = check.re.sub(r"actions/checkout@[0-9a-f]{40}", "actions/checkout@v6", text)
        f.write_text(text, encoding="utf-8")
        self.assertTrue(any("commit SHA" in e for e in check.metadata_errors(self.root)))

    def test_sdk_cli_drift_is_rejected(self):
        f = self.root / ".github/workflows/release.yml"
        f.write_text(f.read_text(encoding="utf-8").replace("vpk --version ", "vpk --version 9.9.9-"), encoding="utf-8")
        self.assertTrue(any("Velopack" in e for e in check.metadata_errors(self.root)))

class SkillTests(unittest.TestCase):
    def test_missing_description_prevents_silent_discovery_failure(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            skill = root / ".agents/skills/example/SKILL.md"
            skill.parent.mkdir(parents=True)
            skill.write_text("---\nname: example\n---\nBody", encoding="utf-8")
            self.assertEqual(check.skill_errors(root), ["Missing skill description: example"])

if __name__ == "__main__":
    unittest.main()
