# img2avif

Convert images to AVIF. Handles camera RAW files (ARW, CR2, NEF, etc.) and standard formats (JPEG, PNG, WebP, etc.) with parallel processing.

## Usage

```sh
img2avif [OPTIONS] <FILES>...
```

### Options

| Flag | Default | Description |
|------|---------|-------------|
| `-q, --quality` | 80 | Quality (1-100) |
| `-s, --speed` | 6 | Encoding speed (1=best, 10=fast) |
| `-x, --xmp` | off | Apply Lightroom XMP sidecar edits |
| `-o, --outdir` | | Write AVIFs to a separate directory |
| `-m, --move-originals` | | Move source files after conversion |

## XMP sidecar support (`--xmp`)

When `--xmp` is passed, img2avif looks for Adobe Camera Raw / Lightroom sidecar files alongside each RAW file:

1. `photo.ARW.xmp`
2. `photo.xmp`

If found, supported settings are applied during RAW processing. Files without a sidecar are processed normally.

### Supported settings

| Setting | XMP field |
|---------|-----------|
| Exposure | `Exposure2012` |
| White balance | `Temperature`, `Tint` (when manually set) |
| Contrast | `Contrast2012` |
| Highlights | `Highlights2012` |
| Shadows | `Shadows2012` |
| Whites | `Whites2012` |
| Blacks | `Blacks2012` |
| Vibrance | `Vibrance` |
| Saturation | `Saturation` |
| Crop | `CropTop`, `CropLeft`, `CropBottom`, `CropRight` |
| Crop rotation | `CropAngle` |

### Not supported

These are silently skipped — they require proprietary Adobe processing:

- Tone curves with custom control points
- Local adjustments (brushes, radial/graduated filters, masks)
- Lens profile corrections
- Sharpening and noise reduction
- Camera profiles (Adobe Standard, etc.)
- Dehaze, texture, clarity
- Split toning
- Per-channel HSL adjustments
- Perspective corrections

Results will differ from Lightroom, especially when advanced edits are used. The `--xmp` flag is best for applying basic global adjustments (exposure, crop, white balance) to get closer to your intended look.
