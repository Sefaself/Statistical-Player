//! Library audit tool: recursively scans an audio folder, probes and decodes ~1s
//! of every audio file, checks for tags, and produces an audit report.

use std::collections::BTreeMap;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result};
use lofty::file::TaggedFileExt;
use lofty::tag::Accessor;
use symphonia::core::codecs::CodecParameters;
use symphonia::core::codecs::audio::well_known::{
    CODEC_ID_AAC, CODEC_ID_ALAC, CODEC_ID_FLAC, CODEC_ID_MP3, CODEC_ID_PCM_F32BE,
    CODEC_ID_PCM_F32LE, CODEC_ID_PCM_S8, CODEC_ID_PCM_S16BE, CODEC_ID_PCM_S16LE,
    CODEC_ID_PCM_S24BE, CODEC_ID_PCM_S24LE, CODEC_ID_PCM_S32BE, CODEC_ID_PCM_S32LE,
    CODEC_ID_PCM_U8, CODEC_ID_VORBIS,
};
use symphonia::core::codecs::audio::{AudioCodecId, AudioDecoderOptions, CODEC_ID_NULL_AUDIO};
use symphonia::core::formats::FormatOptions;
use symphonia::core::formats::probe::Hint;
use symphonia::core::io::{MediaSourceStream, MediaSourceStreamOptions};
use symphonia::core::meta::MetadataOptions;
use walkdir::WalkDir;

/// Status resulting from auditing a single audio file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileAuditStatus {
    /// Successfully opened and decoded at least 1 packet / up to ~1s of frames.
    Success { frames_decoded: u64 },
    /// Failed to open or probe container / track / decoder.
    OpenFailed(String),
    /// Container opened, but failed while decoding audio packets.
    DecodeFailed(String),
}

/// Audit result for an individual audio file.
#[derive(Debug, Clone)]
pub struct FileAuditResult {
    pub path: PathBuf,
    pub codec: Option<String>,
    pub aac_profile: Option<String>,
    pub sample_rate: Option<u32>,
    pub bit_depth: Option<u32>,
    pub has_tags: bool,
    pub status: FileAuditStatus,
}

/// Aggregated report from auditing a directory tree of audio files.
#[derive(Debug, Clone, Default)]
pub struct AuditReport {
    pub root_folder: PathBuf,
    pub total_scanned: usize,
    pub total_successful: usize,
    pub codec_counts: BTreeMap<String, usize>,
    pub aac_profile_counts: BTreeMap<String, usize>,
    pub sample_rate_counts: BTreeMap<u32, usize>,
    pub bit_depth_counts: BTreeMap<String, usize>,
    pub open_failures: Vec<(PathBuf, String)>,
    pub decode_failures: Vec<(PathBuf, String)>,
    pub missing_tag_files: Vec<PathBuf>,
}

impl AuditReport {
    /// Incorporates a single file result into the aggregate report.
    pub fn record(&mut self, result: FileAuditResult) {
        self.total_scanned += 1;

        let is_wav = result
            .path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("wav"))
            .unwrap_or(false);

        // Do not report WAV files as missing tags (Section 4 & owner rule)
        if !result.has_tags && !is_wav {
            self.missing_tag_files.push(result.path.clone());
        }

        if let Some(profile) = result.aac_profile {
            *self.aac_profile_counts.entry(profile).or_insert(0) += 1;
        }

