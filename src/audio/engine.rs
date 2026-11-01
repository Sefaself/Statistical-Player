//! Audio engine coordinating decoding, buffering, volume, and playback.

use anyhow::Result;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;
use tracing::{debug, info};

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
    frames_written: u64,
    track_end_frame: Option<u64>,
    scratch_buf: Vec<f32>,
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
            frames_written: 0,
            track_end_frame: None,
            scratch_buf: vec![0.0_f32; 4096],
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
            frames_written: 0,
            track_end_frame: None,
            scratch_buf: vec![0.0_f32; 4096],
        };

        (engine, null_sink)
    }

    /// Loads and opens a new track from disk.
    pub fn load_file(&mut self, path: impl AsRef<Path>) -> Result<AudioMetadata> {
        let source = AudioSource::open(path)?;
        let metadata = source.metadata().clone();

        self.flush();
        self.current_source = Some(source);
        self.frames_written = 0;
        self.shared.frames_consumed.store(0, Ordering::Relaxed);
        self.track_end_frame = None;

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

        while self.producer.slots() >= 2 {
            // Read available slots up to scratch buffer capacity
            let slots = self.producer.slots();
            let to_read = (slots - (slots % 2)).min(self.scratch_buf.len());
            if to_read == 0 {
                break;
            }

            let read = source.read_stereo(&mut self.scratch_buf[..to_read]);
            if read == 0 {
                // End of track reached
                if source.is_end_of_stream() && self.track_end_frame.is_none() {
                    self.track_end_frame = Some(self.frames_written);
                    debug!(
                        "Source reached end of stream at frame {}",
                        self.frames_written
                    );
                }
                break;
            }

            // Push into ring buffer
            for &sample in &self.scratch_buf[..read] {
                let _ = self.producer.push(sample);
            }

            self.frames_written += (read / 2) as u64;
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

    /// Gets current playback position in source time based on frames consumed.
    pub fn position(&self) -> Duration {
        let consumed = self.shared.frames_consumed.load(Ordering::Relaxed);
        let sample_rate = self
            .current_source
            .as_ref()
            .map(|s| s.sample_rate())
            .unwrap_or_else(|| self.sink.sample_rate());

        Duration::from_secs_f64(consumed as f64 / sample_rate as f64)
    }

    /// Seeks to a given duration in the current track.
    pub fn seek(&mut self, target: Duration) -> Result<()> {
        self.flush();
        if let Some(ref mut source) = self.current_source {
            source.seek(target)?;
            let sample_rate = source.sample_rate();
            let new_frame = (target.as_secs_f64() * sample_rate as f64) as u64;
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
