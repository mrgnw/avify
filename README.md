# avify

Batch convert images to AVIF with parallel encoding.

Supports RAW camera files (ARW, CR2, CR3, DNG, NEF, etc.), HEIC/HEIF, and standard formats (JPEG, PNG, WebP, BMP, TIFF, GIF). 

> **Note:** This is proof-of-concept code. It may be unreliable and is not actively maintained.

## Install

```
cargo install avify
```

HEIC/HEIF support is enabled by default but requires [libheif](https://github.com/nicmcd/libheif) system libraries. To install without HEIC:

```
cargo install avify --no-default-features
```

### macOS (with HEIC)

Install libheif dependencies, then build:

```
brew install pkgconf libde265
```

See `build.sh` for a script that builds a minimal decode-only libheif from source.

## Usage

```
avify [OPTIONS] [FILES]...
```

If no files are given, converts all supported images in the current directory.

### Options

| Flag | Description | Default |
|------|-------------|---------|
| `-q, --quality` | Encoding quality (0-100) | 80 |
| `-s, --speed` | Encoding speed (1-10, higher = faster) | 10 |
| `-o, --outdir` | Output directory for AVIF files | same as input |
| `-m, --move-originals` | Move originals to this directory after conversion | |
| `-t, --trash` | Trash originals after conversion (macOS) | |
| `-x, --xmp` | Apply Lightroom XMP sidecar edits | |

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
- **HEIC/HEIF support** via libheif (optional, enabled by default)
- **XMP sidecar edits** — applies Lightroom exposure, contrast, highlights, shadows, white balance, crop, saturation, and vibrance adjustments
- **10-bit AVIF output** for better color depth

## License

This project is licensed under [MIT](LICENSE).

This project depends on the following libraries with their own licenses:

- [imagepipe](https://github.com/pedrocr/imagepipe) — LGPL-3.0 (RAW decoding)
- [ravif](https://github.com/nicmcd/ravif) — BSD-3-Clause (AVIF encoding)
- [libheif](https://github.com/nicmcd/libheif) / [libheif-rs](https://github.com/nicmcd/libheif-rs) — MIT / LGPL-3.0 (HEIC decoding, optional)
