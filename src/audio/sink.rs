//! Audio sinks: CpalSink for real-time hardware output and NullSink for tests/CI.
//!
//! Enforces real-time audio safety:
//! - Zero heap allocations
//! - Zero locks
//! - Zero logging
//! - Zero panics

use anyhow::{Context, Result, anyhow};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream, StreamConfig};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

/// Real-time safe state shared between player core and audio callback.
#[derive(Debug)]
pub struct CallbackShared {
    /// Total sample frames consumed monotonically by the audio sink.
    pub frames_consumed: AtomicU64,
    /// Discard target frame for lock-free flush on seek/track change.
    pub discard_target: AtomicU64,
    /// Linear volume gain as f32 bits.
    pub volume_target_bits: AtomicU32,
    /// Counter of underrun occurrences.
    pub underruns: AtomicU64,
    /// Peak amplitude of Left channel in current block (f32 bits).
    pub peak_l_bits: AtomicU32,
    /// Peak amplitude of Right channel in current block (f32 bits).
    pub peak_r_bits: AtomicU32,
    /// Whether output is currently paused (fading to silence).
    pub paused: AtomicBool,
}

impl Default for CallbackShared {
    fn default() -> Self {
        Self {
            frames_consumed: AtomicU64::new(0),
            discard_target: AtomicU64::new(0),
            volume_target_bits: AtomicU32::new(1.0_f32.to_bits()),
            underruns: AtomicU64::new(0),
            peak_l_bits: AtomicU32::new(0.0_f32.to_bits()),
            peak_r_bits: AtomicU32::new(0.0_f32.to_bits()),
            paused: AtomicBool::new(false),
        }
    }
}

impl CallbackShared {
    pub fn volume_gain(&self) -> f32 {
        f32::from_bits(self.volume_target_bits.load(Ordering::Relaxed))
    }

    pub fn set_volume_gain(&self, gain: f32) {
        self.volume_target_bits
            .store(gain.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }

    pub fn peak_left(&self) -> f32 {
        f32::from_bits(self.peak_l_bits.load(Ordering::Relaxed))
    }

    pub fn peak_right(&self) -> f32 {
        f32::from_bits(self.peak_r_bits.load(Ordering::Relaxed))
    }
}

/// Common trait for audio output sinks.
pub trait AudioSink: Send + 'static {
    fn sample_rate(&self) -> u32;
    fn channels(&self) -> u16;
    fn play(&self) -> Result<()>;
    fn pause(&self) -> Result<()>;
}

/// Real hardware audio sink using CPAL.
pub struct CpalSink {
    _stream: Stream,
    sample_rate: u32,
    channels: u16,
    shared: Arc<CallbackShared>,
}

impl CpalSink {
    /// Opens the system default output device, creates a 1.5s ring buffer for the device rate, and builds an output stream.
    pub fn new(shared: Arc<CallbackShared>) -> Result<(Self, rtrb::Producer<f32>)> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| anyhow!("No audio output device found"))?;

        let default_config = device
            .default_output_config()
            .context("Failed to get default output audio config")?;

        let sample_rate = default_config.sample_rate();
        let channels = default_config.channels();
        let sample_format = default_config.sample_format();
        let stream_config: StreamConfig = default_config.into();

        // 1.5 s * device rate * 2 channels
        let ring_capacity = (1.5 * sample_rate as f64 * 2.0).ceil() as usize;
        let (producer, mut consumer) = rtrb::RingBuffer::new(ring_capacity);

        let shared_cb = Arc::clone(&shared);

        // Internal volume state inside callback to ramp smoothly over ~10ms
        let mut current_gain = shared.volume_gain();
        let ramp_rate = 1.0 / (sample_rate as f32 * 0.010); // ~10 ms ramp

        let err_fn = move |_err| {
            // Callback real-time rule: do not log or panic inside callback thread
        };

        let stream = match sample_format {
            SampleFormat::F32 => device.build_output_stream(
                stream_config,
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    render_callback_f32(
                        data,
                        &mut consumer,
                        &shared_cb,
                        &mut current_gain,
                        ramp_rate,
                    );
                },
                err_fn,
                None,
            )?,
            SampleFormat::I16 => device.build_output_stream(
                stream_config,
                move |data: &mut [i16], _: &cpal::OutputCallbackInfo| {
                    render_callback_i16(
                        data,
                        &mut consumer,
                        &shared_cb,
                        &mut current_gain,
                        ramp_rate,
                    );
                },
                err_fn,
                None,
            )?,
            SampleFormat::I32 => device.build_output_stream(
                stream_config,
                move |data: &mut [i32], _: &cpal::OutputCallbackInfo| {
                    render_callback_i32(
                        data,
                        &mut consumer,
                        &shared_cb,
                        &mut current_gain,
                        ramp_rate,
                    );
                },
                err_fn,
                None,
            )?,
            other => return Err(anyhow!("Unsupported sample format: {:?}", other)),
        };

        stream
            .play()
            .context("Failed to start audio output stream")?;

        Ok((
            Self {
                _stream: stream,
                sample_rate,
                channels,
                shared,
            },
            producer,
        ))
    }

    pub fn shared(&self) -> &Arc<CallbackShared> {
        &self.shared
    }
}

