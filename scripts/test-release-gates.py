"""Release gating regressions, without modifying any versioned delivery."""
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("release", Path(__file__).with_name("package-release.py"))
release = importlib.util.module_from_spec(spec)
spec.loader.exec_module(release)


class ReleaseGates(unittest.TestCase):
    def test_live_source_versions_and_wrong_requested_release(self):
        root = Path(__file__).resolve().parent.parent
        release.validate_versions(root, json.loads((root / "package.json").read_text())["version"])
        with self.assertRaisesRegex(RuntimeError, "Version mismatch"):
            release.validate_versions(root, "0.3.0-beta.2")

    def test_formal_stable_package_stops_before_version_or_asset_writes(self):
        import subprocess
        with patch.object(release.sys, "argv", ["package-release.py", "--version", "0.3.0"]), patch.object(release.subprocess, "run", side_effect=subprocess.CalledProcessError(1, "stable-release-gate.py")) as gate, patch.object(release, "validate_versions") as versions:
            with self.assertRaises(subprocess.CalledProcessError):
                release.main()
            gate.assert_called_once()
            self.assertTrue(gate.call_args.kwargs["check"])
            self.assertTrue(str(gate.call_args.args[0][1]).endswith("stable-release-gate.py"))
            versions.assert_not_called()

    def test_v0612_uses_committed_source_delivery(self):
        with patch.object(release.sys, "argv", ["package-release.py", "--version", "0.6.12"]), patch.object(release, "validate_versions") as versions, patch.object(release.subprocess, "run") as runner:
            release.main()
            versions.assert_called_once()
            runner.assert_called_once()
            self.assertTrue(runner.call_args.kwargs["check"])
            self.assertTrue(str(runner.call_args.args[0][1]).endswith("release-v0612-delivery.py"))

    def test_v0612_rejects_ambiguous_delivery_folder(self):
        with patch.object(release.sys, "argv", ["package-release.py", "--version", "0.6.12", "--directory-name", "v0.6.12-copy"]), patch.object(release, "validate_versions"), patch.object(release.subprocess, "run") as runner:
            with self.assertRaisesRegex(ValueError, "fixed v0.6.12 directory"):
                release.main()
            runner.assert_not_called()

    def test_edge_delivery_uses_current_builder(self):
        with patch.object(release.sys, "argv", ["package-release.py", "--version", "0.6.13"]), patch.object(release, "validate_versions"), patch.object(release.subprocess, "run") as runner:
            release.main()
            runner.assert_called_once()
            self.assertTrue(str(runner.call_args.args[0][1]).endswith("edge-delivery.py"))

    def test_root_lock_mismatch_is_rejected(self):
        with tempfile.TemporaryDirectory() as name:
            root = Path(name)
            (root / "src-tauri").mkdir()
            for path in ("package.json", "package-lock.json", "src-tauri/tauri.conf.json"):
                (root/path).write_text(json.dumps({"version":"0.3.0-beta.3", "packages":{"":{"version":"0.3.0-beta.2"}}}))
            (root/"src-tauri/Cargo.toml").write_text('[package]\nversion="0.3.0-beta.3"\n')
            (root/"src-tauri/Cargo.lock").write_text('[[package]]\nname="shiqian"\nversion="0.3.0-beta.3"\n')
            with self.assertRaisesRegex(RuntimeError, "Version mismatch"):
                release.validate_versions(root, "0.3.0-beta.3")

    def test_missing_and_stale_native_program_rejected(self):
        with tempfile.TemporaryDirectory() as name:
            program = Path(name)/"app.exe"
            with self.assertRaises(FileNotFoundError):
                release.validate_binary(program, "0.3.0-beta.3")
            program.write_bytes(b"test version resource is supplied by mocked OS inspection")
            with patch.object(release.subprocess, "check_output", return_value="0.3.0-beta.2\n"):
                with self.assertRaisesRegex(RuntimeError, "Binary version mismatch"):
                    release.validate_binary(program, "0.3.0-beta.3")


if __name__ == "__main__":
    unittest.main()
