use std::collections::HashSet;
use std::env;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;

mod file;

pub use file::{MarkdownFile, QueueFile, TextFile, extract_link, parse_markdown, parse_text};
use file::{is_header, links_in_section, write_lines};

/// Name of the section containing successfully downloaded links.
pub const DOWNLOADED: &str = "Downloaded";
/// Name of the section containing links whose downloads failed.
pub const FAILED: &str = "Failed";
/// Name of the section containing links waiting to be downloaded.
pub const PENDING: &str = "Not downloaded yet";
/// Name of the section containing links whose media must be downloaded again.
pub const REDOWNLOAD: &str = "Needs redownload";

/// A logical section in the Markdown queue.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Section {
    Downloaded,
    Failed,
    Pending,
    Redownload,
    Other,
}

impl Section {
    fn name(self) -> Option<&'static str> {
        match self {
            Self::Downloaded => Some(DOWNLOADED),
            Self::Failed => Some(FAILED),
            Self::Pending => Some(PENDING),
            Self::Redownload => Some(REDOWNLOAD),
            Self::Other => None,
        }
    }

    fn header(self) -> Option<String> {
        self.name().map(|name| format!("## {name}"))
    }
}

/// The operation to perform on the queue.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Download,
    Verify,
    Retry,
    Redownload,
}

/// The result of checking a media file with `ffprobe` and `ffmpeg`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MediaStatus {
    /// The file has a recognized container and decoded without errors.
    Ok,
    /// The file can be considered for stream-copy repair.
    NeedsRepair(String),
    /// The file contains corruption that remuxing cannot repair.
    Unrepairable(String),
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
    if let Some(reason) = unrepairable_diagnostics(&diagnostics) {
        return Ok(MediaStatus::Unrepairable(reason));
    }
    if format.contains("mpegts") {
        return Ok(MediaStatus::NeedsRepair("MPEG-TS container".into()));
    }
    if !probe.status.success() || !decode.status.success() || !diagnostics.is_empty() {
        return Ok(MediaStatus::NeedsRepair(if diagnostics.is_empty() {
            "media decode failed".into()
        } else {
            diagnostics
        }));
    }
    Ok(MediaStatus::Ok)
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
    let MediaStatus::NeedsRepair(reason) = status else {
        return Ok(match status {
            MediaStatus::Ok => RepairStatus::Failed("file does not need repair".into()),
            MediaStatus::Unrepairable(reason) => RepairStatus::Unrepairable(reason),
            MediaStatus::NeedsRepair(_) => unreachable!(),
        });
    };
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
    std::fs::rename(temporary, path)?;
    Ok(RepairStatus::Repaired)
}

fn unrepairable_diagnostics(diagnostics: &str) -> Option<String> {
    let lower = diagnostics.to_ascii_lowercase();
    if lower.contains("missing picture in access unit") || lower.contains("invalid nal unit size") {
        Some(diagnostics.to_owned())
    } else {
        None
    }
}

/// Expands a leading `~` using the current user's home directory.
pub fn expand_path(value: &str) -> PathBuf {
    if value == "~" {
        return dirs_home();
    }
    if let Some(rest) = value.strip_prefix("~/") {
        let mut home = dirs_home();
        home.push(rest);
        home
    } else {
        PathBuf::from(value)
    }
}

fn dirs_home() -> PathBuf {
    env::var_os("HOME").map(PathBuf::from).unwrap_or_default()
}

/// Moves one link from `source` to `target`, creating the target section when needed.
pub fn move_link_to_section(
    path: impl AsRef<Path>,
    link: &str,
    target: Section,
    source: Section,
) -> std::io::Result<()> {
    let path = expand_path(path.as_ref().to_string_lossy().as_ref());
    let mut map = MarkdownFile::read(&path)?.lines().to_vec();
    let Some(found) = map
        .iter()
        .position(|(s, l)| *s == source && extract_link(l).as_deref() == Some(link))
    else {
        return Ok(());
    };
    let moving = map.remove(found);
    let Some(target_header) = target.header() else {
        return Ok(());
    };
    if let Some(header) = map
        .iter()
        .position(|(s, l)| *s == target && is_header(l, target))
    {
        let mut insert = header + 1;
        while insert < map.len() && map[insert].0 == target {
            if !map[insert].1.trim().is_empty() {
                insert += 1;
            } else {
                break;
            }
        }
        map.insert(insert, (target, moving.1));
    } else {
        if map.last().is_some_and(|(_, l)| !l.trim().is_empty()) {
            map.push((Section::Other, "\n".into()));
        }
        map.push((target, format!("{target_header}\n")));
        map.push((target, moving.1));
    }
    write_lines(&path, map)
}

