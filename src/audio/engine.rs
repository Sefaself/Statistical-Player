//! Audio engine coordinating decoding, buffering, volume, and playback.

use anyhow::Result;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;
use tracing::{debug, info};

use crate::audio::resampler::AudioResampler;
use crate::audio::sink::{AudioSink, CallbackShared, CpalSink, NullSink};
use crate::audio::source::{AudioMetadata, AudioSource};

/// Ring buffer capacity in stereo samples for ~200ms at 48kHz.
/// 48,000 frames/sec * 2 samples/frame * 0.200 sec = 19,200 samples.
pub const RING_BUFFER_CAPACITY: usize = 19_200;

/// Top-level audio engine.
pub struct AudioEngine {
    sink: Box<dyn AudioSink>,
    shared: Arc<CallbackShared>,
    producer: rtrb::Producer<f32>,
    current_source: Option<AudioSource>,
    current_resampler: Option<AudioResampler>,
    frames_written: u64,
    track_end_frame: Option<u64>,
    scratch_buf: Vec<f32>,
    resample_in_buf: Vec<f32>,
    resample_out_buf: Vec<f32>,
    resample_out_offset: usize,
    resample_out_len: usize,
    resample_delay_to_trim: usize,
    flushing_zeros_remaining: usize,
    filter_flushed: bool,
    source_end_of_stream: bool,
    last_error: Option<String>,
}

impl AudioEngine {
    /// Creates an engine with a real CPAL hardware sink.
    pub fn new() -> Result<Self> {
        let (producer, consumer) = rtrb::RingBuffer::new(RING_BUFFER_CAPACITY);
        let shared = Arc::new(CallbackShared::default());
        let sink = Box::new(CpalSink::new(consumer, Arc::clone(&shared))?);

        info!(
            "Audio engine initialized with device sample rate: {} Hz",
            sink.sample_rate()
        );

        Ok(Self {
            sink,
            shared,
            producer,
            current_source: None,
            current_resampler: None,
            frames_written: 0,
            track_end_frame: None,
            scratch_buf: vec![0.0_f32; 4096],
            resample_in_buf: Vec::with_capacity(8192),
            resample_out_buf: vec![0.0_f32; 16384],
            resample_out_offset: 0,
            resample_out_len: 0,
            resample_delay_to_trim: 0,
            flushing_zeros_remaining: 0,
            filter_flushed: false,
            source_end_of_stream: false,
            last_error: None,
        })
    }

    /// Creates an engine with a NullSink for automated tests or headless environments.
    pub fn new_null(sample_rate: u32) -> (Self, Arc<NullSink>) {
        let (producer, consumer) = rtrb::RingBuffer::new(RING_BUFFER_CAPACITY);
        let shared = Arc::new(CallbackShared::default());
        let null_sink = Arc::new(NullSink::new(consumer, Arc::clone(&shared), sample_rate));

        let engine = Self {
            sink: Box::new(NullSinkWrapper(Arc::clone(&null_sink))),
            shared,
            producer,
            current_source: None,
            current_resampler: None,
            frames_written: 0,
            track_end_frame: None,
            scratch_buf: vec![0.0_f32; 4096],
            resample_in_buf: Vec::with_capacity(8192),
            resample_out_buf: vec![0.0_f32; 16384],
            resample_out_offset: 0,
            resample_out_len: 0,
            resample_delay_to_trim: 0,
            flushing_zeros_remaining: 0,
            filter_flushed: false,
            source_end_of_stream: false,
            last_error: None,
        };

        (engine, null_sink)
    }

