# Contributing to Toonlight

Thanks for helping! This guide explains how to work on the project.

## Before you start

- **Bugs**: open an issue with the bug report form (what you did, what happened, your system).
- **Features**: open an issue first, so we can agree on the design before you write code.
  [`docs/ROADMAP.md`](docs/ROADMAP.md) lists what is planned; P1 items come first.
- **Small fixes** (typos, translations, docs) can go straight to a pull request.

## Setting up

1. Install [Rust](https://rustup.rs) (1.85 or later) and, on Linux, the libraries listed in the
   README.
2. `cargo run` starts the app; `cargo test` runs the tests.

## Before sending a pull request

```sh
cargo fmt              # formatting (checked by CI)
cargo clippy           # lints: don't add new warnings
cargo test             # tests, including the translation check
```

- Keep each pull request about one thing, and describe what it changes and why.
- Add a test for new rules (see the `#[cfg(test)]` modules in `src/`).
- Add a line to the **Unreleased** section of `CHANGELOG.md` for anything users will notice.
- Commit messages: a short summary line in the imperative ("Add layer merge"), then details if
  needed. [Conventional Commits](https://www.conventionalcommits.org) (`feat:`, `fix:`…) are welcome.

## Interface texts and translations

Interface texts are written in French in the code — `tr("Calque")`, or
`trf("Taille : {} × {}", &[&w, &h])` — and translated in `src/i18n.rs`, one row per text with
English, Spanish, German, Chinese and Japanese. `cargo test` fails when a text has no row. When
the same French word needs two translations, give the key a context after `§`
(`tr("Image§menu")`); French shows the part before `§`.

Native speakers are very welcome to improve the translations.

## Clean-room rule

Toonlight must stay free to use and to distribute. **Never copy** code, artwork, icons, sounds,
brushes, palettes, documentation or other files from proprietary software into this repository,
and don't submit work you don't have the right to license under MIT. Implementing a well-known
feature in your own way is fine; copying someone else's implementation or assets is not.

## Licence

By contributing, you agree that your contribution is released under the project's
[MIT licence](LICENSE).
