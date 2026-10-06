import json
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest.mock import Mock, patch
import zipfile

import release

SHA = "a" * 40


def manifest(version):
    return f'[package]\nname = "lfs-openradar"\nversion = "{version}"\n'


def lock(version):
    return f'[[package]]\nname = "lfs-openradar"\nversion = "{version}"\n'


class VersionTests(unittest.TestCase):
    def test_semantic_ordering_including_prereleases(self):
        ordered = ["0.2.0", "0.3.0-alpha", "0.3.0-alpha.1", "0.3.0-alpha.2",
                   "0.3.0-alpha.10", "0.3.0-beta", "0.3.0-rc.1", "0.3.0", "0.3.1", "1.0.0"]
        for before, after in zip(ordered, ordered[1:]):
            self.assertTrue(release.newer(after, before))
            self.assertFalse(release.newer(before, after))
        self.assertFalse(release.newer("0.3.0+build.2", "0.3.0+build.1"))
        for invalid in ["0.3", "v0.3.0", "00.3.0", "0.3.0-01", "0.3.0\n", "0.3.0/other"]:
            with self.assertRaises(ValueError):
                release.semver(invalid)

    def test_lockfile_must_match_manifest(self):
        self.assertEqual(release.project_version(manifest("0.3.0"), lock("0.3.0")), "0.3.0")
        with self.assertRaises(ValueError):
            release.project_version(manifest("0.3.0"), lock("0.2.0"))

    def detect(self, old, new, tag_commit=None):
        def git(*args):
            if args == ("show", "after:Cargo.toml"):
                return manifest(new)
            if args == ("show", "after:Cargo.lock"):
                return lock(new)
            if args == ("show", "before:Cargo.toml"):
                return manifest(old)
            if args[0] == "tag":
                return f"v{new}" if tag_commit else ""
            if args == ("rev-parse", "after"):
                return SHA
            if args[0] == "rev-parse":
                return tag_commit
            raise AssertionError(args)
        with patch.object(release, "git", side_effect=git):
            return release.detect("before", "after")

    def test_unchanged_version_skips_release_and_bump_enables_it(self):
        self.assertEqual(self.detect("0.2.0", "0.2.0")["release"], "false")
        self.assertEqual(self.detect("0.2.0", "0.3.0"),
                         {"release": "true", "version": "0.3.0", "tag": "v0.3.0"})
        self.assertEqual(self.detect("0.2.0", "0.3.0", SHA)["release"], "true")

    def test_downgrade_and_tag_collision_fail(self):
        with self.assertRaises(ValueError):
            self.detect("0.3.0", "0.2.0")
        with self.assertRaises(ValueError):
            self.detect("0.2.0", "0.3.0", "b" * 40)

    def test_initial_push_does_not_invent_a_version_bump(self):
        with patch.object(release, "git", side_effect=[manifest("0.3.0"), lock("0.3.0")]):
            self.assertEqual(release.detect("0" * 40, SHA)["release"], "false")


class PackageTests(unittest.TestCase):
    def test_packages_include_docs_omit_secrets_and_preserve_linux_execution_mode(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "Cargo.toml").write_text(manifest("0.3.0"))
            (root / "Cargo.lock").write_text(lock("0.3.0"))
            for name in ["README.md", "LICENSE", "openradar.example.toml"]:
                (root / name).write_text("public content")
            (root / "docs").mkdir()
            (root / "docs/configuration.md").write_text("configuration")
            (root / "vendor/eframe").mkdir(parents=True)
            for name in ["LICENSE-MIT", "OPENRADAR-PATCH.md"]:
                (root / "vendor/eframe" / name).write_text("notice")
            (root / "openradar.local.toml").write_text("insim_password = 'secret'")
            (root / "openradar.graphics.log").write_text("private paths")
            binary = root / "fixture.exe"
            binary.write_bytes(b"fake binary for packaging tests")
            output = root / "dist"
            for platform in ["windows", "linux"]:
                release.package(binary, output, "0.3.0", platform, SHA, "1.99.0", root=root)
            names = release.asset_names("0.3.0")
            with zipfile.ZipFile(output / names[1]) as archive:
                paths = archive.namelist()
                self.assertIn("lfs-openradar.exe", paths)
                self.assertIn("docs/configuration.md", paths)
                self.assertIn("vendor/eframe/LICENSE-MIT", paths)
                self.assertNotIn("openradar.local.toml", paths)
                self.assertNotIn("openradar.graphics.log", paths)
            with tarfile.open(output / names[3]) as archive:
                self.assertEqual(archive.getmember("lfs-openradar").mode, 0o755)
                self.assertNotIn("openradar.local.toml", archive.getnames())
            files = release.checksums(output, "0.3.0")
            self.assertEqual(len(files), 5)
            self.assertEqual(len(files[-1].read_text().splitlines()), 4)

    def test_missing_platform_prevents_checksums(self):
        with tempfile.TemporaryDirectory() as temporary:
            with self.assertRaises(ValueError):
                release.checksums(Path(temporary), "0.3.0")


class PublishTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.output = Path(self.temporary.name)
        self.version = "0.3.0"
        for name in release.asset_names(self.version):
            (self.output / name).write_bytes(b"release asset")
        self.files = release.checksums(self.output, self.version)

    def assets(self):
        return [{"name": p.name, "size": p.stat().st_size, "state": "uploaded",
                 "digest": f"sha256:{release.digest(p)}"} for p in self.files]

    def test_new_release_is_a_draft_until_all_assets_are_uploaded_and_checked(self):
        client = Mock(spec=release.GitHub)
        client.api.side_effect = [None, {"id": 7, "draft": True}, self.assets(), {}]
        release.publish(client, self.output, self.version, SHA)
        creation = client.api.call_args_list[1].args[2]
        self.assertTrue(creation["draft"])
        self.assertTrue(creation["generate_release_notes"])
        self.assertEqual(creation["target_commitish"], SHA)
        client.upload.assert_called_once()
        self.assertEqual(client.api.call_args_list[-1].args,
                         ("PATCH", "releases/7", {"draft": False, "prerelease": False, "make_latest": "legacy"}))

    def test_failed_upload_does_not_publish(self):
        client = Mock(spec=release.GitHub)
        client.api.side_effect = [None, {"id": 7, "draft": True}]
        client.upload.side_effect = RuntimeError("upload failed")
        with self.assertRaises(RuntimeError):
            release.publish(client, self.output, self.version, SHA)
        self.assertFalse(any(c.args[0] == "PATCH" for c in client.api.call_args_list))

    def test_missing_asset_does_not_publish(self):
        client = Mock(spec=release.GitHub)
        client.api.side_effect = [None, {"id": 7, "draft": True}, self.assets()[:-1]]
        with self.assertRaises(ValueError):
            release.publish(client, self.output, self.version, SHA)
        self.assertFalse(any(c.args[0] == "PATCH" for c in client.api.call_args_list))

    def test_draft_retry_preserves_notes_and_published_retry_leaves_downloads_alone(self):
        client = Mock(spec=release.GitHub)
        client.api.side_effect = [{"id": 7, "draft": True, "target_commitish": SHA}, self.assets(), {}]
        release.publish(client, self.output, self.version, SHA)
        self.assertFalse(any(c.args[0] == "POST" for c in client.api.call_args_list))
        client = Mock(spec=release.GitHub)
        assets = self.assets()
        (self.output / release.asset_names(self.version)[0]).write_bytes(b"different rebuilt binary")
        release.checksums(self.output, self.version)
        client.api.side_effect = [{"id": 7, "draft": False}, assets]
        release.publish(client, self.output, self.version, SHA)
        client.upload.assert_not_called()
        self.assertTrue(all(c.args[0] == "GET" for c in client.api.call_args_list))

    def test_wrong_draft_commit_and_bad_checksums_prevent_upload(self):
        client = Mock(spec=release.GitHub)
        client.api.return_value = {"id": 7, "draft": True, "target_commitish": "b" * 40}
        with self.assertRaises(ValueError):
            release.publish(client, self.output, self.version, SHA)
        client.upload.assert_not_called()
        (self.output / "SHA256SUMS").write_text("incorrect")
        with self.assertRaises(ValueError):
            release.publish(client, self.output, self.version, SHA)

    def test_prerelease_versions_remain_prereleases(self):
        version = "0.3.0-rc.1"
        for name in release.asset_names(version):
            (self.output / name).write_bytes(b"preview")
        files = release.checksums(self.output, version)
        client = Mock(spec=release.GitHub)
        assets = [{"name": p.name, "size": p.stat().st_size, "state": "uploaded"} for p in files]
        client.api.side_effect = [None, {"id": 7, "draft": True}, assets, {}]
        release.publish(client, self.output, version, SHA)
        self.assertTrue(client.api.call_args_list[1].args[2]["prerelease"])
        self.assertEqual(client.api.call_args_list[-1].args[2]["make_latest"], "false")

    def test_annotated_tag_integrity_is_verified(self):
        client = release.GitHub("example/repo", "dummy-token")
        with patch.object(client, "api", side_effect=[
            {"object": {"type": "tag", "sha": "tag-object"}},
            {"object": {"type": "commit", "sha": SHA}},
        ]):
            client.verify_tag("v0.3.0", SHA)
        with patch.object(client, "api", return_value={"object": {"type": "commit", "sha": "b" * 40}}):
            with self.assertRaises(ValueError):
                client.verify_tag("v0.3.0", SHA)


if __name__ == "__main__":
    unittest.main()