/// Removes duplicate links, preferring redownload and downloaded links over
/// failed and pending links.
pub fn cleanup_markdown(path: impl AsRef<Path>) -> std::io::Result<()> {
    let path = expand_path(path.as_ref().to_string_lossy().as_ref());
    let map = MarkdownFile::read(&path)?.lines().to_vec();
    if map.is_empty() {
        return Ok(());
    }
    let redownload = links_in_section(&map, Section::Redownload);
    let downloaded = links_in_section(&map, Section::Downloaded);
    let failed = links_in_section(&map, Section::Failed);
    let mut seen = HashSet::new();
    let mut result = Vec::new();
    for (section, line) in map {
        let Some(link) = extract_link(&line) else {
            result.push((section, line));
            continue;
        };
        let duplicate = match section {
            Section::Redownload => !seen.insert(link.clone()),
            Section::Downloaded => redownload.contains(&link) || !seen.insert(link.clone()),
            Section::Failed => {
                redownload.contains(&link)
                    || downloaded.contains(&link)
                    || !seen.insert(link.clone())
            }
            Section::Pending => {
                redownload.contains(&link)
                    || downloaded.contains(&link)
                    || failed.contains(&link)
                    || !seen.insert(link.clone())
            }
            Section::Other => false,
        };
        if !duplicate {
            result.push((section, line));
        }
    }
    write_lines(&path, result)
}

/// Removes duplicate URLs from a plain-text inbox, preserving the first occurrence.
pub fn cleanup_text(path: impl AsRef<Path>) -> std::io::Result<()> {
    let path = expand_path(path.as_ref().to_string_lossy().as_ref());
    let file = TextFile::read(&path)?;
    let mut seen = HashSet::new();
    let lines = file
        .lines()
        .iter()
        .filter(|(_, line)| extract_link(line).is_none_or(|link| seen.insert(link)))
        .cloned()
        .collect();
    write_text_lines(&path, lines)
}

/// Removes duplicate URLs using the queue format selected by the file extension.
pub fn cleanup_queue(path: impl AsRef<Path>) -> std::io::Result<()> {
    if is_text_path(path.as_ref()) {
        cleanup_text_states(path.as_ref())
    } else {
        cleanup_markdown(path)
    }
}

/// Runs `yt-dlp` and returns `(success, already_existed, gone_410)`.
pub fn download_video(link: &str, output: &Path, executable: &Path) -> (bool, bool, bool) {
    download_video_with_format(link, output, executable, None)
}

/// Runs `yt-dlp` with an optional format selector.
pub fn download_video_with_format(
    link: &str,
    output: &Path,
    executable: &Path,
    format: Option<&str>,
) -> (bool, bool, bool) {
    println!("\n>>> Starting download: {link}");
    let mut command = Command::new(executable);
    command.args(["--color", "always", "-P"]).arg(output);
    if let Some(format) = format {
        command.args(["-f", format]);
    }
    let child = command
        .arg(link)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    let Ok(mut child) = child else {
        return (false, false, false);
    };
    let stdout_thread = child
        .stdout
        .take()
        .map(|stdout| thread::spawn(move || stream_output(stdout, false)));
    let stderr_thread = child
        .stderr
        .take()
        .map(|stderr| thread::spawn(move || stream_output(stderr, true)));
    let (stdout_already, stdout_gone) = stdout_thread
        .and_then(|thread| thread.join().ok())
        .unwrap_or_default();
    let (stderr_already, stderr_gone) = stderr_thread
        .and_then(|thread| thread.join().ok())
        .unwrap_or_default();
    (
        child.wait().map(|s| s.success()).unwrap_or(false),
        stdout_already || stderr_already,
        stdout_gone || stderr_gone,
    )
}

fn stream_output<R: Read>(mut reader: R, is_stderr: bool) -> (bool, bool) {
    let mut buffer = [0; 4096];
    let mut output = Vec::new();
    loop {
        let count = match reader.read(&mut buffer) {
            Ok(0) | Err(_) => break,
            Ok(count) => count,
        };
        output.extend_from_slice(&buffer[..count]);
        let text = String::from_utf8_lossy(&buffer[..count]);
        if is_stderr {
            eprint!("{text}");
            let _ = std::io::stderr().flush();
        } else {
            print!("{text}");
            let _ = std::io::stdout().flush();
        }
    }
    let text = String::from_utf8_lossy(&output);
    (is_already_downloaded(&text), is_gone(&text))
}

fn is_already_downloaded(line: &str) -> bool {
    line.contains("has already been downloaded")
}

fn is_gone(line: &str) -> bool {
    line.contains("HTTP Error 410") || line.contains("410 Gone")
}

