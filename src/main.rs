mod xmp;

use anyhow::{Context, Result};
use clap::Parser;
use imgref::ImgVec;
use ravif::{BitDepth, EncodedImage, Encoder, RGBA8};
use rayon::prelude::*;
use rgb::RGB8;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicUsize;
use std::sync::Mutex;

enum DecodedImage {
    Rgb(ImgVec<RGB8>),
    Rgba(ImgVec<RGBA8>),
}

#[derive(Parser)]
#[command(
    version,
    about = "Convert images to AVIF (supports RAW + standard formats)"
)]
struct Args {
    #[arg(short, long, default_value = "80")]
    quality: f32,

    #[arg(short, long, default_value = "10")]
    speed: u8,

    #[arg(short, long, help = "Output directory for AVIF files")]
    outdir: Option<PathBuf>,

    #[arg(
        short,
        long,
        help = "Move originals to this directory after conversion"
    )]
    move_originals: Option<PathBuf>,

    #[arg(
        short,
        long,
        help = "Keep originals (default: trash each on success, macOS)"
    )]
    keep: bool,

    #[arg(short = 'x', long, help = "Apply Lightroom XMP sidecar edits")]
    xmp: bool,

    #[arg(
        long,
        help = "Also transcode videos to AV1 (.av1.mp4) — requires ffmpeg"
    )]
    video: bool,

    files: Vec<PathBuf>,
}

#[derive(Clone)]
enum Status {
    Pending,
    Processing,
    Done { orig_bytes: u64, avif_bytes: usize },
    Kept { orig_bytes: u64, avif_bytes: usize },
    Failed(String),
}

struct Progress {
    names: Vec<String>,
    statuses: Vec<Status>,
    flushed: usize,
    active_lines: usize,
}

impl Progress {
    fn new(files: &[PathBuf]) -> Self {
        let names = files
            .iter()
            .map(|p| {
                p.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string()
            })
            .collect();
        let statuses = vec![Status::Pending; files.len()];
        Self {
            names,
            statuses,
            flushed: 0,
            active_lines: 0,
        }
    }

    fn set(&mut self, idx: usize, status: Status) {
        self.statuses[idx] = status;
    }

    fn render(&mut self) {
        let total = self.statuses.len();
        let width = total.to_string().len();
        let mut out = io::stderr().lock();

        // Erase the active (in-progress) zone
        if self.active_lines > 0 {
            write!(out, "\x1b[{}A", self.active_lines).ok();
            for _ in 0..self.active_lines {
                write!(out, "\x1b[2K\n").ok();
            }
            write!(out, "\x1b[{}A", self.active_lines).ok();
        }

        // Flush completed files at the front (sequential, never redrawn)
        while self.flushed < total {
            match &self.statuses[self.flushed] {
                Status::Done {
                    orig_bytes,
                    avif_bytes,
                } => {
                    let n = self.flushed + 1;
                    write!(
                        out,
                        "\x1b[2K\x1b[32m{n:>width$}/{total} {} {}\x1b[0m\n",
                        self.names[self.flushed],
                        fmt_savings(*orig_bytes, *avif_bytes as u64)
                    )
                    .ok();
                    self.flushed += 1;
                }
                Status::Kept {
                    orig_bytes,
                    avif_bytes,
                } => {
                    let n = self.flushed + 1;
                    write!(
                        out,
                        "\x1b[2K\x1b[33m{n:>width$}/{total} {} {} — kept original\x1b[0m\n",
                        self.names[self.flushed],
                        fmt_savings(*orig_bytes, *avif_bytes as u64)
                    )
                    .ok();
                    self.flushed += 1;
                }
                Status::Failed(e) => {
                    let n = self.flushed + 1;
                    let e = e.clone();
                    write!(
                        out,
                        "\x1b[2K\x1b[31m{n:>width$}/{total} {} FAIL: {e}\x1b[0m\n",
                        self.names[self.flushed]
                    )
                    .ok();
                    self.flushed += 1;
                }
                _ => break,
            }
        }

        // Draw active (in-progress) lines — only these get redrawn.
        // Each must occupy exactly one terminal row or the erase math above breaks.
        let cols = term_cols();
        let mut active = 0;
        for i in self.flushed..total {
            let n = i + 1;
            let (color, text) = match &self.statuses[i] {
                Status::Processing => ("33", format!("{n:>width$}/{total} {} →", self.names[i])),
                Status::Done {
                    orig_bytes,
                    avif_bytes,
                } => (
                    "32",
                    format!(
                        "{n:>width$}/{total} {} {}",
                        self.names[i],
                        fmt_savings(*orig_bytes, *avif_bytes as u64)
                    ),
                ),
                Status::Kept {
                    orig_bytes,
                    avif_bytes,
                } => (
                    "33",
                    format!(
                        "{n:>width$}/{total} {} {} — kept original",
                        self.names[i],
                        fmt_savings(*orig_bytes, *avif_bytes as u64)
                    ),
                ),
                Status::Failed(e) => (
                    "31",
                    format!("{n:>width$}/{total} {} FAIL: {e}", self.names[i]),
                ),
                Status::Pending => break,
            };
            write!(
                out,
                "\x1b[2K\x1b[{color}m{}\x1b[0m\n",
                fit_one_row(&text, cols)
            )
            .ok();
            active += 1;
        }

        self.active_lines = active;
        out.flush().ok();
    }
}