impl AudioSink for CpalSink {
    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn channels(&self) -> u16 {
        self.channels
    }

    fn play(&self) -> Result<()> {
        self.shared.paused.store(false, Ordering::Relaxed);
        Ok(())
    }

    fn pause(&self) -> Result<()> {
        self.shared.paused.store(true, Ordering::Relaxed);
        Ok(())
    }
}

#[inline(always)]
fn drain_discard(consumer: &mut rtrb::Consumer<f32>, shared: &CallbackShared) {
    let discard_target = shared.discard_target.load(Ordering::Relaxed);
    let mut current_consumed = shared.frames_consumed.load(Ordering::Relaxed);

    while current_consumed < discard_target {
        let l = consumer.pop();
        let r = consumer.pop();
        if l.is_ok() && r.is_ok() {
            current_consumed += 1;
        } else {
            current_consumed = discard_target;
            break;
        }
    }
    shared
        .frames_consumed
        .store(current_consumed, Ordering::Relaxed);
}

/// Real-time audio render callback for `f32` buffers.
#[inline(always)]
pub fn render_callback_f32(
    data: &mut [f32],
    consumer: &mut rtrb::Consumer<f32>,
    shared: &CallbackShared,
    current_gain: &mut f32,
    ramp_rate: f32,
) {
    drain_discard(consumer, shared);

    let target_gain = if shared.paused.load(Ordering::Relaxed) {
        0.0
    } else {
        f32::from_bits(shared.volume_target_bits.load(Ordering::Relaxed))
    };

    let frames = data.len() / 2;
    let mut block_peak_l = 0.0_f32;
    let mut block_peak_r = 0.0_f32;
    let mut frames_rendered = 0_u64;

    for i in 0..frames {
        // Ramp volume smoothly
        if *current_gain < target_gain {
            *current_gain = (*current_gain + ramp_rate).min(target_gain);
        } else if *current_gain > target_gain {
            *current_gain = (*current_gain - ramp_rate).max(target_gain);
        }

        let l_sample = consumer.pop();
        let r_sample = consumer.pop();

        if let (Ok(l), Ok(r)) = (l_sample, r_sample) {
            let out_l = l * *current_gain;
            let out_r = r * *current_gain;

            let abs_l = out_l.abs();
            let abs_r = out_r.abs();
            if abs_l > block_peak_l {
                block_peak_l = abs_l;
            }
            if abs_r > block_peak_r {
                block_peak_r = abs_r;
            }

            data[i * 2] = out_l;
            data[i * 2 + 1] = out_r;
            frames_rendered += 1;
        } else {
            // Buffer empty: silence and count underrun
            data[i * 2] = 0.0;
            data[i * 2 + 1] = 0.0;
            shared.underruns.fetch_add(1, Ordering::Relaxed);
        }
    }

    shared
        .peak_l_bits
        .store(block_peak_l.to_bits(), Ordering::Relaxed);
    shared
        .peak_r_bits
        .store(block_peak_r.to_bits(), Ordering::Relaxed);
    shared
        .frames_consumed
        .fetch_add(frames_rendered, Ordering::Relaxed);
}

/// Real-time audio render callback for `i16` buffers.
#[inline(always)]
pub fn render_callback_i16(
    data: &mut [i16],
    consumer: &mut rtrb::Consumer<f32>,
    shared: &CallbackShared,
    current_gain: &mut f32,
    ramp_rate: f32,
) {
    drain_discard(consumer, shared);

    let target_gain = if shared.paused.load(Ordering::Relaxed) {
        0.0
    } else {
        f32::from_bits(shared.volume_target_bits.load(Ordering::Relaxed))
    };

    let frames = data.len() / 2;
    let mut block_peak_l = 0.0_f32;
    let mut block_peak_r = 0.0_f32;
    let mut frames_rendered = 0_u64;

    for i in 0..frames {
        if *current_gain < target_gain {
            *current_gain = (*current_gain + ramp_rate).min(target_gain);
        } else if *current_gain > target_gain {
            *current_gain = (*current_gain - ramp_rate).max(target_gain);
        }

        let l_sample = consumer.pop();
        let r_sample = consumer.pop();

        if let (Ok(l), Ok(r)) = (l_sample, r_sample) {
            let out_l = l * *current_gain;
            let out_r = r * *current_gain;

            let abs_l = out_l.abs();
            let abs_r = out_r.abs();
            if abs_l > block_peak_l {
                block_peak_l = abs_l;
            }
            if abs_r > block_peak_r {
                block_peak_r = abs_r;
            }

            data[i * 2] = (out_l * 32767.0).clamp(-32768.0, 32767.0) as i16;
            data[i * 2 + 1] = (out_r * 32767.0).clamp(-32768.0, 32767.0) as i16;
            frames_rendered += 1;
        } else {
            data[i * 2] = 0;
            data[i * 2 + 1] = 0;
            shared.underruns.fetch_add(1, Ordering::Relaxed);
        }
    }

    shared
        .peak_l_bits
        .store(block_peak_l.to_bits(), Ordering::Relaxed);
    shared
        .peak_r_bits
        .store(block_peak_r.to_bits(), Ordering::Relaxed);
    shared
        .frames_consumed
        .fetch_add(frames_rendered, Ordering::Relaxed);
}

