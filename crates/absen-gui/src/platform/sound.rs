//! Embedded notification audio decoding and cross-platform playback.

use std::io::{Cursor, ErrorKind};
use std::sync::OnceLock;

use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::default::{get_codecs, get_probe};

/// Embedded MP3 notification chime.
pub const NOTIFICATION_MP3_BYTES: &[u8] = include_bytes!("../../../../assets/notification.mp3");

static DECODED_PCM: OnceLock<Option<DecodedPcm>> = OnceLock::new();
static CACHED_WAVS_BY_VOLUME: [OnceLock<Option<Vec<u8>>>; 101] = [const { OnceLock::new() }; 101];

#[cfg(any(target_os = "macos", target_os = "linux", test))]
static TEMP_FILE_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

struct DecodedPcm {
    sample_rate: u32,
    channels: u16,
    samples: Vec<i16>,
}

fn decode_mp3_to_pcm(mp3_bytes: &[u8]) -> Result<DecodedPcm, String> {
    if mp3_bytes.is_empty() {
        return Err("data MP3 kosong".to_string());
    }

    let mss = MediaSourceStream::new(
        Box::new(Cursor::new(mp3_bytes.to_vec())),
        Default::default(),
    );
    let mut hint = Hint::new();
    hint.with_extension("mp3");

    let probed = get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|e| format!("gagal membaca format MP3: {e}"))?;

    let mut format = probed.format;
    let track = format
        .default_track()
        .filter(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .or_else(|| {
            format
                .tracks()
                .iter()
                .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        })
        .ok_or_else(|| "trek audio MP3 tidak ditemukan".to_string())?;

    let track_id = track.id;
    let mut decoder = get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| format!("gagal inisialisasi dekoder MP3: {e}"))?;

    let mut sample_rate: u32 = 0;
    let mut channels: u16 = 0;
    let mut pcm_samples: Vec<i16> = Vec::new();
    let mut sample_buf: Option<SampleBuffer<i16>> = None;

    loop {
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            Err(SymphoniaError::IoError(err)) if err.kind() == ErrorKind::UnexpectedEof => break,
            Err(SymphoniaError::ResetRequired) => break,
            Err(err) => return Err(format!("gagal membaca paket MP3: {err}")),
        };

        if packet.track_id() != track_id {
            continue;
        }

        match decoder.decode(&packet) {
            Ok(decoded) => {
                let spec = *decoded.spec();
                if sample_rate == 0 {
                    sample_rate = spec.rate;
                    channels = spec.channels.count().min(u16::MAX as usize) as u16;
                }
                let capacity = decoded.capacity() as u64;
                let buf = match &mut sample_buf {
                    Some(existing)
                        if existing.capacity() >= decoded.frames() * spec.channels.count() =>
                    {
                        existing
                    }
                    slot => {
                        *slot = Some(SampleBuffer::<i16>::new(capacity, spec));
                        slot.as_mut().expect("sample buffer just initialized")
                    }
                };
                buf.copy_interleaved_ref(decoded);
                pcm_samples.extend_from_slice(buf.samples());
            }
            Err(SymphoniaError::DecodeError(_)) => continue,
            Err(err) => return Err(format!("gagal mendekode frame MP3: {err}")),
        }
    }

    if pcm_samples.is_empty() || sample_rate == 0 || channels == 0 {
        return Err("hasil dekode MP3 tidak menghasilkan sampel PCM".to_string());
    }

    Ok(DecodedPcm {
        sample_rate,
        channels,
        samples: pcm_samples,
    })
}

fn scale_pcm_sample(sample: i16, volume_pct: u8) -> i16 {
    let clamped = volume_pct.min(100);
    if clamped == 0 {
        return 0;
    }
    if clamped == 100 {
        return sample;
    }
    let gain = (f32::from(clamped) / 100.0).powf(1.5);
    (f32::from(sample) * gain)
        .round()
        .clamp(f32::from(i16::MIN), f32::from(i16::MAX)) as i16
}

