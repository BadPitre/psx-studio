//! WAV -> VAG conversion (SPU-ADPCM, the PS1 sound sample format).
//!
//! SPU-ADPCM encodes 28 16-bit samples into 16-byte blocks: a filter/shift
//! byte, a flags byte and 28 signed nibbles. Five prediction filters are
//! available; the encoder tries each and keeps the one with the smallest
//! error, feeding the *decoded* values back into the predictor state so
//! errors don't accumulate.

/// Prediction filter coefficients (numerators over 64).
const FILTERS: [(i32, i32); 5] = [(0, 0), (60, 0), (115, -52), (98, -55), (122, -60)];

const SAMPLES_PER_BLOCK: usize = 28;

/// Block flag bits.
const FLAG_LOOP_END: u8 = 1 << 0;
const FLAG_LOOP_REPEAT: u8 = 1 << 1;

#[derive(Debug, Default)]
pub struct VagReport {
    pub input_samples: usize,
    pub sample_rate: u32,
    pub spu_bytes: usize,
    pub warnings: Vec<String>,
}

/// Encode mono 16-bit PCM into SPU-ADPCM blocks (VAG body, header not
/// included). A silent one-shot terminator block is appended.
pub fn encode_spu_adpcm(pcm: &[i16]) -> Vec<u8> {
    let mut out = Vec::with_capacity((pcm.len() / SAMPLES_PER_BLOCK + 2) * 16);
    // Predictor state: previous two *decoded* samples.
    let mut s1 = 0i32;
    let mut s2 = 0i32;

    let block_count = pcm.len().div_ceil(SAMPLES_PER_BLOCK);
    for block_idx in 0..block_count {
        let start = block_idx * SAMPLES_PER_BLOCK;
        let mut samples = [0i16; SAMPLES_PER_BLOCK];
        for (k, slot) in samples.iter_mut().enumerate() {
            *slot = pcm.get(start + k).copied().unwrap_or(0);
        }

        /* Pick the filter with the smallest maximum residual. */
        let mut best_filter = 0usize;
        let mut best_max = i64::MAX;
        for (f, (c1, c2)) in FILTERS.iter().enumerate() {
            let (mut ps1, mut ps2) = (s1, s2);
            let mut max_err = 0i64;
            for &s in &samples {
                let pred = (ps1 * c1 + ps2 * c2) >> 6;
                let err = (s as i64 - pred as i64).abs();
                max_err = max_err.max(err);
                // Ideal-tracking approximation for filter selection.
                ps2 = ps1;
                ps1 = s as i32;
            }
            if max_err < best_max {
                best_max = max_err;
                best_filter = f;
            }
        }

        /* Smallest shift that fits every residual in a signed nibble. */
        let mut shift = 0u32;
        while shift < 12 && (best_max >> shift) > 7 {
            shift += 1;
        }

        /* Quantize with decoded-state feedback. */
        let (c1, c2) = FILTERS[best_filter];
        let mut nibbles = [0u8; SAMPLES_PER_BLOCK];
        for (k, &s) in samples.iter().enumerate() {
            let pred = (s1 * c1 + s2 * c2) >> 6;
            let err = s as i32 - pred;
            let mut n = err >> shift;
            // Round to nearest for the dropped bits.
            if shift > 0 && (err & (1 << (shift - 1))) != 0 {
                n += 1;
            }
            let n = n.clamp(-8, 7);
            nibbles[k] = (n & 0xF) as u8;
            let decoded = (pred + (n << shift)).clamp(-32768, 32767);
            s2 = s1;
            s1 = decoded;
        }

        /* SPU stores the shift as 12 - shift? No: the hardware field is the
         * right-shift applied at decode: sample = nibble << (12 - field).
         * Our `shift` is the left-shift at decode, so field = 12 - shift. */
        let shift_field = (12 - shift) as u8;
        let flags = if block_idx == block_count - 1 {
            FLAG_LOOP_END
        } else {
            0
        };
        out.push((best_filter as u8) << 4 | (shift_field & 0xF));
        out.push(flags);
        for pair in nibbles.chunks_exact(2) {
            out.push(pair[0] | (pair[1] << 4));
        }
    }

    /* Silent looping terminator so the voice parks quietly. */
    out.push(0);
    out.push(FLAG_LOOP_END | FLAG_LOOP_REPEAT | (1 << 2));
    out.extend_from_slice(&[0u8; 14]);

    out
}

/// Full VAG file: 48-byte header (big-endian fields, Sony convention)
/// followed by the ADPCM body.
pub fn write_vag(name: &str, sample_rate: u32, body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(48 + body.len());
    out.extend_from_slice(b"VAGp");
    out.extend_from_slice(&0x20u32.to_be_bytes()); // version
    out.extend_from_slice(&0u32.to_be_bytes()); // interleave
    out.extend_from_slice(&(body.len() as u32).to_be_bytes());
    out.extend_from_slice(&sample_rate.to_be_bytes());
    out.extend_from_slice(&[0u8; 12]); // reserved
    let mut name_bytes = [0u8; 16];
    for (i, b) in name.bytes().take(16).enumerate() {
        name_bytes[i] = b;
    }
    out.extend_from_slice(&name_bytes);
    out.extend_from_slice(body);
    out
}

