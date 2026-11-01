//! Audio file decoder source using Symphonia.
//!
//! Probes format, decodes to full-precision interleaved stereo `f32`,
//! and handles channel duplication / downmixing.

use anyhow::{Context, Result, anyhow};
use std::fs::File;
use std::path::{Path, PathBuf};
use std::time::Duration as StdDuration;
use symphonia::core::audio::GenericAudioBufferRef;
use symphonia::core::codecs::CodecParameters;
use symphonia::core::codecs::audio::well_known::{
    CODEC_ID_AAC, CODEC_ID_ALAC, CODEC_ID_FLAC, CODEC_ID_MP3, CODEC_ID_PCM_F32BE,
    CODEC_ID_PCM_F32LE, CODEC_ID_PCM_S8, CODEC_ID_PCM_S16BE, CODEC_ID_PCM_S16LE,
    CODEC_ID_PCM_S24BE, CODEC_ID_PCM_S24LE, CODEC_ID_PCM_S32BE, CODEC_ID_PCM_S32LE,
    CODEC_ID_PCM_U8, CODEC_ID_VORBIS,
};
use symphonia::core::codecs::audio::{AudioDecoder, AudioDecoderOptions, CODEC_ID_NULL_AUDIO};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo};
use symphonia::core::io::{MediaSourceStream, MediaSourceStreamOptions};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::units::{Time, Timestamp};

/// Audio format and stream metadata extracted during probing.
#[derive(Debug, Clone)]
pub struct AudioMetadata {
    pub path: PathBuf,
    pub codec_name: String,
    pub sample_rate: u32,
    pub channels: u16,
    pub bit_depth: Option<u32>,
    pub duration: Option<StdDuration>,
}

impl AudioMetadata {
    /// Formats a short format display line (e.g., "FLAC 16-bit 44.1 kHz" or "AAC 256 kbps").
    pub fn format_line(&self) -> String {
        let bit_info = match self.bit_depth {
            Some(bits) => format!(" {}-bit", bits),
            None => String::new(),
        };
        let rate_khz = (self.sample_rate as f32) / 1000.0;
        format!("{}{bit_info} {:.1} kHz", self.codec_name, rate_khz)
    }
}

/// Symphonia-backed audio decoder source.
pub struct AudioSource {
    reader: Box<dyn FormatReader>,
    decoder: Box<dyn AudioDecoder>,
    track_id: u32,
    metadata: AudioMetadata,
    raw_buffer: Vec<f32>,
    stereo_buffer: Vec<f32>,
    buffer_offset: usize,
    end_of_stream: bool,
}

impl AudioSource {
    /// Opens and probes an audio file from disk.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let file =
            File::open(&path).with_context(|| format!("Failed to open {}", path.display()))?;
        let mss = MediaSourceStream::new(Box::new(file), MediaSourceStreamOptions::default());

