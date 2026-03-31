# img2avif

Batch convert images to AVIF. Supports RAW files (via imagepipe) and standard formats (JPEG, PNG, WebP, BMP, TIFF, GIF). Optionally applies Lightroom XMP sidecar edits.

> **Note:** This is proof-of-concept code. It may be unreliable and is not actively maintained.

## Usage

```
img2avif [OPTIONS] <FILES>...
```

**Options:**

- `-q, --quality <QUALITY>` — Encoding quality (default: 80)
- `-s, --speed <SPEED>` — Encoding speed 1-10 (default: 6)
- `-o, --outdir <DIR>` — Output directory for AVIF files
- `-m, --move-originals <DIR>` — Move originals to this directory after conversion
- `-x, --xmp` — Apply Lightroom XMP sidecar edits

## Build

```
cargo build --release
```

## License

MIT
