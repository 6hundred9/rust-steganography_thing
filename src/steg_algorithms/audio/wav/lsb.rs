use hound::{WavReader, WavWriter, SampleFormat};
use std::path::Path;

pub fn hide_wav(path_in: &Path, path_out: &Path, msg: &[u8]) -> Result<(), String> {
    let mut r = WavReader::open(path_in).map_err(|e| e.to_string())?;
    let spec = r.spec();
    if spec.sample_format != SampleFormat::Int || spec.bits_per_sample != 16 {
        return Err("Only PCM16 WAV supported".into());
    }
    let mut samples: Vec<i16> = r.samples::<i16>().map(|s| s.unwrap()).collect();

    // make bit stream: 32-bit len header (big-endian) + message (MSB-first per byte)
    let len = msg.len() as u32;
    let mut bits = Vec::with_capacity(32 + msg.len() * 8);
    for i in (0..32).rev() { bits.push(((len >> i) & 1) as u8); }
    for &b in msg {
        for i in (0..8).rev() { bits.push(((b >> i) & 1) as u8); }
    }
    if bits.len() > samples.len() {
        return Err(format!("Too big: need {} samples, have {}", bits.len(), samples.len()));
    }

    // embed 1 LSB per sample
    for (i, bit) in bits.iter().enumerate() {
        let s = samples[i];
        samples[i] = (s & !1) | (*bit as i16); // set LSB
    }

    // write out
    let mut w = WavWriter::create(path_out, spec).map_err(|e| e.to_string())?;
    for s in samples { w.write_sample(s).map_err(|e| e.to_string())?; }
    w.finalize().map_err(|e| e.to_string())
}

pub fn find_wav(path: &Path) -> Result<Vec<u8>, String> {
    let mut r = WavReader::open(path).map_err(|e| e.to_string())?;
    let spec = r.spec();
    if spec.sample_format != SampleFormat::Int || spec.bits_per_sample != 16 {
        return Err("Only PCM16 WAV supported".into());
    }
    let samples: Vec<i16> = r.samples::<i16>().map(|s| s.unwrap()).collect();
    let bits: Vec<u8> = samples.iter().map(|&s| (s as u16 & 1) as u8).collect();

    if bits.len() < 32 { return Err("Too short for header".into()); }
    // read 32-bit len
    let mut len: u32 = 0;
    for i in 0..32 { len = (len << 1) | bits[i] as u32; }
    let need = (len as usize) * 8;
    if bits.len() < 32 + need { return Err("Truncated payload".into()); }

    let mut out = Vec::with_capacity(len as usize);
    let start = 32;
    for i in 0..len as usize {
        let mut b = 0u8;
        for j in 0..8 { b = (b << 1) | bits[start + i*8 + j]; }
        out.push(b);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use hound::{WavWriter, WavSpec, SampleFormat};
    use tempfile::tempdir;
    use std::path::PathBuf;

    // helper: make a silent 16-bit PCM wav with N samples
    fn make_test_wav(path: &PathBuf, samples: usize) {
        let spec = WavSpec {
            channels: 2,
            sample_rate: 44100,
            bits_per_sample: 16,
            sample_format: SampleFormat::Int,
        };
        let mut w = WavWriter::create(path, spec).unwrap();
        for _ in 0..samples {
            w.write_sample::<i16>(0).unwrap(); // silence
        }
        w.finalize().unwrap();
    }

    #[test]
    fn hide_and_find_roundtrip() {
        let dir = tempdir().unwrap();
        let in_path = dir.path().join("in.wav");
        let out_path = dir.path().join("out.wav");

        // enough samples for our message
        make_test_wav(&in_path, 100000);

        let msg = b"hello wav stego!";
        hide_wav(&in_path, &out_path, msg).unwrap();

        let decoded = find_wav(&out_path).unwrap();
        assert_eq!(decoded, msg);
    }

    #[test]
    fn hide_empty_message() {
        let dir = tempdir().unwrap();
        let in_path = dir.path().join("in.wav");
        let out_path = dir.path().join("out.wav");

        make_test_wav(&in_path, 1000);

        let msg = b"";
        hide_wav(&in_path, &out_path, msg).unwrap();

        let decoded = find_wav(&out_path).unwrap();
        assert_eq!(decoded, msg);
    }

    #[test]
    fn too_big_message_fails() {
        let dir = tempdir().unwrap();
        let in_path = dir.path().join("in.wav");
        let out_path = dir.path().join("out.wav");

        make_test_wav(&in_path, 100); // only 100 samples

        let msg = vec![42u8; 20]; // way too big
        let result = hide_wav(&in_path, &out_path, &msg);
        assert!(result.is_err(), "should fail for oversized message");
    }

    #[test]
    fn truncated_payload_fails() {
        let dir = tempdir().unwrap();
        let in_path = dir.path().join("in.wav");

        // craft tiny wav
        make_test_wav(&in_path, 10);

        // run find_wav on it: should error since no header/payload
        let res = find_wav(&in_path);
        assert!(res.is_err());
    }
}