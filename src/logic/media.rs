//! Images as Base64 / data URIs, and QR codes.

use base64::Engine as _;

pub struct ImageInfo {
    pub mime: &'static str,
    pub ext: &'static str,
    pub bytes: Vec<u8>,
    pub size: Option<(u32, u32)>,
}

/// The image type, going by the first bytes.
pub fn sniff(b: &[u8]) -> Option<(&'static str, &'static str)> {
    Some(match b {
        [0x89, b'P', b'N', b'G', ..] => ("image/png", "png"),
        [0xFF, 0xD8, 0xFF, ..] => ("image/jpeg", "jpg"),
        [b'G', b'I', b'F', b'8', ..] => ("image/gif", "gif"),
        [b'R', b'I', b'F', b'F', _, _, _, _, b'W', b'E', b'B', b'P', ..] => ("image/webp", "webp"),
        [b'B', b'M', ..] => ("image/bmp", "bmp"),
        [0, 0, 1, 0, ..] => ("image/x-icon", "ico"),
        [b'I', b'I', 0x2A, 0, ..] | [b'M', b'M', 0, 0x2A, ..] => ("image/tiff", "tiff"),
        _ => {
            let head = String::from_utf8_lossy(&b[..b.len().min(512)]).to_lowercase();
            if head.trim_start().starts_with("<svg") || (head.contains("<?xml") && head.contains("<svg")) {
                ("image/svg+xml", "svg")
            } else {
                return None;
            }
        }
    })
}

fn be16(b: &[u8], i: usize) -> Option<u32> {
    Some(u16::from_be_bytes([*b.get(i)?, *b.get(i + 1)?]) as u32)
}

fn le16(b: &[u8], i: usize) -> Option<u32> {
    Some(u16::from_le_bytes([*b.get(i)?, *b.get(i + 1)?]) as u32)
}

fn le32(b: &[u8], i: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(i..i + 4)?.try_into().ok()?))
}

/// Pixel dimensions from the header, for the common formats.
pub fn dimensions(b: &[u8], ext: &str) -> Option<(u32, u32)> {
    match ext {
        "png" => Some((u32::from_be_bytes(b.get(16..20)?.try_into().ok()?), u32::from_be_bytes(b.get(20..24)?.try_into().ok()?))),
        "gif" => Some((le16(b, 6)?, le16(b, 8)?)),
        "bmp" => Some((le32(b, 18)?, (le32(b, 22)? as i32).unsigned_abs())),
        "jpg" => {
            let mut i = 2;
            while i + 9 < b.len() {
                if b[i] != 0xFF {
                    return None;
                }
                let marker = b[i + 1];
                let len = be16(b, i + 2)? as usize;
                // SOF0..SOF15, except DHT (C4), JPG (C8) and DAC (CC).
                if (0xC0..=0xCF).contains(&marker) && ![0xC4, 0xC8, 0xCC].contains(&marker) {
                    return Some((be16(b, i + 7)?, be16(b, i + 5)?));
                }
                i += 2 + len;
            }
            None
        }
        "webp" => match b.get(12..16)? {
            b"VP8X" => {
                let w = u32::from_le_bytes([b[24], b[25], b[26], 0]) + 1;
                let h = u32::from_le_bytes([b[27], b[28], b[29], 0]) + 1;
                Some((w, h))
            }
            b"VP8 " => Some((le16(b, 26)? & 0x3FFF, le16(b, 28)? & 0x3FFF)),
            b"VP8L" => {
                let bits = le32(b, 21)?;
                Some(((bits & 0x3FFF) + 1, ((bits >> 14) & 0x3FFF) + 1))
            }
            _ => None,
        },
        _ => None,
    }
}

/// A data URI or bare Base64 → image bytes and what they are.
pub fn decode_image(input: &str) -> Result<ImageInfo, String> {
    let t = input.trim();
    let b64 = match t.strip_prefix("data:") {
        Some(rest) => rest.split_once(',').map(|(_, d)| d).ok_or("The data URI has no comma before its data")?,
        None => t,
    };
    let clean: String = b64.chars().filter(|c| !c.is_whitespace()).collect();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&clean)
        .or_else(|_| base64::engine::general_purpose::STANDARD_NO_PAD.decode(clean.trim_end_matches('=')))
        .or_else(|_| base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(clean.trim_end_matches('=')))
        .map_err(|_| "This is not valid Base64".to_string())?;
    let (mime, ext) = sniff(&bytes).ok_or("The decoded bytes are not a PNG, JPEG, GIF, WebP, BMP, ICO, TIFF or SVG image")?;
    let size = dimensions(&bytes, ext);
    Ok(ImageInfo { mime, ext, bytes, size })
}

pub fn data_uri(bytes: &[u8]) -> Result<String, String> {
    let (mime, _) = sniff(bytes).ok_or("That file is not a recognized image")?;
    Ok(format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)))
}

pub const QR_LEVELS: &[&str] = &["Low (7%)", "Medium (15%)", "Quartile (25%)", "High (30%)"];

/// An SVG QR code (black on white, with the quiet zone) and its size in modules.
pub fn qr_svg(text: &str, level: usize) -> Result<(String, usize), String> {
    use qrcode::render::svg;
    use qrcode::{EcLevel, QrCode};
    let ec = [EcLevel::L, EcLevel::M, EcLevel::Q, EcLevel::H][level.min(3)];
    let code = QrCode::with_error_correction_level(text.as_bytes(), ec)
        .map_err(|_| "Too much text for a QR code at this error-correction level".to_string())?;
    let svg = code
        .render::<svg::Color>()
        .min_dimensions(320, 320)
        .dark_color(svg::Color("#000000"))
        .light_color(svg::Color("#ffffff"))
        .build();
    Ok((svg, code.width()))
}

#[cfg(test)]
mod tests {
    use super::*;

    // A 3×2 PNG.
    const PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAAAMAAAACCAYAAACddGYaAAAAEUlEQVR4nGP4z8DwH4YZkDkAm34L9XKwuTwAAAAASUVORK5CYII=";

    #[test]
    fn images() {
        let i = decode_image(&format!("data:image/png;base64,{PNG}")).unwrap();
        assert_eq!((i.mime, i.size), ("image/png", Some((3, 2))));
        assert_eq!(decode_image(PNG).unwrap().ext, "png");
        assert!(data_uri(&i.bytes).unwrap().starts_with("data:image/png;base64,iVBOR"));
        assert!(decode_image("aGVsbG8=").err().unwrap().contains("not a PNG"));
        assert!(decode_image("%%%").is_err());
        assert_eq!(sniff(b"<svg xmlns='x'/>").unwrap().1, "svg");
        let gif = [b'G', b'I', b'F', b'8', b'9', b'a', 10, 0, 20, 0];
        assert_eq!(dimensions(&gif, "gif"), Some((10, 20)));
    }

    #[test]
    fn qr() {
        let (svg, w) = qr_svg("https://example.com", 1).unwrap();
        assert!(svg.starts_with("<?xml") && svg.contains("<svg"));
        assert_eq!(w, 25);
        assert!(qr_svg(&"x".repeat(4000), 3).is_err());
    }
}