fn fmt_size(bytes: u64) -> String {
    if bytes > 1_048_576 {
        format!("{:.1}MB", bytes as f64 / 1_048_576.0)
    } else {
        format!("{}KB", bytes / 1024)
    }
}

// "12.3MB → -88% → 1.5MB"; positive % means the file grew
fn fmt_savings(orig: u64, new: u64) -> String {
    if orig == 0 {
        return format!("→ {}", fmt_size(new));
    }
    let pct = (new as i64 - orig as i64) * 100 / orig as i64;
    format!("{} → {pct:+}% → {}", fmt_size(orig), fmt_size(new))
}

fn term_cols() -> Option<usize> {
    let mut ws: libc::winsize = unsafe { std::mem::zeroed() };
    let ok = unsafe { libc::ioctl(libc::STDERR_FILENO, libc::TIOCGWINSZ, &mut ws) };
    (ok == 0 && ws.ws_col > 0).then(|| ws.ws_col as usize)
}

// ponytail: counts chars, not display width — wide (CJK) names can still wrap
fn fit_one_row(s: &str, cols: Option<usize>) -> String {
    let first = s.lines().next().unwrap_or("");
    let Some(max) = cols else {
        return first.to_string();
    };
    if first.chars().count() <= max {
        return first.to_string();
    }
    let cut: String = first.chars().take(max.saturating_sub(1)).collect();
    format!("{cut}…")
}

#[cfg(test)]
mod tests {
    use super::{fit_one_row, fmt_savings, has_keep_marker, is_video, set_keep_marker};
    use std::path::Path;

    #[test]
    fn keep_marker_round_trip() {
        let f = std::env::temp_dir().join("avify_keep_marker_test");
        std::fs::write(&f, b"x").unwrap();
        assert!(!has_keep_marker(&f));
        set_keep_marker(&f);
        assert!(has_keep_marker(&f));
        std::fs::remove_file(&f).unwrap();
    }

    #[test]
    fn savings_line_shows_orig_ratio_final() {
        assert_eq!(fmt_savings(10_485_760, 2_097_152), "10.0MB → -80% → 2.0MB");
        assert_eq!(fmt_savings(102_400, 204_800), "100KB → +100% → 200KB");
        assert_eq!(fmt_savings(0, 1024), "→ 1KB");
    }

    #[test]
    fn video_detection() {
        assert!(is_video(Path::new("clip.MOV")));
        assert!(is_video(Path::new("rec.mp4")));
        assert!(is_video(Path::new("animation.GIF")));
        assert!(!is_video(Path::new("photo.png")));
        assert!(!is_video(Path::new("rec.av1.mp4")));
        assert!(!is_video(Path::new("REC.AV1.MP4")));
    }

    #[test]
    fn fit_one_row_keeps_lines_to_one_terminal_row() {
        assert_eq!(fit_one_row("abcd", Some(4)), "abcd");
        assert_eq!(fit_one_row("abcdef", Some(4)), "abc…");
        assert_eq!(fit_one_row("multi\nline error", Some(80)), "multi");
        assert_eq!(
            fit_one_row("no tty → no truncation", None),
            "no tty → no truncation"
        );
    }
}

#[cfg(all(not(feature = "heic"), not(target_os = "macos")))]
const HEIC_UNSUPPORTED: &str = "HEIC support not compiled in";

