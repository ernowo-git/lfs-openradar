"""Version-gated releases and portable packages. Python 3.11+, stdlib only."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tarfile
import tempfile
import tomllib
from urllib.error import HTTPError
from urllib.parse import quote
from urllib.request import Request, urlopen
import zipfile

ROOT = Path(__file__).resolve().parents[2]
VERSION = re.compile(
    r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)"
    r"(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?"
    r"(?:\+([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?"
)


def semver(value: str) -> tuple[tuple[int, ...], tuple[str, ...]]:
    match = VERSION.fullmatch(value)
    if not match:
        raise ValueError(f"Invalid semantic version: {value}")
    pre = tuple(match[4].split(".")) if match[4] else ()
    if any(p.isdigit() and len(p) > 1 and p.startswith("0") for p in pre):
        raise ValueError("Numeric prerelease identifiers cannot have leading zeros")
    return tuple(int(match[i]) for i in range(1, 4)), pre


def newer(after: str, before: str) -> bool:
    core_a, pre_a = semver(after)
    core_b, pre_b = semver(before)
    if core_a != core_b:
        return core_a > core_b
    if not pre_a or not pre_b:
        return bool(pre_b) and not pre_a
    for a, b in zip(pre_a, pre_b):
        if a != b:
            if a.isdigit() and b.isdigit():
                return int(a) > int(b)
            if a.isdigit() != b.isdigit():
                return not a.isdigit()
            return a > b
    return len(pre_a) > len(pre_b)


def project_version(manifest: str, lock: str) -> str:
    package = tomllib.loads(manifest)["package"]
    version = package["version"]
    semver(version)
    entries = [p for p in tomllib.loads(lock)["package"] if p["name"] == package["name"]]
    if len(entries) != 1 or entries[0]["version"] != version:
        raise ValueError("Cargo.toml and this project's Cargo.lock version must match")
    return version


def check(root: Path = ROOT) -> str:
    return project_version((root / "Cargo.toml").read_text(), (root / "Cargo.lock").read_text())


def git(*args: str) -> str:
    return subprocess.check_output(["git", *args], cwd=ROOT, text=True).strip()


def detect(before: str, after: str) -> dict[str, str]:
    version = project_version(git("show", f"{after}:Cargo.toml"), git("show", f"{after}:Cargo.lock"))
    outputs = {"release": "false", "version": version, "tag": f"v{version}"}
    if before and set(before) == {"0"}:
        print("Initial branch push: no previous version to compare; no automatic release")
        return outputs
    previous = tomllib.loads(git("show", f"{before}:Cargo.toml"))["package"]["version"]
    if version == previous:
        print(f"Version unchanged ({version}); skipping release")
        return outputs
    if not newer(version, previous):
        raise ValueError(f"Version must increase: {previous} -> {version}")
    tag = outputs["tag"]
    tags = git("tag", "--list", tag)
    if tags and git("rev-parse", f"refs/tags/{tag}^{{commit}}") != git("rev-parse", after):
        raise ValueError(f"Tag {tag} already points to another commit; use a new version")
    outputs["release"] = "true"
    print(f"Version bumped: {previous} -> {version}; release {tag}")
    return outputs


def asset_names(version: str) -> list[str]:
    semver(version)
    base = f"lfs-openradar-v{version}"
    return [f"{base}-windows-x86_64.exe", f"{base}-windows-x86_64.zip",
            f"{base}-linux-x86_64", f"{base}-linux-x86_64.tar.gz"]


def package(binary: Path, output: Path, version: str, platform: str,
            commit: str, toolchain: str, smoke: bool = False, root: Path = ROOT) -> list[Path]:
    if check(root) != version:
        raise ValueError("Package version does not match Cargo.toml")
    names = asset_names(version)
    raw_name, archive_name = names[:2] if platform == "windows" else names[2:]
    output.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="openradar-package-") as temporary:
        stage = Path(temporary)
        executable = stage / ("lfs-openradar.exe" if platform == "windows" else "lfs-openradar")
        shutil.copy2(binary, executable)
        if platform == "linux":
            executable.chmod(0o755)
        # An explicit allowlist keeps local TOML passwords, logs, and caches out.
        for name in ["README.md", "LICENSE", "openradar.example.toml", "Cargo.lock"]:
            shutil.copy2(root / name, stage / name)
        documentation = list((root / "docs").rglob("*.md"))
        documentation.extend(path for path in (root / "docs" / "images").rglob("*")
                             if path.is_file() and path.suffix.lower() in {".png", ".gif", ".jpg", ".jpeg", ".svg"})
        for source in sorted(documentation):
            target = stage / source.relative_to(root)
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(source, target)
        for name in ["LICENSE-MIT", "OPENRADAR-PATCH.md"]:
            target = stage / "vendor" / "eframe" / name
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(root / "vendor" / "eframe" / name, target)
        (stage / "BUILD-INFO.txt").write_text(
            f"Version: {version}\nCommit: {commit}\nRust: {toolchain}\n"
            f"Platform: {platform} x86_64\nLinux baseline: Ubuntu 22.04 (glibc 2.35)\n"
            "Windows requires the Microsoft Visual C++ x64 runtime.\n"
            "Linux gaming overlays require manual platform validation; see README.md.\n",
            encoding="utf-8",
        )
        if smoke:
            subprocess.run([str(executable), "--demo", "--headless", "--seconds", "1"],
                           check=True, cwd=stage, timeout=30)
        raw = output / raw_name
        shutil.copy2(executable, raw)
        archive = output / archive_name
        if platform == "windows":
            with zipfile.ZipFile(archive, "w", compression=zipfile.ZIP_DEFLATED) as zipped:
                for path in sorted(stage.rglob("*")):
                    if path.is_file():
                        zipped.write(path, path.relative_to(stage).as_posix())
        else:
            with tarfile.open(archive, "w:gz") as tar:
                for path in sorted(stage.rglob("*")):
                    if path.is_file():
                        def permissions(info):
                            info.mode = 0o755 if path == executable else 0o644
                            return info
                        tar.add(path, arcname=path.relative_to(stage).as_posix(),
                                recursive=False, filter=permissions)
    print(f"Packaged {raw.name} and {archive.name}")
    return [raw, archive]


def digest(path: Path) -> str:
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def checksums(output: Path, version: str) -> list[Path]:
    files = [output / name for name in asset_names(version)]
    if not all(p.is_file() and p.stat().st_size > 0 for p in files):
        raise ValueError("Both Windows and Linux binaries and archives must exist before publishing")
    checksum = output / "SHA256SUMS"
    checksum.write_text("".join(f"{digest(p)}  {p.name}\n" for p in files), encoding="utf-8")
    return [*files, checksum]


class GitHub:
    def __init__(self, repository: str, token: str):
        self.repository = repository
        self.token = token
        self.base = os.environ.get("GITHUB_API_URL", "https://api.github.com").rstrip("/")

    def api(self, method: str, endpoint: str, payload: dict | None = None):
        request = Request(f"{self.base}/repos/{self.repository}/{endpoint}",
                          data=json.dumps(payload).encode() if payload is not None else None,
                          method=method, headers={
                              "Authorization": f"Bearer {self.token}",
                              "Accept": "application/vnd.github+json",
                              "Content-Type": "application/json",
                              "X-GitHub-Api-Version": "2026-03-10",
                              "User-Agent": "lfs-openradar-release",
                          })
        try:
            with urlopen(request, timeout=30) as response:
                return json.load(response)
        except HTTPError as error:
            if method == "GET" and error.code == 404:
                return None
            raise RuntimeError(f"GitHub {method} {endpoint} failed (HTTP {error.code})") from error

    def verify_tag(self, tag: str, commit: str):
        ref = self.api("GET", f"git/ref/tags/{quote(tag, safe='')}")
        if ref is None:
            return
        obj = ref["object"]
        # Handle both annotated and lightweight tags.
        while obj["type"] == "tag":
            obj = self.api("GET", f"git/tags/{obj['sha']}")["object"]
        if obj["type"] != "commit" or obj["sha"] != commit:
            raise ValueError(f"Tag {tag} already points to another commit")

    def upload(self, tag: str, files: list[Path]):
        subprocess.run(["gh", "release", "upload", tag, *map(str, files),
                        "--clobber", "--repo", self.repository], check=True)


def verify_assets(assets: list[dict], files: list[Path], exact: bool = True):
    by_name = {asset["name"]: asset for asset in assets}
    for path in files:
        asset = by_name.get(path.name)
        if asset is None or asset["size"] <= 0 or asset["state"] != "uploaded":
            raise ValueError(f"Missing or incomplete release asset: {path.name}")
        if exact and asset["size"] != path.stat().st_size:
            raise ValueError(f"Release asset size mismatch: {path.name}")
        if exact and asset.get("digest") and asset["digest"] != f"sha256:{digest(path)}":
            raise ValueError(f"Release asset checksum mismatch: {path.name}")


def publish(client: GitHub, output: Path, version: str, commit: str):
    if not re.fullmatch(r"[0-9a-f]{40,64}", commit):
        raise ValueError("Publishing requires the exact commit SHA")
    files = [output / name for name in [*asset_names(version), "SHA256SUMS"]]
    # Require the checksums step to have completed and verify it again before upload.
    expected = "".join(f"{digest(p)}  {p.name}\n" for p in files[:-1])
    if files[-1].read_text(encoding="utf-8") != expected:
        raise ValueError("SHA256SUMS does not match the release downloads")
    tag = f"v{version}"
    prerelease = bool(semver(version)[1])
    client.verify_tag(tag, commit)
    release = client.api("GET", f"releases/tags/{quote(tag, safe='')}")
    if release is not None and not release["draft"]:
        assets = client.api("GET", f"releases/{release['id']}/assets")
        # Rebuilds/archives need not be byte-identical. Never overwrite an already
        # published release from this commit merely because a rerun rebuilt it.
        verify_assets(assets, files, exact=False)
        print(f"Release {tag} already published; leaving it unchanged")
        return
    if release is None:
        release = client.api("POST", "releases", {
            "tag_name": tag, "target_commitish": commit, "name": tag,
            "draft": True, "prerelease": prerelease, "generate_release_notes": True,
            "body": "Windows x64 and experimental Linux x64 downloads are attached. "
                    "See the bundled README and BUILD-INFO.txt for runtime requirements "
                    "and platform validation limits.",
        })
    elif release["target_commitish"] != commit:
        raise ValueError("Existing draft release targets another commit")
    client.upload(tag, files)
    assets = client.api("GET", f"releases/{release['id']}/assets")
    verify_assets(assets, files)
    client.verify_tag(tag, commit)
    client.api("PATCH", f"releases/{release['id']}", {
        "name": tag,
        "draft": False, "prerelease": prerelease,
        "make_latest": "false" if prerelease else "legacy",
    })
    print(f"Published {tag} with both platforms and SHA256SUMS")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("check")
    commands.add_parser("detect")
    pack = commands.add_parser("package")
    pack.add_argument("--binary", type=Path, required=True)
    pack.add_argument("--platform", choices=["windows", "linux"], required=True)
    pack.add_argument("--commit", required=True)
    pack.add_argument("--toolchain", required=True)
    pack.add_argument("--smoke-test", action="store_true")
    sums = commands.add_parser("checksums")
    release = commands.add_parser("publish")
    release.add_argument("--commit", required=True)
    for command in [pack, sums, release]:
        command.add_argument("--version", required=True)
        command.add_argument("--output", type=Path, default=Path("dist"))
    args = parser.parse_args()
    if args.command == "check":
        print(f"Package and lockfile version: {check()}")
    elif args.command == "detect":
        outputs = detect(os.environ["BEFORE_SHA"], os.environ["AFTER_SHA"])
        with open(os.environ["GITHUB_OUTPUT"], "a", encoding="utf-8") as output:
            output.write("".join(f"{name}={value}\n" for name, value in outputs.items()))
    elif args.command == "package":
        package(args.binary.resolve(), args.output, args.version, args.platform,
                args.commit, args.toolchain, args.smoke_test)
    elif args.command == "checksums":
        checksums(args.output, args.version)
    else:
        publish(GitHub(os.environ["GITHUB_REPOSITORY"], os.environ["GH_TOKEN"]),
                args.output, args.version, args.commit)


if __name__ == "__main__":
    main()
