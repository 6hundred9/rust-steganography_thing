use std::fs::File;
use std::path::{Path};
use image::{ImageFormat, ImageReader};
use png::{Encoder, ColorType, BitDepth};

pub fn hide(path: &Path, msg: &str, out_path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Err(format!("Path {} doesn't exist!", path.display()));
    }

    let ext = path.extension()
        .and_then(|e| e.to_str())
        .ok_or("Invalid file extension")?;

    // load and normalize to RGBA8 (so layout is predictable)
    let dyn_i = ImageReader::open(path).map_err(|e| e.to_string())?.decode().map_err(|e| e.to_string())?;
    let mut img = dyn_i.to_rgba8();
    let (w, h) = img.dimensions();
    let bytes_per_pixel = 4usize; // RGBA8

    // --- build bitstream: 32-bit BE length header + message bits (MSB-first per byte) ---
    let msg_len = msg.len() as u32;
    let mut bits: Vec<u8> = Vec::with_capacity(32 + msg.len() * 8);
    for i in (0..32).rev() {
        bits.push(((msg_len >> i) & 1) as u8);
    }
    for b in msg.bytes() {
        for i in (0..8).rev() {
            bits.push(((b >> i) & 1) as u8);
        }
    }
    // -------------------------------------------------------------------------------

    // capacity check (we use RGB channels only)
    let pixels = (w as usize) * (h as usize);
    let capacity_bits = pixels * 3; // R,G,B per pixel
    if bits.len() > capacity_bits {
        return Err(format!(
            "Message too big: need {} bits but capacity is {} bits",
            bits.len(),
            capacity_bits
        ));
    }

    // embed bits into LSBs of R,G,B, preserve alpha
    let buf = img.as_mut(); // &mut [u8] raw RGBA bytes
    let mut it = bits.iter();
    'outer: for chunk in buf.chunks_mut(bytes_per_pixel) {
        for c in 0..3 { // R,G,B
            if let Some(&bit) = it.next() {
                // chunk[c] and bit are u8; ensure only use lowest bit
                chunk[c] = (chunk[c] & !1) | (bit & 1);
            } else {
                break 'outer;
            }
        }
    }
    img.save_with_format(out_path, ImageFormat::from_extension(ext).unwrap()).map_err(|e| e.to_string())
}

pub fn find(path: &Path) -> Result<String, String> {
    if !path.exists() {
        return Err(format!("Path {} doesn't exist!", path.display()));
    }

    // open + normalize to RGBA8 so buffer layout is predictable
    let dyn_i = ImageReader::open(path).map_err(|e| e.to_string())?.decode().map_err(|e| e.to_string())?;
    let img = dyn_i.to_rgba8();
    let (w, h) = img.dimensions();
    let bytes_per_pixel = 4usize; // RGBA8

    let buf = img.into_raw(); // Vec<u8> with layout [R,G,B,A, R,G,B,A, ...]
    let pixels = (w as usize) * (h as usize);

    // collect LSBs (RGB order) into bits vec
    let mut bits: Vec<u8> = Vec::with_capacity(pixels * 3);
    for chunk in buf.chunks(bytes_per_pixel) {
        // chunk length is 4 because we normalized to RGBA8
        bits.push(chunk[0] & 1);
        bits.push(chunk[1] & 1);
        bits.push(chunk[2] & 1);
    }

    if bits.len() < 32 {
        return Err("Image too small to contain header".to_string());
    }

    // read 32-bit big-endian length header
    let mut len: u32 = 0;
    for i in 0..32 {
        len = (len << 1) | (bits[i] as u32);
    }

    let needed_bits = (len as usize) * 8;
    if bits.len() < 32 + needed_bits {
        return Err(format!(
            "Image does not contain full message: header says {} bytes but capacity is {} bits",
            len,
            bits.len() - 32
        ));
    }

    // reconstruct message bytes (MSB-first per byte)
    let mut bytes: Vec<u8> = Vec::with_capacity(len as usize);
    let start = 32;
    for byte_idx in 0..(len as usize) {
        let base = start + byte_idx * 8;
        let mut b: u8 = 0;
        for j in 0..8 {
            b = (b << 1) | (bits[base + j] & 1);
        }
        bytes.push(b);
    }

    String::from_utf8(bytes).map_err(|_| "<invalid utf8>".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{File};
    use std::path::Path;
    use image::codecs::png;
    use tempfile::tempdir;

    // create a test PNG at `path` with given width/height, RGB
    fn create_test_png(path: &Path, width: usize, height: usize) {
        let mut buf = Vec::with_capacity(width * height * 3);
        for i in 0..(width * height) {
            buf.push(((i * 3) % 256) as u8);       // R
            buf.push(((i * 3 + 1) % 256) as u8);   // G
            buf.push(((i * 3 + 2) % 256) as u8);   // B
        }

        let file = File::create(path).unwrap();
        let mut encoder = Encoder::new(file, width as u32, height as u32);
        encoder.set_color(ColorType::Rgb);
        encoder.set_depth(BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&buf).unwrap();
    }

    #[test]
    fn test_hide_and_find_basic() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test_basic.png");

        let width = 1960;
        let height = 1034;
        create_test_png(&path, width, height);

        // Capacity in bytes = (pixels * 3 channels) / 8
        let capacity_bytes = (width * height * 3) / 8;
        assert!(capacity_bytes > 0);

        let message = "fart hill";
        assert!(message.len() <= capacity_bytes, "Test message must fit in image");

        hide(&path, message, &path).expect("Failed to hide message");

        let decoded = find(&path).expect("Failed to decode message");

        // compare as bytes to avoid weird utf8/trailing-null issues
        let decoded_bytes = decoded.as_bytes();
        assert!(
            decoded_bytes.len() >= message.len(),
            "decoded shorter than original"
        );
        assert_eq!(&decoded_bytes[..message.len()], message.as_bytes());
    }

    #[test]
    fn test_message_too_big() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test_big.png");

        let width = 4096;
        let height = 4096;
        create_test_png(&path, width, height);

        let capacity_bytes = (width * height * 3) / 8;
        // make a message one byte bigger than capacity
        let too_big = "A".repeat(capacity_bytes + 1);

        let res = hide(&path, &too_big, &dir.path().join(Path::new("out.png")));
        assert!(res.is_err(), "Should fail because message is too big");
    }

    #[test]
    fn test_empty_message() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test_empty.png");

        let width = 4096;
        let height = 4096;
        create_test_png(&path, width, height);

        let message = "";
        hide(&path, message, &path).expect("Failed to hide empty message");

        let decoded = find(&path).expect("Failed to decode empty message");
        // just ensure decoding didn't return the invalid-utf8 sentinel
        assert_ne!(decoded, "<invalid utf8>");
    }

    #[test]
    fn test_nonexistent_file() {
        let bogus = Path::new("this_file_definitely_doesnt_exist_12345.png");
        let result = hide(bogus, "hi", Path::new("bleh"));
        assert!(result.is_err());

        let result2 = find(bogus);
        assert!(result2.is_err());
    }
}


// 4096x4096 with only test_hide_and_find_basic() took 750 ms with test --release, that's 100M fucking operations (hiding+finding)
// I still feel like it could be improved
// Update: It increased to 1.4s </3
// Update: It decreased to 0.8-0.5s