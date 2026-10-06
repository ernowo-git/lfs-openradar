# CI and automatic releases

The workflows live in [ci.yml](../.github/workflows/ci.yml) and
[release.yml](../.github/workflows/release.yml). They take effect once committed
and pushed to GitHub. Their first hosted execution still needs verification;
the workflow definitions, release script, and Windows packaging are checked locally.

## Pull request checks

Opening, updating, reopening, editing, or marking a PR ready runs the CI matrix,
including draft PRs and PRs from forks. Both `windows-2022` and `ubuntu-22.04`
use Rust 1.99.0 and the checked-in Cargo.lock and vendored eframe patch.

Each platform checks:

- The package version in Cargo.toml matches its Cargo.lock entry.
- Python tests for version detection, packaging, checksums, and release recovery.
- `cargo fmt --check` and `cargo check --locked`.
- `cargo clippy --all-targets --locked -- -D warnings`.
- `cargo test --locked` and `cargo test --no-default-features --locked`.

PR jobs have read-only repository permissions. Release credentials are used
only by the publishing job on `main`. CI can also be run manually from the
Actions page. To require CI before merging, configure branch protection for
`Checks (windows-2022)` and `Checks (ubuntu-22.04)` after their first run.

## Trigger a release

1. Increase `[package].version` in Cargo.toml, for example `0.2.0` to `0.3.0`.
2. Run `cargo check` to update this project's version entry in Cargo.lock.
   Commit both files along with the changes to release.
3. Open a PR, let its checks pass, and merge it into `main`.

Every push to `main` compares the package version before and after the push.
This works with merge commits, squash merges, and rebase merges, and also covers
direct pushes. The unchanged version skips builds and publication; decreases
and changes that do not increase semantic-version precedence fail. The workflow
addition itself keeps version `0.2.0`, so merging it alone does not create a release.

A bump reruns the same CI checks on the exact resulting commit, then builds
native release executables for `x86_64-pc-windows-msvc` and
`x86_64-unknown-linux-gnu`. Both packaged executables must pass a headless demo
startup before publication. Linux builds use Ubuntu 22.04; the workflow records
ELF and linked-library information. Graphics and live driving still require
manual acceptance as described in the README.

After both builds succeed, the workflow creates a tag such as `v0.3.0` at that
exact commit and a GitHub Release. GitHub generates notes from the merged PRs
using [release.yml](../.github/release.yml): features, fixes, documentation, and
other changes. PRs labeled `skip-changelog` are omitted. Historical short tags
such as `v0.1` and `v0.2` remain unchanged; future tags include all three version parts.

Stable versions publish as normal releases. Versions such as `0.3.0-rc.1`
publish as prereleases and do not become the latest stable release. The publishing
job uses the built-in `GITHUB_TOKEN` with `contents: write`; no personal access
token is needed. GitHub Actions must be enabled in the repository.

## Downloads

Each release provides these assets, with its full version substituted:

| Asset | Contents |
| --- | --- |
| `lfs-openradar-v0.3.0-windows-x86_64.exe` | Raw Windows executable. |
| `lfs-openradar-v0.3.0-windows-x86_64.zip` | Windows executable, example TOML, README, docs, license, vendored patch notice, lockfile, and build information. |
| `lfs-openradar-v0.3.0-linux-x86_64` | Raw native Linux executable. |
| `lfs-openradar-v0.3.0-linux-x86_64.tar.gz` | Linux executable with execution permission and the same documentation. |
| `SHA256SUMS` | SHA-256 checksums for the four downloads. |

Packaging uses an explicit file list and excludes local TOML settings,
passwords, logs, recordings, and build caches. Windows needs the Microsoft
Visual C++ x64 runtime. Linux targets Ubuntu 22.04/glibc 2.35 or newer with
X11 libraries and suitable graphics drivers; Linux gaming overlay support
remains experimental. See [platform status](usage.md#platform-status).

## Failed runs and retries

No release is published unless both platform builds and all checks pass. The
publishing job creates a temporary draft, uploads all five assets, verifies
their sizes and available server checksums, then publishes it automatically.
An upload failure leaves the draft unpublished.

Rerun the original failed workflow from GitHub's Actions page after resolving
transient build or upload problems. It retains the original version comparison
and commit. An existing draft for that commit is resumed, preserving its release
notes. An already published release for that commit is left unchanged. A tag or
draft belonging to another commit fails instead of replacing that release.

For a code fix, open a new PR. If its package version stays unchanged, its merge
does not retrigger a release; bump the version again to publish the fixed commit.

## Local validation

The Python helper uses only the standard library and requires Python 3.11+:

```text
python .github/scripts/release.py check
python -m unittest discover -s .github/scripts -p "test_*.py" -v
```

Run the Rust commands listed in [the README](../README.md#testing)
for application changes. Workflow definitions can additionally be checked with
`actionlint .github/workflows/ci.yml .github/workflows/release.yml`.
Linux builds run on GitHub's Linux runners; no local Linux build is required.