        match result.status {
            FileAuditStatus::Success { .. } => {
                self.total_successful += 1;
                if let Some(codec) = result.codec {
                    *self.codec_counts.entry(codec).or_insert(0) += 1;
                }
                if let Some(rate) = result.sample_rate {
                    *self.sample_rate_counts.entry(rate).or_insert(0) += 1;
                }
                let bit_depth_str = match result.bit_depth {
                    Some(bits) => format!("{bits}-bit"),
                    None => "N/A (lossy)".to_string(),
                };
                *self.bit_depth_counts.entry(bit_depth_str).or_insert(0) += 1;
            }
            FileAuditStatus::OpenFailed(err) => {
                self.open_failures.push((result.path, err));
            }
            FileAuditStatus::DecodeFailed(err) => {
                self.decode_failures.push((result.path, err));
            }
        }
    }

    /// Renders a human-readable console summary.
    pub fn render_console(&self) -> String {
        let mut out = String::new();
        out.push_str(
            "================================================================================\n",
        );
        out.push_str("Statistical Player — Library Audit Report\n");
        out.push_str(&format!(
            "Target Directory: {}\n",
            self.root_folder.display()
        ));
        out.push_str(&format!(
            "Total Audio Files Scanned: {} (Successful: {})\n",
            self.total_scanned, self.total_successful
        ));
        out.push_str(
            "================================================================================\n\n",
        );

        out.push_str("--- Codec Distribution ---\n");
        if self.codec_counts.is_empty() {
            out.push_str("  (none)\n");
        } else {
            for (codec, count) in &self.codec_counts {
                let pct = (*count as f64 / self.total_successful as f64) * 100.0;
                out.push_str(&format!("  - {codec}: {count} ({pct:.1}%)\n"));
            }
        }
        out.push('\n');

        if !self.aac_profile_counts.is_empty() {
            let total_aac: usize = self.aac_profile_counts.values().sum();
            out.push_str("--- AAC Audio Object Types (Profiles) ---\n");
            for (profile, count) in &self.aac_profile_counts {
                let pct = (*count as f64 / total_aac as f64) * 100.0;
                out.push_str(&format!("  - {profile}: {count} ({pct:.1}%)\n"));
            }
            out.push('\n');
        }

        out.push_str("--- Sample Rate Distribution ---\n");
        if self.sample_rate_counts.is_empty() {
            out.push_str("  (none)\n");
        } else {
            for (rate, count) in &self.sample_rate_counts {
                let pct = (*count as f64 / self.total_successful as f64) * 100.0;
                out.push_str(&format!("  - {rate} Hz: {count} ({pct:.1}%)\n"));
            }
        }
        out.push('\n');

        out.push_str("--- Bit Depth Distribution ---\n");
        if self.bit_depth_counts.is_empty() {
            out.push_str("  (none)\n");
        } else {
            for (depth, count) in &self.bit_depth_counts {
                let pct = (*count as f64 / self.total_successful as f64) * 100.0;
                out.push_str(&format!("  - {depth}: {count} ({pct:.1}%)\n"));
            }
        }
        out.push('\n');

        out.push_str(&format!(
            "--- Open / Probe Failures ({}) ---\n",
            self.open_failures.len()
        ));
        if self.open_failures.is_empty() {
            out.push_str("  (none)\n");
        } else {
            for (path, err) in &self.open_failures {
                out.push_str(&format!("  - {}: {}\n", path.display(), err));
            }
        }
        out.push('\n');

        out.push_str(&format!(
            "--- Decode Failures ({}) ---\n",
            self.decode_failures.len()
        ));
        if self.decode_failures.is_empty() {
            out.push_str("  (none)\n");
        } else {
            for (path, err) in &self.decode_failures {
                out.push_str(&format!("  - {}: {}\n", path.display(), err));
            }
        }
        out.push('\n');

        out.push_str(&format!(
            "--- Files Missing Tags ({}) ---\n",
            self.missing_tag_files.len()
        ));
        if self.missing_tag_files.is_empty() {
            out.push_str("  (none)\n");
        } else {
            for path in &self.missing_tag_files {
                out.push_str(&format!("  - {}\n", path.display()));
            }
        }
        out.push_str(
            "================================================================================\n",
        );

        out
    }

    /// Generates markdown documentation suitable for saving as `docs/audit-report.md`.
    pub fn render_markdown(&self) -> String {
        let mut out = String::new();
        out.push_str("# Library Audit Report\n\n");
        out.push_str(&format!(
            "- **Target Folder:** `{}`\n",
            self.root_folder.display()
        ));
        out.push_str(&format!(
            "- **Total Audio Files Found:** {}\n",
            self.total_scanned
        ));
        out.push_str(&format!(
            "- **Successfully Opened & Decoded (~1s):** {}\n",
            self.total_successful
        ));
        out.push_str(&format!(
            "- **Open / Probe Failures:** {}\n",
            self.open_failures.len()
        ));
        out.push_str(&format!(
            "- **Decode Failures:** {}\n",
            self.decode_failures.len()
        ));
        out.push_str(&format!(
            "- **Files Missing Tags:** {}\n\n",
            self.missing_tag_files.len()
        ));

        out.push_str("## Codec Distribution\n\n");
        if self.codec_counts.is_empty() {
            out.push_str("*(No successfully decoded codecs)*\n\n");
        } else {
            out.push_str("| Codec | Count | Share |\n");
            out.push_str("| :--- | :--- | :--- |\n");
            for (codec, count) in &self.codec_counts {
                let pct = (*count as f64 / self.total_successful as f64) * 100.0;
                out.push_str(&format!("| {codec} | {count} | {pct:.1}% |\n"));
            }
            out.push('\n');
        }

        if !self.aac_profile_counts.is_empty() {
            let total_aac: usize = self.aac_profile_counts.values().sum();
            out.push_str("## AAC Audio Object Types\n\n");
            out.push_str("| Profile / Audio Object Type | Count | Share |\n");
            out.push_str("| :--- | :--- | :--- |\n");
            for (profile, count) in &self.aac_profile_counts {
                let pct = (*count as f64 / total_aac as f64) * 100.0;
                out.push_str(&format!("| {profile} | {count} | {pct:.1}% |\n"));
            }
            out.push('\n');
        }

        out.push_str("## Sample Rate Distribution\n\n");
        if self.sample_rate_counts.is_empty() {
            out.push_str("*(No sample rate data)*\n\n");
        } else {
            out.push_str("| Sample Rate | Count | Share |\n");
            out.push_str("| :--- | :--- | :--- |\n");
            for (rate, count) in &self.sample_rate_counts {
                let pct = (*count as f64 / self.total_successful as f64) * 100.0;
                out.push_str(&format!("| {rate} Hz | {count} | {pct:.1}% |\n"));
            }
            out.push('\n');
        }

        out.push_str("## Bit Depth Distribution\n\n");
        if self.bit_depth_counts.is_empty() {
            out.push_str("*(No bit depth data)*\n\n");
        } else {
            out.push_str("| Bit Depth | Count | Share |\n");
            out.push_str("| :--- | :--- | :--- |\n");
            for (depth, count) in &self.bit_depth_counts {
                let pct = (*count as f64 / self.total_successful as f64) * 100.0;
                out.push_str(&format!("| {depth} | {count} | {pct:.1}% |\n"));
            }
            out.push('\n');
        }

        out.push_str("## Files That Failed to Open / Probe\n\n");
        if self.open_failures.is_empty() {
            out.push_str("*(None)*\n\n");
        } else {
            out.push_str("| File | Error |\n");
            out.push_str("| :--- | :--- |\n");
            for (path, err) in &self.open_failures {
                out.push_str(&format!("| `{}` | {} |\n", path.display(), err));
            }
            out.push('\n');
        }

        out.push_str("## Files That Failed to Decode\n\n");
        if self.decode_failures.is_empty() {
            out.push_str("*(None)*\n\n");
        } else {
            out.push_str("| File | Error |\n");
            out.push_str("| :--- | :--- |\n");
            for (path, err) in &self.decode_failures {
                out.push_str(&format!("| `{}` | {} |\n", path.display(), err));
            }
            out.push('\n');
        }

        out.push_str("## Files With Missing Tags\n\n");
        if self.missing_tag_files.is_empty() {
            out.push_str("*(None)*\n\n");
        } else {
            for path in &self.missing_tag_files {
                out.push_str(&format!("- `{}`\n", path.display()));
            }
            out.push('\n');
        }

        out
    }
}

