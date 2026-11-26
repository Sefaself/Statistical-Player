//! Background folder scanner and metadata tag reader.
//!
//! Walks directories recursively, skips hidden directories and symlinks,
//! and reads audio tags using `lofty` across at most 3 worker threads.
//! Emits progressive batches and a final sorted `Done` event over a crossbeam channel.

use std::cmp::Ordering;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
use std::time::{Duration, Instant};

use crossbeam_channel::{RecvTimeoutError, Sender};
use lofty::file::{AudioFile, FileType, TaggedFileExt};
use lofty::probe::Probe;
use lofty::tag::Accessor;
use walkdir::{DirEntry, WalkDir};

use crate::core::TrackItem;

/// Events emitted by the background folder loader.
#[derive(Debug, Clone)]
pub enum LoaderEvent {
    /// Progressive batch of scanned tracks.
    Batch(Vec<TrackItem>),
    /// Scanning completed with the final sorted track list.
    Done {
        tracks: Vec<TrackItem>,
        total: usize,
        skipped: usize,
        elapsed: Duration,
    },
}

/// Checks if a file has an extension supported by the folder loader.
pub fn is_supported_extension(path: &Path) -> bool {
    let Some(ext) = path.extension().and_then(|s| s.to_str()) else {
        return false;
    };
    matches!(
        ext.to_ascii_lowercase().as_str(),
        "mp3" | "m4a" | "flac" | "wav" | "ogg"
    )
}

fn is_hidden_dir_or_symlink(entry: &DirEntry) -> bool {
    if entry.depth() == 0 {
        return false;
    }
    if entry.path_is_symlink() {
        return true;
    }
    if entry.file_type().is_dir()
        && let Some(name) = entry.file_name().to_str()
        && name.starts_with('.')
    {
        return true;
    }
    false
}

fn codec_label(file_type: FileType, path: &Path) -> String {
    match file_type {
        FileType::Flac => "FLAC".to_string(),
        FileType::Mpeg => "MP3".to_string(),
        FileType::Mp4 => "AAC".to_string(),
        FileType::Wav => "WAV".to_string(),
        FileType::Vorbis => "Vorbis".to_string(),
        FileType::Opus => "Opus".to_string(),
        _ => path
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| s.to_ascii_uppercase())
            .unwrap_or_else(|| "Unknown".to_string()),
    }
}

fn read_track_item(id: u64, path: &Path) -> Result<TrackItem, ()> {
    let tagged = match Probe::open(path).and_then(|p| p.read()) {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!("Skipping unreadable audio file {}: {e}", path.display());
            return Err(());
        }
    };

    let props = tagged.properties();
    let tag = tagged.primary_tag().or_else(|| tagged.first_tag());

    let fallback_title = path
        .file_stem()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("Unknown")
        .to_string();

    let title = tag
        .and_then(|t| t.title())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or(fallback_title);

    let artist = tag
        .and_then(|t| t.artist())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Unknown Artist".to_string());

    let album = tag
        .and_then(|t| t.album())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Unknown Album".to_string());

    let track_no = tag.and_then(|t| t.track());
    let disc_no = tag.and_then(|t| t.disk());
    let year = tag.and_then(|t| t.date()).map(|d| d.year as u32);

    let duration = props.duration();
    let sample_rate = props.sample_rate().unwrap_or(0);
    let bit_depth = props.bit_depth().map(|b| b as u32);
    let channels = props.channels().map(|c| c as u16).unwrap_or(2);
    let bitrate_kbps = props.audio_bitrate();
    let codec = codec_label(tagged.file_type(), path);

    Ok(TrackItem {
        id,
        path: path.to_path_buf(),
        track_no,
        disc_no,
        title,
        artist,
        album,
        year,
        duration,
        play_count: 0,
        codec,
        sample_rate,
        bit_depth,
        channels,
        bitrate_kbps,
    })
}

/// Case-insensitive numeric-aware ("natural") string comparison.
///
/// Compares ASCII digit runs numerically without integer overflow, and compares
/// non-digit characters case-insensitively.
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let mut ia = a.char_indices().peekable();
    let mut ib = b.char_indices().peekable();

    while let (Some(&(pos_a, ca)), Some(&(pos_b, cb))) = (ia.peek(), ib.peek()) {
        if ca.is_ascii_digit() && cb.is_ascii_digit() {
            let start_a = pos_a;
            let mut end_a = pos_a;
            while let Some(&(p, c)) = ia.peek() {
                if c.is_ascii_digit() {
                    end_a = p + c.len_utf8();
                    ia.next();
                } else {
                    break;
                }
            }

            let start_b = pos_b;
            let mut end_b = pos_b;
            while let Some(&(p, c)) = ib.peek() {
                if c.is_ascii_digit() {
                    end_b = p + c.len_utf8();
                    ib.next();
                } else {
                    break;
                }
            }

            let num_a = &a[start_a..end_a];
            let num_b = &b[start_b..end_b];
            let trimmed_a = num_a.trim_start_matches('0');
            let trimmed_b = num_b.trim_start_matches('0');

            match trimmed_a.len().cmp(&trimmed_b.len()) {
                Ordering::Equal => match trimmed_a.cmp(trimmed_b) {
                    Ordering::Equal => {
                        // Tie-break on leading zero count so ordering is deterministic
                        match num_a.len().cmp(&num_b.len()) {
                            Ordering::Equal => continue,
                            non_eq => return non_eq,
                        }
                    }
                    non_eq => return non_eq,
                },
                non_eq => return non_eq,
            }
        } else {
            ia.next();
            ib.next();
            let ord = ca.to_lowercase().cmp(cb.to_lowercase());
            if ord != Ordering::Equal {
                return ord;
            }
        }
    }

    ia.peek().is_some().cmp(&ib.peek().is_some())
}

