# avify

Batch convert images to AVIF with parallel encoding.

Supports RAW camera files (ARW, CR2, CR3, DNG, NEF, etc.), HEIC/HEIF, and standard formats (JPEG, PNG, WebP, BMP, TIFF, GIF). 

> **Note:** This is proof-of-concept code. It may be unreliable and is not actively maintained.

## Install

```
cargo install avify
```

### HEIC/HEIF support

On macOS, HEIC works out of the box: files are decoded with the system `sips` tool.

Elsewhere (or for in-process decoding on macOS), build with the `libheif` system library:

```
brew install libheif
cargo install avify --features heic
```

## Usage

```
avify [OPTIONS] [FILES]...
```

If no files are given, converts all supported images in the current directory.

> **Warning:** each original is trashed once its AVIF is written successfully. Pass `-k, --keep` to keep originals.

### Options

| Flag | Description | Default |
|------|-------------|---------|
| `-q, --quality` | Encoding quality (0-100) | 80 |
| `-s, --speed` | Encoding speed (1-10, higher = faster) | 10 |
| `-o, --outdir` | Output directory for AVIF files | same as input |
| `-m, --move-originals` | Move originals to this directory after conversion | |
| `-k, --keep` | Keep originals (default trashes each on success, macOS) | |
| `-x, --xmp` | Apply Lightroom XMP sidecar edits | |
| `--video` | Also transcode videos to AV1 (`.av1.mp4`) — requires `ffmpeg` | off |

### Video (`--video`)

Off by default; images-only. With `--video`, video files (`mov`, `mp4`, `m4v`, `webm`, `mkv`, `avi`) are transcoded to AV1 (`name.av1.mp4`, audio copied) using SVT-AV1 (`-preset 10 -crf 32` — on screen recordings: ~87% smaller, faster than realtime, visually transparent). Requires `ffmpeg` with `libsvtav1`:

```sh
brew install ffmpeg
```

avify probes `PATH` and the homebrew locations for an ffmpeg with `libsvtav1`; if none has it, it falls back to slow libaom with a warning. `--quality`/`--speed` do not affect video. Videos encode one at a time (SVT-AV1 already uses all cores). AV1 may not preview in QuickLook on pre-M3 Macs.

If a conversion (image or video) is not smaller than the original, the output is discarded and the original kept — already-compressed files often grow when re-encoded. Kept originals are tagged with a `com.avify.keep` xattr so later runs skip them instead of re-encoding; retry one with:

```sh
xattr -d com.avify.keep file.mp4
```

### Examples

```sh
avify *.jpg

avify -q 90 -s 6 photo.arw

avify -o converted/ -m originals/

avify -x raw_photos/*.cr3
```

## Features

- **Parallel encoding** via rayon — uses all CPU cores
- **RAW support** via imagepipe (ARW, CR2, CR3, DNG, NEF, ORF, RAF, RW2, PEF, SRW, X3F)
- **HEIC/HEIF support** — via `sips` on macOS, or libheif with `--features heic`
- **XMP sidecar edits** — applies Lightroom exposure, contrast, highlights, shadows, white balance, crop, saturation, and vibrance adjustments
- **10-bit AVIF output** for better color depth

## License

This project is licensed under [MIT](LICENSE).

This project depends on the following libraries with their own licenses:

- [imagepipe](https://github.com/pedrocr/imagepipe) — LGPL-3.0 (RAW decoding)
- [ravif](https://github.com/nicmcd/ravif) — BSD-3-Clause (AVIF encoding)
- [libheif](https://github.com/nicmcd/libheif) / [libheif-rs](https://github.com/nicmcd/libheif-rs) — MIT / LGPL-3.0 (HEIC decoding, optional)