/// Checks whether a file path has an audio extension supported by the player.
pub fn is_supported_audio_extension(path: &Path) -> bool {
    let ext = match path.extension().and_then(|s| s.to_str()) {
        Some(e) => e.to_lowercase(),
        None => return false,
    };
    matches!(
        ext.as_str(),
        "mp3" | "m4a" | "flac" | "wav" | "ogg" | "alac"
    )
}

fn is_pcm_codec(id: AudioCodecId) -> bool {
    matches!(
        id,
        CODEC_ID_PCM_S16LE
            | CODEC_ID_PCM_S16BE
            | CODEC_ID_PCM_S24LE
            | CODEC_ID_PCM_S24BE
            | CODEC_ID_PCM_S32LE
            | CODEC_ID_PCM_S32BE
            | CODEC_ID_PCM_F32LE
            | CODEC_ID_PCM_F32BE
            | CODEC_ID_PCM_S8
            | CODEC_ID_PCM_U8
    )
}

struct BitReader<'a> {
    data: &'a [u8],
    bit_offset: usize,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            bit_offset: 0,
        }
    }

    fn read_bits(&mut self, n: usize) -> Option<u32> {
        if self.bit_offset + n > self.data.len() * 8 {
            return None;
        }
        let mut val = 0;
        for _ in 0..n {
            let byte = self.data[self.bit_offset / 8];
            let bit = (byte >> (7 - (self.bit_offset % 8))) & 1;
            val = (val << 1) | (bit as u32);
            self.bit_offset += 1;
        }
        Some(val)
    }

    fn remaining_bits(&self) -> usize {
        (self.data.len() * 8).saturating_sub(self.bit_offset)
    }
}

