use anyhow::{Context, Result};
use clap::Parser;
use imgref::ImgVec;
use ravif::{BitDepth, EncodedImage, Encoder, RGBA8};
use rayon::prelude::*;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Parser)]
#[command(about = "Convert camera RAW files to AVIF")]
struct Args {
    #[arg(short, long, default_value = "80")]
    quality: f32,

    #[arg(short, long, default_value = "6")]
    speed: u8,

    #[arg(required = true)]
    files: Vec<PathBuf>,
}

fn decode_raw(path: &PathBuf) -> Result<ImgVec<RGBA8>> {
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
    done: &AtomicUsize,
    total: usize,
) -> Result<()> {
    let out_path = path.with_extension("avif");
    let name = path.file_name().unwrap_or_default().to_string_lossy();

    let img = decode_raw(path).with_context(|| format!("Failed to decode {}", path.display()))?;
    let avif_data = encode_avif(img, quality, speed)?;

    fs::write(&out_path, &avif_data)
        .with_context(|| format!("Failed to write {}", out_path.display()))?;

    let n = done.fetch_add(1, Ordering::Relaxed) + 1;
    println!("[{n}/{total}] {name} → {}KB", avif_data.len() / 1024);
    Ok(())
}

fn main() -> Result<()> {
    let args = Args::parse();
    let total = args.files.len();
    let done = AtomicUsize::new(0);

    args.files
        .par_iter()
        .try_for_each(|path| process_file(path, args.quality, args.speed, &done, total))?;

    Ok(())
}