        let mut hint = Hint::new();
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            hint.with_extension(ext);
        }

        let format_opts = FormatOptions::default();
        let meta_opts = MetadataOptions::default();

        let reader = symphonia::default::get_probe()
            .probe(&hint, mss, format_opts, meta_opts)
            .with_context(|| format!("Failed to probe format of {}", path.display()))?;

        // Find the first audio track
        let track = reader
            .tracks()
            .iter()
            .find(|t| match &t.codec_params {
                Some(CodecParameters::Audio(params)) => params.codec != CODEC_ID_NULL_AUDIO,
                _ => false,
            })
            .ok_or_else(|| anyhow!("No supported audio track in {}", path.display()))?;

        let track_id = track.id;
        let audio_params = match &track.codec_params {
            Some(CodecParameters::Audio(params)) => params.clone(),
            _ => unreachable!(),
        };

        let sample_rate = audio_params.sample_rate.unwrap_or(44100);
        let channels = audio_params
            .channels
            .as_ref()
            .map(|c| c.count() as u16)
            .unwrap_or(2);
        let bit_depth = audio_params.bits_per_sample;

        let duration = if let (Some(tb), Some(dur)) = (track.time_base, track.duration) {
            tb.calc_time(Timestamp::new(dur.get() as i64))
                .map(|t| StdDuration::from_secs_f64(t.as_secs_f64()))
        } else {
            track
                .num_frames
                .map(|n_frames| StdDuration::from_secs_f64(n_frames as f64 / sample_rate as f64))
        };

        let codec_name = match audio_params.codec {
            CODEC_ID_FLAC => "FLAC".to_string(),
            CODEC_ID_MP3 => "MP3".to_string(),
            CODEC_ID_AAC => "AAC".to_string(),
            CODEC_ID_VORBIS => "Vorbis".to_string(),
            CODEC_ID_ALAC => "ALAC".to_string(),
            CODEC_ID_PCM_S16LE | CODEC_ID_PCM_S16BE | CODEC_ID_PCM_S24LE | CODEC_ID_PCM_S24BE
            | CODEC_ID_PCM_S32LE | CODEC_ID_PCM_S32BE | CODEC_ID_PCM_F32LE | CODEC_ID_PCM_F32BE
            | CODEC_ID_PCM_S8 | CODEC_ID_PCM_U8 => "PCM".to_string(),
            other => {
                let raw = format!("{other:?}");
                raw.replace("AudioCodecId(0x", "").replace(')', "")
            }
        };

        let decoder_opts = AudioDecoderOptions::default();

        let decoder = symphonia::default::get_codecs()
            .make_audio_decoder(&audio_params, &decoder_opts)
            .with_context(|| format!("Failed to instantiate decoder for {}", path.display()))?;

        let metadata = AudioMetadata {
            path,
            codec_name,
            sample_rate,
            channels,
            bit_depth,
            duration,
        };

        Ok(Self {
            reader,
            decoder,
            track_id,
            metadata,
            raw_buffer: Vec::new(),
            stereo_buffer: Vec::new(),
            buffer_offset: 0,
            end_of_stream: false,
        })
    }

    pub fn metadata(&self) -> &AudioMetadata {
        &self.metadata
    }

    pub fn sample_rate(&self) -> u32 {
        self.metadata.sample_rate
    }

    pub fn is_end_of_stream(&self) -> bool {
        self.end_of_stream && self.buffer_offset >= self.stereo_buffer.len()
    }

    /// Reads decoded interleaved stereo `f32` samples into `output`.
    /// Returns the number of stereo samples written (always a multiple of 2 for L/R pairs).
    pub fn read_stereo(&mut self, output: &mut [f32]) -> usize {
        let mut written = 0;

        while written < output.len() {
            // If internal buffer has remaining samples, copy them out
            if self.buffer_offset < self.stereo_buffer.len() {
                let available = self.stereo_buffer.len() - self.buffer_offset;
                let needed = output.len() - written;
                let to_copy = available.min(needed);

                output[written..written + to_copy].copy_from_slice(
                    &self.stereo_buffer[self.buffer_offset..self.buffer_offset + to_copy],
                );

                self.buffer_offset += to_copy;
                written += to_copy;
                continue;
            }

            if self.end_of_stream {
                break;
            }

            // Decode next packet
            if !self.decode_next_packet() {
                self.end_of_stream = true;
                break;
            }
        }

        written
    }

    /// Decodes the next packet into `self.stereo_buffer`.
    fn decode_next_packet(&mut self) -> bool {
        self.buffer_offset = 0;
        self.stereo_buffer.clear();

        loop {
            let maybe_packet = match self.reader.next_packet() {
                Ok(Some(packet)) => packet,
                Ok(None) => return false, // End of stream
                Err(SymphoniaError::ResetRequired) => {
                    self.decoder.reset();
                    continue;
                }
                Err(SymphoniaError::DecodeError(_)) => {
                    // Skip corrupt packet and continue
                    continue;
                }
                Err(_) => return false,
            };

            if maybe_packet.track_id != self.track_id {
                continue;
            }

            match self.decoder.decode(&maybe_packet) {
                Ok(buf) => {
                    convert_to_stereo(&buf, &mut self.raw_buffer, &mut self.stereo_buffer);
                    return !self.stereo_buffer.is_empty();
                }
                Err(SymphoniaError::DecodeError(_)) => {
                    // Skip corrupt packet and retry with next
                    continue;
                }
                Err(SymphoniaError::ResetRequired) => {
                    self.decoder.reset();
                    continue;
                }
                Err(_) => return false,
            }
        }
    }

    /// Seeks to a position in time.
    pub fn seek(&mut self, time: StdDuration) -> Result<()> {
        let seek_time =
            Time::try_new(time.as_secs() as i64, time.subsec_nanos()).unwrap_or_default();
        let seek_to = SeekTo::Time {
            time: seek_time,
            track_id: Some(self.track_id),
        };

        self.reader
            .seek(SeekMode::Coarse, seek_to)
            .with_context(|| format!("Failed to seek to {:?}", time))?;

        self.decoder.reset();
        self.buffer_offset = 0;
        self.stereo_buffer.clear();
        self.end_of_stream = false;
        Ok(())
    }
}

/// Converts decoded planar/multichannel buffer into interleaved stereo `f32`.
fn convert_to_stereo(
    buf: &GenericAudioBufferRef<'_>,
    raw_buffer: &mut Vec<f32>,
    stereo_buffer: &mut Vec<f32>,
) {
    let planes = buf.num_planes();
    let frames = buf.frames();
    if frames == 0 {
        return;
    }

    raw_buffer.clear();
    buf.copy_to_vec_interleaved::<f32>(raw_buffer);

    stereo_buffer.reserve(frames * 2);

    match planes {
        1 => {
            // Mono -> duplicate L and R
            for &sample in raw_buffer.iter() {
                stereo_buffer.push(sample);
                stereo_buffer.push(sample);
            }
        }
        2 => {
            // Stereo -> pass through
            stereo_buffer.extend_from_slice(raw_buffer);
        }
        n => {
            // Multichannel -> downmix to stereo:
            // C and surrounds at -3 dB (0.707), LFE dropped.
            let inv_sqrt2 = std::f32::consts::FRAC_1_SQRT_2;
            for frame_idx in 0..frames {
                let base = frame_idx * n;
                let l = raw_buffer[base];
                let r = raw_buffer[base + 1];
                let c = if n > 2 { raw_buffer[base + 2] } else { 0.0 };
                let ls = if n > 4 { raw_buffer[base + 4] } else { 0.0 };
                let rs = if n > 5 { raw_buffer[base + 5] } else { 0.0 };

                let stereo_l = l + inv_sqrt2 * c + inv_sqrt2 * ls;
                let stereo_r = r + inv_sqrt2 * c + inv_sqrt2 * rs;

                stereo_buffer.push(stereo_l);
                stereo_buffer.push(stereo_r);
            }
        }
    }
}
