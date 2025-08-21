use std::fs;
use std::io::{self, Write};
use std::path::Path;

const SOI: [u8; 2] = [0xFF, 0xD8];
const SOS_MARKER: u8 = 0xDA;
const MAX_SEGMENT_TOTAL_LEN: usize = 65_535;
const MAX_SEGMENT_PAYLOAD: usize = 65_533;

fn make_app_segment(app_marker: u8, payload: &[u8]) -> Vec<u8> {
    let mut seg = Vec::with_capacity(4 + payload.len());
    seg.push(0xFF);
    seg.push(app_marker);
    let len = (payload.len() + 2) as u16; // length includes the two length bytes
    seg.extend(&len.to_be_bytes());
    seg.extend_from_slice(payload);
    seg
}



fn find_sos_index(buf: &[u8]) -> Option<usize> {
    let mut i = 2usize; // skip initial SOI (0..1)
    while i + 1 < buf.len() {
        if buf[i] != 0xFF {
            // JPEG markers should be 0xFF followed by marker byte.
            // sometimes padding 0xFF bytes exist; skip them.
            i += 1;
            continue;
        }
        let marker = buf[i + 1];
        if marker == SOS_MARKER {
            return Some(i);
        }

        // markers without length (RSTn, SOI, EOI) can be skipped, but here we assume we're inside header
        // for APPn/COM we have a 2 byte length after marker
        if marker == 0x00 || (marker >= 0xD0 && marker <= 0xD7) {
            // stuffed byte or RSTn, move on
            i += 2;
            continue;
        }
        if i + 3 >= buf.len() {
            return None;
        }
        let len = u16::from_be_bytes([buf[i + 2], buf[i + 3]]) as usize;
        // length includes the two length bytes, so the payload length is len - 2
        i += 2 + len;
    }
    None
}

fn collect_app_segments(buf: &[u8]) -> Vec<(u8, usize, usize)> {
    let mut res = Vec::new();
    let mut i = 2usize; // skip SOI
    while i + 1 < buf.len() {
        if buf[i] != 0xFF {
            i += 1;
            continue;
        }
        let marker = buf[i + 1];
        if marker == SOS_MARKER {
            break;
        }
        if marker == 0x00 || (marker >= 0xD0 && marker <= 0xD7) {
            i += 2;
            continue;
        }
        if i + 3 >= buf.len() { break; }
        let len = u16::from_be_bytes([buf[i + 2], buf[i + 3]]) as usize;
        let seg_start = i;
        let seg_end = i + 2 + len; // exclusive
        if seg_end > buf.len() { break; }
        // keep only APPn (0xE0..0xEF) and COM (0xFE) if you want; here we return all segments
        res.push((marker, seg_start, seg_end));
        i = seg_end;
    }
    res
}

fn chunk_payload_with_identifier(payload: &[u8], identifier: &[u8]) -> Vec<Vec<u8>> {
    let header_len = identifier.len() + 4; // seq(u16) + total(u16)
    let max_body = MAX_SEGMENT_PAYLOAD.saturating_sub(header_len);
    assert!(max_body > 0, "identifier too large for APPn segment");
    let mut chunks = Vec::new();
    let total = ((payload.len() + max_body - 1) / max_body) as u16;
    for (i, chunk) in payload.chunks(max_body).enumerate() {
        let mut v = Vec::with_capacity(header_len + chunk.len());
        v.extend_from_slice(identifier);
        v.extend_from_slice(&(i as u16).to_be_bytes());
        v.extend_from_slice(&total.to_be_bytes());
        v.extend_from_slice(chunk);
        chunks.push(v);
    }
    chunks
}

