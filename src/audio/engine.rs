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

/// Ring buffer duration in seconds (1.5 s).
pub const RING_BUFFER_DURATION_SECS: f64 = 1.5;

/// Calculates ring buffer capacity in stereo samples for 1.5 seconds at the given device rate:
/// 1.5 s * device_rate * 2 channels.
pub fn ring_buffer_capacity(sample_rate: u32) -> usize {
    (RING_BUFFER_DURATION_SECS * sample_rate as f64 * 2.0).ceil() as usize
}

/// Fixed hardware drain tail duration (200 ms) to ensure the device DAC and OS audio mixer
/// finish playing all buffered samples before declaring the track ended.
pub const DRAIN_TAIL_DURATION: Duration = Duration::from_millis(200);

/// Top-level audio engine.
pub struct AudioEngine {
    sink: Box<dyn AudioSink>,
    shared: Arc<CallbackShared>,
    producer: rtrb::Producer<f32>,
    current_source: Option<AudioSource>,
    current_resampler: Option<AudioResampler>,
    frames_written: u64,
    timeline_base_frame: u64,
    timeline_base_time: Duration,
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
        let shared = Arc::new(CallbackShared::default());
        let (sink, producer) = CpalSink::new(Arc::clone(&shared))?;

        info!(
            "Audio engine initialized with device sample rate: {} Hz (ring buffer capacity: {} samples / 1.5s)",
            sink.sample_rate(),
            producer.slots()
        );

        Ok(Self {
            sink: Box::new(sink),
            shared,
            producer,
            current_source: None,
            current_resampler: None,
            frames_written: 0,
            timeline_base_frame: 0,
            timeline_base_time: Duration::ZERO,
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
        let capacity = ring_buffer_capacity(sample_rate);
        let (producer, consumer) = rtrb::RingBuffer::new(capacity);
        let shared = Arc::new(CallbackShared::default());
        let null_sink = Arc::new(NullSink::new(consumer, Arc::clone(&shared), sample_rate));

        let engine = Self {
            sink: Box::new(NullSinkWrapper(Arc::clone(&null_sink))),
            shared,
            producer,
            current_source: None,
            current_resampler: None,
            frames_written: 0,
            timeline_base_frame: 0,
            timeline_base_time: Duration::ZERO,
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

        self.timeline_base_frame = self.frames_written;
        self.timeline_base_time = Duration::ZERO;
        self.current_source = Some(source);
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

    fn mark_end_of_track(&mut self) {
        if self.track_end_frame.is_none() {
            let dev_rate = self.sink.sample_rate();
            let drain_frames = (dev_rate as f64 * DRAIN_TAIL_DURATION.as_secs_f64()).ceil() as u64;
            self.shared
                .end_of_audio_frame
                .store(self.frames_written, Ordering::Release);
            self.track_end_frame = Some(self.frames_written + drain_frames);
            debug!(
                "Track reached end of stream at audio frame {}, drain target frame {}",
                self.frames_written,
                self.frames_written + drain_frames
            );
        }
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
                        self.mark_end_of_track();
                        break;
                    }
                };

                if read == 0 {
                    if source.is_end_of_stream() {
                        self.mark_end_of_track();
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
                        self.mark_end_of_track();
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
                        self.mark_end_of_track();
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
                        self.mark_end_of_track();
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
                        self.mark_end_of_track();
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
                self.mark_end_of_track();
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

    /// Gets current playback position based on device frames consumed and timeline base.
    pub fn position(&self) -> Duration {
        let consumed = self.shared.frames_consumed.load(Ordering::Relaxed);
        let end_of_audio = self.shared.end_of_audio_frame.load(Ordering::Relaxed);
        let effective_consumed = consumed.min(end_of_audio);
        let dev_rate = self.sink.sample_rate();
        if dev_rate == 0 {
            return Duration::ZERO;
        }

        let pos = if effective_consumed <= self.timeline_base_frame {
            self.timeline_base_time
        } else {
            let elapsed_frames = effective_consumed - self.timeline_base_frame;
            self.timeline_base_time
                + Duration::from_secs_f64(elapsed_frames as f64 / dev_rate as f64)
        };

        if let Some(duration) = self
            .current_source
            .as_ref()
            .and_then(|s| s.metadata().duration)
        {
            pos.min(duration)
        } else {
            pos
        }
    }

    /// Seeks to a given duration in the current track.
    pub fn seek(&mut self, target: Duration) -> Result<()> {
        self.flush();
        if let Some(ref mut source) = self.current_source {
            source.seek(target)?;
            self.timeline_base_frame = self.frames_written;
            self.timeline_base_time = target;

            // Refill buffer after seek
            self.fill_buffer();
        }
        Ok(())
    }

    /// Flushes currently buffered audio and signals the audio callback to discard old samples.
    pub fn flush(&mut self) {
        // Publish discard target so the callback drops any buffered unplayed samples
        self.shared
            .discard_target
            .store(self.frames_written, Ordering::Release);
        self.shared
            .end_of_audio_frame
            .store(u64::MAX, Ordering::Release);

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
        self.track_end_frame = None;
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

    pub fn track_end_frame(&self) -> Option<u64> {
        self.track_end_frame
    }

    pub fn end_of_audio_frame(&self) -> Option<u64> {
        let val = self.shared.end_of_audio_frame.load(Ordering::Relaxed);
        if val == u64::MAX { None } else { Some(val) }
    }

    /// Exposes the shared atomics between the engine and audio callback.
    pub fn shared(&self) -> &Arc<CallbackShared> {
        &self.shared
    }

    /// Returns true if the ring buffer is less than half full.
    pub fn is_buffer_below_half(&self) -> bool {
        let capacity = ring_buffer_capacity(self.sink.sample_rate());
        self.producer.slots() >= capacity / 2
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
