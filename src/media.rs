//! Media integrity checks, repair attempts, and external-tool integration.
//!
//! This module keeps process execution separate from diagnostic classification:
//! [`check_video_with_tools`] collects `ffprobe`/`ffmpeg` results, while the
//! rule-based classifier turns those results into a [`MediaStatus`]. Repairs
//! are validated before the original file is replaced.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::media_rules::{
    classify_media as classify_media_result, diagnostics_are_valid_after_repair,
};

/// The result of checking a media file with `ffprobe` and `ffmpeg`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MediaStatus {
    /// The file has a recognized container and decoded without errors.
    Ok,
    /// The file can be repaired using the indicated strategy.
    NeedsRepair(MediaIssue),
    /// The file contains corruption that remuxing cannot repair.
    Unrepairable(MediaIssue),
}

/// The strategy selected for a detected media issue.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RepairStrategy {
    /// Remux the existing streams without re-encoding.
    StreamCopy,
    /// Download the source again instead of attempting a media repair.
    Redownload,
}

/// A classified media issue and the strategy selected to handle it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MediaIssue {
    reason: String,
    strategy: RepairStrategy,
}

impl MediaIssue {
    /// Creates a media issue with a repair strategy.
    pub fn new(reason: impl Into<String>, strategy: RepairStrategy) -> Self {
        Self {
            reason: reason.into(),
            strategy,
        }
    }

    /// Returns the human-readable explanation for this issue.
    pub fn reason(&self) -> &str {
        &self.reason
    }

    /// Returns the selected handling strategy.
    pub fn strategy(&self) -> RepairStrategy {
        self.strategy
    }
}

/// The result of attempting to repair a media file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RepairStatus {
    /// The temporary repaired file replaced the original.
    Repaired,
    /// The file was not changed because its corruption is not repairable.
    Unrepairable(String),
    /// Repair was attempted but `ffmpeg` did not produce a valid output.
    Failed(String),
}

const VIDEO_EXTENSIONS: [&str; 6] = ["mp4", "mkv", "ts", "webm", "mov", "avi"];

/// Checks one media file using the supplied `ffprobe` and `ffmpeg` executables.
pub fn check_video(path: impl AsRef<Path>) -> std::io::Result<MediaStatus> {
    check_video_with_tools(path, Path::new("ffprobe"), Path::new("ffmpeg"))
}

/// Checks one media file using explicitly selected media tools.
pub fn check_video_with_tools(
    path: impl AsRef<Path>,
    ffprobe: &Path,
    ffmpeg: &Path,
) -> std::io::Result<MediaStatus> {
    let path = path.as_ref();
    let probe = Command::new(ffprobe)
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=format_name",
            "-of",
            "default=nw=1:nk=1",
        ])
        .arg(path)
        .output()?;
    let format = String::from_utf8_lossy(&probe.stdout);
    let decode = Command::new(ffmpeg)
        .args(["-v", "error", "-i"])
        .arg(path)
        .args(["-f", "null", "-"])
        .output()?;
    let diagnostics = String::from_utf8_lossy(&decode.stderr).trim().to_owned();
    Ok(classify_media_result(
        &format,
        probe.status.success(),
        decode.status.success(),
        &diagnostics,
    ))
}

/// Classifies media tool results without running external processes.
pub fn classify_media(
    format: &str,
    probe_succeeded: bool,
    decode_succeeded: bool,
    diagnostics: &str,
) -> MediaStatus {
    classify_media_result(format, probe_succeeded, decode_succeeded, diagnostics)
}

/// Scans direct video files in a directory and checks each one.
pub fn check_videos(directory: impl AsRef<Path>) -> std::io::Result<Vec<(PathBuf, MediaStatus)>> {
    check_videos_with_tools(directory, Path::new("ffprobe"), Path::new("ffmpeg"))
}