pub fn insert_or_replace_appn(
    original: &[u8],
    app_marker: u8,
    identifier: Option<&[u8]>,
    payload: &[u8],
) -> io::Result<Vec<u8>> {
    // find SOS index
    let sos_idx = find_sos_index(original).ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidData, "no SOS marker found in JPEG")
    })?;

    // collect segments before SOS
    let segments = collect_app_segments(original);

    // build a new header area: keep segments that do NOT match identifier
    let mut new_buf = Vec::new();
    // push SOI
    new_buf.extend_from_slice(&original[0..2]);

    // iterate through existing segments before SOS, keep those not matching the identifier
    for (marker, start, end) in segments.iter() {
        // only operate on APPn or COM if desired; here we check payload start for identifier
        let payload_start = start + 4; // 0xFF, marker, len_hi, len_lo -> payload
        if payload_start > *end { continue; }
        let payload_slice = &original[payload_start..*end];
        let should_remove = if let Some(id) = identifier {
            payload_slice.starts_with(id)
        } else {
            false
        };
        if !should_remove {
            new_buf.extend_from_slice(&original[*start..*end]);
        } else {
            // skip removing segment (effectively replaced)
        }
    }

    // build new chunks from payload and insert them as new APPn segments
    let id = identifier.unwrap_or(&[]);
    let chunks = chunk_payload_with_identifier(payload, id);
    for chunk_payload in chunks {
        let seg = make_app_segment(app_marker, &chunk_payload);
        new_buf.extend_from_slice(&seg);
    }

    // append the rest of original jpeg starting at sos_idx
    new_buf.extend_from_slice(&original[sos_idx..]);

    Ok(new_buf)
}

/// Hide payload (bytes) into `input_jpeg_path` and write result to `output_jpeg_path`.
/// `app_marker` is the second byte of the APP marker (e.g. 0xEB for APP11).
/// `identifier` must match the one used by `chunk_payload_with_identifier`.
pub fn hide_payload_file(
    input_jpeg_path: &str,
    output_jpeg_path: &str,
    app_marker: u8,
    identifier: &[u8],
    payload: &[u8],
) -> io::Result<()> {
    let original = fs::read(input_jpeg_path)?;
    let new_jpeg = insert_or_replace_appn(&original, app_marker, Some(identifier), payload)?;
    fs::write(output_jpeg_path, new_jpeg)?;
    Ok(())
}

/// Extract payload bytes from a JPEG buffer. Returns Ok(Some(payload)) if found,
/// Ok(None) if no matching identifier segments exist, Err on malformed/incomplete sets.
pub fn extract_payload_from_bytes(original: &[u8], identifier: &[u8]) -> io::Result<Option<Vec<u8>>> {
    // gather segments before SOS
    let segments = collect_app_segments(original);

    // collect all matching chunks: (seq, total, chunk_bytes)
    let mut chunks: Vec<(u16, u16, Vec<u8>)> = Vec::new();
    for (_marker, start, end) in segments.iter() {
        let payload_start = start + 4;
        if payload_start > *end { continue; }
        let payload_slice = &original[payload_start..*end];
        if !payload_slice.starts_with(identifier) {
            continue;
        }
        // need at least identifier + 4 bytes for seq+total
        let hdr_len = identifier.len() + 4;
        if payload_slice.len() < hdr_len {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "found matching segment with too-small header"));
        }
        let seq_off = identifier.len();
        let seq = u16::from_be_bytes([payload_slice[seq_off], payload_slice[seq_off + 1]]);
        let total = u16::from_be_bytes([payload_slice[seq_off + 2], payload_slice[seq_off + 3]]);
        let chunk_data = payload_slice[hdr_len..].to_vec();
        chunks.push((seq, total, chunk_data));
    }

    if chunks.is_empty() {
        return Ok(None);
    }

    // determine expected total (take the max total reported)
    let mut expected_total: Option<usize> = None;
    for &(_seq, total, _) in &chunks {
        let t = total as usize;
        expected_total = Some(expected_total.map_or(t, |cur| cur.max(t)));
    }
    let expected_total = expected_total.unwrap();

    if expected_total == 0 {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "invalid total=0 in headers"));
    }

    // place chunks into a vector by seq
    let mut placed: Vec<Option<Vec<u8>>> = vec![None; expected_total];
    for (seq, total, data) in chunks {
        let seq_idx = seq as usize;
        if seq_idx >= expected_total {
            return Err(io::Error::new(io::ErrorKind::InvalidData, format!("chunk seq {} >= total {}", seq_idx, expected_total)));
        }
        // simple overwrite if duplicates — keep the first if you prefer, but overwrite is fine
        placed[seq_idx] = Some(data);
        // optional sanity: check total matches
        if total as usize != expected_total {
            // inconsistent totals observed is suspicious but we allow as long as we have a consistent expected_total
            // you could reject here if you want strictness
        }
    }

    // verify all chunks present
    for (i, slot) in placed.iter().enumerate() {
        if slot.is_none() {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, format!("missing chunk {}", i)));
        }
    }

    // concat all chunks in order
    let mut out = Vec::new();
    for slot in placed.into_iter() {
        if let Some(mut s) = slot {
            out.append(&mut s);
        }
    }

    Ok(Some(out))
}

