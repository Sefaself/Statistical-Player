//! Integration tests for the dedicated AudioThread.
//!
//! Verifies:
//! - Loading, playback, and reaching end-of-track (`track_ended`).
//! - Generation tracking and clearing stale `track_ended` on new Load or Seek.
//! - Seeking with timeline position updates.
//! - Pause, Play, and Load failure handling.

use std::path::PathBuf;
use std::time::Duration;

use statistical_player::audio::{AudioCommand, AudioEvent, AudioThread};

fn fixture_flac() -> PathBuf {
    PathBuf::from("tests/fixtures/flac_16bit.flac")
}

#[test]
fn test_audio_thread_load_play_reach_end() {
    let (audio, null_sink) =
        AudioThread::spawn_null(44100).expect("Failed to spawn null audio thread");

    let path = fixture_flac();
    assert!(path.exists(), "Fixture must exist");

    audio
        .send(AudioCommand::Load {
            path,
            generation: 1,
            start: Duration::ZERO,
            autoplay: true,
        })
        .expect("Failed to send Load command");

    // Expect Loaded event
    let event = audio
        .events()
        .recv_timeout(Duration::from_secs(2))
        .expect("Timed out waiting for Loaded event");

    match event {
        AudioEvent::Loaded {
            generation,
            metadata,
        } => {
            assert_eq!(generation, 1);
            assert_eq!(metadata.sample_rate, 44100);
        }
        other => panic!("Expected AudioEvent::Loaded, got {:?}", other),
    }

    assert_eq!(audio.status().generation(), 1);
    assert!(!audio.status().track_ended());

    // Step null sink until track finishes playing
    let start = std::time::Instant::now();
    while !audio.status().track_ended() {
        null_sink.step(1024);
        std::thread::sleep(Duration::from_millis(5));
        if start.elapsed() > Duration::from_secs(5) {
            panic!("Timed out waiting for track_ended");
        }
    }

    assert!(audio.status().track_ended());
    assert_eq!(audio.status().generation_and_track_ended(), (1, true));
}

#[test]
fn test_audio_thread_stale_generation_ignored() {
    let (audio, null_sink) =
        AudioThread::spawn_null(44100).expect("Failed to spawn null audio thread");

    let path = fixture_flac();

    // 1. Play generation 1 to completion
    audio
        .send(AudioCommand::Load {
            path: path.clone(),
            generation: 1,
            start: Duration::ZERO,
            autoplay: true,
        })
        .expect("Failed to send Load");

    let event = audio
        .events()
        .recv_timeout(Duration::from_secs(2))
        .expect("Timed out waiting for Loaded");
    assert!(matches!(event, AudioEvent::Loaded { generation: 1, .. }));

    let start = std::time::Instant::now();
    while !audio.status().track_ended() {
        null_sink.step(1024);
        std::thread::sleep(Duration::from_millis(5));
        if start.elapsed() > Duration::from_secs(5) {
            panic!("Timed out waiting for track_ended");
        }
    }
    assert_eq!(audio.status().generation_and_track_ended(), (1, true));

    // 2. Load generation 2: track_ended must be immediately cleared and generation updated
    audio
        .send(AudioCommand::Load {
            path,
            generation: 2,
            start: Duration::ZERO,
            autoplay: true,
        })
        .expect("Failed to send Load generation 2");

    let event = audio
        .events()
        .recv_timeout(Duration::from_secs(2))
        .expect("Timed out waiting for Loaded generation 2");
    assert!(matches!(event, AudioEvent::Loaded { generation: 2, .. }));

    // Generation must be 2 and track_ended must NOT be carried over from generation 1
    let (generation, ended) = audio.status().generation_and_track_ended();
    assert_eq!(generation, 2);
    assert!(
        !ended,
        "track_ended must be cleared when a new track is loaded"
    );
}

#[test]
fn test_audio_thread_seek() {
    let (audio, null_sink) =
        AudioThread::spawn_null(44100).expect("Failed to spawn null audio thread");

    let path = fixture_flac();

    audio
        .send(AudioCommand::Load {
            path,
            generation: 1,
            start: Duration::ZERO,
            autoplay: true,
        })
        .expect("Failed to send Load");

    let event = audio
        .events()
        .recv_timeout(Duration::from_secs(2))
        .expect("Timed out waiting for Loaded");
    assert!(matches!(event, AudioEvent::Loaded { generation: 1, .. }));

    // Seek to 500ms
    audio
        .send(AudioCommand::Seek(Duration::from_millis(500)))
        .expect("Failed to send Seek");

    std::thread::sleep(Duration::from_millis(30));

    let pos = audio.status().position();
    assert!(
        pos >= Duration::from_millis(490) && pos <= Duration::from_millis(550),
        "Position after seek was {:?}",
        pos
    );
    assert!(!audio.status().track_ended());

    // Step null sink to discard stale buffered audio
    null_sink.step(1);

    // Wait for the audio thread (~100ms) to refill the emptied ring buffer
    std::thread::sleep(Duration::from_millis(120));

    // Advance sink by ~100ms (4410 frames) of newly refilled audio
    null_sink.step(4410);

    // Wait for the periodic audio thread tick (~100ms) to publish updated position
    let start_wait = std::time::Instant::now();
    let mut pos_after = audio.status().position();
    while pos_after < Duration::from_millis(580) {
        if start_wait.elapsed() > Duration::from_secs(2) {
            panic!(
                "Timed out waiting for position update; last was {:?}",
                pos_after
            );
        }
        std::thread::sleep(Duration::from_millis(15));
        pos_after = audio.status().position();
    }

    assert!(
        pos_after >= Duration::from_millis(580) && pos_after <= Duration::from_millis(650),
        "Position after step was {:?}",
        pos_after
    );
}

#[test]
fn test_audio_thread_pause_and_resume() {
    let (audio, _null_sink) =
        AudioThread::spawn_null(44100).expect("Failed to spawn null audio thread");

    let path = fixture_flac();

    audio
        .send(AudioCommand::Load {
            path,
            generation: 1,
            start: Duration::ZERO,
            autoplay: true,
        })
        .expect("Failed to send Load");

    let _ = audio
        .events()
        .recv_timeout(Duration::from_secs(2))
        .expect("Timed out waiting for Loaded");

    assert!(!audio.status().paused());

    // Pause
    audio
        .send(AudioCommand::Pause)
        .expect("Failed to send Pause");
    std::thread::sleep(Duration::from_millis(20));
    assert!(audio.status().paused());

    // Resume
    audio.send(AudioCommand::Play).expect("Failed to send Play");
    std::thread::sleep(Duration::from_millis(20));
    assert!(!audio.status().paused());
}

#[test]
fn test_audio_thread_load_failed() {
    let (audio, _null_sink) =
        AudioThread::spawn_null(44100).expect("Failed to spawn null audio thread");

    let bad_path = PathBuf::from("tests/fixtures/non_existent_file.flac");

    audio
        .send(AudioCommand::Load {
            path: bad_path,
            generation: 7,
            start: Duration::ZERO,
            autoplay: true,
        })
        .expect("Failed to send Load");

    let event = audio
        .events()
        .recv_timeout(Duration::from_secs(2))
        .expect("Timed out waiting for LoadFailed");

    match event {
        AudioEvent::LoadFailed { generation, error } => {
            assert_eq!(generation, 7);
            assert!(!error.is_empty());
        }
        other => panic!("Expected LoadFailed, got {:?}", other),
    }

    assert!(!audio.status().track_ended());
}