#[cfg(all(not(feature = "heic"), not(target_os = "macos")))]
const HEIC_HINT: &str = "\nHEIC needs libheif. Install it (e.g. apt install libheif-dev), then:\n\
     \n\
     \x1b[36m  cargo install avify --features heic\x1b[0m\n";

enum ImageFormat {
    Raw,
    Heic,
    Jxl,
    Psd,
    Standard,
    StandardAlpha,
}

fn sniff_format(path: &Path) -> Option<ImageFormat> {
    use std::io::Read;
    let mut f = fs::File::open(path).ok()?;
    let mut buf = [0u8; 12];
    let n = f.read(&mut buf).ok()?;
    if n < 12 {
        return None;
    }
    if buf.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some(ImageFormat::StandardAlpha);
    }
    if buf.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some(ImageFormat::Standard);
    }
    if buf.starts_with(b"GIF87a") || buf.starts_with(b"GIF89a") {
        return Some(ImageFormat::Standard);
    }
    if &buf[0..4] == b"RIFF" && &buf[8..12] == b"WEBP" {
        return Some(ImageFormat::StandardAlpha);
    }
    if buf.starts_with(b"BM") {
        return Some(ImageFormat::Standard);
    }
    if buf.starts_with(b"II*\0") || buf.starts_with(b"MM\0*") {
        return Some(ImageFormat::Standard);
    }
    if &buf[4..8] == b"ftyp" {
        return Some(ImageFormat::Heic);
    }
    if buf.starts_with(&[0xFF, 0x0A])
        || buf.starts_with(&[0x00, 0x00, 0x00, 0x0C, b'J', b'X', b'L', b' '])
    {
        return Some(ImageFormat::Jxl);
    }
    if buf.starts_with(b"8BPS") {
        return Some(ImageFormat::Psd);
    }
    None
}

fn classify(path: &Path) -> ImageFormat {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase());

    match ext.as_deref() {
        Some(
            "arw" | "cr2" | "cr3" | "dng" | "nef" | "orf" | "raf" | "raw" | "rw2" | "pef" | "srw"
            | "x3f",
        ) => return ImageFormat::Raw,
        _ => {}
    }

    if let Some(sniffed) = sniff_format(path) {
        return sniffed;
    }

    match ext.as_deref() {
        Some("heic" | "heif") => ImageFormat::Heic,
        Some("jxl") => ImageFormat::Jxl,
        Some("psd") => ImageFormat::Psd,
        Some("png" | "webp") => ImageFormat::StandardAlpha,
        _ => ImageFormat::Standard,
    }
}

fn is_dataless(path: &Path) -> bool {
    use std::os::macos::fs::MetadataExt;
    const SF_DATALESS: u32 = 0x4000_0000;
    fs::metadata(path)
        .map(|m| m.st_flags() & SF_DATALESS != 0)
        .unwrap_or(false)
}