/// Parses AAC Audio Object Type (e.g. LC, HE-AAC, HE-AACv2) from MPEG-4 AudioSpecificConfig payload.
pub fn parse_aac_aot(extra_data: Option<&[u8]>) -> String {
    let Some(data) = extra_data else {
        return "Cannot be determined (no extradata)".to_string();
    };

    if data.is_empty() {
        return "Cannot be determined (empty extradata)".to_string();
    }

    let mut reader = BitReader::new(data);
    let mut aot = match reader.read_bits(5) {
        Some(val) => val,
        None => return "Cannot be determined".to_string(),
    };

    if aot == 31 {
        let ext = match reader.read_bits(6) {
            Some(e) => e,
            None => return "Cannot be determined".to_string(),
        };
        aot = 32 + ext;
    }

    let sampling_freq_idx = match reader.read_bits(4) {
        Some(s) => s,
        None => return format_base_aot(aot),
    };

    if sampling_freq_idx == 0x0f && reader.read_bits(24).is_none() {
        return format_base_aot(aot);
    }

    let _channel_config = match reader.read_bits(4) {
        Some(c) => c,
        None => return format_base_aot(aot),
    };

    let mut is_sbr = aot == 5;
    let mut is_ps = aot == 29;

    // Explicit hierarchical SBR
    if is_sbr && let Some(ext_sampling) = reader.read_bits(4) {
        if ext_sampling == 0x0f {
            let _ = reader.read_bits(24);
        }
        if let Some(base_aot) = reader.read_bits(5) {
            aot = if base_aot == 31 {
                32 + reader.read_bits(6).unwrap_or(0)
            } else {
                base_aot
            };
        }
    }

    // Check for backward-compatible explicit extension:
    // syncExtensionType (11 bits) == 0x2b7
    // extensionAudioObjectType (5 bits)
    while reader.remaining_bits() >= 16 {
        if let Some(sync) = reader.read_bits(11) {
            if sync == 0x2b7 {
                if let Some(ext_aot) = reader.read_bits(5) {
                    if ext_aot == 5 {
                        let has_sbr = reader.read_bits(1).unwrap_or(0);
                        if has_sbr == 1 {
                            is_sbr = true;
                        }
                    } else if ext_aot == 29 {
                        is_ps = true;
                    }
                }
                break;
            }
        } else {
            break;
        }
    }

    if is_ps {
        "HE-AACv2".to_string()
    } else if is_sbr {
        "HE-AAC".to_string()
    } else {
        format_base_aot(aot)
    }
}

