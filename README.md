# avify

Batch-convert photo folders to AVIF. At default settings, a test folder of CC0 photos shrank 10.9x: RAW 21.5x, PNG 11.1x, JPEG 5.3x.

[![License: MIT](https://img.shields.io/github/license/mrgnw/avify?style=flat)](LICENSE)
[![Last commit](https://img.shields.io/github/last-commit/mrgnw/avify?style=flat)](https://github.com/mrgnw/avify/commits/main/)
[![Open issues](https://img.shields.io/github/issues/mrgnw/avify?style=flat)](https://github.com/mrgnw/avify/issues)
[![Latest release](https://img.shields.io/github/v/release/mrgnw/avify?style=flat)](https://github.com/mrgnw/avify/releases/latest)
[![Rust: 1.88+](https://img.shields.io/badge/Rust-1.88%2B-orange?style=flat)](Cargo.toml)
[![Platform: macOS and Linux](https://img.shields.io/badge/platform-macOS%20%7C%20Linux-lightgrey?style=flat)](#install)

![A folder of three photos (JPEG 10.2 MB, DNG 20.5 MB, PNG 47.1 MB, 77.8 MB in total) is converted by avify to 7.1 MB, 10.9x smaller; then a 100% crop of the RAW photo sweeps between the original and the AVIF.](docs/media/hero.gif)

[Install](#install) · [Usage](#usage) · [Size and quality](#size-and-quality) · [Roadmap](ROADMAP.md) · [Contributing](CONTRIBUTING.md)

> [!NOTE]
> This project uses AI-assisted development and documentation. Contributions are welcome; changes need review and local checks. avify is pre-1.0, so behavior and flags may change.

## Size and quality

avify 0.3.4 converted three freely licensed photos ([credits](docs/media/CREDITS.md)) on an Apple M4 Pro. Default settings are quality 80 and speed 10; every run used `--keep --quiet --speed 10`.

![Bar chart of file size per photo at default settings: PNG 47.1 MB to 4.3 MB (11.1x), RAW 20.5 MB to 0.95 MB (21.5x), JPEG 10.2 MB to 1.9 MB (5.3x).](docs/media/results.png)

| Input | Quality | Original bytes | AVIF bytes | Smaller by | SSIMULACRA2 |
|-|-|-|-|-|-|
| RAW (Pentax DNG, 20 MP) | 80 (default) | 20,474,979 | 950,934 | 21.5x | 72.5 |
| PNG (export of a 24 MP RAW) | 80 (default) | 47,112,910 | 4,253,997 | 11.1x | 76.2 |
| JPEG (camera, quality about 98, 18 MP) | 80 (default) | 10,221,369 | 1,936,646 | 5.3x | 71.6 |
| RAW | 90 | 20,474,979 | 2,415,453 | 8.5x | 83.6 |
| PNG | 90 | 47,112,910 | 8,711,257 | 5.4x | 87.0 |
| JPEG | 90 | 10,221,369 | 3,886,977 | 2.6x | 83.2 |
| RAW | 95 | 20,474,979 | 5,815,586 | 3.5x | 88.4 |
| PNG | 95 | 47,112,910 | 15,714,266 | 3.0x | 91.4 |
| JPEG | 95 | 10,221,369 | 6,465,510 | 1.6x | 87.3 |

The whole folder went from 77,809,258 to 7,141,577 bytes at the default quality: 10.9x smaller. The ratio depends on the source. RAW and PNG files store far more data than the picture needs, so they shrink 10x or more; a JPEG is already compressed, so it shrinks about 5x. The three-photo run took 1.6–7.5 s of wall-clock time across repeated runs on a busy machine.

### How quality was measured

[SSIMULACRA2](https://github.com/cloudinary/ssimulacra2) (the `ssimulacra2_rs` 0.5.2 implementation) compares each AVIF with the exact pixels avify encoded: the decoded JPEG or PNG, or, for the RAW file, avify's own development of it. On its scale, 90 means visually lossless at 1:1 viewing, 80 very high quality, 70 high quality and 50 medium quality. `results.json` also records SSIM from scikit-image.

At the default quality 80 the scores are 71.6–76.2: high quality, not visually lossless. Quality 90 scores 83.2–87.0 at 2.6–8.5x smaller. Only the PNG crossed 90, at quality 95. If you need copies that hold up to pixel peeping, use `--quality 90` or `--quality 95`.

Judge for yourself: each row is a 100% crop, with no scaling, of the original, the default-quality AVIF, and their absolute difference amplified eight times. The difference is mostly fine grain and sensor noise; edges stay in place.

![100% crops at default quality. Rows: RAW rock texture, JPEG snowy mountainside, PNG tulip and grass. Columns: original, AVIF, difference amplified eight times.](docs/media/quality-crops.png)

### Reproduce the numbers

The photos are downloaded from the URLs in [CREDITS.md](docs/media/CREDITS.md) and checked against their SHA-256 sums. You need [uv](https://docs.astral.sh/uv/), Rust, and about 1 GB of free space. Choose a working directory:

```sh
BENCH_DIR=$(mktemp -d)
```

```sh
cargo install ssimulacra2_rs --no-default-features
cargo build --release
cargo build --release --example raw_reference
uv run docs/media/benchmark.py "$BENCH_DIR" --avify target/release/avify --raw-reference target/release/examples/raw_reference --json docs/media/results.json
```

Redraw the GIF, chart, crops and social preview from those results (uses macOS system fonts):

```sh
uv run docs/media/render.py "$BENCH_DIR"
```

## Who it is for

Photographers and people with folders of photos, screenshots, or screen recordings who want smaller sharing copies without opening an editor. Give avify files or a folder; it prints per-file savings and a final summary.

> [!WARNING]
> By default, each original is moved to the trash after a smaller output is written. Use `--keep` to retain originals. The examples below keep them unless they explicitly move them to an archive folder.

## Features and limits

- **Parallel still-image encoding** across CPU cores, powered by [ravif](https://github.com/kornelski/cavif-rs) and [rayon](https://github.com/rayon-rs/rayon).
- **Camera RAW and common image formats:** ARW, CR2, CR3, DNG, NEF, ORF, RAF, RW2, PEF, SRW, X3F; JPEG, PNG, WebP, BMP, TIFF, TGA, GIF, JPEG XL, and flattened PSD.
- **HEIC/HEIF:** system `sips` on macOS, or an optional build using libheif.
- **Smaller outputs only:** if conversion is not smaller, avify discards the output and keeps the original. A keep marker lets subsequent runs skip that file.
- **Optional AV1 video:** `--video` enables common video formats. Animated GIFs become AV1 video automatically, preserving animation; ffmpeg is required for either route.
- **Experimental XMP edits, RAW only:** approximations of Lightroom exposure, contrast, highlights, shadows, white balance, crop, saturation, and vibrance. Other edits are ignored; see [#8](https://github.com/mrgnw/avify/issues/8).
- **8-bit decoding, 10-bit AVIF encoding:** source HDR and higher-bit-depth detail are not preserved.
- **Metadata is dropped:** EXIF, ICC color profiles, and orientation tags are not copied. Color and orientation can differ; see [#3](https://github.com/mrgnw/avify/issues/3). Keep archival originals.

Folders are scanned one level deep. Recursive scanning, collision protection, and shell completions are planned in the [roadmap](ROADMAP.md). Use distinct input stems and a fresh output folder: inputs such as `scene.jpg` and `scene.png` both write `scene.avif`, and existing outputs can be overwritten.

## Install

Source builds require **Rust 1.88 or newer**. On x86_64 Linux, the AV1 encoder also needs `nasm`.

| Platform | Available route | Status |
|-|-|-|
| macOS, Apple silicon | [Prebuilt v0.2.1 archive](https://github.com/mrgnw/avify/releases/download/v0.2.1/avify-v0.2.1-aarch64-apple-darwin.tar.gz), or Cargo from Git | Native HEIC via `sips` |
| macOS, Intel | Cargo from Git | No prebuilt release asset |
| Linux | Cargo from Git | HEIC needs the optional libheif build; trash and keep-marker parity remain [open](https://github.com/mrgnw/avify/issues/9) |
| Windows | No verified installation route | Support is [in progress](https://github.com/mrgnw/avify/issues/9) |

The latest GitHub release is **v0.2.1**. This README and demo describe **v0.3.4 from Git**, which has newer flags and behavior. Use the source route for the features shown here.

### Current source, macOS and Linux

```sh
cargo install --git https://github.com/mrgnw/avify.git --locked
```

Check the installed binary:

```sh
avify --version
```

### Prebuilt macOS release, Apple silicon

Download into an empty temporary directory:

```sh
INSTALL_DIR=$(mktemp -d)
```

```sh
curl -fL https://github.com/mrgnw/avify/releases/download/v0.2.1/avify-v0.2.1-aarch64-apple-darwin.tar.gz -o "$INSTALL_DIR/avify.tar.gz"
```

```sh
tar -xzf "$INSTALL_DIR/avify.tar.gz" -C "$INSTALL_DIR"
```

Install into a directory on your PATH:

```sh
mkdir -p "$HOME/.local/bin"
```

```sh
install "$INSTALL_DIR/avify" "$HOME/.local/bin/avify"
```

```sh
export PATH="$HOME/.local/bin:$PATH"
```

### HEIC/HEIF outside the default macOS build

Install the system library on macOS:

```sh
brew install libheif
```

Or on Debian/Ubuntu:

```sh
sudo apt-get install libheif-dev
```

Then rebuild with HEIC support:

```sh
cargo install --git https://github.com/mrgnw/avify.git --locked --features heic
```

The default macOS build uses `sips` and needs no extra HEIC library. The optional build uses `libheif-rs`; a system HEVC decoder is needed, so enabling the Cargo feature alone does not provide the native libraries.

## Usage

Define your input and output folders first:

```sh
PHOTO_DIR="$HOME/Pictures"
OUTPUT_DIR="$HOME/Pictures/avify-output"
ARCHIVE_DIR="$HOME/Pictures/avify-originals"
```

Convert the folder's immediate files and keep originals:

```sh
avify --keep --outdir "$OUTPUT_DIR" "$PHOTO_DIR"
```

Choose higher quality and slower encoding:

```sh
avify --keep --quality 90 --speed 6 --outdir "$OUTPUT_DIR" "$PHOTO_DIR"
```

Print only the final summary:

```sh
avify --keep --quiet --outdir "$OUTPUT_DIR" "$PHOTO_DIR"
```

Move successfully converted originals into an archive folder instead of trashing them. Use an archive folder without conflicting filenames; existing archive files can be overwritten:

```sh
avify --outdir "$OUTPUT_DIR" --move-originals "$ARCHIVE_DIR" "$PHOTO_DIR"
```

Apply experimental Lightroom sidecar edits to RAW files in the folder:

```sh
avify --keep --xmp --outdir "$OUTPUT_DIR" "$PHOTO_DIR"
```

With no paths, avify scans the current directory. See all available flags:

```sh
avify --help
```

### Video and animated GIFs

On macOS, install ffmpeg:

```sh
brew install ffmpeg
```

Add videos to a folder conversion:

```sh
avify --keep --video --outdir "$OUTPUT_DIR" "$PHOTO_DIR"
```

Animated GIFs are always converted to `name.av1.mp4`, even without `--video`. A larger first attempt is retried at a lower bitrate before keeping the original. Single-frame GIFs become AVIF stills, including transparency, with quality capped at 60.

With `--video`, MOV, MP4, M4V, WebM, MKV, and AVI files become `name.av1.mp4`. avify prefers ffmpeg's SVT-AV1 encoder (preset 10, constant rate factor 32) and falls back to slower libaom with a warning. Video conversions run one at a time; image quality and speed flags do not affect them. Audio is copied using ffmpeg's default stream selection; extra tracks, subtitles, and HDR preservation are not guaranteed. AV1 may not preview in QuickLook on pre-M3 Macs.

### Retry a file kept as smaller

On macOS, clear its extended-attribute keep marker before retrying:

```sh
RETRY_FILE="$HOME/Pictures/clip.mp4"
```

```sh
xattr -d com.avify.keep "$RETRY_FILE"
```

Linux uses `user.avify.keep`; filesystems without extended attributes need further work. See [platform support](https://github.com/mrgnw/avify/issues/9).

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for setup, tests, and the local pre-push hook. There is no hosted continuous integration. Small fixes, reproducible bug reports, and documentation improvements are welcome.

Planned work lives in the [roadmap](ROADMAP.md) and [issue tracker](https://github.com/mrgnw/avify/issues). Shell completions and a man page are tracked in [#13](https://github.com/mrgnw/avify/issues/13).

## License

This project is licensed under [MIT](LICENSE).

Notable dependencies have their own licenses:

- [imagepipe](https://github.com/pedrocr/imagepipe) — LGPL-3.0 (RAW decoding)
- [rawloader](https://github.com/pedrocr/rawloader) — LGPL-2.1 (RAW decoding, pulled in by imagepipe)
- [ravif](https://github.com/kornelski/cavif-rs) — BSD-3-Clause (AVIF encoding)
- [libheif](https://github.com/strukturag/libheif) — LGPL-3.0, and [libheif-rs](https://github.com/Cykooz/libheif-rs) — MIT (HEIC decoding, optional)
