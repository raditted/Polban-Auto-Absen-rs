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

static CACHED_WAV: OnceLock<Option<Vec<u8>>> = OnceLock::new();

/// Decode an in-memory MP3 buffer into a standard 44-byte RIFF PCM (`i16` LE) WAV byte vector.
pub fn decode_mp3_to_wav(mp3_bytes: &[u8]) -> Result<Vec<u8>, String> {
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

    let data_len = u32::try_from(pcm_samples.len().saturating_mul(2))
        .map_err(|_| "ukuran PCM melebihi batas WAV".to_string())?;
    let riff_len = 36u32
        .checked_add(data_len)
        .ok_or_else(|| "ukuran RIFF melebihi batas WAV".to_string())?;
    let block_align = channels.saturating_mul(2);
    let byte_rate = sample_rate.saturating_mul(u32::from(block_align));

    let mut wav = Vec::with_capacity(44 + data_len as usize);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&riff_len.to_le_bytes());
    wav.extend_from_slice(b"WAVE");
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&channels.to_le_bytes());
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&byte_rate.to_le_bytes());
    wav.extend_from_slice(&block_align.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_len.to_le_bytes());
    for sample in pcm_samples {
        wav.extend_from_slice(&sample.to_le_bytes());
    }

    Ok(wav)
}

/// Return the lazily decoded and cached notification WAV bytes.
pub fn cached_notification_wav() -> Option<&'static [u8]> {
    CACHED_WAV
        .get_or_init(|| decode_mp3_to_wav(NOTIFICATION_MP3_BYTES).ok())
        .as_deref()
}

/// Play the embedded notification sound asynchronously without blocking the caller thread.
pub fn play_notification_sound() {
    #[cfg(target_os = "windows")]
    {
        if let Some(wav) = cached_notification_wav() {
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
        if let Some(wav) = cached_notification_wav() {
            let _ = std::thread::Builder::new()
                .name("polban-sound".to_string())
                .spawn(move || {
                    let path = std::env::temp_dir().join("polban-presensi-notification.wav");
                    if !path.is_file() && std::fs::write(&path, wav).is_err() {
                        return;
                    }
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
        if let Some(wav) = cached_notification_wav() {
            let _ = std::thread::Builder::new()
                .name("polban-sound".to_string())
                .spawn(move || {
                    let path = std::env::temp_dir().join("polban-presensi-notification.wav");
                    if !path.is_file() && std::fs::write(&path, wav).is_err() {
                        return;
                    }
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_embedded_mp3_produces_valid_wav() {
        let wav = decode_mp3_to_wav(NOTIFICATION_MP3_BYTES)
            .expect("embedded notification.mp3 should decode cleanly");
        assert!(wav.len() > 1024);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(&wav[12..16], b"fmt ");
        assert_eq!(u16::from_le_bytes([wav[20], wav[21]]), 1);
        assert_eq!(&wav[36..40], b"data");
    }

    #[test]
    fn decode_invalid_mp3_returns_error() {
        assert!(decode_mp3_to_wav(&[]).is_err());
        assert!(decode_mp3_to_wav(b"not a valid mp3 stream").is_err());
    }
}
