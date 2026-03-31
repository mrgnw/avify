use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Default)]
pub struct Adjustments {
    pub exposure: f32,
    pub temperature: Option<f32>,
    pub tint: Option<f32>,
    pub contrast: f32,
    pub highlights: f32,
    pub shadows: f32,
    pub whites: f32,
    pub blacks: f32,
    pub vibrance: f32,
    pub saturation: f32,
    pub crop_top: f32,
    pub crop_left: f32,
    pub crop_bottom: f32,
    pub crop_right: f32,
    pub crop_angle: f32,
    pub has_crop: bool,
}

pub fn find_sidecar(raw_path: &Path) -> Option<PathBuf> {
    // photo.ARW.xmp
    let mut xmp = raw_path.as_os_str().to_owned();
    xmp.push(".xmp");
    let path = PathBuf::from(&xmp);
    if path.exists() {
        return Some(path);
    }

    // photo.xmp
    let path = raw_path.with_extension("xmp");
    if path.exists() {
        return Some(path);
    }

    None
}

pub fn parse(path: &Path) -> Result<Adjustments> {
    let xml = fs::read_to_string(path).context("Failed to read XMP sidecar")?;
    let doc = roxmltree::Document::parse(&xml).context("Failed to parse XMP XML")?;

    let mut adj = Adjustments::default();

    for node in doc.descendants() {
        if !node.is_element() {
            continue;
        }

        for attr in node.attributes() {
            let name = attr.name();
            let val = attr.value();

            match name {
                "Exposure2012" => adj.exposure = parse_f32(val),
                "Temperature" => adj.temperature = Some(parse_f32(val)),
                "Tint" => adj.tint = Some(parse_f32(val)),
                "Contrast2012" => adj.contrast = parse_f32(val),
                "Highlights2012" => adj.highlights = parse_f32(val),
                "Shadows2012" => adj.shadows = parse_f32(val),
                "Whites2012" => adj.whites = parse_f32(val),
                "Blacks2012" => adj.blacks = parse_f32(val),
                "Vibrance" => adj.vibrance = parse_f32(val),
                "Saturation" => adj.saturation = parse_f32(val),
                "CropTop" => adj.crop_top = parse_f32(val),
                "CropLeft" => adj.crop_left = parse_f32(val),
                "CropBottom" => adj.crop_bottom = parse_f32(val),
                "CropRight" => adj.crop_right = parse_f32(val),
                "CropAngle" => adj.crop_angle = parse_f32(val),
                "HasCrop" => adj.has_crop = val.eq_ignore_ascii_case("true"),
                _ => {}
            }
        }
    }

    Ok(adj)
}

fn parse_f32(s: &str) -> f32 {
    s.trim_start_matches('+').parse().unwrap_or(0.0)
}

/// Apply tone adjustments to an 8-bit RGB buffer in-place.
pub fn apply_tone(data: &mut [u8], adj: &Adjustments) {
    if adj.exposure.abs() < 0.01
        && adj.contrast.abs() < 0.5
        && adj.highlights.abs() < 0.5
        && adj.shadows.abs() < 0.5
        && adj.whites.abs() < 0.5
        && adj.blacks.abs() < 0.5
        && adj.vibrance.abs() < 0.5
        && adj.saturation.abs() < 0.5
    {
        return;
    }

    // Build a per-channel LUT for exposure/contrast/highlights/shadows/whites/blacks
    let exposure_mul = (2.0_f32).powf(adj.exposure);
    let mut lut = [0u8; 256];
    let contrast_factor = 1.0 + adj.contrast / 100.0;
    let whites_shift = adj.whites / 100.0 * 64.0;
    let blacks_shift = adj.blacks / 100.0 * 64.0;

    for i in 0..256 {
        let mut v = i as f32 / 255.0;

        // Exposure: multiply in linear-ish space
        v *= exposure_mul;

        // Contrast: S-curve around midpoint
        v = 0.5 + (v - 0.5) * contrast_factor;

        // Highlights: affect upper range (luminance > 0.5)
        if v > 0.5 {
            let weight = (v - 0.5) * 2.0;
            v += adj.highlights / 100.0 * weight * 0.3;
        }

        // Shadows: affect lower range (luminance < 0.5)
        if v < 0.5 {
            let weight = (0.5 - v) * 2.0;
            v += adj.shadows / 100.0 * weight * 0.3;
        }

        // Whites/blacks: shift endpoints
        v = v * (255.0 + whites_shift) / 255.0 + blacks_shift / 255.0;

        lut[i] = (v * 255.0).round().clamp(0.0, 255.0) as u8;
    }

    let sat_factor = 1.0 + adj.saturation / 100.0;
    let vib_amount = adj.vibrance / 100.0;

    for pixel in data.chunks_exact_mut(3) {
        let r = pixel[0] as f32;
        let g = pixel[1] as f32;
        let b = pixel[2] as f32;

        // Apply luminance LUT
        let lum = (0.299 * r + 0.587 * g + 0.114 * b) as u8;
        let lut_lum = lut[lum as usize] as f32;
        let lum_ratio = if lum > 0 { lut_lum / lum as f32 } else { 1.0 };

        let mut r = r * lum_ratio;
        let mut g = g * lum_ratio;
        let mut b = b * lum_ratio;

        // Saturation + vibrance
        let gray = 0.299 * r + 0.587 * g + 0.114 * b;
        let max_ch = r.max(g).max(b);
        let min_ch = r.min(g).min(b);
        let current_sat = if max_ch > 0.0 {
            (max_ch - min_ch) / max_ch
        } else {
            0.0
        };

        // Vibrance: boost less-saturated colors more
        let vib_factor = 1.0 + vib_amount * (1.0 - current_sat);
        let total_sat = sat_factor * vib_factor;

        r = gray + (r - gray) * total_sat;
        g = gray + (g - gray) * total_sat;
        b = gray + (b - gray) * total_sat;

        pixel[0] = r.round().clamp(0.0, 255.0) as u8;
        pixel[1] = g.round().clamp(0.0, 255.0) as u8;
        pixel[2] = b.round().clamp(0.0, 255.0) as u8;
    }
}