/// Convenience: read a JPEG file, extract payload with `identifier`, and write payload to `out_path`.
/// Returns Ok(true) if found+written, Ok(false) if not found.
pub fn extract_payload_file(jpeg_path: &str, identifier: &[u8], out_path: &str) -> io::Result<bool> {
    let buf = fs::read(jpeg_path)?;
    match extract_payload_from_bytes(&buf, identifier)? {
        Some(payload) => {
            fs::write(out_path, &payload)?;
            Ok(true)
        }
        None => Ok(false),
    }
}

/// Hide `msg` string into JPEG at `path`, write stego JPEG to `out_path`.
/// Uses APP11 (0xEB) segments and identifier `b"Ducky\0"`.
pub fn hide(path: &Path, msg: &str, out_path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Err(format!("Path {} doesn't exist!", path.display()));
    }

    // read original jpeg bytes
    let original = fs::read(path).map_err(|e| e.to_string())?;

    // build payload: 4-byte BE length header + message bytes
    let msg_bytes = msg.as_bytes();
    if msg_bytes.len() > u32::MAX as usize {
        return Err("message too large".to_string());
    }
    let len_be = (msg_bytes.len() as u32).to_be_bytes();
    let mut payload: Vec<u8> = Vec::with_capacity(4 + msg_bytes.len());
    payload.extend_from_slice(&len_be);
    payload.extend_from_slice(msg_bytes);

    // insert/replace APPn segments (this uses your helper)
    // APP11 = 0xEB, identifier = b"Ducky\0"
    let app_marker: u8 = 0xEB;
    let identifier: &[u8] = b"Ducky\0";

    let new_jpeg = insert_or_replace_appn(&original, app_marker, Some(identifier), &payload)
        .map_err(|e| e.to_string())?;

    fs::write(out_path, &new_jpeg).map_err(|e| e.to_string())?;
    Ok(())
}

