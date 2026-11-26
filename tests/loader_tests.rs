//! Integration tests for the background folder loader (`src/library/loader.rs`).
//!
//! Verifies:
//! - Ordering: folder path, disc (missing = 1), track number (Track 2 before Track 10),
//!   and case-insensitive numeric-aware filename.
//! - Corrupt / truncated files are skipped and counted.
//! - Nested folders are scanned recursively while hidden directories are skipped.
//! - Empty folders emit Done with 0 tracks and 0 skipped.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crossbeam_channel::unbounded;
use statistical_player::core::TrackItem;
use statistical_player::library::{LoaderEvent, load_folder, natural_cmp, sort_tracks};

struct TempDir(PathBuf);

impl TempDir {
    fn new(prefix: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("stat_player_{prefix}_{nanos}"));
        fs::create_dir_all(&dir).expect("Failed to create temp dir");
        Self(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn test_natural_cmp_numeric_and_case_insensitive() {
    use std::cmp::Ordering;
    assert_eq!(natural_cmp("Track 2.flac", "Track 10.flac"), Ordering::Less);
    assert_eq!(natural_cmp("track 2.flac", "TRACK 10.flac"), Ordering::Less);
    assert_eq!(
        natural_cmp("02 - Song.mp3", "10 - Song.mp3"),
        Ordering::Less
    );
    assert_eq!(natural_cmp("abc", "ABC"), Ordering::Equal);
}

#[test]
fn test_sort_tracks_ordering_disc_track_filename() {
    let mut tracks = vec![
        TrackItem {
            track_no: Some(10),
            disc_no: None,
            ..TrackItem::simple(1, PathBuf::from("/music/Album/10 - Ten.flac"))
        },
        TrackItem {
            track_no: Some(2),
            disc_no: None,
            ..TrackItem::simple(2, PathBuf::from("/music/Album/2 - Two.flac"))
        },
        TrackItem {
            track_no: Some(1),
            disc_no: Some(2),
            ..TrackItem::simple(3, PathBuf::from("/music/Album/D2T1.flac"))
        },
        TrackItem {
            track_no: None,
            disc_no: Some(1),
            ..TrackItem::simple(4, PathBuf::from("/music/Album/Track 10.flac"))
        },
        TrackItem {
            track_no: None,
            disc_no: Some(1),
            ..TrackItem::simple(5, PathBuf::from("/music/Album/Track 2.flac"))
        },
    ];

    sort_tracks(&mut tracks);

    let names: Vec<String> = tracks
        .iter()
        .map(|t| t.path.file_name().unwrap().to_string_lossy().to_string())
        .collect();

    assert_eq!(
        names,
        vec![
            "2 - Two.flac",  // disc 1 (missing=1), track 2
            "10 - Ten.flac", // disc 1 (missing=1), track 10
            "Track 2.flac",  // disc 1, no track_no, natural filename 2 < 10
            "Track 10.flac", // disc 1, no track_no, natural filename 10
            "D2T1.flac",     // disc 2, track 1
        ]
    );
}

#[test]
fn test_loader_corrupt_file_skipped_and_nested_folders() {
    let tmp = TempDir::new("loader_nested");
    let sub = tmp.path().join("SubAlbum");
    let hidden = tmp.path().join(".hidden");
    fs::create_dir_all(&sub).unwrap();
    fs::create_dir_all(&hidden).unwrap();

    // Copy valid fixtures
    fs::copy("tests/fixtures/flac_16bit.flac", tmp.path().join("01.flac")).unwrap();
    fs::copy("tests/fixtures/wav_16bit.wav", sub.join("02.wav")).unwrap();
    fs::copy("tests/fixtures/notags.mp3", sub.join("notags.mp3")).unwrap();

    // Copy corrupt/empty fixtures
    fs::copy("tests/fixtures/empty.mp3", tmp.path().join("empty.mp3")).unwrap();
    fs::copy("tests/fixtures/truncated.mp3", sub.join("truncated.mp3")).unwrap();

    // Put a valid file inside hidden directory (must be skipped!)
    fs::copy("tests/fixtures/flac_16bit.flac", hidden.join("secret.flac")).unwrap();

    let (tx, rx) = unbounded();
    let handle = load_folder(tmp.path().to_path_buf(), tx);

    let mut done_event = None;
    while let Ok(ev) = rx.recv_timeout(Duration::from_secs(5)) {
        if let LoaderEvent::Done {
            tracks,
            total,
            skipped,
            ..
        } = ev
        {
            done_event = Some((tracks, total, skipped));
            break;
        }
    }
    handle.join().unwrap();

    let (tracks, total, skipped) = done_event.expect("Expected LoaderEvent::Done");
    assert_eq!(total, 3, "Expected 3 valid tracks (hidden dir ignored)");
    assert_eq!(tracks.len(), 3);
    assert_eq!(skipped, 2, "Expected 2 corrupt/empty files skipped");

    // Verify fallback title & artist on notags.mp3
    let notags = tracks
        .iter()
        .find(|t| t.path.file_name().unwrap() == "notags.mp3")
        .expect("notags.mp3 should be present");
    assert_eq!(notags.title, "notags");
    assert_eq!(notags.artist, "Unknown Artist");
}

#[test]
fn test_loader_empty_folder() {
    let tmp = TempDir::new("loader_empty");
    let (tx, rx) = unbounded();
    let handle = load_folder(tmp.path().to_path_buf(), tx);

    let ev = rx
        .recv_timeout(Duration::from_secs(5))
        .expect("Expected Done event");
    handle.join().unwrap();

    match ev {
        LoaderEvent::Done {
            tracks,
            total,
            skipped,
            ..
        } => {
            assert!(tracks.is_empty());
            assert_eq!(total, 0);
            assert_eq!(skipped, 0);
        }
        other => panic!("Expected Done for empty folder, got {:?}", other),
    }
}