fn format_base_aot(aot: u32) -> String {
    match aot {
        2 => "LC".to_string(),
        1 => "Main".to_string(),
        3 => "SSR".to_string(),
        4 => "LTP".to_string(),
        5 => "HE-AAC".to_string(),
        23 => "LD".to_string(),
        29 => "HE-AACv2".to_string(),
        39 => "ELD".to_string(),
        other => format!("AOT {other}"),
    }
}

/// Checks whether an audio file has valid metadata tags using Lofty.
fn check_file_tags(path: &Path) -> bool {
    match lofty::read_from_path(path) {
        Ok(tagged) => {
            let tags = tagged.tags();
            !tags.is_empty()
                && tags
                    .iter()
                    .any(|t| t.title().is_some() || t.artist().is_some() || t.album().is_some())
        }
        Err(_) => false,
    }
}

/// Audits a single file: tests tag parsing with Lofty, probes format and decodes ~1s with Symphonia.
pub fn audit_single_file(path: &Path) -> FileAuditResult {
    let has_tags = check_file_tags(path);

    let file = match File::open(path) {
        Ok(f) => f,
        Err(e) => {
            return FileAuditResult {
                path: path.to_path_buf(),
                codec: None,
                aac_profile: None,
                sample_rate: None,
                bit_depth: None,
                has_tags,
                status: FileAuditStatus::OpenFailed(format!("Failed to open file: {e}")),
            };
        }
    };

    let mss = MediaSourceStream::new(Box::new(file), MediaSourceStreamOptions::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    let format_opts = FormatOptions::default();
    let meta_opts = MetadataOptions::default();

    let mut reader = match symphonia::default::get_probe().probe(&hint, mss, format_opts, meta_opts)
    {
        Ok(r) => r,
        Err(e) => {
            return FileAuditResult {
                path: path.to_path_buf(),
                codec: None,
                aac_profile: None,
                sample_rate: None,
                bit_depth: None,
                has_tags,
                status: FileAuditStatus::OpenFailed(format!("Failed to probe format: {e}")),
            };
        }
    };

    let track = match reader.tracks().iter().find(|t| match &t.codec_params {
        Some(CodecParameters::Audio(params)) => params.codec != CODEC_ID_NULL_AUDIO,
        _ => false,
    }) {
        Some(t) => t,
        None => {
            return FileAuditResult {
                path: path.to_path_buf(),
                codec: None,
                aac_profile: None,
                sample_rate: None,
                bit_depth: None,
                has_tags,
                status: FileAuditStatus::OpenFailed("No supported audio track found".to_string()),
            };
        }
    };

    let track_id = track.id;
    let audio_params = match &track.codec_params {
        Some(CodecParameters::Audio(params)) => params.clone(),
        _ => unreachable!(),
    };

    let sample_rate = audio_params.sample_rate.unwrap_or(0);
    let bit_depth = audio_params.bits_per_sample;
    let codec_name = match audio_params.codec {
        CODEC_ID_FLAC => "FLAC".to_string(),
        CODEC_ID_MP3 => "MP3".to_string(),
        CODEC_ID_AAC => "AAC".to_string(),
        CODEC_ID_VORBIS => "Vorbis".to_string(),
        CODEC_ID_ALAC => "ALAC".to_string(),
        id if is_pcm_codec(id) => "PCM".to_string(),
        other => {
            let raw = format!("{other:?}");
            raw.replace("AudioCodecId(0x", "").replace(')', "")
        }
    };

    let aac_profile = if codec_name == "AAC" || audio_params.codec == CODEC_ID_AAC {
        Some(parse_aac_aot(audio_params.extra_data.as_deref()))
    } else {
        None
    };

    let decoder_opts = AudioDecoderOptions::default();
    let mut decoder =
        match symphonia::default::get_codecs().make_audio_decoder(&audio_params, &decoder_opts) {
            Ok(d) => d,
            Err(e) => {
                return FileAuditResult {
                    path: path.to_path_buf(),
                    codec: Some(codec_name),
                    aac_profile,
                    sample_rate: Some(sample_rate),
                    bit_depth,
                    has_tags,
                    status: FileAuditStatus::OpenFailed(format!("Failed to create decoder: {e}")),
                };
            }
        };

    // Decode ~1 second of frames (or until end of stream)
    let target_frames = if sample_rate > 0 {
        sample_rate as u64
    } else {
        44100
    };
    let mut decoded_frames: u64 = 0;
    let mut packet_index = 0;

    loop {
        if decoded_frames >= target_frames {
            break;
        }

        let packet = match reader.next_packet() {
            Ok(Some(p)) => p,
            Ok(None) => break, // EOF reached before 1s (short file)
            Err(symphonia::core::errors::Error::ResetRequired) => {
                decoder.reset();
                continue;
            }
            Err(e) => {
                return FileAuditResult {
                    path: path.to_path_buf(),
                    codec: Some(codec_name),
                    aac_profile,
                    sample_rate: Some(sample_rate),
                    bit_depth,
                    has_tags,
                    status: FileAuditStatus::DecodeFailed(format!("Error reading packet: {e}")),
                };
            }
        };

        if packet.track_id != track_id {
            continue;
        }

        match decoder.decode(&packet) {
            Ok(buf) => {
                decoded_frames += buf.frames() as u64;
                packet_index += 1;
            }
            Err(symphonia::core::errors::Error::DecodeError(e)) => {
                return FileAuditResult {
                    path: path.to_path_buf(),
                    codec: Some(codec_name),
                    aac_profile,
                    sample_rate: Some(sample_rate),
                    bit_depth,
                    has_tags,
                    status: FileAuditStatus::DecodeFailed(format!(
                        "Decode error in packet {packet_index}: {e}"
                    )),
                };
            }
            Err(e) => {
                return FileAuditResult {
                    path: path.to_path_buf(),
                    codec: Some(codec_name),
                    aac_profile,
                    sample_rate: Some(sample_rate),
                    bit_depth,
                    has_tags,
                    status: FileAuditStatus::DecodeFailed(format!("Decoder failure: {e}")),
                };
            }
        }
    }

    if decoded_frames == 0 {
        return FileAuditResult {
            path: path.to_path_buf(),
            codec: Some(codec_name),
            aac_profile,
            sample_rate: Some(sample_rate),
            bit_depth,
            has_tags,
            status: FileAuditStatus::DecodeFailed("Zero audio frames decoded".to_string()),
        };
    }

    FileAuditResult {
        path: path.to_path_buf(),
        codec: Some(codec_name),
        aac_profile,
        sample_rate: Some(sample_rate),
        bit_depth,
        has_tags,
        status: FileAuditStatus::Success {
            frames_decoded: decoded_frames,
        },
    }
}

/// Runs a full audit over all supported audio files in `folder`.
/// Optionally saves the generated markdown report to `save_report_path`.
pub fn run_audit(folder: &Path, save_report_path: Option<&Path>) -> Result<AuditReport> {
    let start_time = Instant::now();
    let mut report = AuditReport {
        root_folder: folder.to_path_buf(),
        ..Default::default()
    };

    for entry in WalkDir::new(folder)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            // Skip hidden directories/files (starting with dot)
            !e.file_name()
                .to_str()
                .map(|s| s.starts_with('.'))
                .unwrap_or(false)
        })
    {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!("Failed to read filesystem entry: {e}");
                continue;
            }
        };

        let path = entry.path();
        if path.is_file() && is_supported_audio_extension(path) {
            let res = audit_single_file(path);
            report.record(res);
        }
    }

    if let Some(dest) = save_report_path {
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory {}", parent.display()))?;
        }
        let md = report.render_markdown();
        std::fs::write(dest, md)
            .with_context(|| format!("Failed to write audit report to {}", dest.display()))?;
    }

    tracing::info!(
        "Audit completed in {:.2?}: {} files scanned, {} successful",
        start_time.elapsed(),
        report.total_scanned,
        report.total_successful
    );

    Ok(report)
}
