//! Integration tests for the PlayerCore thread.
//!
//! Verifies:
//! - 3-track queue auto-advance through completion.
//! - Repeat modes: Off (stops at boundary), Track (repeats same), Queue (wraps to start).
//! - Randomize queue preserves the currently playing track and earlier tracks.
//! - Auto-advance skips failed tracks (stopping after queue-length failures).
//! - Explicit play on a bad track stays Stopped with an error message.
//! - Stop halts playback and resets position while preserving the queue.
//! - Stale track_ended events from previous generations do not skip tracks.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use statistical_player::audio::AudioThread;
use statistical_player::core::{PlaybackState, PlayerCommand, PlayerCore, RepeatMode, TrackItem};

fn track_flac(id: u64) -> TrackItem {
    TrackItem::simple(id, PathBuf::from("tests/fixtures/flac_16bit.flac"))
}

fn track_wav(id: u64) -> TrackItem {
    TrackItem::simple(id, PathBuf::from("tests/fixtures/wav_16bit.wav"))
}

fn track_vorbis(id: u64) -> TrackItem {
    TrackItem::simple(id, PathBuf::from("tests/fixtures/vorbis_44100.ogg"))
}

fn track_bad(id: u64) -> TrackItem {
    TrackItem::simple(id, PathBuf::from("tests/fixtures/non_existent.flac"))
}