/// Scans direct video files using explicitly selected media tools.
pub fn check_videos_with_tools(
    directory: impl AsRef<Path>,
    ffprobe: &Path,
    ffmpeg: &Path,
) -> std::io::Result<Vec<(PathBuf, MediaStatus)>> {
    let mut files = std::fs::read_dir(directory.as_ref())?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path.extension().is_some_and(|extension| {
                    VIDEO_EXTENSIONS
                        .iter()
                        .any(|known| extension.eq_ignore_ascii_case(known))
                })
        })
        .collect::<Vec<_>>();
    files.sort();
    files
        .into_iter()
        .map(|path| check_video_with_tools(&path, ffprobe, ffmpeg).map(|status| (path, status)))
        .collect()
}

/// Repairs one stream-copy-repairable media file in place.
pub fn repair_video(path: impl AsRef<Path>) -> std::io::Result<RepairStatus> {
    repair_video_with_tools(path, Path::new("ffprobe"), Path::new("ffmpeg"))
}

/// Repairs one media file using explicitly selected media tools.
pub fn repair_video_with_tools(
    path: impl AsRef<Path>,
    ffprobe: &Path,
    ffmpeg: &Path,
) -> std::io::Result<RepairStatus> {
    let path = path.as_ref();
    let status = check_video_with_tools(path, ffprobe, ffmpeg)?;
    let MediaStatus::NeedsRepair(issue) = status else {
        return Ok(match status {
            MediaStatus::Ok => RepairStatus::Failed("file does not need repair".into()),
            MediaStatus::Unrepairable(issue) => {
                RepairStatus::Unrepairable(issue.reason().to_owned())
            }
            MediaStatus::NeedsRepair(_) => unreachable!(),
        });
    };
    if issue.strategy() != RepairStrategy::StreamCopy {
        return Ok(RepairStatus::Failed(format!(
            "{} requires {:?}",
            issue.reason(),
            issue.strategy()
        )));
    }
    let reason = issue.reason();
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("video");
    let temporary = path.with_file_name(format!(
        "{}.fixed.{extension}",
        path.file_stem().unwrap_or_default().to_string_lossy()
    ));
    let result = Command::new(ffmpeg)
        .args(["-hide_banner", "-loglevel", "error", "-y", "-i"])
        .arg(path)
        .args(["-c", "copy", "-movflags", "+faststart"])
        .arg(&temporary)
        .output()?;
    if !result.status.success() || !temporary.is_file() || std::fs::metadata(&temporary)?.len() == 0
    {
        let _ = std::fs::remove_file(&temporary);
        return Ok(RepairStatus::Failed(format!(
            "{reason}: ffmpeg repair failed"
        )));
    }
    if let Err(validation) = validate_video(&temporary, ffprobe, ffmpeg) {
        let _ = std::fs::remove_file(&temporary);
        return Ok(RepairStatus::Failed(format!(
            "{reason}: repaired output failed validation: {validation}"
        )));
    }
    std::fs::rename(temporary, path)?;
    Ok(RepairStatus::Repaired)
}

fn validate_video(path: &Path, ffprobe: &Path, ffmpeg: &Path) -> Result<(), String> {
    let probe = Command::new(ffprobe)
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=format_name",
            "-of",
            "default=nw=1:nk=1",
        ])
        .arg(path)
        .output()
        .map_err(|error| error.to_string())?;
    let format = String::from_utf8_lossy(&probe.stdout).trim().to_owned();
    if !probe.status.success() || format.is_empty() {
        return Err("ffprobe could not identify a container".into());
    }
    let decode = Command::new(ffmpeg)
        .args(["-v", "error", "-i"])
        .arg(path)
        .args(["-f", "null", "-"])
        .output()
        .map_err(|error| error.to_string())?;
    let diagnostics = String::from_utf8_lossy(&decode.stderr).trim().to_owned();
    if !decode.status.success() {
        return Err(if diagnostics.is_empty() {
            "ffmpeg decode failed".into()
        } else {
            diagnostics
        });
    }
    if !diagnostics.is_empty() && !diagnostics_are_valid_after_repair(&diagnostics) {
        return Err(diagnostics);
    }
    Ok(())
}
