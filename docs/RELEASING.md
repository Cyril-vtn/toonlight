# Releasing Toonlight

Every stable version gets installers for Windows, macOS and Linux on the
[Releases page](https://github.com/Cyril-vtn/toonlight/releases), built by GitHub Actions — nobody
has to build them by hand.

## Version numbers

Toonlight follows [Semantic Versioning](https://semver.org): `MAJOR.MINOR.PATCH`.

- **PATCH** (0.15.1): bug fixes only.
- **MINOR** (0.16.0): new features; old projects still open.
- **MAJOR** (1.0.0): changes that break something (project format, removed features).
- Test versions get a suffix: `0.16.0-beta.1`. They are published as *pre-releases*.

## Making a release

1. Make sure CI is green on `main`.
2. In `Cargo.toml`, set `version = "0.16.0"`.
3. In `CHANGELOG.md`, rename the `## [Unreleased]` section to `## [0.16.0] — 2026-12-01` and start a
   new empty `## [Unreleased]` above it.
4. Commit: `git commit -am "Release 0.16.0"`.
5. Tag and push:

   ```sh
   git tag v0.16.0
   git push origin main v0.16.0
   ```

6. The **Release** workflow (`.github/workflows/release.yml`) then, on Windows, macOS and Linux:
   checks that the tag matches `Cargo.toml`, runs the tests, builds the installers with
   [cargo-packager](https://github.com/crabnebula-dev/cargo-packager), and creates a **draft
   release** with the files, a `SHA256SUMS.txt` and the changelog section as notes.
7. Open the draft on GitHub, check it, and press **Publish release**.

| System | What is built | Made with |
|---|---|---|
| Windows | `Toonlight_<version>_x64-setup.exe` (installer with Start-menu entry and uninstaller) | NSIS |
| macOS | `Toonlight.app` inside `Toonlight_<version>_aarch64.dmg` | cargo-packager |
| Linux | `.deb` and `.AppImage` | cargo-packager |

To try the packaging on your own computer: `cargo install cargo-packager --locked`, then
`cargo packager --release` (the files appear in `dist/`). The settings are in
`[package.metadata.packager]` in `Cargo.toml`.

## Signing (optional, recommended later)

Without signing, the files work but the systems warn on first start:

- **Windows** shows "Windows protected your PC" (SmartScreen) until the installer has a code-signing
  certificate. Free options for open-source projects: the
  [SignPath Foundation](https://signpath.org). Paid: Azure Trusted Signing (a few dollars a month)
  or a regular certificate. The certificate goes into the repository's *secrets*, never into the code.
- **macOS** asks to right-click → Open the first time until the app is signed with an Apple
  *Developer ID* and notarised. That needs the Apple Developer Program (99 USD a year). cargo-packager
  signs and notarises when these secrets are set in the workflow: the certificate
  (`APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`), the identity (`APPLE_SIGNING_IDENTITY`) and
  the notarisation account (`APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID`).
- **Linux** packages don't need signing; the published `SHA256SUMS.txt` lets people check the files.

## Intel Macs

The workflow builds for Apple Silicon. For Intel Macs, add a matrix entry on an Intel runner (or
cross-compile with `rustup target add x86_64-apple-darwin`) — see the GitHub runner list for the
current labels.
