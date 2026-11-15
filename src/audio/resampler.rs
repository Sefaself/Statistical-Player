//! Audio resampler using `rubato`.
//!
//! Provides sample-rate conversion between audio source rates (e.g. 44.1 kHz, 48 kHz)
//! and output device rates (e.g. 44.1 kHz, 48 kHz, 192 kHz).
//! When the source rate matches the target rate, samples pass through bit-identical.

use anyhow::Result;
use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::audioadapter_buffers::owned::InterleavedOwned;
use rubato::{Fft, FixedSync, Resampler};

/// Audio sample-rate converter for interleaved stereo f32 audio.
pub struct AudioResampler {
    source_rate: u32,
    target_rate: u32,
    channels: usize,
    inner: Option<Fft<f32>>,
}

impl AudioResampler {
    /// Creates a new resampler.
    /// If `source_rate == target_rate`, no FFT resampler is allocated and audio passes through untouched.
    pub fn new(source_rate: u32, target_rate: u32, channels: usize) -> Result<Self> {
        let inner = if source_rate == target_rate {
            None
        } else {
            // 1024 frames provides low delay with high anti-aliasing cutoff
            let resampler = Fft::<f32>::new(
                source_rate as usize,
                target_rate as usize,
                1024,
                channels,
                FixedSync::Both,
            )
            .map_err(|e| anyhow::anyhow!("Failed to construct rubato Fft resampler: {e}"))?;
            Some(resampler)
        };

        Ok(Self {
            source_rate,
            target_rate,
            channels,
            inner,
        })
    }

    pub fn source_rate(&self) -> u32 {
        self.source_rate
    }

    pub fn target_rate(&self) -> u32 {
        self.target_rate
    }

    pub fn channels(&self) -> usize {
        self.channels
    }

    pub fn is_passthrough(&self) -> bool {
        self.inner.is_none()
    }

    pub fn input_frames_next(&self) -> usize {
        self.inner
            .as_ref()
            .map(|r| r.input_frames_next())
            .unwrap_or(1024)
    }

    pub fn output_frames_max(&self) -> usize {
        self.inner
            .as_ref()
            .map(|r| r.output_frames_max())
            .unwrap_or(1024)
    }

    pub fn output_delay(&self) -> usize {
        self.inner.as_ref().map(|r| r.output_delay()).unwrap_or(0)
    }

    pub fn reset(&mut self) {
        if let Some(ref mut r) = self.inner {
            r.reset();
        }
    }

    /// Resamples an entire buffer of interleaved audio samples.
    ///
    /// When `source_rate == target_rate`, returns a clone of `input` bit-identically.
    /// Otherwise, runs the FFT resampler with delay trimming to produce clean output.
    pub fn resample_all(&mut self, input: &[f32]) -> Result<Vec<f32>> {
        if self.inner.is_none() {
            return Ok(input.to_vec());
        }

        let channels = self.channels;
        let input_frames = input.len() / channels;
        if input_frames == 0 {
            return Ok(Vec::new());
        }

        let resampler = self.inner.as_mut().unwrap();
        let input_adapter = InterleavedSlice::new(input, channels, input_frames)
            .map_err(|e| anyhow::anyhow!("Invalid input buffer dimensions: {e:?}"))?;

        let needed_frames = resampler.process_all_needed_output_len(input_frames);
        let mut output_buf = InterleavedOwned::<f32>::new(0.0, channels, needed_frames);

        let (_consumed, produced_frames) = resampler
            .process_all_into_buffer(&input_adapter, &mut output_buf, input_frames, None)
            .map_err(|e| anyhow::anyhow!("Resampling error: {e:?}"))?;

        let mut output_vec = output_buf.take_data();
        output_vec.truncate(produced_frames * channels);
        Ok(output_vec)
    }

    /// Resamples a chunk into a pre-allocated output slice without heap allocations.
    /// `partial_len` can be provided when the final input chunk has fewer than `input_frames_next()` valid frames.
    /// Returns `(frames_consumed, frames_produced)`.
    pub fn process_into_buffer(
        &mut self,
        input: &[f32],
        output: &mut [f32],
        partial_len: Option<usize>,
    ) -> Result<(usize, usize)> {
        let channels = self.channels;
        if let Some(ref mut resampler) = self.inner {
            let in_frames = input.len() / channels;
            let out_frames = output.len() / channels;
            let in_adapter = InterleavedSlice::new(input, channels, in_frames)
                .map_err(|e| anyhow::anyhow!("{e:?}"))?;
            let mut out_adapter = InterleavedSlice::new_mut(output, channels, out_frames)
                .map_err(|e| anyhow::anyhow!("{e:?}"))?;

            let indexing = if partial_len.is_some() {
                Some(rubato::Indexing {
                    input_offset: 0,
                    output_offset: 0,
                    active_channels_mask: None,
                    partial_len,
                })
            } else {
                None
            };

            let (c, p) = resampler
                .process_into_buffer(&in_adapter, &mut out_adapter, indexing.as_ref())
                .map_err(|e| anyhow::anyhow!("Resample error: {e:?}"))?;
            Ok((c, p))
        } else {
            let frames = (input.len() / channels).min(output.len() / channels);
            let samples = frames * channels;
            output[..samples].copy_from_slice(&input[..samples]);
            Ok((frames, frames))
        }
    }
}
