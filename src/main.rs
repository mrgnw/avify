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
#[command(about = "Convert images to AVIF (supports RAW + standard formats)")]
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

    #[arg(short, long, help = "Trash originals after conversion (macOS)")]
    trash: bool,

    #[arg(short = 'x', long, help = "Apply Lightroom XMP sidecar edits")]
    xmp: bool,

    files: Vec<PathBuf>,
}

#[derive(Clone)]
enum Status {
    Pending,
    Processing,
    Done { orig_bytes: u64, avif_bytes: usize },
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
                Status::Done { avif_bytes, .. } => {
                    let n = self.flushed + 1;
                    let kb = avif_bytes / 1024;
                    write!(
                        out,
                        "\x1b[2K\x1b[32m{n:>width$}/{total} {} → {kb}KB\x1b[0m\n",
                        self.names[self.flushed]
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

        // Draw active (in-progress) lines — only these get redrawn
        let mut active = 0;
        for i in self.flushed..total {
            match &self.statuses[i] {
                Status::Processing => {
                    let n = i + 1;
                    write!(
                        out,
                        "\x1b[2K\x1b[33m{n:>width$}/{total} {} →\x1b[0m\n",
                        self.names[i]
                    )
                    .ok();
                    active += 1;
                }
                Status::Done { avif_bytes, .. } => {
                    let n = i + 1;
                    let kb = avif_bytes / 1024;
                    write!(
                        out,
                        "\x1b[2K\x1b[32m{n:>width$}/{total} {} → {kb}KB\x1b[0m\n",
                        self.names[i]
                    )
                    .ok();
                    active += 1;
                }
                Status::Failed(e) => {
                    let n = i + 1;
                    write!(
                        out,
                        "\x1b[2K\x1b[31m{n:>width$}/{total} {} FAIL: {e}\x1b[0m\n",
                        self.names[i]
                    )
                    .ok();
                    active += 1;
                }
                Status::Pending => break,
            }
        }

        self.active_lines = active;
        out.flush().ok();
    }
}

enum ImageFormat {
    Raw,
    #[cfg(feature = "heic")]
    Heic,
    Standard,
    StandardAlpha,
}

fn classify(path: &Path) -> ImageFormat {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .as_deref()
    {
        Some(
            "arw" | "cr2" | "cr3" | "dng" | "nef" | "orf" | "raf" | "raw" | "rw2" | "pef" | "srw"
            | "x3f",
        ) => ImageFormat::Raw,
        #[cfg(feature = "heic")]
        Some("heic" | "heif") => ImageFormat::Heic,
        Some("png" | "webp") => ImageFormat::StandardAlpha,
        _ => ImageFormat::Standard,
    }
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

fn decode_standard(path: &Path, alpha: bool) -> Result<DecodedImage> {
    let img = image::open(path).with_context(|| format!("Failed to open {}", path.display()))?;

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

fn trash_file(path: &Path) -> Result<()> {
    let posix = path
        .canonicalize()
        .with_context(|| format!("Failed to resolve {}", path.display()))?
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    let script = format!(
        "tell application \"Finder\" to delete (POSIX file \"{}\" as alias)",
        posix
    );
    let status = std::process::Command::new("osascript")
        .args(["-e", &script])
        .output()
        .context("Failed to run osascript")?;
    if !status.status.success() {
        anyhow::bail!(
            "Trash failed: {}",
            String::from_utf8_lossy(&status.stderr).trim()
        );
    }
    Ok(())
}

fn process_file(
    idx: usize,
    path: &PathBuf,
    quality: f32,
    speed: u8,
    use_xmp: bool,
    trash_originals: bool,
    outdir: Option<&Path>,
    move_originals: Option<&Path>,
    progress: &Mutex<Progress>,
) -> Result<()> {
    {
        let mut p = progress.lock().unwrap();
        p.set(idx, Status::Processing);
        p.render();
    }

    let out_path = match outdir {
        Some(dir) => dir
            .join(path.file_stem().unwrap_or_default())
            .with_extension("avif"),
        None => path.with_extension("avif"),
    };

    let result = match classify(path) {
        ImageFormat::Raw => decode_raw(path, use_xmp),
        #[cfg(feature = "heic")]
        ImageFormat::Heic => decode_heic(path),
        ImageFormat::StandardAlpha => decode_standard(path, true),
        ImageFormat::Standard => decode_standard(path, false),
    };

    let img = match result {
        Ok(img) => img,
        Err(e) => {
            let msg = format!("{e:#}");
            let mut p = progress.lock().unwrap();
            p.set(idx, Status::Failed(msg.clone()));
            p.render();
            return Err(e).with_context(|| format!("Failed to decode {}", path.display()));
        }
    };

    let avif_data = encode_avif(img, quality, speed)?;

    let orig_bytes = fs::metadata(path).map(|m| m.len()).unwrap_or(0);

    fs::write(&out_path, &avif_data)
        .with_context(|| format!("Failed to write {}", out_path.display()))?;

    if let Some(dir) = move_originals {
        let dest = dir.join(path.file_name().unwrap_or_default());
        fs::rename(path, &dest)
            .with_context(|| format!("Failed to move {} → {}", path.display(), dest.display()))?;
    } else if trash_originals {
        trash_file(path)?;
    }

    {
        let mut p = progress.lock().unwrap();
        p.set(
            idx,
            Status::Done {
                orig_bytes,
                avif_bytes: avif_data.len(),
            },
        );
        p.render();
    }

    Ok(())
}

#[cfg(feature = "heic")]
const SUPPORTED_EXTENSIONS: &[&str] = &[
    "arw", "cr2", "cr3", "dng", "nef", "orf", "raf", "raw", "rw2", "pef", "srw", "x3f", "heic",
    "heif", "jpg", "jpeg", "png", "webp", "bmp", "tiff", "tif", "gif",
];

#[cfg(not(feature = "heic"))]
const SUPPORTED_EXTENSIONS: &[&str] = &[
    "arw", "cr2", "cr3", "dng", "nef", "orf", "raf", "raw", "rw2", "pef", "srw", "x3f", "jpg",
    "jpeg", "png", "webp", "bmp", "tiff", "tif", "gif",
];

fn collect_images_from_cwd() -> Result<Vec<PathBuf>> {
    let mut files: Vec<PathBuf> = fs::read_dir(".")
        .context("Failed to read current directory")?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .map(|e| SUPPORTED_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
                .unwrap_or(false)
        })
        .collect();
    files.sort();
    Ok(files)
}

fn main() -> Result<()> {
    let mut args = Args::parse();

    if args.files.is_empty() {
        args.files = collect_images_from_cwd()?;
        if args.files.is_empty() {
            anyhow::bail!("No image files found in current directory");
        }
    }

    if let Some(ref dir) = args.outdir {
        fs::create_dir_all(dir).context("Failed to create output directory")?;
    }
    if let Some(ref dir) = args.move_originals {
        fs::create_dir_all(dir).context("Failed to create originals directory")?;
    }

    let progress = Mutex::new(Progress::new(&args.files));
    let next = AtomicUsize::new(0);

    let result: Result<()> = (0..rayon::current_num_threads())
        .into_par_iter()
        .try_for_each(|_| -> Result<()> {
            loop {
                let idx = next.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                if idx >= args.files.len() {
                    break;
                }
                process_file(
                    idx,
                    &args.files[idx],
                    args.quality,
                    args.speed,
                    args.xmp,
                    args.trash,
                    args.outdir.as_deref(),
                    args.move_originals.as_deref(),
                    &progress,
                )?;
            }
            Ok(())
        });

    {
        let p = progress.lock().unwrap();
        // Don't re-render, just print summary
        let (mut orig_total, mut avif_total, mut count) = (0u64, 0u64, 0u64);
        for status in &p.statuses {
            if let Status::Done {
                orig_bytes,
                avif_bytes,
            } = status
            {
                orig_total += orig_bytes;
                avif_total += *avif_bytes as u64;
                count += 1;
            }
        }
        drop(p);

        if count > 0 && orig_total > 0 {
            let saved = orig_total.saturating_sub(avif_total);
            let pct = saved * 100 / orig_total;
            let mut out = io::stderr().lock();
            if orig_total > 1_048_576 {
                write!(
                    out,
                    "{count} files: {:.1}MB → {:.1}MB (saved {:.1}MB, {pct}%)\n",
                    orig_total as f64 / 1_048_576.0,
                    avif_total as f64 / 1_048_576.0,
                    saved as f64 / 1_048_576.0,
                )
                .ok();
            } else {
                write!(
                    out,
                    "{count} files: {}KB → {}KB (saved {}KB, {pct}%)\n",
                    orig_total / 1024,
                    avif_total / 1024,
                    saved / 1024,
                )
                .ok();
            }
        }
    }

    result
}
