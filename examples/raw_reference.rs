//! Writes the 8-bit PNG that avify encodes for a RAW file, so benchmarks can compare it with the AVIF.

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    anyhow::ensure!(
        args.len() == 3,
        "usage: raw_reference <input.raw> <output.png>"
    );
    let mut pipeline =
        imagepipe::Pipeline::new_from_file(&args[1]).map_err(|e| anyhow::anyhow!("{e}"))?;
    pipeline.globals.settings.maxwidth = 0;
    pipeline.globals.settings.maxheight = 0;
    let decoded = pipeline
        .output_8bit(None)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    image::save_buffer(
        &args[2],
        &decoded.data,
        decoded.width as u32,
        decoded.height as u32,
        image::ColorType::Rgb8,
    )?;
    Ok(())
}