/// Find and extract hidden message from JPEG at `path`. Returns the recovered string.
/// Expects the same marker/identifier used by `hide`.
pub fn find(path: &Path) -> Result<String, String> {
    if !path.exists() {
        return Err(format!("Path {} doesn't exist!", path.display()));
    }

    let buf = fs::read(path).map_err(|e| e.to_string())?;
    let identifier: &[u8] = b"Ducky\0";

    // use helper to reassemble payload across chunks
    let opt_payload = extract_payload_from_bytes(&buf, identifier)
        .map_err(|e| e.to_string())?;

    let payload = match opt_payload {
        Some(p) => p,
        None => return Err("no matching segments found".to_string()),
    };

    // payload format: [4-byte BE length][msg bytes]
    if payload.len() < 4 {
        return Err("payload too small to contain length header".to_string());
    }
    let len = u32::from_be_bytes([payload[0], payload[1], payload[2], payload[3]]) as usize;
    if payload.len() < 4 + len {
        return Err(format!(
            "payload shorter than claimed length: header says {} bytes but have {}",
            len,
            payload.len() - 4
        ));
    }
    let msg_bytes = &payload[4..4 + len];
    String::from_utf8(msg_bytes.to_vec()).map_err(|_| "<invalid utf8>".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// Helper to build a minimal "jpeg-like" buffer:
    /// SOI, then zero or more APP segments, then SOS, some dummy scan bytes, and EOI.
    fn build_dummy_jpeg(app_segments: Vec<(u8, Vec<u8>)>) -> Vec<u8> {
        let mut v: Vec<u8> = Vec::new();
        // SOI
        v.extend_from_slice(&[0xFF, 0xD8]);
        // append requested APPn segments
        for (marker, payload) in app_segments.into_iter() {
            v.extend_from_slice(&make_app_segment(marker, &payload));
        }
        // SOS marker (start of scan) + minimal "scan" bytes
        v.extend_from_slice(&[0xFF, 0xDA, 0x00, 0x00, 0x11, 0x22, 0x33]);
        // EOI
        v.extend_from_slice(&[0xFF, 0xD9]);
        v
    }

    #[test]
    fn test_insert_and_extract_simple() {
        // original has one APP1 and one APP11 (Ducky) existing
        let orig = build_dummy_jpeg(vec![
            (0xE1, b"JFIF\0".to_vec()),
            (0xEB, b"Ducky\0\x00\x00oldchunk".to_vec()),
        ]);

        // payload to embed (raw bytes)
        let payload = b"hello-stego".to_vec();

        // insert/replace using APP11 (0xEB) and identifier Ducky\0
        let out = insert_or_replace_appn(&orig, 0xEB, Some(b"Ducky\0"), &payload)
            .expect("insert_or_replace_appn failed");

        // extraction should find our payload
        let recovered_opt = extract_payload_from_bytes(&out, b"Ducky\0")
            .expect("extract returned Err");
        assert!(recovered_opt.is_some(), "expected payload present");
        let recovered = recovered_opt.unwrap();
        assert_eq!(recovered, payload, "recovered payload must equal original");
    }
    
    #[test]
    fn test_replace_existing_segments() {
        // build original with two Ducky segments (simulate older payload)
        let orig = build_dummy_jpeg(vec![
            (0xE2, b"EXTRA".to_vec()),
            (0xEB, {
                // first chunk header: identifier + seq(0) + total(2) + data
                let mut v = Vec::new();
                v.extend_from_slice(b"Ducky\0");
                v.extend_from_slice(&0u16.to_be_bytes()); // seq 0
                v.extend_from_slice(&2u16.to_be_bytes()); // total 2
                v.extend_from_slice(b"partA");
                v
            }),
            (0xEB, {
                let mut v = Vec::new();
                v.extend_from_slice(b"Ducky\0");
                v.extend_from_slice(&1u16.to_be_bytes()); // seq 1
                v.extend_from_slice(&2u16.to_be_bytes()); // total 2
                v.extend_from_slice(b"partB");
                v
            }),
        ]);

        // Now replace with a single new payload
        let new_payload = b"NEW".to_vec();
        let out = insert_or_replace_appn(&orig, 0xEB, Some(b"Ducky\0"), &new_payload)
            .expect("insert_or_replace_appn failed");

        // Ensure extracted payload equals new_payload
        let recovered = extract_payload_from_bytes(&out, b"Ducky\0")
            .expect("extract returned Err")
            .expect("expected payload present");
        assert_eq!(recovered, new_payload);

        // Ensure there's at least one APP11 segment starting with our identifier
        let segs = collect_app_segments(&out);
        let identifier = b"Ducky\0";
        let mut found = false;
        for (marker, start, end) in segs.iter() {
            if *marker == 0xEB {
                let payload_start = start + 4;
                if payload_start < *end {
                    let payload_slice = &out[payload_start..*end];
                    if payload_slice.starts_with(identifier) {
                        found = true;
                        break;
                    }
                }
            }
        }
        assert!(found, "expected at least one APP11 segment starting with Ducky\\0");
    }


    #[test]
    fn test_missing_chunk_returns_error() {
        // craft a jpeg containing a Ducky header that claims total=2 but only include seq=0
        let mut seg_payload = Vec::new();
        seg_payload.extend_from_slice(b"Ducky\0");
        seg_payload.extend_from_slice(&0u16.to_be_bytes()); // seq 0
        seg_payload.extend_from_slice(&2u16.to_be_bytes()); // total 2 (but we'll only provide one chunk)
        seg_payload.extend_from_slice(b"onlypart");

        let orig = build_dummy_jpeg(vec![(0xEB, seg_payload)]);

        // extract should return Err because chunk 1 missing
        let res = extract_payload_from_bytes(&orig, b"Ducky\0");
        assert!(res.is_err(), "expected error due to missing chunk");
    }
}
