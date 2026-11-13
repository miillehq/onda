//! The local profile: a display name and a picture that never leave this computer unless the
//! user exports a statistics image and chooses to include them.
use crate::error::{AppError, AppResult};
use image::{codecs::png::PngEncoder, ImageEncoder, ImageFormat, ImageReader, Limits};
use std::io::Cursor;
use unicode_segmentation::UnicodeSegmentation;

pub const MAX_NAME_CHARACTERS: usize = 40;
/// Larger files are refused before decoding.
pub const MAX_PICTURE_INPUT_BYTES: usize = 10 * 1024 * 1024;
/// Stored and shown as a square of this many pixels.
pub const PICTURE_SIZE: u32 = 256;
/// An RGBA PNG of `PICTURE_SIZE` is far below this; anything larger is not ours.
pub const MAX_STORED_PICTURE_BYTES: usize = 512 * 1024;
const MAX_PICTURE_DIMENSION: u32 = 12_000;
const MAX_DECODE_ALLOCATION: u64 = 512 * 1024 * 1024;

/// Trims and collapses whitespace; an empty name clears it.
pub fn normalize_name(raw: &str) -> AppResult<Option<String>> {
    let name = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if name.is_empty() {
        return Ok(None);
    }
    if name.chars().any(char::is_control) {
        return Err(AppError::invalid("Names can't contain control characters."));
    }
    if name.graphemes(true).count() > MAX_NAME_CHARACTERS {
        return Err(AppError::invalid(&format!(
            "Use a name of at most {MAX_NAME_CHARACTERS} characters."
        )));
    }
    Ok(Some(name))
}

