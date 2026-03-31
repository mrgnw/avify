use anyhow::{Context, Result};
use clap::Parser;
use imgref::ImgVec;
use ravif::{BitDepth, EncodedImage, Encoder, RGBA8};
use rayon::prelude::*;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicUsize;
use std::sync::Mutex;

#[derive(Parser)]
#[command(about = "Convert images to AVIF (supports RAW + standard formats)")]
struct Args {
    #[arg(short, long, default_value = "80")]
    quality: f32,

    #[arg(short, long, default_value = "6")]
    speed: u8,

    #[arg(short, long, help = "Output directory for AVIF files")]
    outdir: Option<PathBuf>,

    #[arg(
        short,
        long,
        help = "Move originals to this directory after conversion"
    )]
    move_originals: Option<PathBuf>,

    #[arg(required = true)]
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
    rendered_lines: usize,
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
            rendered_lines: 0,
        }
    }

    fn set(&mut self, idx: usize, status: Status) {
        self.statuses[idx] = status;
    }

    fn render(&mut self) {
        let total = self.statuses.len();
        let width = total.to_string().len();
        let mut out = io::stderr().lock();

        if self.rendered_lines > 0 {
            write!(out, "\x1b[{}A", self.rendered_lines).ok();
        }

        let mut lines = 0;
        for (i, status) in self.statuses.iter().enumerate() {
            let n = i + 1;
            match status {
                Status::Pending => continue,
                Status::Processing => {
                    // yellow
                    write!(
                        out,
                        "\x1b[2K\x1b[33m{n:>width$}/{total} {} →\x1b[0m\n",
                        self.names[i]
                    )
                    .ok();
                    lines += 1;
                }
                Status::Done { avif_bytes, .. } => {
                    let kb = avif_bytes / 1024;
                    // green
                    write!(
                        out,
                        "\x1b[2K\x1b[32m{n:>width$}/{total} {} → {kb}KB\x1b[0m\n",
                        self.names[i]
                    )
                    .ok();
                    lines += 1;
                }
                Status::Failed(e) => {
                    // red
                    write!(
                        out,
                        "\x1b[2K\x1b[31m{n:>width$}/{total} {} FAIL: {e}\x1b[0m\n",
                        self.names[i]
                    )
                    .ok();
                    lines += 1;
                }
            }
        }

        self.rendered_lines = lines;
        out.flush().ok();
    }
}

fn is_raw(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .as_deref(),
        Some(
            "arw"
                | "cr2"
                | "cr3"
                | "dng"
                | "nef"
                | "orf"
                | "raf"
                | "raw"
                | "rw2"
                | "pef"
                | "srw"
                | "x3f"
        )
    )
}

fn decode_raw(path: &Path) -> Result<ImgVec<RGBA8>> {
    let mut pipeline =
        imagepipe::Pipeline::new_from_file(path).map_err(|e| anyhow::anyhow!("{e}"))?;
    pipeline.globals.settings.maxwidth = 0;
    pipeline.globals.settings.maxheight = 0;
    let decoded = pipeline
        .output_8bit(None)
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let width = decoded.width as usize;
    let height = decoded.height as usize;
    let pixels: Vec<RGBA8> = decoded
        .data
        .chunks_exact(3)
        .map(|rgb| RGBA8::new(rgb[0], rgb[1], rgb[2], 255))
        .collect();

    Ok(ImgVec::new(pixels, width, height))
}

fn decode_image(path: &Path) -> Result<ImgVec<RGBA8>> {
    let img = image::open(path)
        .with_context(|| format!("Failed to open {}", path.display()))?
        .into_rgba8();
    let width = img.width() as usize;
    let height = img.height() as usize;
    let pixels: Vec<RGBA8> = img
        .pixels()
        .map(|px| RGBA8::new(px[0], px[1], px[2], px[3]))
        .collect();

    Ok(ImgVec::new(pixels, width, height))
}

fn encode_avif(img: ImgVec<RGBA8>, quality: f32, speed: u8) -> Result<Vec<u8>> {
    let enc = Encoder::new()
        .with_quality(quality)
        .with_speed(speed)
        .with_bit_depth(BitDepth::Ten);

    let EncodedImage { avif_file, .. } = enc
        .encode_rgba(img.as_ref())
        .context("AVIF encoding failed")?;

    Ok(avif_file)
}

fn process_file(
    idx: usize,
    path: &PathBuf,
    quality: f32,
    speed: u8,
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

    let result = if is_raw(path) {
        decode_raw(path)
    } else {
        decode_image(path)
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

    fs::write(&out_path, &avif_data)
        .with_context(|| format!("Failed to write {}", out_path.display()))?;

    if let Some(dir) = move_originals {
        let dest = dir.join(path.file_name().unwrap_or_default());
        fs::rename(path, &dest)
            .with_context(|| format!("Failed to move {} → {}", path.display(), dest.display()))?;
    }

    let orig_bytes = fs::metadata(path).map(|m| m.len()).unwrap_or(0);

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

fn main() -> Result<()> {
    let args = Args::parse();

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