    /// Loads and opens a new track from disk.
    pub fn load_file(&mut self, path: impl AsRef<Path>) -> Result<AudioMetadata> {
        let source = AudioSource::open(path)?;
        let metadata = source.metadata().clone();

        self.flush();
        let source_rate = source.sample_rate();
        let target_rate = self.sink.sample_rate();
        let resampler = AudioResampler::new(source_rate, target_rate, 2)?;
        self.resample_delay_to_trim = resampler.output_delay();
        self.current_resampler = Some(resampler);
        self.resample_in_buf.clear();
        self.resample_out_offset = 0;
        self.resample_out_len = 0;
        self.flushing_zeros_remaining = 0;
        self.filter_flushed = false;
        self.source_end_of_stream = false;

        self.current_source = Some(source);
        self.frames_written = 0;
        self.shared.frames_consumed.store(0, Ordering::Relaxed);
        self.track_end_frame = None;
        self.last_error = None;

        info!(
            "Loaded track: {} ({})",
            metadata.path.display(),
            metadata.format_line()
        );

        // Pre-fill ring buffer with initial ~100ms
        self.fill_buffer();

        Ok(metadata)
    }

    /// Fills the ring buffer from the current audio source.
    /// Returns `true` if audio is still actively decoding, `false` if finished.
    pub fn fill_buffer(&mut self) -> bool {
        let Some(ref mut source) = self.current_source else {
            return false;
        };

        // If in passthrough mode (rates equal), bypass chunking & resampling
        let is_passthrough = self
            .current_resampler
            .as_ref()
            .map(|r| r.is_passthrough())
            .unwrap_or(true);

        if is_passthrough {
            while self.producer.slots() >= 2 {
                let slots = self.producer.slots();
                let to_read = (slots - (slots % 2)).min(self.scratch_buf.len());
                if to_read == 0 {
                    break;
                }

                let read = match source.read_stereo(&mut self.scratch_buf[..to_read]) {
                    Ok(n) => n,
                    Err(e) => {
                        tracing::warn!("AudioSource decode error: {e}");
                        self.last_error = Some(e.to_string());
                        self.track_end_frame = Some(self.frames_written);
                        break;
                    }
                };

                if read == 0 {
                    if source.is_end_of_stream() && self.track_end_frame.is_none() {
                        self.track_end_frame = Some(self.frames_written);
                        debug!(
                            "Source reached end of stream at frame {}",
                            self.frames_written
                        );
                    }
                    break;
                }

                for &sample in &self.scratch_buf[..read] {
                    let _ = self.producer.push(sample);
                }

                self.frames_written += (read / 2) as u64;
            }

            return !self.is_track_ended();
        }

        // Resampling mode: AudioResampler converts source_rate to device_rate
        let resampler = self.current_resampler.as_mut().unwrap();

        while self.producer.slots() >= 2 {
            // 1. Drain any remaining resampled samples from resample_out_buf first
            if self.resample_out_offset < self.resample_out_len {
                let available = self.resample_out_len - self.resample_out_offset;
                let slots = self.producer.slots();
                let to_push = (slots - (slots % 2)).min(available);
                if to_push == 0 {
                    break;
                }
                for i in 0..to_push {
                    let _ = self
                        .producer
                        .push(self.resample_out_buf[self.resample_out_offset + i]);
                }
                self.resample_out_offset += to_push;
                self.frames_written += (to_push / 2) as u64;
                continue;
            }

            self.resample_out_offset = 0;
            self.resample_out_len = 0;

            let chunk_frames = resampler.input_frames_next();
            let needed_samples = chunk_frames * 2;

            // 2. Decode from source until resample_in_buf has at least chunk_frames or EOF is hit
            while self.resample_in_buf.len() < needed_samples && !self.source_end_of_stream {
                let slots_to_read = self.scratch_buf.len();
                let read = match source.read_stereo(&mut self.scratch_buf[..slots_to_read]) {
                    Ok(n) => n,
                    Err(e) => {
                        tracing::warn!("AudioSource decode error: {e}");
                        self.last_error = Some(e.to_string());
                        self.track_end_frame = Some(self.frames_written);
                        return false;
                    }
                };

                if read == 0 {
                    if source.is_end_of_stream() {
                        self.source_end_of_stream = true;
                    }
                    break;
                }

                self.resample_in_buf
                    .extend_from_slice(&self.scratch_buf[..read]);
            }

            // 3. Process full chunk
            if self.resample_in_buf.len() >= needed_samples {
                let max_out_frames = resampler.output_frames_max();
                if self.resample_out_buf.len() < max_out_frames * 2 {
                    self.resample_out_buf.resize(max_out_frames * 2, 0.0);
                }

                let (_consumed, produced_frames) = match resampler.process_into_buffer(
                    &self.resample_in_buf[..needed_samples],
                    &mut self.resample_out_buf,
                    None,
                ) {
                    Ok(res) => res,
                    Err(e) => {
                        tracing::warn!("Resampler error: {e}");
                        self.last_error = Some(e.to_string());
                        self.track_end_frame = Some(self.frames_written);
                        return false;
                    }
                };

                self.resample_in_buf.drain(..needed_samples);

                // Trim startup delay
                let mut out_start_frame = 0;
                let mut out_frames = produced_frames;
                if self.resample_delay_to_trim > 0 {
                    if out_frames <= self.resample_delay_to_trim {
                        self.resample_delay_to_trim -= out_frames;
                        out_frames = 0;
                    } else {
                        out_start_frame = self.resample_delay_to_trim;
                        out_frames -= self.resample_delay_to_trim;
                        self.resample_delay_to_trim = 0;
                    }
                }

                self.resample_out_offset = out_start_frame * 2;
                self.resample_out_len = (out_start_frame + out_frames) * 2;
                continue;
            }

            // 4. Source reached EOF with partial frames remaining
            if self.source_end_of_stream && !self.resample_in_buf.is_empty() {
                let partial_frames = self.resample_in_buf.len() / 2;
                self.resample_in_buf.resize(needed_samples, 0.0);

                let max_out_frames = resampler.output_frames_max();
                if self.resample_out_buf.len() < max_out_frames * 2 {
                    self.resample_out_buf.resize(max_out_frames * 2, 0.0);
                }

                let (_consumed, produced_frames) = match resampler.process_into_buffer(
                    &self.resample_in_buf[..needed_samples],
                    &mut self.resample_out_buf,
                    Some(partial_frames),
                ) {
                    Ok(res) => res,
                    Err(e) => {
                        tracing::warn!("Resampler partial flush error: {e}");
                        self.last_error = Some(e.to_string());
                        self.track_end_frame = Some(self.frames_written);
                        return false;
                    }
                };

                self.resample_in_buf.clear();
                self.flushing_zeros_remaining = resampler.output_delay();
                self.filter_flushed = true;

                let mut out_start_frame = 0;
                let mut out_frames = produced_frames;
                if self.resample_delay_to_trim > 0 {
                    if out_frames <= self.resample_delay_to_trim {
                        self.resample_delay_to_trim -= out_frames;
                        out_frames = 0;
                    } else {
                        out_start_frame = self.resample_delay_to_trim;
                        out_frames -= self.resample_delay_to_trim;
                        self.resample_delay_to_trim = 0;
                    }
                }

                self.resample_out_offset = out_start_frame * 2;
                self.resample_out_len = (out_start_frame + out_frames) * 2;
                continue;
            }

            // 5. Source reached EOF & input buffer is empty: flush filter delay tail
            if self.source_end_of_stream && !self.filter_flushed {
                self.filter_flushed = true;
                self.flushing_zeros_remaining = resampler.output_delay();
            }

            if self.source_end_of_stream && self.flushing_zeros_remaining > 0 {
                self.resample_in_buf.resize(needed_samples, 0.0);

                let max_out_frames = resampler.output_frames_max();
                if self.resample_out_buf.len() < max_out_frames * 2 {
                    self.resample_out_buf.resize(max_out_frames * 2, 0.0);
                }

                let (_consumed, produced_frames) = match resampler.process_into_buffer(
                    &self.resample_in_buf[..needed_samples],
                    &mut self.resample_out_buf,
                    Some(0),
                ) {
                    Ok(res) => res,
                    Err(e) => {
                        tracing::warn!("Resampler zero flush error: {e}");
                        self.last_error = Some(e.to_string());
                        self.track_end_frame = Some(self.frames_written);
                        return false;
                    }
                };

                self.resample_in_buf.clear();

                let frames_to_take = produced_frames.min(self.flushing_zeros_remaining);
                self.flushing_zeros_remaining =
                    self.flushing_zeros_remaining.saturating_sub(frames_to_take);

                self.resample_out_offset = 0;
                self.resample_out_len = frames_to_take * 2;
                continue;
            }

            // 6. Complete
            if self.source_end_of_stream
                && self.resample_in_buf.is_empty()
                && self.flushing_zeros_remaining == 0
                && self.resample_out_offset >= self.resample_out_len
            {
                if self.track_end_frame.is_none() {
                    self.track_end_frame = Some(self.frames_written);
                    debug!(
                        "Resampled track reached end of stream at device frame {}",
                        self.frames_written
                    );
                }
                break;
            }

            break;
        }

        !self.is_track_ended()
    }