/// Convert a WAV file (mono or stereo, 16-bit or float PCM) to VAG bytes.
pub fn wav_to_vag(wav_path: &std::path::Path, name: &str) -> Result<(Vec<u8>, VagReport), String> {
    let mut reader =
        hound::WavReader::open(wav_path).map_err(|e| format!("{}: {e}", wav_path.display()))?;
    let spec = reader.spec();
    let mut report = VagReport {
        sample_rate: spec.sample_rate,
        ..Default::default()
    };

    let mono: Vec<i16> = match spec.sample_format {
        hound::SampleFormat::Int => {
            let shift = spec.bits_per_sample.saturating_sub(16);
            let raw: Result<Vec<i32>, _> = reader.samples::<i32>().collect();
            let raw = raw.map_err(|e| e.to_string())?;
            raw.chunks(spec.channels as usize)
                .map(|frame| {
                    let sum: i64 = frame.iter().map(|&s| (s >> shift) as i64).sum();
                    (sum / frame.len() as i64) as i16
                })
                .collect()
        }
        hound::SampleFormat::Float => {
            let raw: Result<Vec<f32>, _> = reader.samples::<f32>().collect();
            let raw = raw.map_err(|e| e.to_string())?;
            raw.chunks(spec.channels as usize)
                .map(|frame| {
                    let avg = frame.iter().sum::<f32>() / frame.len() as f32;
                    (avg.clamp(-1.0, 1.0) * 32767.0) as i16
                })
                .collect()
        }
    };
    report.input_samples = mono.len();

    if spec.sample_rate > 44100 {
        report.warnings.push(format!(
            "sample rate {} Hz exceeds the SPU maximum of 44100 Hz; it will play pitched down",
            spec.sample_rate
        ));
    }
    if spec.channels > 1 {
        report
            .warnings
            .push(format!("{} channels mixed down to mono", spec.channels));
    }

    let body = encode_spu_adpcm(&mono);
    report.spu_bytes = body.len();
    if body.len() > 512 * 1024 - 0x1010 {
        report
            .warnings
            .push("sample is larger than the available SPU RAM (~508 KB)".into());
    }

    Ok((write_vag(name, spec.sample_rate, &body), report))
}

/// Reference SPU-ADPCM decoder, used by the tests to verify the encoder.
pub fn decode_spu_adpcm(data: &[u8]) -> Vec<i16> {
    let mut out = Vec::new();
    let mut s1 = 0i32;
    let mut s2 = 0i32;
    for block in data.chunks_exact(16) {
        let filter = ((block[0] >> 4) & 0x7) as usize;
        let shift_field = (block[0] & 0xF) as u32;
        let flags = block[1];
        let (c1, c2) = FILTERS[filter.min(4)];
        for i in 0..SAMPLES_PER_BLOCK {
            let byte = block[2 + i / 2];
            let nibble = if i % 2 == 0 { byte & 0xF } else { byte >> 4 };
            // Sign-extend the nibble, then apply the decode shift.
            let n = ((nibble as i32) << 28) >> 28;
            let pred = (s1 * c1 + s2 * c2) >> 6;
            let sample = (pred + (n << (12 - shift_field))).clamp(-32768, 32767);
            s2 = s1;
            s1 = sample;
            out.push(sample as i16);
        }
        if flags & FLAG_LOOP_END != 0 && flags & FLAG_LOOP_REPEAT != 0 {
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(len: usize, freq: f32, rate: f32) -> Vec<i16> {
        (0..len)
            .map(|i| {
                let t = i as f32 / rate;
                ((t * freq * std::f32::consts::TAU).sin() * 12000.0) as i16
            })
            .collect()
    }

    #[test]
    fn adpcm_roundtrip_is_close() {
        let pcm = sine(28 * 20, 440.0, 22050.0);
        let body = encode_spu_adpcm(&pcm);
        // 20 data blocks + terminator.
        assert_eq!(body.len(), 21 * 16);
        let decoded = decode_spu_adpcm(&body);
        assert!(decoded.len() >= pcm.len());

        // Signal-to-error check: ADPCM should stay well within ~5% RMS.
        let mut err_sq = 0f64;
        let mut sig_sq = 0f64;
        for (a, b) in pcm.iter().zip(&decoded) {
            let e = (*a as f64) - (*b as f64);
            err_sq += e * e;
            sig_sq += (*a as f64) * (*a as f64);
        }
        let ratio = (err_sq / sig_sq).sqrt();
        assert!(ratio < 0.05, "ADPCM error too high: {ratio}");
    }

    #[test]
    fn last_data_block_marks_loop_end() {
        let pcm = sine(28 * 3, 440.0, 22050.0);
        let body = encode_spu_adpcm(&pcm);
        assert_eq!(body[2 * 16 + 1], FLAG_LOOP_END);
        // Terminator parks the voice on a silent loop.
        assert_eq!(body[3 * 16 + 1], FLAG_LOOP_END | FLAG_LOOP_REPEAT | (1 << 2));
    }

    #[test]
    fn vag_header_is_big_endian() {
        let body = encode_spu_adpcm(&sine(28, 440.0, 22050.0));
        let vag = write_vag("blip", 22050, &body);
        assert_eq!(&vag[0..4], b"VAGp");
        assert_eq!(
            u32::from_be_bytes(vag[12..16].try_into().unwrap()) as usize,
            body.len()
        );
        assert_eq!(u32::from_be_bytes(vag[16..20].try_into().unwrap()), 22050);
        assert_eq!(vag.len(), 48 + body.len());
    }
}