/// Real-time audio render callback for `i32` buffers.
#[inline(always)]
pub fn render_callback_i32(
    data: &mut [i32],
    consumer: &mut rtrb::Consumer<f32>,
    shared: &CallbackShared,
    current_gain: &mut f32,
    ramp_rate: f32,
) {
    drain_discard(consumer, shared);

    let target_gain = if shared.paused.load(Ordering::Relaxed) {
        0.0
    } else {
        f32::from_bits(shared.volume_target_bits.load(Ordering::Relaxed))
    };

    let frames = data.len() / 2;
    let mut block_peak_l = 0.0_f32;
    let mut block_peak_r = 0.0_f32;
    let mut frames_rendered = 0_u64;

    for i in 0..frames {
        if *current_gain < target_gain {
            *current_gain = (*current_gain + ramp_rate).min(target_gain);
        } else if *current_gain > target_gain {
            *current_gain = (*current_gain - ramp_rate).max(target_gain);
        }

        let l_sample = consumer.pop();
        let r_sample = consumer.pop();

        if let (Ok(l), Ok(r)) = (l_sample, r_sample) {
            let out_l = l * *current_gain;
            let out_r = r * *current_gain;

            let abs_l = out_l.abs();
            let abs_r = out_r.abs();
            if abs_l > block_peak_l {
                block_peak_l = abs_l;
            }
            if abs_r > block_peak_r {
                block_peak_r = abs_r;
            }

            data[i * 2] = (out_l * 2147483647.0).clamp(-2147483648.0, 2147483647.0) as i32;
            data[i * 2 + 1] = (out_r * 2147483647.0).clamp(-2147483648.0, 2147483647.0) as i32;
            frames_rendered += 1;
        } else {
            data[i * 2] = 0;
            data[i * 2 + 1] = 0;
            shared.underruns.fetch_add(1, Ordering::Relaxed);
        }
    }

    shared
        .peak_l_bits
        .store(block_peak_l.to_bits(), Ordering::Relaxed);
    shared
        .peak_r_bits
        .store(block_peak_r.to_bits(), Ordering::Relaxed);
    shared
        .frames_consumed
        .fetch_add(frames_rendered, Ordering::Relaxed);
}

/// Null audio sink for tests and headless CI environments.
pub struct NullSink {
    sample_rate: u32,
    channels: u16,
    consumer: std::sync::Mutex<Option<rtrb::Consumer<f32>>>,
    shared: Arc<CallbackShared>,
}

impl NullSink {
    pub fn new(
        consumer: rtrb::Consumer<f32>,
        shared: Arc<CallbackShared>,
        sample_rate: u32,
    ) -> Self {
        Self {
            sample_rate,
            channels: 2,
            consumer: std::sync::Mutex::new(Some(consumer)),
            shared,
        }
    }

    /// Manually consume `frames` from the ring buffer in tests.
    pub fn step(&self, frames: usize) -> usize {
        let mut guard = self.consumer.lock().unwrap();
        let Some(ref mut consumer) = *guard else {
            return 0;
        };

        drain_discard(consumer, &self.shared);

        let mut consumed = 0;
        let mut block_peak_l = 0.0_f32;
        let mut block_peak_r = 0.0_f32;

        for _ in 0..frames {
            let l = consumer.pop();
            let r = consumer.pop();
            if let (Ok(l), Ok(r)) = (l, r) {
                block_peak_l = block_peak_l.max(l.abs());
                block_peak_r = block_peak_r.max(r.abs());
                consumed += 1;
            } else {
                self.shared.underruns.fetch_add(1, Ordering::Relaxed);
                break;
            }
        }

        self.shared
            .peak_l_bits
            .store(block_peak_l.to_bits(), Ordering::Relaxed);
        self.shared
            .peak_r_bits
            .store(block_peak_r.to_bits(), Ordering::Relaxed);
        self.shared
            .frames_consumed
            .fetch_add(consumed as u64, Ordering::Relaxed);
        consumed
    }

    pub fn shared(&self) -> &Arc<CallbackShared> {
        &self.shared
    }
}

impl AudioSink for NullSink {
    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn channels(&self) -> u16 {
        self.channels
    }

    fn play(&self) -> Result<()> {
        self.shared.paused.store(false, Ordering::Relaxed);
        Ok(())
    }

    fn pause(&self) -> Result<()> {
        self.shared.paused.store(true, Ordering::Relaxed);
        Ok(())
    }
}