#[test]
fn test_core_queue_advances_and_stops_on_repeat_off() {
    let (audio, null_sink) = AudioThread::spawn_null(44100).expect("spawn_null failed");
    let wake_count = Arc::new(AtomicUsize::new(0));
    let wc = Arc::clone(&wake_count);
    let wake_ui = Box::new(move || {
        wc.fetch_add(1, Ordering::SeqCst);
    });

    let player = PlayerCore::spawn(audio, wake_ui);

    let queue = vec![track_flac(1), track_wav(2), track_vorbis(3)];
    player
        .send(PlayerCommand::ReplaceQueue(queue))
        .expect("ReplaceQueue failed");

    // Wait until track 0 is playing
    let start = std::time::Instant::now();
    while player.snapshot().current_index != Some(0)
        || player.snapshot().state != PlaybackState::Playing
    {
        if start.elapsed() > Duration::from_secs(3) {
            panic!(
                "Timed out waiting for track 0; state={:?}, idx={:?}",
                player.snapshot().state,
                player.snapshot().current_index
            );
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(player.snapshot().current_index, Some(0));

    // Play track 0 to end by stepping null sink
    while player.snapshot().current_index == Some(0) {
        null_sink.step(2048);
        std::thread::sleep(Duration::from_millis(5));
        if start.elapsed() > Duration::from_secs(6) {
            panic!("Timed out waiting for advance to track 1");
        }
    }

    // Auto-advanced to track 1
    assert_eq!(player.snapshot().current_index, Some(1));

    // Play track 1 to end
    let start_t1 = std::time::Instant::now();
    while player.snapshot().current_index == Some(1) {
        null_sink.step(2048);
        std::thread::sleep(Duration::from_millis(5));
        if start_t1.elapsed() > Duration::from_secs(6) {
            panic!("Timed out waiting for advance to track 2");
        }
    }

    // Auto-advanced to track 2
    assert_eq!(player.snapshot().current_index, Some(2));

    // Play track 2 to end
    let start_t2 = std::time::Instant::now();
    while player.snapshot().state == PlaybackState::Playing
        || player.snapshot().state == PlaybackState::Loading
    {
        null_sink.step(2048);
        std::thread::sleep(Duration::from_millis(5));
        if start_t2.elapsed() > Duration::from_secs(6) {
            panic!("Timed out waiting for queue to end");
        }
    }

    // With RepeatMode::Off, queue stops at the end
    assert_eq!(player.snapshot().state, PlaybackState::Stopped);
    assert_eq!(player.snapshot().current_index, Some(2));
    assert!(wake_count.load(Ordering::SeqCst) > 0);
}

#[test]
fn test_core_repeat_track() {
    let (audio, null_sink) = AudioThread::spawn_null(44100).expect("spawn_null failed");
    let player = PlayerCore::spawn(audio, Box::new(|| {}));

    player
        .send(PlayerCommand::SetRepeat(RepeatMode::Track))
        .expect("SetRepeat failed");
    player
        .send(PlayerCommand::ReplaceQueue(vec![
            track_flac(1),
            track_wav(2),
        ]))
        .expect("ReplaceQueue failed");

    // Wait until track 0 is playing
    let start = std::time::Instant::now();
    while player.snapshot().current_index != Some(0)
        || player.snapshot().state != PlaybackState::Playing
    {
        if start.elapsed() > Duration::from_secs(3) {
            panic!("Timed out waiting for track 0");
        }
        std::thread::sleep(Duration::from_millis(10));
    }

    let initial_gen = player.status().generation();

    // Step track 0 until it ends and reloads with a new generation
    let start_step = std::time::Instant::now();
    while player.status().generation() == initial_gen {
        null_sink.step(2048);
        std::thread::sleep(Duration::from_millis(5));
        if start_step.elapsed() > Duration::from_secs(6) {
            panic!("Timed out waiting for track to reload");
        }
    }

    // Current index must still be 0 (repeating same track)
    assert_eq!(player.snapshot().current_index, Some(0));
    assert!(player.status().generation() > initial_gen);
}

#[test]
fn test_core_repeat_queue() {
    let (audio, null_sink) = AudioThread::spawn_null(44100).expect("spawn_null failed");
    let player = PlayerCore::spawn(audio, Box::new(|| {}));

    player
        .send(PlayerCommand::SetRepeat(RepeatMode::Queue))
        .expect("SetRepeat failed");
    player
        .send(PlayerCommand::ReplaceQueue(vec![
            track_flac(1),
            track_wav(2),
        ]))
        .expect("ReplaceQueue failed");

    // Wait until track 0 plays
    let start = std::time::Instant::now();
    while player.snapshot().current_index != Some(0)
        || player.snapshot().state != PlaybackState::Playing
    {
        if start.elapsed() > Duration::from_secs(3) {
            panic!("Timed out waiting for track 0");
        }
        std::thread::sleep(Duration::from_millis(10));
    }

    // Step through track 0 to track 1
    while player.snapshot().current_index == Some(0) {
        null_sink.step(2048);
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(player.snapshot().current_index, Some(1));

    // Step through track 1 to end -> should wrap back to track 0
    let start_t1 = std::time::Instant::now();
    while player.snapshot().current_index == Some(1) {
        null_sink.step(2048);
        std::thread::sleep(Duration::from_millis(5));
        if start_t1.elapsed() > Duration::from_secs(6) {
            panic!("Timed out waiting for queue to wrap");
        }
    }

    // Wrapped to 0
    assert_eq!(player.snapshot().current_index, Some(0));
    assert_eq!(player.snapshot().repeat_mode, RepeatMode::Queue);
}

#[test]
fn test_core_randomize_keeps_current() {
    let (audio, _null_sink) = AudioThread::spawn_null(44100).expect("spawn_null failed");
    let player = PlayerCore::spawn(audio, Box::new(|| {}));

    let tracks = vec![
        track_flac(10),
        track_flac(20),
        track_wav(30),
        track_vorbis(40),
        track_wav(50),
        track_flac(60),
    ];

    player
        .send(PlayerCommand::ReplaceQueue(tracks))
        .expect("ReplaceQueue failed");

    // Wait until track 0 plays
    let start = std::time::Instant::now();
    while player.snapshot().current_index != Some(0) {
        if start.elapsed() > Duration::from_secs(3) {
            panic!("Timed out");
        }
        std::thread::sleep(Duration::from_millis(10));
    }

    // Advance to track 1
    player.send(PlayerCommand::Next).expect("Next failed");
    while player.snapshot().current_index != Some(1) {
        std::thread::sleep(Duration::from_millis(10));
    }

    // Current is track 1 (id 20), preceding is track 0 (id 10)
    let pre_snap = player.snapshot();
    assert_eq!(pre_snap.queue[0].id, 10);
    assert_eq!(pre_snap.queue[1].id, 20);

    // Randomize
    player
        .send(PlayerCommand::RandomizeQueue)
        .expect("RandomizeQueue failed");
    std::thread::sleep(Duration::from_millis(50));

    let post_snap = player.snapshot();
    // Indices 0 and 1 must remain untouched!
    assert_eq!(post_snap.current_index, Some(1));
    assert_eq!(post_snap.queue[0].id, 10);
    assert_eq!(post_snap.queue[1].id, 20);

    // Remaining items (indices 2..6) contain all original IDs [30, 40, 50, 60]
    let mut post_tail_ids: Vec<u64> = post_snap.queue[2..].iter().map(|t| t.id).collect();
    post_tail_ids.sort();
    assert_eq!(post_tail_ids, vec![30, 40, 50, 60]);
}

#[test]
fn test_core_auto_advance_skips_failed_track() {
    let (audio, null_sink) = AudioThread::spawn_null(44100).expect("spawn_null failed");
    let player = PlayerCore::spawn(audio, Box::new(|| {}));

    // Queue: [Valid, Bad, Valid]
    let queue = vec![track_flac(1), track_bad(999), track_wav(2)];
    player
        .send(PlayerCommand::ReplaceQueue(queue))
        .expect("ReplaceQueue failed");

    // Wait until track 0 plays
    let start = std::time::Instant::now();
    while player.snapshot().current_index != Some(0)
        || player.snapshot().state != PlaybackState::Playing
    {
        if start.elapsed() > Duration::from_secs(3) {
            panic!("Timed out waiting for track 0");
        }
        std::thread::sleep(Duration::from_millis(10));
    }

    // Step track 0 to end. When it ends, core should try track 1 (bad), fail, and auto-advance to track 2!
    let start_step = std::time::Instant::now();
    while player.snapshot().current_index != Some(2) {
        null_sink.step(2048);
        std::thread::sleep(Duration::from_millis(5));
        if start_step.elapsed() > Duration::from_secs(6) {
            panic!(
                "Timed out waiting for auto-advance to skip bad track to track 2; idx={:?}, state={:?}",
                player.snapshot().current_index,
                player.snapshot().state
            );
        }
    }

    assert_eq!(player.snapshot().current_index, Some(2));
}

#[test]
fn test_core_explicit_play_stays_stopped_on_failure() {
    let (audio, _null_sink) = AudioThread::spawn_null(44100).expect("spawn_null failed");
    let player = PlayerCore::spawn(audio, Box::new(|| {}));

    let queue = vec![track_bad(999), track_flac(1)];
    player
        .send(PlayerCommand::ReplaceQueue(queue))
        .expect("ReplaceQueue failed");

    // Explicit load and play track 0 (bad)
    let start = std::time::Instant::now();
    while player.snapshot().state != PlaybackState::Stopped
        || player.snapshot().error_message.is_none()
    {
        if start.elapsed() > Duration::from_secs(3) {
            panic!(
                "Timed out; state={:?}, err={:?}",
                player.snapshot().state,
                player.snapshot().error_message
            );
        }
        std::thread::sleep(Duration::from_millis(10));
    }

    // Must STAY stopped on explicit play failure and NOT auto-advance to track 1!
    assert_eq!(player.snapshot().state, PlaybackState::Stopped);
    assert_eq!(player.snapshot().current_index, Some(0));
    assert!(player.snapshot().error_message.is_some());
}

#[test]
fn test_core_stop_preserves_queue() {
    let (audio, _null_sink) = AudioThread::spawn_null(44100).expect("spawn_null failed");
    let player = PlayerCore::spawn(audio, Box::new(|| {}));

    let queue = vec![track_flac(1), track_wav(2)];
    player
        .send(PlayerCommand::ReplaceQueue(queue.clone()))
        .expect("ReplaceQueue failed");

    let start = std::time::Instant::now();
    while player.snapshot().current_index != Some(0)
        || player.snapshot().state != PlaybackState::Playing
    {
        if start.elapsed() > Duration::from_secs(3) {
            panic!("Timed out waiting for track 0");
        }
        std::thread::sleep(Duration::from_millis(10));
    }

    // Stop
    player.send(PlayerCommand::Stop).expect("Stop failed");
    std::thread::sleep(Duration::from_millis(50));

    let snap = player.snapshot();
    assert_eq!(snap.state, PlaybackState::Stopped);
    assert_eq!(snap.position, Duration::ZERO);
    assert_eq!(snap.queue.len(), 2);
    assert_eq!(snap.queue, queue);
}

#[test]
fn test_core_stale_track_ended_ignored() {
    let (audio, null_sink) = AudioThread::spawn_null(44100).expect("spawn_null failed");
    let player = PlayerCore::spawn(audio, Box::new(|| {}));

    let queue = vec![track_flac(1), track_wav(2), track_vorbis(3)];
    player
        .send(PlayerCommand::ReplaceQueue(queue))
        .expect("ReplaceQueue failed");

    // Wait until track 0 is playing
    let start = std::time::Instant::now();
    while player.snapshot().current_index != Some(0)
        || player.snapshot().state != PlaybackState::Playing
    {
        if start.elapsed() > Duration::from_secs(3) {
            panic!("Timed out waiting for track 0");
        }
        std::thread::sleep(Duration::from_millis(10));
    }

    // Step track 0 to end -> advances to track 1
    while player.snapshot().current_index == Some(0) {
        null_sink.step(2048);
        std::thread::sleep(Duration::from_millis(5));
        if start.elapsed() > Duration::from_secs(6) {
            panic!("Timed out waiting for track 1");
        }
    }
    assert_eq!(player.snapshot().current_index, Some(1));

    // While track 1 just started loading/playing, verify that stale track_ended from track 0 does not skip track 1
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(player.snapshot().current_index, Some(1));
}