fn ensure_local(path: &Path) -> Result<()> {
    if !is_dataless(path) {
        return Ok(());
    }
    std::process::Command::new("brctl")
        .arg("download")
        .arg(path)
        .output()
        .context("Failed to run brctl download")?;
    for _ in 0..600 {
        if !is_dataless(path) {
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    anyhow::bail!("iCloud download timed out for {}", path.display())
}

fn decode_raw(path: &Path, use_xmp: bool) -> Result<DecodedImage> {
    let mut pipeline =
        imagepipe::Pipeline::new_from_file(path).map_err(|e| anyhow::anyhow!("{e}"))?;
    pipeline.globals.settings.maxwidth = 0;
    pipeline.globals.settings.maxheight = 0;

    let adj = if use_xmp {
        xmp::find_sidecar(path).and_then(|p| xmp::parse(&p).ok())
    } else {
        None
    };

    if let Some(ref adj) = adj {
        if let (Some(temp), Some(tint)) = (adj.temperature, adj.tint) {
            pipeline.ops.tolab.set_temp(temp, tint);
        }

        if adj.has_crop {
            pipeline.ops.rotatecrop.crop_top = adj.crop_top;
            pipeline.ops.rotatecrop.crop_left = adj.crop_left;
            pipeline.ops.rotatecrop.crop_bottom = 1.0 - adj.crop_bottom;
            pipeline.ops.rotatecrop.crop_right = 1.0 - adj.crop_right;
            pipeline.ops.rotatecrop.rotation = adj.crop_angle;
        }
    }

    let decoded = pipeline
        .output_8bit(None)
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let width = decoded.width as usize;
    let height = decoded.height as usize;
    let mut data = decoded.data;

    if let Some(ref adj) = adj {
        xmp::apply_tone(&mut data, adj);
    }

    let pixels: Vec<RGB8> = data
        .chunks_exact(3)
        .map(|rgb| RGB8::new(rgb[0], rgb[1], rgb[2]))
        .collect();

    Ok(DecodedImage::Rgb(ImgVec::new(pixels, width, height)))
}

#[cfg(feature = "heic")]
fn decode_heic(path: &Path) -> Result<DecodedImage> {
    use libheif_rs::{ColorSpace, HeifContext, LibHeif, RgbChroma};

    let lib_heif = LibHeif::new();
    let ctx = HeifContext::read_from_file(path.to_str().unwrap())
        .with_context(|| format!("Failed to open {}", path.display()))?;
    let handle = ctx.primary_image_handle()?;
    let width = handle.width() as usize;
    let height = handle.height() as usize;

    let image = lib_heif.decode(&handle, ColorSpace::Rgb(RgbChroma::Rgb), None)?;
    let plane = image.planes().interleaved.unwrap();
    let stride = plane.stride;
    let data = plane.data;

    let row_bytes = width * 3;
    let mut pixels = Vec::with_capacity(width * height);
    for y in 0..height {
        let row = &data[y * stride..y * stride + row_bytes];
        for chunk in row.chunks_exact(3) {
            pixels.push(RGB8::new(chunk[0], chunk[1], chunk[2]));
        }
    }

    Ok(DecodedImage::Rgb(ImgVec::new(pixels, width, height)))
}

#[cfg(all(not(feature = "heic"), target_os = "macos"))]
fn decode_heic(path: &Path) -> Result<DecodedImage> {
    static SEQ: AtomicUsize = AtomicUsize::new(0);
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let tmp = std::env::temp_dir().join(format!("avify-{}-{seq}.tiff", std::process::id()));

    let out = std::process::Command::new("sips")
        .args(["-s", "format", "tiff"])
        .arg(path)
        .arg("--out")
        .arg(&tmp)
        .output()
        .context("Failed to run sips")?;
    if !out.status.success() {
        let _ = fs::remove_file(&tmp);
        let msg = String::from_utf8_lossy(&out.stderr);
        let reason = msg
            .lines()
            .find(|l| l.starts_with("Error:"))
            .unwrap_or("unknown error");
        anyhow::bail!("sips failed: {reason}");
    }

    let img = decode_standard(&tmp, false);
    let _ = fs::remove_file(&tmp);
    img
}

#[cfg(all(not(feature = "heic"), not(target_os = "macos")))]
fn decode_heic(_path: &Path) -> Result<DecodedImage> {
    anyhow::bail!("{HEIC_UNSUPPORTED}")
}

fn decode_jxl(path: &Path) -> Result<DecodedImage> {
    use jxl_oxide::{JxlImage, PixelFormat};

    let image = JxlImage::builder()
        .open(path)
        .map_err(|e| anyhow::anyhow!("{e}"))
        .with_context(|| format!("Failed to open {}", path.display()))?;

    let render = image.render_frame(0).map_err(|e| anyhow::anyhow!("{e}"))?;

    let pf = image.pixel_format();
    let mut stream = render.stream();
    let width = stream.width() as usize;
    let height = stream.height() as usize;
    let channels = stream.channels() as usize;

    let mut buf = vec![0f32; width * height * channels];
    stream.write_to_buffer(&mut buf);

    let to_u8 = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;

    match pf {
        PixelFormat::Rgb => {
            let pixels: Vec<RGB8> = buf
                .chunks_exact(3)
                .map(|c| RGB8::new(to_u8(c[0]), to_u8(c[1]), to_u8(c[2])))
                .collect();
            Ok(DecodedImage::Rgb(ImgVec::new(pixels, width, height)))
        }
        PixelFormat::Rgba => {
            let pixels: Vec<RGBA8> = buf
                .chunks_exact(4)
                .map(|c| RGBA8::new(to_u8(c[0]), to_u8(c[1]), to_u8(c[2]), to_u8(c[3])))
                .collect();
            Ok(DecodedImage::Rgba(ImgVec::new(pixels, width, height)))
        }
        PixelFormat::Gray => {
            let pixels: Vec<RGB8> = buf
                .iter()
                .map(|&v| {
                    let g = to_u8(v);
                    RGB8::new(g, g, g)
                })
                .collect();
            Ok(DecodedImage::Rgb(ImgVec::new(pixels, width, height)))
        }
        PixelFormat::Graya => {
            let pixels: Vec<RGBA8> = buf
                .chunks_exact(2)
                .map(|c| {
                    let g = to_u8(c[0]);
                    RGBA8::new(g, g, g, to_u8(c[1]))
                })
                .collect();
            Ok(DecodedImage::Rgba(ImgVec::new(pixels, width, height)))
        }
        PixelFormat::Cmyk | PixelFormat::Cmyka => {
            anyhow::bail!("CMYK JXL not supported")
        }
    }
}

fn decode_psd(path: &Path) -> Result<DecodedImage> {
    let bytes = fs::read(path).with_context(|| format!("Failed to read {}", path.display()))?;
    let psd = psd::Psd::from_bytes(&bytes)
        .map_err(|e| anyhow::anyhow!("{e}"))
        .with_context(|| format!("Failed to parse {}", path.display()))?;

    let width = psd.width() as usize;
    let height = psd.height() as usize;
    let rgba = psd.rgba();

    let pixels: Vec<RGBA8> = rgba
        .chunks_exact(4)
        .map(|c| RGBA8::new(c[0], c[1], c[2], c[3]))
        .collect();

    Ok(DecodedImage::Rgba(ImgVec::new(pixels, width, height)))
}

fn decode_standard(path: &Path, alpha: bool) -> Result<DecodedImage> {
    let img = image::io::Reader::open(path)
        .with_context(|| format!("Failed to open {}", path.display()))?
        .with_guessed_format()
        .with_context(|| format!("Failed to guess format for {}", path.display()))?
        .decode()
        .with_context(|| format!("Failed to decode {}", path.display()))?;

    if alpha {
        let rgba = img.into_rgba8();
        let width = rgba.width() as usize;
        let height = rgba.height() as usize;
        let pixels: Vec<RGBA8> = rgba
            .pixels()
            .map(|px| RGBA8::new(px[0], px[1], px[2], px[3]))
            .collect();
        Ok(DecodedImage::Rgba(ImgVec::new(pixels, width, height)))
    } else {
        let rgb = img.into_rgb8();
        let width = rgb.width() as usize;
        let height = rgb.height() as usize;
        let pixels: Vec<RGB8> = rgb
            .pixels()
            .map(|px| RGB8::new(px[0], px[1], px[2]))
            .collect();
        Ok(DecodedImage::Rgb(ImgVec::new(pixels, width, height)))
    }
}

fn encode_avif(img: DecodedImage, quality: f32, speed: u8) -> Result<Vec<u8>> {
    let enc = Encoder::new()
        .with_quality(quality)
        .with_speed(speed)
        .with_bit_depth(BitDepth::Ten);

    let EncodedImage { avif_file, .. } = match img {
        DecodedImage::Rgb(rgb) => enc.encode_rgb(rgb.as_ref()),
        DecodedImage::Rgba(rgba) => enc.encode_rgba(rgba.as_ref()),
    }
    .context("AVIF encoding failed")?;

    Ok(avif_file)
}

const VIDEO_EXTENSIONS: &[&str] = &["mov", "mp4", "m4v", "webm", "mkv", "avi", "gif"];

fn is_video(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    if name.to_ascii_lowercase().ends_with(".av1.mp4") {
        return false;
    }
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| VIDEO_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

// Set on originals whose conversion came out bigger, so later runs skip
// them without re-encoding. Local-only: iCloud eviction can strip it, which
// just costs one wasted re-encode. Clear with: xattr -d com.avify.keep <file>
const KEEP_XATTR: &std::ffi::CStr = c"com.avify.keep";

fn path_cstring(path: &Path) -> Option<std::ffi::CString> {
    use std::os::unix::ffi::OsStrExt;
    std::ffi::CString::new(path.as_os_str().as_bytes()).ok()
}

fn has_keep_marker(path: &Path) -> bool {
    let Some(c) = path_cstring(path) else {
        return false;
    };
    let n = unsafe {
        libc::getxattr(
            c.as_ptr(),
            KEEP_XATTR.as_ptr(),
            std::ptr::null_mut(),
            0,
            0,
            0,
        )
    };
    n >= 0
}

fn set_keep_marker(path: &Path) {
    let Some(c) = path_cstring(path) else {
        return;
    };
    unsafe {
        libc::setxattr(
            c.as_ptr(),
            KEEP_XATTR.as_ptr(),
            b"1".as_ptr().cast(),
            1,
            0,
            0,
        );
    }
}

fn video_out_path(path: &Path, outdir: Option<&Path>) -> PathBuf {
    let name = format!(
        "{}.av1.mp4",
        path.file_stem().unwrap_or_default().to_string_lossy()
    );
    match outdir {
        Some(dir) => dir.join(name),
        None => path.with_file_name(name),
    }
}

struct VideoEncoder {
    ffmpeg: PathBuf,
    svt: bool,
}

// Prefer an ffmpeg with libsvtav1 (fast); PATH builds without it (e.g. tessus)
// only have slow libaom, so also probe the homebrew locations directly.
fn detect_video_encoder() -> Result<VideoEncoder> {
    let candidates = [
        "ffmpeg",
        "/opt/homebrew/bin/ffmpeg",
        "/usr/local/bin/ffmpeg",
    ];
    let mut fallback: Option<PathBuf> = None;
    for cand in candidates {
        let Ok(out) = std::process::Command::new(cand)
            .args(["-hide_banner", "-encoders"])
            .output()
        else {
            continue;
        };
        if !out.status.success() {
            continue;
        }
        let encoders = String::from_utf8_lossy(&out.stdout);
        if encoders.contains("libsvtav1") {
            return Ok(VideoEncoder {
                ffmpeg: cand.into(),
                svt: true,
            });
        }
        if fallback.is_none() && encoders.contains("libaom-av1") {
            fallback = Some(cand.into());
        }
    }
    if let Some(ffmpeg) = fallback {
        eprintln!(
            "\x1b[33mwarning: ffmpeg without libsvtav1 — using slow libaom (brew install ffmpeg for SVT-AV1)\x1b[0m"
        );
        return Ok(VideoEncoder { ffmpeg, svt: false });
    }
    anyhow::bail!(
        "--video requires ffmpeg with an AV1 encoder\n\
         \n\
         \x1b[36m  brew install ffmpeg\x1b[0m"
    )
}

// SVT-AV1 parallelizes internally across all cores; concurrent instances
// oversubscribe and can deadlock in svt_av1_enc_send_picture. One at a time.
static VIDEO_ENCODE_LOCK: Mutex<()> = Mutex::new(());

// ponytail: fixed crf 32 / preset 10 — validated on real screen recordings
// (87% smaller, 5.7x realtime, VMAF 97). Expose knobs if tuning ever matters.
fn transcode_video(src: &Path, dst: &Path, enc: &VideoEncoder) -> Result<u64> {
    let _serial = VIDEO_ENCODE_LOCK.lock().unwrap();
    let vargs: &[&str] = if enc.svt {
        &["-c:v", "libsvtav1", "-crf", "32", "-preset", "10"]
    } else {
        &[
            "-c:v",
            "libaom-av1",
            "-crf",
            "32",
            "-b:v",
            "0",
            "-cpu-used",
            "8",
            "-row-mt",
            "1",
            "-pix_fmt",
            "yuv420p",
        ]
    };
    let out = std::process::Command::new(&enc.ffmpeg)
        .args(["-y", "-loglevel", "error", "-i"])
        .arg(src)
        .args(vargs)
        .args(["-c:a", "copy"])
        .arg(dst)
        .output()
        .context("Failed to run ffmpeg")?;
    if !out.status.success() {
        fs::remove_file(dst).ok();
        anyhow::bail!(
            "ffmpeg failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    fs::metadata(dst)
        .map(|m| m.len())
        .with_context(|| format!("Failed to stat {}", dst.display()))
}

fn preserve_timestamps(src: &Path, dst: &Path) -> Result<()> {
    use std::fs::FileTimes;
    use std::os::darwin::fs::FileTimesExt;

    let meta = fs::metadata(src)
        .with_context(|| format!("Failed to read timestamps from {}", src.display()))?;

    let mut times = FileTimes::new();
    if let Ok(created) = meta.created() {
        times = times.set_created(created);
    }
    if let Ok(modified) = meta.modified() {
        times = times.set_modified(modified);
    }
    if let Ok(accessed) = meta.accessed() {
        times = times.set_accessed(accessed);
    }

    let f = fs::File::options()
        .write(true)
        .open(dst)
        .with_context(|| format!("Failed to open {}", dst.display()))?;
    f.set_times(times)
        .with_context(|| format!("Failed to set timestamps on {}", dst.display()))?;
    Ok(())
}

fn trash_file(path: &Path) -> Result<()> {
    // ponytail: NsFileManager instead of Finder AppleScript — same trash, no crumple sound
    let mut ctx = trash::TrashContext::default();
    #[cfg(target_os = "macos")]
    {
        use trash::macos::{DeleteMethod, TrashContextExtMacos};
        ctx.set_delete_method(DeleteMethod::NsFileManager);
    }
    ctx.delete(path)
        .with_context(|| format!("Trash failed for {}", path.display()))?;
    Ok(())
}

fn process_file(
    idx: usize,
    path: &PathBuf,
    quality: f32,
    speed: u8,
    use_xmp: bool,
    video_enc: Option<&VideoEncoder>,
    keep_originals: bool,
    outdir: Option<&Path>,
    move_originals: Option<&Path>,
    progress: &Mutex<Progress>,
) -> Result<()> {
    {
        let mut p = progress.lock().unwrap();
        p.set(idx, Status::Processing);
        p.render();
    }

    ensure_local(path)?;

    let orig_bytes = fs::metadata(path).map(|m| m.len()).unwrap_or(0);

    let (out_path, out_bytes) = match video_enc {
        Some(enc) if is_video(path) => {
            let out = video_out_path(path, outdir);
            let bytes = transcode_video(path, &out, enc)?;
            (out, bytes as usize)
        }
        _ => encode_image(path, quality, speed, use_xmp, outdir)?,
    };

    if orig_bytes > 0 && out_bytes as u64 >= orig_bytes {
        fs::remove_file(&out_path)
            .with_context(|| format!("Failed to remove {}", out_path.display()))?;
        set_keep_marker(path);
        let mut p = progress.lock().unwrap();
        p.set(
            idx,
            Status::Kept {
                orig_bytes,
                avif_bytes: out_bytes,
            },
        );
        p.render();
        return Ok(());
    }

    preserve_timestamps(path, &out_path).ok();

    if let Some(dir) = move_originals {
        let dest = dir.join(path.file_name().unwrap_or_default());
        fs::rename(path, &dest)
            .with_context(|| format!("Failed to move {} → {}", path.display(), dest.display()))?;
    } else if !keep_originals {
        trash_file(path)?;
    }

    {
        let mut p = progress.lock().unwrap();
        p.set(
            idx,
            Status::Done {
                orig_bytes,
                avif_bytes: out_bytes,
            },
        );
        p.render();
    }

    Ok(())
}

fn encode_image(
    path: &Path,
    quality: f32,
    speed: u8,
    use_xmp: bool,
    outdir: Option<&Path>,
) -> Result<(PathBuf, usize)> {
    let out_path = match outdir {
        Some(dir) => dir
            .join(path.file_stem().unwrap_or_default())
            .with_extension("avif"),
        None => path.with_extension("avif"),
    };

    let result = match classify(path) {
        ImageFormat::Raw => decode_raw(path, use_xmp),
        ImageFormat::Heic => decode_heic(path),
        ImageFormat::Jxl => decode_jxl(path),
        ImageFormat::Psd => decode_psd(path),
        ImageFormat::StandardAlpha => decode_standard(path, true),
        ImageFormat::Standard => decode_standard(path, false),
    };

    let img = result?;

    let avif_data = encode_avif(img, quality, speed)?;

    fs::write(&out_path, &avif_data)
        .with_context(|| format!("Failed to write {}", out_path.display()))?;

    Ok((out_path, avif_data.len()))
}

const SUPPORTED_EXTENSIONS: &[&str] = &[
    "arw", "cr2", "cr3", "dng", "nef", "orf", "raf", "raw", "rw2", "pef", "srw", "x3f", "heic",
    "heif", "jpg", "jpeg", "png", "webp", "bmp", "tiff", "tif", "gif", "tga", "jxl", "psd",
];

fn collect_images_from_dir(dir: &Path, include_video: bool) -> Result<Vec<PathBuf>> {
    let mut files: Vec<PathBuf> = fs::read_dir(dir)
        .with_context(|| format!("Failed to read {}", dir.display()))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.extension()
                    .and_then(|e| e.to_str())
                    .map(|e| SUPPORTED_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
                    .unwrap_or(false)
                || (include_video && is_video(p))
        })
        .collect();
    files.sort();
    Ok(files)
}

fn expand_dirs(files: Vec<PathBuf>, include_video: bool) -> Result<Vec<PathBuf>> {
    let mut out = Vec::with_capacity(files.len());
    for p in files {
        if p.is_dir() {
            out.extend(collect_images_from_dir(&p, include_video)?);
        } else {
            out.push(p);
        }
    }
    Ok(out)
}

fn main() -> Result<()> {
    let mut args = Args::parse();

    if args.files.is_empty() {
        args.files = collect_images_from_dir(Path::new("."), args.video)?;
        if args.files.is_empty() {
            anyhow::bail!("No image files found in current directory");
        }
    } else {
        args.files = expand_dirs(std::mem::take(&mut args.files), args.video)?;
        if args.files.is_empty() {
            anyhow::bail!("No image files found in given paths");
        }
    }

    let before = args.files.len();
    args.files.retain(|p| !has_keep_marker(p));
    let marked = before - args.files.len();
    if marked > 0 {
        eprintln!(
            "\x1b[33m{marked} file(s) skipped — previously kept as smaller than conversion (xattr -d com.avify.keep to retry)\x1b[0m"
        );
    }
    if args.files.is_empty() {
        return Ok(());
    }

    if let Some(ref dir) = args.outdir {
        fs::create_dir_all(dir).context("Failed to create output directory")?;
    }
    if let Some(ref dir) = args.move_originals {
        fs::create_dir_all(dir).context("Failed to create originals directory")?;
    }

    // GIFs are animations even though they are collected with image formats;
    // always transcode them so their frames are preserved. Other video formats
    // remain opt-in behind --video.
    let video_enc = if args.video || args.files.iter().any(|path| is_video(path)) {
        Some(detect_video_encoder()?)
    } else {
        None
    };

    let progress = Mutex::new(Progress::new(&args.files));
    let next = AtomicUsize::new(0);

    (0..rayon::current_num_threads())
        .into_par_iter()
        .for_each(|_| loop {
            let idx = next.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if idx >= args.files.len() {
                break;
            }
            if let Err(e) = process_file(
                idx,
                &args.files[idx],
                args.quality,
                args.speed,
                args.xmp,
                video_enc.as_ref(),
                args.keep,
                args.outdir.as_deref(),
                args.move_originals.as_deref(),
                &progress,
            ) {
                let mut p = progress.lock().unwrap();
                p.set(idx, Status::Failed(format!("{e:#}")));
                p.render();
            }
        });

    let failed = {
        let p = progress.lock().unwrap();
        // Don't re-render, just print summary
        let (mut orig_total, mut avif_total, mut count, mut kept) = (0u64, 0u64, 0u64, 0u64);
        let mut failures: Vec<(&str, &str)> = Vec::new();
        for (idx, status) in p.statuses.iter().enumerate() {
            match status {
                Status::Done {
                    orig_bytes,
                    avif_bytes,
                } => {
                    orig_total += orig_bytes;
                    avif_total += *avif_bytes as u64;
                    count += 1;
                }
                Status::Kept { .. } => kept += 1,
                Status::Failed(err) => failures.push((p.names[idx].as_str(), err.as_str())),
                _ => {}
            }
        }

        let mut out = io::stderr().lock();
        if count > 0 && orig_total > 0 {
            let saved = orig_total.saturating_sub(avif_total);
            let pct = saved * 100 / orig_total;
            write!(
                out,
                "{count} files: {} → {} (saved {}, {pct}%)\n",
                fmt_size(orig_total),
                fmt_size(avif_total),
                fmt_size(saved),
            )
            .ok();
        }
        if kept > 0 {
            write!(out, "{kept} file(s) kept — conversion was not smaller\n").ok();
        }
        if !failures.is_empty() {
            write!(out, "{} file(s) failed:\n", failures.len()).ok();
            for (name, err) in &failures {
                write!(out, "  {name}: {err}\n").ok();
            }
            #[cfg(all(not(feature = "heic"), not(target_os = "macos")))]
            if failures.iter().any(|(_, err)| *err == HEIC_UNSUPPORTED) {
                write!(out, "{HEIC_HINT}").ok();
            }
        }
        drop(out);
        failures.len()
    };

    if failed > 0 {
        std::process::exit(1);
    }
    Ok(())
}