/// Processes links in the section selected by `mode` until no links remain.
pub fn process_queue(
    file: &Path,
    output: &Path,
    mode: Mode,
    executable: &Path,
) -> std::io::Result<(Vec<String>, Vec<String>, Vec<String>)> {
    let source_file = queue_path_for_mode(file, mode);
    let source = match mode {
        Mode::Download => Section::Pending,
        Mode::Verify => Section::Downloaded,
        Mode::Retry => Section::Failed,
        Mode::Redownload => Section::Redownload,
    };
    let mut ok = Vec::new();
    let mut bad = Vec::new();
    let mut skipped = Vec::new();
    loop {
        if is_text_path(file) && !source_file.exists() {
            break;
        }
        let map = if is_text_path(file) {
            read_queue(&source_file)?
                .lines()
                .iter()
                .map(|(_, line)| (source, line.clone()))
                .collect()
        } else {
            read_queue(&source_file)?.lines().to_vec()
        };
        let Some(link) = map
            .iter()
            .filter(|(s, _)| *s == source)
            .find_map(|(_, l)| extract_link(l))
        else {
            break;
        };
        if ok.contains(&link) || skipped.contains(&link) {
            if mode != Mode::Verify {
                move_link_in_queue(file, &link, Section::Downloaded, source)?;
            } else {
                break;
            }
            continue;
        }
        if bad.contains(&link) {
            break;
        }
        let format = (mode == Mode::Redownload).then_some("bv*+ba/b");
        let (success, existing, gone) =
            download_video_with_format(&link, output, executable, format);
        if success {
            if existing {
                skipped.push(link.clone());
            } else {
                ok.push(link.clone());
            }
            if mode != Mode::Verify {
                move_link_in_queue(file, &link, Section::Downloaded, source)?;
            }
        } else {
            let _ = gone;
            bad.push(link.clone());
            move_link_in_queue(file, &link, Section::Failed, source)?;
        }
    }
    Ok((ok, bad, skipped))
}

fn is_text_path(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("txt"))
}

fn text_state_path(path: &Path, state: &str) -> PathBuf {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("txt");
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("queue");
    path.with_file_name(format!("{stem}.{state}.{extension}"))
}

fn queue_path_for_mode(path: &Path, mode: Mode) -> PathBuf {
    if !is_text_path(path) {
        return path.to_path_buf();
    }
    match mode {
        Mode::Download => path.to_path_buf(),
        Mode::Retry => text_state_path(path, "failed"),
        Mode::Verify => text_state_path(path, "done"),
        Mode::Redownload => text_state_path(path, "redownload"),
    }
}

fn cleanup_text_states(path: &Path) -> std::io::Result<()> {
    let paths = [
        text_state_path(path, "redownload"),
        text_state_path(path, "done"),
        text_state_path(path, "failed"),
        path.to_path_buf(),
    ];
    let mut seen = HashSet::new();
    for state_path in paths {
        if !state_path.exists() {
            continue;
        }
        let file = TextFile::read(&state_path)?;
        let lines = file
            .lines()
            .iter()
            .filter(|(_, line)| extract_link(line).is_none_or(|link| seen.insert(link)))
            .cloned()
            .collect();
        write_text_lines(&state_path, lines)?;
    }
    Ok(())
}

fn read_queue(path: &Path) -> std::io::Result<Box<dyn QueueFile>> {
    if is_text_path(path) {
        Ok(Box::new(TextFile::read(path)?))
    } else {
        Ok(Box::new(MarkdownFile::read(path)?))
    }
}

fn move_link_in_queue(
    path: &Path,
    link: &str,
    target: Section,
    source: Section,
) -> std::io::Result<()> {
    if is_text_path(path) {
        if source == target {
            return Ok(());
        }
        let source_path = queue_path_for_mode(
            path,
            match source {
                Section::Pending => Mode::Download,
                Section::Failed => Mode::Retry,
                Section::Downloaded => Mode::Verify,
                Section::Redownload => Mode::Redownload,
                Section::Other => return Ok(()),
            },
        );
        let file = TextFile::read(&source_path)?;
        let lines = file
            .lines()
            .iter()
            .filter(|(_, line)| extract_link(line).as_deref() != Some(link))
            .cloned()
            .collect();
        write_text_lines(&source_path, lines)?;
        let target_path = match target {
            Section::Downloaded => text_state_path(path, "done"),
            Section::Failed => text_state_path(path, "failed"),
            Section::Redownload => text_state_path(path, "redownload"),
            Section::Pending | Section::Other => return Ok(()),
        };
        let mut target_lines = if target_path.exists() {
            TextFile::read(&target_path)?.lines().to_vec()
        } else {
            Vec::new()
        };
        target_lines.push((
            Section::Pending,
            file.lines()
                .iter()
                .find(|(_, line)| extract_link(line).as_deref() == Some(link))
                .map(|(_, line)| line.clone())
                .unwrap_or_else(|| format!("{link}\n")),
        ));
        write_text_lines(&target_path, target_lines)
    } else {
        move_link_to_section(path, link, target, source)
    }
}

fn write_text_lines(path: &Path, lines: Vec<(Section, String)>) -> std::io::Result<()> {
    TextFile::from_lines(lines).write(path)
}
