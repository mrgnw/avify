# Roadmap

Open work is tracked as [GitHub issues](https://github.com/mrgnw/avify/issues). Rough order:

## Correctness and fidelity

- [#3](https://github.com/mrgnw/avify/issues/3) Keep EXIF, ICC profile and orientation in the AVIF output
- [#7](https://github.com/mrgnw/avify/issues/7) Handle output name collisions and add `--skip-existing`
- [#8](https://github.com/mrgnw/avify/issues/8) Fix Lightroom XMP sidecar support and cover it with golden tests
- [#14](https://github.com/mrgnw/avify/issues/14) Video options: CRF, preset, audio and subtitle stream mapping

## Workflow

- [#4](https://github.com/mrgnw/avify/issues/4) Quality presets: web, photo, archive
- [#5](https://github.com/mrgnw/avify/issues/5) `--dry-run`
- [#6](https://github.com/mrgnw/avify/issues/6) Recursive conversion (`-r`)
- [#15](https://github.com/mrgnw/avify/issues/15) `--json` summary and `--jobs`

## Quality and platforms

- [#9](https://github.com/mrgnw/avify/issues/9) Linux and Windows parity for the keep marker and trash
- [#10](https://github.com/mrgnw/avify/issues/10) End-to-end CLI tests with generated fixtures
- [#11](https://github.com/mrgnw/avify/issues/11) Split `main.rs` into modules, clippy clean in CI

## Distribution

- [#12](https://github.com/mrgnw/avify/issues/12) Tagged releases with prebuilt binaries and a Homebrew formula
- [#13](https://github.com/mrgnw/avify/issues/13) Shell completions and a man page
