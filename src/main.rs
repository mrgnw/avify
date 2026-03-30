use anyhow::{Context, Result};
use clap::Parser;
use imgref::ImgVec;
use ravif::{BitDepth, EncodedImage, Encoder, RGBA8};
use rayon::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

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
    path: &PathBuf,
    quality: f32,
    speed: u8,
    outdir: Option<&Path>,
    move_originals: Option<&Path>,
    done: &AtomicUsize,
    total: usize,
) -> Result<()> {
    let out_path = match outdir {
        Some(dir) => dir
            .join(path.file_stem().unwrap_or_default())
            .with_extension("avif"),
        None => path.with_extension("avif"),
    };
    let name = path.file_name().unwrap_or_default().to_string_lossy();

    let img = if is_raw(path) {
        decode_raw(path)
    } else {
        decode_image(path)
    }
    .with_context(|| format!("Failed to decode {}", path.display()))?;

    let avif_data = encode_avif(img, quality, speed)?;

    fs::write(&out_path, &avif_data)
        .with_context(|| format!("Failed to write {}", out_path.display()))?;

    if let Some(dir) = move_originals {
        let dest = dir.join(path.file_name().unwrap_or_default());
        fs::rename(path, &dest)
            .with_context(|| format!("Failed to move {} → {}", path.display(), dest.display()))?;
    }

    let n = done.fetch_add(1, Ordering::Relaxed) + 1;
    println!("[{n}/{total}] {name} → {}KB", avif_data.len() / 1024);
    Ok(())
}

fn main() -> Result<()> {
    let args = Args::parse();
    let total = args.files.len();
    let done = AtomicUsize::new(0);

    if let Some(ref dir) = args.outdir {
        fs::create_dir_all(dir).context("Failed to create output directory")?;
    }
    if let Some(ref dir) = args.move_originals {
        fs::create_dir_all(dir).context("Failed to create originals directory")?;
    }

    args.files.par_iter().try_for_each(|path| {
        process_file(
            path,
            args.quality,
            args.speed,
            args.outdir.as_deref(),
            args.move_originals.as_deref(),
            &done,
            total,
        )
    })?;

    Ok(())
}
