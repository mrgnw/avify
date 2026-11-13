# Contributing

Bug reports, documentation improvements, generated test fixtures, and focused Rust changes are welcome. Start with the [roadmap](ROADMAP.md) or [good first issues](https://github.com/mrgnw/avify/issues?q=is%3Aissue%20is%3Aopen%20label%3A%22good%20first%20issue%22). Discuss larger changes in an issue first.

## Build and test

Use Rust 1.88 or newer. On x86_64 Linux, install `nasm` for the AV1 encoder. Optional HEIC builds also need the `libheif` development library; video tests need ffmpeg with an AV1 encoder.

Clone the repository, then enter it:

```sh
git clone https://github.com/mrgnw/avify.git
```

```sh
cd avify
```

Build with the checked-in dependency versions:

```sh
cargo build --locked
```

Run the tests:

```sh
cargo test --locked
```

Check formatting:

```sh
cargo fmt --check
```

Enable the pre-push hook once per clone:

```sh
git config core.hooksPath .githooks
```

The hook runs the three checks above before every push. Checks run locally; there is no GitHub Actions workflow.

## Pull requests

Keep each change focused. Describe the problem, the resulting behavior, and the checks you ran. Add regression coverage for behavior changes. Use generated or freely licensed images in tests and reports, and include the source and license for third-party samples. Never upload personal photos or embedded location metadata.

For conversion reports, include your operating system, avify version, exact command, input format, and a small reproducible sample. XMP edits are experimental, metadata is dropped, and decoding is 8-bit; see the [README](README.md) before reporting fidelity issues.
