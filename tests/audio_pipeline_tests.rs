//! Automated tests for the audio engine, Symphonia source decoding, and NullSink.

use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use statistical_player::audio::AudioEngine;
use statistical_player::audio::AudioSource;

/// Generates a valid 16-bit stereo 44.1 kHz PCM WAV file with a 440 Hz sine tone.
fn generate_wav_fixture(path: &Path, duration_secs: f32) {
    let sample_rate: u32 = 44100;
    let channels: u16 = 2;
    let bits_per_sample: u16 = 16;
    let num_frames = (sample_rate as f32 * duration_secs) as u32;
    let data_len = num_frames * channels as u32 * (bits_per_sample as u32 / 8);

    let mut file = File::create(path).expect("Failed to create wav fixture");

    // RIFF header
    file.write_all(b"RIFF").unwrap();
    file.write_all(&(36 + data_len).to_le_bytes()).unwrap();
    file.write_all(b"WAVE").unwrap();

    // fmt subchunk
    file.write_all(b"fmt ").unwrap();
    file.write_all(&16_u32.to_le_bytes()).unwrap(); // Subchunk1Size (16 for PCM)
    file.write_all(&1_u16.to_le_bytes()).unwrap(); // AudioFormat (1 for PCM)
    file.write_all(&channels.to_le_bytes()).unwrap();
    file.write_all(&sample_rate.to_le_bytes()).unwrap();
    let byte_rate = sample_rate * channels as u32 * (bits_per_sample as u32 / 8);
    file.write_all(&byte_rate.to_le_bytes()).unwrap();
    let block_align = channels * (bits_per_sample / 8);
    file.write_all(&block_align.to_le_bytes()).unwrap();
    file.write_all(&bits_per_sample.to_le_bytes()).unwrap();

    // data subchunk
    file.write_all(b"data").unwrap();
    file.write_all(&data_len.to_le_bytes()).unwrap();

    // Generate stereo sine samples (Left: 440 Hz, Right: 880 Hz)
    for frame in 0..num_frames {
        let t = frame as f32 / sample_rate as f32;
        let left_sample = (t * 440.0 * 2.0 * std::f32::consts::PI).sin();
        let right_sample = (t * 880.0 * 2.0 * std::f32::consts::PI).sin();

        let left_i16 = (left_sample * 30000.0) as i16;
        let right_i16 = (right_sample * 30000.0) as i16;

        file.write_all(&left_i16.to_le_bytes()).unwrap();
        file.write_all(&right_i16.to_le_bytes()).unwrap();
    }
}

fn fixture_path() -> PathBuf {
    let dir = PathBuf::from("tests/fixtures");
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("test_sine_44100.wav")
}

#[test]
fn test_audio_source_probe_and_read() {
    let wav_path = fixture_path();
    generate_wav_fixture(&wav_path, 1.0);

    let mut source = AudioSource::open(&wav_path).expect("Failed to open AudioSource");
    let meta = source.metadata();

    assert_eq!(meta.sample_rate, 44100);
    assert_eq!(meta.channels, 2);
    assert!(meta.duration.is_some());

    let mut buffer = vec![0.0_f32; 1024];
    let read = source.read_stereo(&mut buffer);

    assert!(read > 0);
    assert_eq!(read % 2, 0, "Stereo samples must be read in pairs");

    // Verify samples are non-zero
    let has_non_zero = buffer[..read].iter().any(|&s| s.abs() > 0.01);
    assert!(
        has_non_zero,
        "Expected decoded audio samples to be non-zero"
    );
}

#[test]
fn test_audio_engine_with_null_sink() {
    let wav_path = fixture_path();
    generate_wav_fixture(&wav_path, 1.0);

    let (mut engine, null_sink) = AudioEngine::new_null(44100);

    let meta = engine
        .load_file(&wav_path)
        .expect("Failed to load file into engine");
    assert_eq!(meta.sample_rate, 44100);

    // Initial position is 0
    assert_eq!(engine.position(), Duration::ZERO);

    // Step null sink forward by 4410 frames (~100ms)
    let stepped = null_sink.step(4410);
    assert_eq!(stepped, 4410);

    // Position advances accurately
    let pos = engine.position();
    assert!(
        pos.as_millis() >= 90 && pos.as_millis() <= 110,
        "Position was {:?}",
        pos
    );

    // Peak levels reflect non-zero playback
    assert!(engine.peak_left() > 0.0);
    assert!(engine.peak_right() > 0.0);

    // Volume quadratic mapping: 0.5 slider position -> 0.25 gain
    engine.set_volume(0.5);
    let shared = null_sink.shared();
    let gain = shared.volume_gain();
    assert!((gain - 0.25).abs() < 1e-4, "Gain was {}", gain);

    // Test seek
    engine
        .seek(Duration::from_millis(500))
        .expect("Seek failed");
    let seek_pos = engine.position();
    assert!(seek_pos.as_millis() >= 490 && seek_pos.as_millis() <= 510);
}
