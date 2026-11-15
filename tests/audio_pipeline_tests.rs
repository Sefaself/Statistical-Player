//! Automated tests for the audio engine, Symphonia source decoding, and NullSink.

use std::path::PathBuf;
use std::time::Duration;

use statistical_player::audio::AudioEngine;
use statistical_player::audio::AudioSource;

fn fixture_path() -> PathBuf {
    PathBuf::from("tests/fixtures/wav_16bit.wav")
}

#[test]
fn test_audio_source_probe_and_read() {
    let wav_path = fixture_path();
    assert!(wav_path.exists(), "Expected fixture wav_16bit.wav to exist");

    let mut source = AudioSource::open(&wav_path).expect("Failed to open AudioSource");
    let meta = source.metadata();

    assert_eq!(meta.sample_rate, 44100);
    assert_eq!(meta.channels, 2);
    assert!(meta.duration.is_some());

    let mut buffer = vec![0.0_f32; 1024];
    let read = source.read_stereo(&mut buffer).expect("read_stereo failed");

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

#[test]
fn test_ogg_vorbis_pipeline_decoding() {
    for fixture in &["vorbis_44100.ogg", "vorbis_48000.ogg"] {
        let ogg_path = PathBuf::from("tests/fixtures").join(fixture);
        let (mut engine, _null_sink) = AudioEngine::new_null(44100);

        let meta = engine
            .load_file(&ogg_path)
            .unwrap_or_else(|e| panic!("Failed to load {fixture} into engine: {e}"));

        assert_eq!(meta.codec_name, "Vorbis");
        assert!(
            engine.frames_written() > 0,
            "{fixture} wrote 0 frames into engine"
        );

        let mut buf = vec![0.0_f32; 1024];
        let mut source = AudioSource::open(&ogg_path)
            .unwrap_or_else(|e| panic!("Failed to open AudioSource for {fixture}: {e}"));
        let read = source
            .read_stereo(&mut buf)
            .unwrap_or_else(|e| panic!("read_stereo failed for {fixture}: {e}"));
        assert!(
            read > 0,
            "AudioSource::read_stereo produced 0 frames for {fixture}"
        );
    }
}

#[test]
fn test_engine_resampling_output_frame_counts() {
    let fixture = PathBuf::from("tests/fixtures/flac_16bit.flac");
    assert!(fixture.exists(), "flac_16bit.flac fixture must exist");

    for &dev_rate in &[44100_u32, 48000_u32, 96000_u32] {
        let (mut engine, null_sink) = AudioEngine::new_null(dev_rate);
        let meta = engine
            .load_file(&fixture)
            .unwrap_or_else(|e| panic!("Failed to load fixture at {dev_rate} Hz: {e}"));

        let duration = meta
            .duration
            .expect("Fixture must have known duration")
            .as_secs_f64();
        let expected_frames = duration * dev_rate as f64;

        // Play to completion using null sink
        let mut loop_count = 0;
        while !engine.is_track_ended() {
            engine.fill_buffer();
            null_sink.step(1024);
            loop_count += 1;
            assert!(
                loop_count < 100_000,
                "Playback loop exceeded safety limit without ending at {dev_rate} Hz"
            );
        }

        let output_frames = engine.frames_written() as f64;
        let diff = (output_frames - expected_frames).abs() / expected_frames;
        assert!(
            diff < 0.01,
            "At {dev_rate} Hz: expected ~{expected_frames:.0} frames, got {output_frames:.0} (diff: {:.2}%)",
            diff * 100.0
        );
    }
}
