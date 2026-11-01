use std::collections::HashSet;
use std::env;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

mod file;

pub use file::{MarkdownFile, QueueFile, extract_link, parse_markdown};
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
    let mut map = parse_markdown(&path);
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
    let map = parse_markdown(&path);
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
    let mut already = false;
    let mut gone = false;
    if let Some(stdout) = child.stdout.take() {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            already |= is_already_downloaded(&line);
            gone |= is_gone(&line);
            println!("{line}");
        }
    }
    if let Some(stderr) = child.stderr.take() {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            gone |= is_gone(&line);
            eprintln!("{line}");
        }
    }
    (
        child.wait().map(|s| s.success()).unwrap_or(false),
        already,
        gone,
    )
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
) -> (Vec<String>, Vec<String>, Vec<String>) {
    let source = match mode {
        Mode::Download => Section::Pending,
        Mode::Verify => Section::Downloaded,
        Mode::Retry => Section::Failed,
    };
    let mut ok = Vec::new();
    let mut bad = Vec::new();
    let mut skipped = Vec::new();
    loop {
        let map = parse_markdown(file);
        let Some(link) = map
            .iter()
            .find(|(s, _)| *s == source)
            .and_then(|(_, l)| extract_link(l))
        else {
            break;
        };
        if ok.contains(&link) || skipped.contains(&link) {
            if mode != Mode::Verify {
                let _ = move_link_to_section(file, &link, Section::Downloaded, source);
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
                let _ = move_link_to_section(file, &link, Section::Downloaded, source);
            }
        } else {
            let _ = gone;
            bad.push(link.clone());
            let _ = move_link_to_section(file, &link, Section::Failed, source);
        }
    }
    (ok, bad, skipped)
}