fn encode_pcm_to_wav(pcm: &DecodedPcm, volume_pct: u8) -> Result<Vec<u8>, String> {
    let clamped = volume_pct.min(100);
    let data_len = u32::try_from(pcm.samples.len().saturating_mul(2))
        .map_err(|_| "ukuran PCM melebihi batas WAV".to_string())?;
    let riff_len = 36u32
        .checked_add(data_len)
        .ok_or_else(|| "ukuran RIFF melebihi batas WAV".to_string())?;
    let block_align = pcm.channels.saturating_mul(2);
    let byte_rate = pcm.sample_rate.saturating_mul(u32::from(block_align));

    let mut wav = Vec::with_capacity(44 + data_len as usize);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&riff_len.to_le_bytes());
    wav.extend_from_slice(b"WAVE");
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&pcm.channels.to_le_bytes());
    wav.extend_from_slice(&pcm.sample_rate.to_le_bytes());
    wav.extend_from_slice(&byte_rate.to_le_bytes());
    wav.extend_from_slice(&block_align.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_len.to_le_bytes());
    for &sample in &pcm.samples {
        let scaled = scale_pcm_sample(sample, clamped);
        wav.extend_from_slice(&scaled.to_le_bytes());
    }

    Ok(wav)
}

/// Decode an in-memory MP3 buffer into a standard 44-byte RIFF PCM (`i16` LE) WAV byte vector.
pub fn decode_mp3_to_wav(mp3_bytes: &[u8]) -> Result<Vec<u8>, String> {
    decode_mp3_to_wav_with_volume(mp3_bytes, 100)
}

/// Decode an in-memory MP3 buffer into a WAV byte vector scaled to `volume_pct` (`0..=100`).
pub fn decode_mp3_to_wav_with_volume(mp3_bytes: &[u8], volume_pct: u8) -> Result<Vec<u8>, String> {
    let clamped = volume_pct.min(100);
    let pcm = decode_mp3_to_pcm(mp3_bytes)?;
    encode_pcm_to_wav(&pcm, clamped)
}

fn cached_notification_pcm() -> Option<&'static DecodedPcm> {
    DECODED_PCM
        .get_or_init(|| decode_mp3_to_pcm(NOTIFICATION_MP3_BYTES).ok())
        .as_ref()
}

/// Return the lazily decoded and cached notification WAV bytes at 100% volume.
pub fn cached_notification_wav() -> Option<&'static [u8]> {
    cached_notification_wav_for_volume(100)
}

/// Return the lazily decoded and cached notification WAV bytes scaled to `volume_pct` (`0..=100`).
pub fn cached_notification_wav_for_volume(volume_pct: u8) -> Option<&'static [u8]> {
    let clamped = volume_pct.min(100);
    CACHED_WAVS_BY_VOLUME[usize::from(clamped)]
        .get_or_init(|| {
            let pcm = cached_notification_pcm()?;
            encode_pcm_to_wav(pcm, clamped).ok()
        })
        .as_deref()
}

#[cfg(any(target_os = "macos", target_os = "linux", test))]
fn ensure_temp_wav_file(
    dir: &std::path::Path,
    volume_pct: u8,
    wav: &[u8],
) -> std::io::Result<std::path::PathBuf> {
    use std::io::Write;
    use std::sync::atomic::Ordering;

    let clamped = volume_pct.min(100);
    let path = dir.join(format!("polban-presensi-notification-v{clamped}.wav"));
    if std::fs::metadata(&path).is_ok_and(|meta| meta.is_file() && meta.len() == wav.len() as u64) {
        return Ok(path);
    }

    let seq = TEMP_FILE_SEQ.fetch_add(1, Ordering::Relaxed);
    let tmp_path = dir.join(format!(
        ".polban-presensi-notification-v{clamped}.{}.{seq}.tmp",
        std::process::id()
    ));

    let write_res = (|| -> std::io::Result<()> {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp_path)?;
        file.write_all(wav)?;
        file.flush()?;
        Ok(())
    })();

    if let Err(err) = write_res {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(err);
    }

    if let Err(err) = std::fs::rename(&tmp_path, &path) {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(err);
    }

    Ok(path)
}

/// Play the embedded notification sound asynchronously at 100% volume without blocking the caller.
pub fn play_notification_sound() {
    play_notification_sound_with_volume(100);
}

