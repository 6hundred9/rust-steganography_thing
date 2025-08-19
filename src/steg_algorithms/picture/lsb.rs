use std::fs::File;
use std::path::{Path, PathBuf};
use png::ColorType;

pub fn hide(path: &Path, msg: &str, outPath: &Path) -> Result<(), String> {
    if !path.exists() {
        return Err(format!("Path {} doesn't exist!", path.display()));
    }
    let ext = path.extension().and_then(|e| e.to_str()).ok_or("Invalid file extension".to_string())?;
    if ext.to_lowercase() != "png" {
        return Err("Only PNG supported".to_string());
    }

    // open file & create decoder
    let file = File::open(path).map_err(|e| e.to_string())?;
    let decoder = png::Decoder::new(file);
    let mut reader = decoder.read_info().map_err(|e| e.to_string())?;

    // --- COPY needed info out (no long-lived & reference) ---
    let info = reader.info();
    let width = info.width;
    let height = info.height;
    let color_type = info.color_type;
    let bit_depth = info.bit_depth;
    let bytes_per_pixel = color_type.samples() as usize;
    // -------------------------------------------------------

    // allocate buffer and read the frame (mutable borrow is safe now)
    let mut buf = vec![0; reader.output_buffer_size()];
    reader.next_frame(&mut buf).map_err(|e| e.to_string())?;

    // build bits: 32-bit big-endian length header + message bits (MSB-first)
    let msg_len = msg.len() as u32;
    let mut bits: Vec<u8> = Vec::with_capacity(32 + msg.len() * 8);
    for i in (0..32).rev() { bits.push(((msg_len >> i) & 1) as u8); }
    for b in msg.bytes() {
        for i in (0..8).rev() { bits.push(((b >> i) & 1) as u8); }
    }

    // capacity check (use only RGB channels)
    let pixels = buf.len() / bytes_per_pixel;
    let capacity_bits = pixels * 3;
    if bits.len() > capacity_bits {
        return Err(format!("Message too big: need {} bits but capacity is {} bits", bits.len(), capacity_bits));
    }

    // embed bits into LSBs (ignore alpha if present)
    let mut it = bits.iter();
    'outer: for chunk in buf.chunks_mut(bytes_per_pixel) {
        for c in 0..3 {
            if let Some(&bit) = it.next() {
                chunk[c] = (chunk[c] & 0b1111_1110) | (bit & 1);
            } else {
                break 'outer;
            }
        }
    }

    // write back preserving color_type & bit_depth
    let file_out = File::create(outPath).map_err(|e| e.to_string())?;
    let mut encoder = png::Encoder::new(file_out, width, height);
    encoder.set_color(color_type);
    encoder.set_depth(bit_depth);
    let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
    writer.write_image_data(&buf).map_err(|e| e.to_string())?;

    Ok(())
}

pub fn find(path: &Path) -> Result<String, String> {
    if !path.exists() {
        return Err(format!("Path {} doesn't exist!", path.display()));
    }
    let ext = path.extension().and_then(|e| e.to_str()).ok_or("Invalid file extension".to_string())?;
    if ext.to_lowercase() != "png" {
        return Err("Only PNG supported".to_string());
    }

    let file = File::open(path).map_err(|e| e.to_string())?;
    let decoder = png::Decoder::new(file);
    let mut reader = decoder.read_info().map_err(|e| e.to_string())?;

    // copy info fields before mutating reader
    let info = reader.info();
    let bytes_per_pixel = info.color_type.samples() as usize;
    if !(info.color_type == ColorType::Rgb || info.color_type == ColorType::Rgba) {
        return Err(format!("Unsupported PNG color type: {:?}. Convert to RGB/RGBA.", info.color_type));
    }

    let mut buf = vec![0; reader.output_buffer_size()];
    reader.next_frame(&mut buf).map_err(|e| e.to_string())?;

    // collect LSBs (RGB order)
    let mut bits: Vec<u8> = Vec::with_capacity((buf.len() / bytes_per_pixel) * 3);
    for chunk in buf.chunks(bytes_per_pixel) {
        bits.push(chunk[0] & 1);
        bits.push(chunk[1] & 1);
        bits.push(chunk[2] & 1);
    }

    if bits.len() < 32 {
        return Err("Image too small to contain header".to_string());
    }

    // read 32-bit big-endian length
    let mut len: u32 = 0;
    for i in 0..32 {
        len = (len << 1) | (bits[i] as u32);
    }

    let needed_bits = (len as usize) * 8;
    if bits.len() < 32 + needed_bits {
        return Err(format!("Image does not contain full message: header says {} bytes but capacity is {} bits", len, bits.len() - 32));
    }

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
    use std::fs::{DirBuilder, File};
    use std::path::Path;
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
        let mut encoder = png::Encoder::new(file, width as u32, height as u32);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
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

        hide(&path, message).expect("Failed to hide message");

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

        let res = hide(&path, &too_big);
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
        hide(&path, message).expect("Failed to hide empty message");

        let decoded = find(&path).expect("Failed to decode empty message");
        // just ensure decoding didn't return the invalid-utf8 sentinel
        assert_ne!(decoded, "<invalid utf8>");
    }

    #[test]
    fn test_nonexistent_file() {
        let bogus = Path::new("this_file_definitely_doesnt_exist_12345.png");
        let result = hide(bogus, "hi");
        assert!(result.is_err());

        let result2 = find(bogus);
        assert!(result2.is_err());
    }
}


// 4096x4096 with only test_hide_and_find_basic() took 750 ms with test --release, that's 100M fucking operations (hiding+finding)
// I still feel like it could be improved
// Update: It increased to 1.4s </3
// Update: It decreased to 0.8-0.5s