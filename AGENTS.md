# Repository instructions

## Commits and logs

- Follow [Conventional Commits 1.0.0](https://www.conventionalcommits.org/en/v1.0.0/) for every commit message: `<type>[optional scope]: <description>`. Use `feat` for features, `fix` for bug fixes, and the appropriate type for other changes. Mark breaking changes with `!` or a `BREAKING CHANGE:` footer as specified.
- Start logged error messages with a lowercase letter.

## Pull requests and versions

- After creating or opening a pull request, ask the user whether to bump the version. Offer these four choices: **Yes - major**, **Yes - minor**, **Yes - patch**, or **No**. Wait for the answer before changing the version.

## Releases

- Build a Windows `.exe` and a Linux amd64 (x86_64) binary for every release, subject to the WSL availability rule below.
- To build the Linux binary, list the available WSL distributions and use an available distribution. If none is available, skip the Linux build and tell the user to handle it manually.
- Use the same version format for Git tags and release names: `vMAJOR.MINOR.PATCH`, for example `v0.1.0`, `v0.2.1`, or `v0.3.0`.