    /// Checks if the entire track has finished playing out through the sink.
    pub fn is_track_ended(&self) -> bool {
        if let Some(end_frame) = self.track_end_frame {
            let consumed = self.shared.frames_consumed.load(Ordering::Relaxed);
            consumed >= end_frame
        } else {
            false
        }
    }

    /// Gets current playback position based on device frames consumed.
    pub fn position(&self) -> Duration {
        let consumed = self.shared.frames_consumed.load(Ordering::Relaxed);
        let dev_rate = self.sink.sample_rate();
        if dev_rate == 0 {
            return Duration::ZERO;
        }

        Duration::from_secs_f64(consumed as f64 / dev_rate as f64)
    }

    /// Seeks to a given duration in the current track.
    pub fn seek(&mut self, target: Duration) -> Result<()> {
        self.flush();
        if let Some(ref mut source) = self.current_source {
            source.seek(target)?;
            if let Some(ref mut resampler) = self.current_resampler {
                resampler.reset();
                self.resample_delay_to_trim = resampler.output_delay();
            }
            self.resample_in_buf.clear();
            self.resample_out_offset = 0;
            self.resample_out_len = 0;
            self.flushing_zeros_remaining = 0;
            self.filter_flushed = false;
            self.source_end_of_stream = false;

            let dev_rate = self.sink.sample_rate();
            let new_frame = (target.as_secs_f64() * dev_rate as f64) as u64;
            self.frames_written = new_frame;
            self.shared
                .frames_consumed
                .store(new_frame, Ordering::Relaxed);
            self.track_end_frame = None;

            // Refill buffer after seek
            self.fill_buffer();
        }
        Ok(())
    }