/// Play the embedded notification sound asynchronously scaled to `volume_pct` (`0..=100`).
pub fn play_notification_sound_with_volume(volume_pct: u8) {
    let clamped = volume_pct.min(100);
    if clamped == 0 {
        return;
    }

    #[cfg(target_os = "windows")]
    {
        if let Some(wav) = cached_notification_wav_for_volume(clamped) {
            unsafe {
                windows_sys::Win32::Media::Audio::PlaySoundW(
                    wav.as_ptr().cast(),
                    std::ptr::null_mut(),
                    windows_sys::Win32::Media::Audio::SND_MEMORY
                        | windows_sys::Win32::Media::Audio::SND_ASYNC
                        | windows_sys::Win32::Media::Audio::SND_NODEFAULT,
                );
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        if let Some(wav) = cached_notification_wav_for_volume(clamped) {
            let _ = std::thread::Builder::new()
                .name("polban-sound".to_string())
                .spawn(move || {
                    let Ok(path) = ensure_temp_wav_file(&std::env::temp_dir(), clamped, wav) else {
                        return;
                    };
                    let _ = std::process::Command::new("afplay")
                        .arg(&path)
                        .stdin(std::process::Stdio::null())
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null())
                        .status();
                });
        }
    }

    #[cfg(target_os = "linux")]
    {
        if let Some(wav) = cached_notification_wav_for_volume(clamped) {
            let _ = std::thread::Builder::new()
                .name("polban-sound".to_string())
                .spawn(move || {
                    let Ok(path) = ensure_temp_wav_file(&std::env::temp_dir(), clamped, wav) else {
                        return;
                    };
                    for player in ["paplay", "pw-play", "aplay"] {
                        if let Ok(status) = std::process::Command::new(player)
                            .arg(&path)
                            .stdin(std::process::Stdio::null())
                            .stdout(std::process::Stdio::null())
                            .stderr(std::process::Stdio::null())
                            .status()
                        {
                            if status.success() {
                                break;
                            }
                        }
                    }
                });
        }
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        let _ = clamped;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_valid_pcm_wav(wav: &[u8]) {
        assert!(wav.len() > 1024);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(&wav[12..16], b"fmt ");
        assert_eq!(u16::from_le_bytes([wav[20], wav[21]]), 1);
        assert_eq!(&wav[36..40], b"data");
        let data_len = u32::from_le_bytes([wav[40], wav[41], wav[42], wav[43]]) as usize;
        assert_eq!(wav.len(), 44 + data_len);
        assert_eq!(data_len % 2, 0);
    }

    fn peak_pcm_amplitude(wav: &[u8]) -> i32 {
        assert_valid_pcm_wav(wav);
        wav[44..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&pair| i32::from(i16::from_le_bytes(pair)).abs())
            .max()
            .unwrap_or(0)
    }

    #[test]
    fn decode_embedded_mp3_produces_valid_wav() {
        let wav = decode_mp3_to_wav(NOTIFICATION_MP3_BYTES)
            .expect("embedded notification.mp3 should decode cleanly");
        assert_valid_pcm_wav(&wav);
    }

    #[test]
    fn decode_invalid_mp3_returns_error() {
        assert!(decode_mp3_to_wav(&[]).is_err());
        assert!(decode_mp3_to_wav(b"not a valid mp3 stream").is_err());
        assert!(decode_mp3_to_wav_with_volume(&[], 0).is_err());
        assert!(decode_mp3_to_wav_with_volume(b"not a valid mp3 stream", 50).is_err());
    }

    #[test]
    fn decode_with_volume_scales_monotonically_and_clamps() {
        let wav_0 = decode_mp3_to_wav_with_volume(NOTIFICATION_MP3_BYTES, 0)
            .expect("0% volume should produce valid silent WAV");
        let wav_25 = decode_mp3_to_wav_with_volume(NOTIFICATION_MP3_BYTES, 25)
            .expect("25% volume should decode");
        let wav_50 = decode_mp3_to_wav_with_volume(NOTIFICATION_MP3_BYTES, 50)
            .expect("50% volume should decode");
        let wav_75 = decode_mp3_to_wav_with_volume(NOTIFICATION_MP3_BYTES, 75)
            .expect("75% volume should decode");
        let wav_100 = decode_mp3_to_wav_with_volume(NOTIFICATION_MP3_BYTES, 100)
            .expect("100% volume should decode");
        let wav_over = decode_mp3_to_wav_with_volume(NOTIFICATION_MP3_BYTES, 180)
            .expect(">100% volume should clamp to 100%");
        let wav_base = decode_mp3_to_wav(NOTIFICATION_MP3_BYTES)
            .expect("base decode_mp3_to_wav should succeed");

        assert_eq!(wav_100, wav_base);
        assert_eq!(wav_over, wav_100);
        assert_eq!(wav_0.len(), wav_100.len());
        assert_eq!(&wav_0[..44], &wav_100[..44]);

        let peak_0 = peak_pcm_amplitude(&wav_0);
        let peak_25 = peak_pcm_amplitude(&wav_25);
        let peak_50 = peak_pcm_amplitude(&wav_50);
        let peak_75 = peak_pcm_amplitude(&wav_75);
        let peak_100 = peak_pcm_amplitude(&wav_100);

        assert_eq!(peak_0, 0);
        assert!(peak_0 < peak_25);
        assert!(peak_25 < peak_50);
        assert!(peak_50 < peak_75);
        assert!(peak_75 < peak_100);
    }

    #[test]
    fn cached_notification_wav_for_volume_returns_static_slices_and_clamps() {
        let c0 = cached_notification_wav_for_volume(0).expect("cached 0% WAV");
        let c50 = cached_notification_wav_for_volume(50).expect("cached 50% WAV");
        let c50_again = cached_notification_wav_for_volume(50).expect("cached 50% WAV repeat");
        let c100 = cached_notification_wav_for_volume(100).expect("cached 100% WAV");
        let c_default = cached_notification_wav().expect("cached default WAV");
        let c_over = cached_notification_wav_for_volume(200).expect("cached clamped WAV");

        assert_eq!(peak_pcm_amplitude(c0), 0);
        assert!(peak_pcm_amplitude(c50) > 0);
        assert!(peak_pcm_amplitude(c50) < peak_pcm_amplitude(c100));
        assert!(std::ptr::eq(c50.as_ptr(), c50_again.as_ptr()));
        assert!(std::ptr::eq(c100.as_ptr(), c_default.as_ptr()));
        assert!(std::ptr::eq(c100.as_ptr(), c_over.as_ptr()));
    }

    #[test]
    fn play_notification_sound_with_volume_zero_is_silent_noop() {
        let v0_path = std::env::temp_dir().join("polban-presensi-notification-v0.wav");
        let _ = std::fs::remove_file(&v0_path);
        play_notification_sound_with_volume(0);
        assert!(!v0_path.exists());
    }

    #[test]
    fn ensure_temp_wav_file_writes_volume_keyed_file_atomically() {
        let dir = std::env::temp_dir().join(format!(
            "polban-sound-test-{}-{}",
            std::process::id(),
            TEMP_FILE_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).expect("create temp test dir");

        let wav_50 = cached_notification_wav_for_volume(50).expect("cached 50%");
        let wav_100 = cached_notification_wav_for_volume(100).expect("cached 100%");

        let p50 = ensure_temp_wav_file(&dir, 50, wav_50).expect("write v50");
        assert_eq!(
            p50.file_name().and_then(|s| s.to_str()),
            Some("polban-presensi-notification-v50.wav")
        );
        assert_eq!(std::fs::read(&p50).expect("read v50"), wav_50);

        let p100 = ensure_temp_wav_file(&dir, 150, wav_100).expect("write clamped v100");
        assert_eq!(
            p100.file_name().and_then(|s| s.to_str()),
            Some("polban-presensi-notification-v100.wav")
        );
        assert_eq!(std::fs::read(&p100).expect("read v100"), wav_100);
        assert_ne!(wav_50, wav_100);

        // Corrupted/truncated file is replaced atomically when length differs.
        std::fs::write(&p50, b"stale").expect("write stale payload");
        let p50_repaired = ensure_temp_wav_file(&dir, 50, wav_50).expect("repair v50");
        assert_eq!(p50_repaired, p50);
        assert_eq!(std::fs::read(&p50).expect("read repaired v50"), wav_50);

        // No leftover .tmp files in dir.
        let leftover_tmp = std::fs::read_dir(&dir)
            .expect("read test dir")
            .filter_map(Result::ok)
            .any(|entry| entry.path().extension().and_then(|s| s.to_str()) == Some("tmp"));
        assert!(!leftover_tmp);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
