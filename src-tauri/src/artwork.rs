//! Préparation des pochettes : image JPEG carrée, 1000 × 1000 pixels par défaut,
//! qualité 85, 1 Mo au plus (cahier des charges, « Pochettes »).

use image::imageops::FilterType;
use image::{DynamicImage, GenericImageView, ImageReader};
use std::io::Cursor;

use crate::tags::write::CoverData;

pub const MAX_BYTES: usize = 1_000_000;

#[derive(Debug, thiserror::Error)]
pub enum ArtworkError {
    #[error("image illisible ou format non pris en charge : {0}")]
    Decode(String),
    #[error("image trop petite ({0} × {1} pixels)")]
    TooSmall(u32, u32),
    #[error("conversion en JPEG impossible : {0}")]
    Encode(String),
}

#[derive(Debug, Clone)]
pub struct Prepared {
    pub cover: CoverData,
    pub width: u32,
    pub height: u32,
    /// L'image n'était pas carrée : elle n'a pas été recadrée (l'éditeur de recadrage arrive en phase 2).
    pub not_square: bool,
}

/// Recadrage carré centré si l'image est presque carrée (écart de 10 % au plus).
fn square_if_close(img: DynamicImage) -> (DynamicImage, bool) {
    let (w, h) = img.dimensions();
    let (big, small) = (w.max(h) as f64, w.min(h) as f64);
    if (big - small) / big <= 0.10 {
        let side = w.min(h);
        let x = (w - side) / 2;
        let y = (h - side) / 2;
        (img.crop_imm(x, y, side, side), false)
    } else {
        (img, w != h)
    }
}

/// Transforme une image quelconque (JPEG, PNG, WebP…) en pochette prête à écrire.
pub fn prepare(bytes: &[u8], target: u32) -> Result<Prepared, ArtworkError> {
    let img = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| ArtworkError::Decode(e.to_string()))?
        .decode()
        .map_err(|e| ArtworkError::Decode(e.to_string()))?;
    let (w, h) = img.dimensions();
    if w < 64 || h < 64 {
        return Err(ArtworkError::TooSmall(w, h));
    }
    let (img, not_square) = square_if_close(img);
    let (w, h) = img.dimensions();
    // Jamais agrandie : une petite image reste petite (signalée « trop petite » en phase 3).
    let img = if w.max(h) > target { img.resize(target, target, FilterType::Lanczos3) } else { img };
    let rgb = DynamicImage::ImageRgb8(img.to_rgb8());
    let (w, h) = rgb.dimensions();
    // Qualité 85, abaissée par paliers si le fichier dépasse 1 Mo.
    for quality in [85u8, 80, 75, 70, 60, 50] {
        let mut out = Vec::new();
        let enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality);
        rgb.write_with_encoder(enc).map_err(|e| ArtworkError::Encode(e.to_string()))?;
        if out.len() <= MAX_BYTES || quality == 50 {
            return Ok(Prepared {
                cover: CoverData { mime: "image/jpeg".into(), data: out },
                width: w,
                height: h,
                not_square,
            });
        }
    }
    unreachable!()
}

/// Vignette (petite image) pour l'affichage, à partir des octets d'une pochette.
pub fn thumbnail(bytes: &[u8], size: u32) -> Option<Vec<u8>> {
    let img = ImageReader::new(Cursor::new(bytes)).with_guessed_format().ok()?.decode().ok()?;
    let img = img.resize(size, size, FilterType::Triangle);
    let mut out = Vec::new();
    let enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 80);
    DynamicImage::ImageRgb8(img.to_rgb8()).write_with_encoder(enc).ok()?;
    Some(out)
}

#[cfg(test)]
pub fn test_png(w: u32, h: u32) -> Vec<u8> {
    let img = image::RgbImage::from_fn(w, h, |x, y| image::Rgb([(x % 256) as u8, (y % 256) as u8, 128]));
    let mut out = Vec::new();
    DynamicImage::ImageRgb8(img).write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png).unwrap();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_presque_carree_recadree_et_reduite() {
        let p = prepare(&test_png(1500, 1420), 1000).unwrap();
        assert_eq!((p.width, p.height), (1000, 1000));
        assert!(!p.not_square);
        assert!(p.cover.data.len() <= MAX_BYTES);
        assert_eq!(&p.cover.data[0..2], &[0xFF, 0xD8]); // JPEG
    }

    #[test]
    fn image_rectangulaire_non_recadree() {
        let p = prepare(&test_png(1600, 900), 1000).unwrap();
        assert!(p.not_square);
        assert_eq!(p.width, 1000);
    }

    #[test]
    fn petite_image_non_agrandie() {
        let p = prepare(&test_png(300, 300), 1000).unwrap();
        assert_eq!(p.width, 300);
    }

    #[test]
    fn donnees_invalides_refusees() {
        assert!(prepare(b"pas une image", 1000).is_err());
        assert!(matches!(prepare(&test_png(20, 20), 1000), Err(ArtworkError::TooSmall(..))));
    }
}