/// Decodes a PNG, JPEG or WebP (first frame), applies its camera orientation, crops the centre
/// square and re-encodes it as a `PICTURE_SIZE` PNG. Re-encoding the pixels drops EXIF, GPS and
/// every other piece of the original file's metadata.
pub fn normalize_picture(bytes: &[u8]) -> AppResult<Vec<u8>> {
    if bytes.is_empty() {
        return Err(AppError::invalid("The picture file is empty."));
    }
    if bytes.len() > MAX_PICTURE_INPUT_BYTES {
        return Err(AppError::invalid("Choose a picture smaller than 10 MB."));
    }
    let unreadable =
        || AppError::invalid("Onda can't read this picture. Choose a PNG, JPEG or WebP image.");
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| unreadable())?;
    if !matches!(
        reader.format(),
        Some(ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::WebP)
    ) {
        return Err(unreadable());
    }
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_PICTURE_DIMENSION);
    limits.max_image_height = Some(MAX_PICTURE_DIMENSION);
    limits.max_alloc = Some(MAX_DECODE_ALLOCATION);
    reader.limits(limits);
    let mut decoder = reader.into_decoder().map_err(|_| unreadable())?;
    let orientation = image::ImageDecoder::orientation(&mut decoder).map_err(|_| unreadable())?;
    let mut picture = image::DynamicImage::from_decoder(decoder).map_err(|_| unreadable())?;
    picture.apply_orientation(orientation);
    let side = picture.width().min(picture.height());
    if side == 0 {
        return Err(unreadable());
    }
    let square = picture.crop_imm(
        (picture.width() - side) / 2,
        (picture.height() - side) / 2,
        side,
        side,
    );
    let resized = square
        .resize_exact(
            PICTURE_SIZE,
            PICTURE_SIZE,
            image::imageops::FilterType::Lanczos3,
        )
        .into_rgba8();
    let mut encoded = Vec::new();
    PngEncoder::new(&mut encoded)
        .write_image(
            resized.as_raw(),
            PICTURE_SIZE,
            PICTURE_SIZE,
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|_| AppError::invalid("The picture could not be saved."))?;
    if encoded.len() > MAX_STORED_PICTURE_BYTES {
        return Err(AppError::invalid("The picture could not be saved."));
    }
    Ok(encoded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgba};

    fn encoded(format: ImageFormat, width: u32, height: u32) -> Vec<u8> {
        let picture = ImageBuffer::from_fn(width, height, |x, _| {
            Rgba([if x < width / 2 { 255 } else { 0 }, 0, 0, 255])
        });
        let mut bytes = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(picture)
            .to_rgb8()
            .write_to(&mut bytes, format)
            .unwrap();
        bytes.into_inner()
    }

    #[test]
    fn names_are_trimmed_bounded_and_clearable() {
        assert_eq!(
            normalize_name("  Daniel \t Y. ").unwrap().as_deref(),
            Some("Daniel Y.")
        );
        assert_eq!(normalize_name("   ").unwrap(), None);
        assert!(normalize_name(&"a".repeat(41)).is_err());
        // Graphemes, not bytes: 40 flags fit.
        assert!(normalize_name(&"🇳🇿".repeat(40)).unwrap().is_some());
        assert!(normalize_name("a\u{0007}b").is_err());
    }

    #[test]
    fn pictures_are_cropped_resized_and_reencoded_as_png() {
        for format in [ImageFormat::Png, ImageFormat::Jpeg, ImageFormat::WebP] {
            let output = normalize_picture(&encoded(format, 640, 480)).unwrap();
            let decoded = image::load_from_memory_with_format(&output, ImageFormat::Png).unwrap();
            assert_eq!(
                (decoded.width(), decoded.height()),
                (PICTURE_SIZE, PICTURE_SIZE)
            );
            assert!(output.len() <= MAX_STORED_PICTURE_BYTES);
        }

        // The centre square is kept. Landscape and portrait pictures carry a different colour
        // in each outer margin (red before, black after) and a green/blue split in the centre
        // square, so a crop anchored to either edge brings a margin colour into the result.
        let (red, green, blue, black) = ([255, 0, 0], [0, 255, 0], [0, 0, 255], [0, 0, 0]);
        let banded = |format: ImageFormat, width: u32, height: u32| {
            let (long, short) = (width.max(height), width.min(height));
            let margin = (long - short) / 2;
            let picture = ImageBuffer::from_fn(width, height, |x, y| {
                let along = if width >= height { x } else { y };
                image::Rgb(if along < margin {
                    red
                } else if along >= long - margin {
                    black
                } else if along < long / 2 {
                    green
                } else {
                    blue
                })
            });
            let mut bytes = Cursor::new(Vec::new());
            image::DynamicImage::ImageRgb8(picture)
                .write_to(&mut bytes, format)
                .unwrap();
            bytes.into_inner()
        };
        let near = |actual: &[u8], expected: [u8; 3]| {
            actual
                .iter()
                .zip(expected)
                .all(|(actual, expected)| actual.abs_diff(expected) <= 48)
        };
        let (early, late) = (PICTURE_SIZE / 8, PICTURE_SIZE - PICTURE_SIZE / 8);
        for format in [ImageFormat::Png, ImageFormat::Jpeg, ImageFormat::WebP] {
            for (width, height) in [(640, 480), (480, 640)] {
                let output = normalize_picture(&banded(format, width, height)).unwrap();
                let decoded = image::load_from_memory_with_format(&output, ImageFormat::Png)
                    .unwrap()
                    .into_rgb8();
                let sample = |along: u32| {
                    let middle = PICTURE_SIZE / 2;
                    let (x, y) = if width >= height {
                        (along, middle)
                    } else {
                        (middle, along)
                    };
                    decoded.get_pixel(x, y).0
                };
                for (along, expected) in [(early, green), (late, blue)] {
                    let actual = sample(along);
                    assert!(
                        near(&actual, expected),
                        "{format:?} {width}x{height}: pixel {along} along the long side is {actual:?}, expected {expected:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn metadata_does_not_survive_reencoding() {
        // A JPEG with an APP1 Exif segment carrying a recognisable marker.
        let jpeg = encoded(ImageFormat::Jpeg, 64, 64);
        let marker = b"GPSLatitude-onda-test";
        let mut exif = b"Exif\0\0".to_vec();
        exif.extend_from_slice(marker);
        let mut with_exif = jpeg[..2].to_vec();
        with_exif.extend_from_slice(&[0xFF, 0xE1]);
        with_exif.extend_from_slice(&u16::try_from(exif.len() + 2).unwrap().to_be_bytes());
        with_exif.extend_from_slice(&exif);
        with_exif.extend_from_slice(&jpeg[2..]);
        assert!(with_exif
            .windows(marker.len())
            .any(|window| window == marker));
        let output = normalize_picture(&with_exif).unwrap();
        assert!(!output.windows(marker.len()).any(|window| window == marker));
    }

    #[test]
    fn unsupported_empty_oversized_and_corrupt_files_are_refused() {
        assert!(normalize_picture(&[]).is_err());
        assert!(normalize_picture(b"GIF89a\x01\x00\x01\x00\x00\x00\x00;").is_err());
        assert!(normalize_picture(&vec![0u8; MAX_PICTURE_INPUT_BYTES + 1]).is_err());
        let mut truncated = encoded(ImageFormat::Png, 32, 32);
        truncated.truncate(40);
        assert!(normalize_picture(&truncated).is_err());
        assert!(normalize_picture(b"<svg xmlns='http://www.w3.org/2000/svg'/>").is_err());

        // Each refusal names its reason. A real, readable PNG padded past the limit is refused
        // for its size before decoding, and an empty file is not reported as unreadable.
        assert_eq!(
            normalize_picture(&[]).unwrap_err().message,
            "The picture file is empty."
        );
        let mut padded = encoded(ImageFormat::Png, 32, 32);
        assert!(normalize_picture(&padded).is_ok());
        padded.resize(MAX_PICTURE_INPUT_BYTES + 1, 0);
        assert_eq!(
            normalize_picture(&padded).err().map(|error| error.message),
            Some("Choose a picture smaller than 10 MB.".to_owned()),
            "a picture file over 10 MB must be refused before decoding"
        );
        padded.truncate(MAX_PICTURE_INPUT_BYTES);
        assert!(
            normalize_picture(&padded).is_ok(),
            "exactly 10 MB is allowed"
        );
    }

    #[test]
    fn oversized_dimensions_are_refused_before_decoding_pixels() {
        // Cheap to encode, but wider than any picture Onda will decode.
        let wide = encoded(ImageFormat::Png, MAX_PICTURE_DIMENSION + 1, 1);
        assert!(normalize_picture(&wide).is_err());
        assert!(normalize_picture(&encoded(ImageFormat::Png, MAX_PICTURE_DIMENSION, 1)).is_ok());
    }
}