    /// Flushes currently buffered audio.
    pub fn flush(&mut self) {
        // Drop any unread items from producer by letting the sink consume or recreating
        // In rtrb, we can abandon/drain if needed, or sink will consume silence while paused
    }

    pub fn play(&mut self) -> Result<()> {
        self.sink.play()
    }

    pub fn pause(&mut self) -> Result<()> {
        self.sink.pause()
    }

    /// Sets volume: position `p` in 0.0..=1.0 maps to quadratic gain `p * p`.
    pub fn set_volume(&self, position: f32) {
        let p = position.clamp(0.0, 1.0);
        let gain = p * p;
        self.shared.set_volume_gain(gain);
    }

    pub fn peak_left(&self) -> f32 {
        self.shared.peak_left()
    }

    pub fn peak_right(&self) -> f32 {
        self.shared.peak_right()
    }

    pub fn underruns(&self) -> u64 {
        self.shared.underruns.load(Ordering::Relaxed)
    }

    pub fn current_metadata(&self) -> Option<&AudioMetadata> {
        self.current_source.as_ref().map(|s| s.metadata())
    }

    pub fn sink_sample_rate(&self) -> u32 {
        self.sink.sample_rate()
    }

    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    pub fn frames_written(&self) -> u64 {
        self.frames_written
    }
}

/// Helper wrapper so Arc<NullSink> can be boxed as dyn AudioSink.
struct NullSinkWrapper(Arc<NullSink>);

impl AudioSink for NullSinkWrapper {
    fn sample_rate(&self) -> u32 {
        self.0.sample_rate()
    }

    fn channels(&self) -> u16 {
        self.0.channels()
    }

    fn play(&self) -> Result<()> {
        self.0.play()
    }

    fn pause(&self) -> Result<()> {
        self.0.pause()
    }
}