/// Sorts tracks by: folder path, disc (missing = 1), track number, then natural filename.
pub fn sort_tracks(tracks: &mut [TrackItem]) {
    tracks.sort_by(|a, b| {
        let dir_a = a
            .path
            .parent()
            .map(|p| p.to_string_lossy())
            .unwrap_or_default();
        let dir_b = b
            .path
            .parent()
            .map(|p| p.to_string_lossy())
            .unwrap_or_default();

        natural_cmp(&dir_a, &dir_b)
            .then_with(|| a.disc_no.unwrap_or(1).cmp(&b.disc_no.unwrap_or(1)))
            .then_with(|| {
                a.track_no
                    .unwrap_or(u32::MAX)
                    .cmp(&b.track_no.unwrap_or(u32::MAX))
            })
            .then_with(|| {
                let name_a = a
                    .path
                    .file_name()
                    .map(|n| n.to_string_lossy())
                    .unwrap_or_default();
                let name_b = b
                    .path
                    .file_name()
                    .map(|n| n.to_string_lossy())
                    .unwrap_or_default();
                natural_cmp(&name_a, &name_b)
            })
    });
}

/// Spawns a background thread that recursively scans `root` and sends `LoaderEvent`s.
///
/// Uses at most 3 worker threads to read tags with `lofty` (suited for HDDs),
/// emits the first batch after ~50 files or 500 ms, subsequent batches every ~200 files,
/// and finishes with `LoaderEvent::Done` containing the fully sorted track list.
pub fn load_folder(root: PathBuf, sender: Sender<LoaderEvent>) -> std::thread::JoinHandle<()> {
    std::thread::Builder::new()
        .name("folder-loader".to_string())
        .spawn(move || {
            let start = Instant::now();
            let next_id = AtomicU64::new(1);

            let (work_tx, work_rx) = crossbeam_channel::bounded::<(u64, PathBuf)>(256);
            let (res_tx, res_rx) = crossbeam_channel::unbounded::<Result<TrackItem, ()>>();

            std::thread::scope(|s| {
                // Walker thread
                let walker_tx = work_tx.clone();
                let root_ref = &root;
                let next_id_ref = &next_id;
                s.spawn(move || {
                    let walker = WalkDir::new(root_ref)
                        .follow_links(false)
                        .into_iter()
                        .filter_entry(|e| !is_hidden_dir_or_symlink(e));

                    for entry in walker {
                        let entry = match entry {
                            Ok(e) => e,
                            Err(e) => {
                                tracing::warn!("Error walking directory: {e}");
                                continue;
                            }
                        };

                        if entry.path_is_symlink() {
                            continue;
                        }

                        if entry.file_type().is_file() && is_supported_extension(entry.path()) {
                            let id = next_id_ref.fetch_add(1, AtomicOrdering::Relaxed);
                            if walker_tx.send((id, entry.into_path())).is_err() {
                                break;
                            }
                        }
                    }
                    drop(walker_tx);
                });
                drop(work_tx);

                // At most 3 worker threads for hard-disk tag reading
                for _ in 0..3 {
                    let rx = work_rx.clone();
                    let tx = res_tx.clone();
                    s.spawn(move || {
                        while let Ok((id, path)) = rx.recv() {
                            let res = read_track_item(id, &path);
                            if tx.send(res).is_err() {
                                break;
                            }
                        }
                    });
                }
                drop(res_tx);

                // Coordinator collects results and emits progressive batches
                let mut all_tracks = Vec::new();
                let mut batch = Vec::new();
                let mut skipped = 0_usize;
                let mut sent_first_batch = false;

                loop {
                    match res_rx.recv_timeout(Duration::from_millis(100)) {
                        Ok(Ok(track)) => {
                            all_tracks.push(track.clone());
                            batch.push(track);

                            if !sent_first_batch
                                && (batch.len() >= 50
                                    || start.elapsed() >= Duration::from_millis(500))
                            {
                                let _ = sender.send(LoaderEvent::Batch(std::mem::take(&mut batch)));
                                sent_first_batch = true;
                            } else if sent_first_batch && batch.len() >= 200 {
                                let _ = sender.send(LoaderEvent::Batch(std::mem::take(&mut batch)));
                            }
                        }
                        Ok(Err(())) => {
                            skipped += 1;
                        }
                        Err(RecvTimeoutError::Timeout) => {
                            if !sent_first_batch
                                && !batch.is_empty()
                                && start.elapsed() >= Duration::from_millis(500)
                            {
                                let _ = sender.send(LoaderEvent::Batch(std::mem::take(&mut batch)));
                                sent_first_batch = true;
                            }
                        }
                        Err(RecvTimeoutError::Disconnected) => {
                            if !batch.is_empty() {
                                let _ = sender.send(LoaderEvent::Batch(std::mem::take(&mut batch)));
                            }
                            break;
                        }
                    }
                }

                sort_tracks(&mut all_tracks);
                let total = all_tracks.len();
                let elapsed = start.elapsed();

                let _ = sender.send(LoaderEvent::Done {
                    tracks: all_tracks,
                    total,
                    skipped,
                    elapsed,
                });
            });
        })
        .expect("Failed to spawn folder-loader thread")
}
