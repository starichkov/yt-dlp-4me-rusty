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

/// A logical section in the Markdown queue.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Section {
    Downloaded,
    Failed,
    Pending,
    Other,
}

impl Section {
    fn name(self) -> Option<&'static str> {
        match self {
            Self::Downloaded => Some(DOWNLOADED),
            Self::Failed => Some(FAILED),
            Self::Pending => Some(PENDING),
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

/// Removes duplicate links, preferring downloaded links over failed and pending links.
pub fn cleanup_markdown(path: impl AsRef<Path>) -> std::io::Result<()> {
    let path = expand_path(path.as_ref().to_string_lossy().as_ref());
    let map = MarkdownFile::read(&path)?.lines().to_vec();
    if map.is_empty() {
        return Ok(());
    }
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
            Section::Downloaded => !seen.insert(link.clone()),
            Section::Failed => downloaded.contains(&link) || !seen.insert(link.clone()),
            Section::Pending => {
                downloaded.contains(&link) || failed.contains(&link) || !seen.insert(link.clone())
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
    println!("\n>>> Starting download: {link}");
    let child = Command::new(executable)
        .args(["--color", "always", "-P"])
        .arg(output)
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
        let (success, existing, gone) = download_video(&link, output, executable);
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
    }
}

fn cleanup_text_states(path: &Path) -> std::io::Result<()> {
    let paths = [
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
