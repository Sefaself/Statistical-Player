//! Integration tests for the library audit CLI tool.

use statistical_player::library::audit::{is_supported_audio_extension, run_audit};
use std::path::PathBuf;

#[test]
fn test_supported_audio_extensions() {
    assert!(is_supported_audio_extension(&PathBuf::from("track.mp3")));
    assert!(is_supported_audio_extension(&PathBuf::from("track.MP3")));
    assert!(is_supported_audio_extension(&PathBuf::from("track.m4a")));
    assert!(is_supported_audio_extension(&PathBuf::from("track.flac")));
    assert!(is_supported_audio_extension(&PathBuf::from("track.wav")));
    assert!(is_supported_audio_extension(&PathBuf::from("track.ogg")));
    assert!(!is_supported_audio_extension(&PathBuf::from("cover.jpg")));
    assert!(!is_supported_audio_extension(&PathBuf::from("lyrics.lrc")));
    assert!(!is_supported_audio_extension(&PathBuf::from("notes.txt")));
}

#[test]
fn test_audit_fixtures_directory() {
    let fixtures_dir = PathBuf::from("tests/fixtures");
    assert!(
        fixtures_dir.exists(),
        "tests/fixtures must exist to run audit tests"
    );

    let temp_dir =
        std::env::temp_dir().join(format!("statistical_player_audit_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&temp_dir);
    let report_file = temp_dir.join("audit-report.md");

    let report =
        run_audit(&fixtures_dir, Some(&report_file)).expect("Audit execution failed on fixtures");

    // Scanned count should include our test fixtures
    assert!(
        report.total_scanned >= 7,
        "Expected at least 7 test files, found {}",
        report.total_scanned
    );

    // Verify successful decodes
    assert!(
        report.total_successful >= 5,
        "Expected at least 5 successful decodes, found {}",
        report.total_successful
    );

    // Verify codecs identified
    assert!(
        report.codec_counts.contains_key("FLAC"),
        "Missing FLAC codec in report"
    );
    assert!(
        report.codec_counts.contains_key("MP3"),
        "Missing MP3 codec in report"
    );
    assert!(
        report.codec_counts.contains_key("AAC"),
        "Missing AAC codec in report"
    );
    assert!(
        report.codec_counts.contains_key("PCM"),
        "Missing PCM codec in report"
    );

    // Verify sample rates (44100 Hz fixture)
    assert!(
        report.sample_rate_counts.contains_key(&44100),
        "Expected 44100 Hz in sample rate distribution"
    );

    // Verify bit depths: 16-bit, 24-bit, and lossy N/A
    assert!(
        report.bit_depth_counts.contains_key("16-bit"),
        "Missing 16-bit depth"
    );
    assert!(
        report.bit_depth_counts.contains_key("24-bit"),
        "Missing 24-bit depth"
    );
    assert!(
        report.bit_depth_counts.contains_key("N/A (lossy)"),
        "Missing N/A (lossy) depth"
    );

    // Verify open failures (empty file)
    let has_empty_failure = report
        .open_failures
        .iter()
        .any(|(p, _)| p.file_name().and_then(|n| n.to_str()) == Some("empty.mp3"));
    assert!(has_empty_failure, "empty.mp3 should fail open/probe");

    // Verify missing tags (notags.mp3)
    let has_notags = report
        .missing_tag_files
        .iter()
        .any(|p| p.file_name().and_then(|n| n.to_str()) == Some("notags.mp3"));
    assert!(has_notags, "notags.mp3 should be reported as missing tags");

    // Verify AAC audio object types
    assert!(
        report.aac_profile_counts.contains_key("LC"),
        "Missing AAC LC profile in report: {:?}",
        report.aac_profile_counts
    );

    // Verify WAV files are not counted as missing tags
    let has_wav_missing_tag = report.missing_tag_files.iter().any(|p| {
        p.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("wav"))
            .unwrap_or(false)
    });
    assert!(
        !has_wav_missing_tag,
        "WAV files must not be reported as missing tags"
    );

    // Verify markdown report generation
    assert!(report_file.exists(), "Markdown report was not written");
    let content = std::fs::read_to_string(&report_file).unwrap();
    assert!(content.contains("# Library Audit Report"));
    assert!(content.contains("Codec Distribution"));
    assert!(content.contains("AAC Audio Object Types"));
    assert!(content.contains("Sample Rate Distribution"));
}
