//! Audio engine: decoder, ring buffers, resampler, output sink, and analysis taps.

pub mod engine;
pub mod sink;
pub mod source;

pub use engine::AudioEngine;
pub use sink::{AudioSink, CallbackShared, CpalSink, NullSink};
pub use source::{AudioMetadata, AudioSource};
