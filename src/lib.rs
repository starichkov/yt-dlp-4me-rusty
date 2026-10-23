use std::collections::HashSet;
use std::env;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Name of the section containing successfully downloaded links.
pub const DOWNLOADED: &str = "Downloaded";
/// Name of the section containing links whose downloads failed.
pub const FAILED: &str = "Failed";
/// Name of the section containing links waiting to be downloaded.
pub const PENDING: &str = "Not downloaded yet";
const URL_PREFIXES: [&str; 2] = ["http://", "https://"];

/// A logical section in the Markdown queue.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Section {
    Downloaded,
    Failed,
    Pending,
    Other,
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

fn header_kind(line: &str) -> Option<Section> {
    let s = line.trim();
    let hashes = s.chars().take_while(|c| *c == '#').count();
    if hashes == 0 || hashes > 2 || s.chars().nth(hashes) == Some('#') {
        return None;
    }
    let name = s[hashes..].trim().to_ascii_lowercase();
    if name.starts_with("downloaded") {
        Some(Section::Downloaded)
    } else if name.starts_with("failed") {
        Some(Section::Failed)
    } else if name.starts_with("not downloaded yet") {
        Some(Section::Pending)
    } else {
        Some(Section::Other)
    }
}

/// Parses a queue into `(section, line)` pairs while preserving line text.
pub fn parse_markdown(path: impl AsRef<Path>) -> Vec<(Section, String)> {
    let path = expand_path(path.as_ref().to_string_lossy().as_ref());
    let Ok(content) = fs::read_to_string(path) else {
        return Vec::new();
    };
    let mut section = Section::Other;
    let mut lines: Vec<String> = content.split_inclusive('\n').map(str::to_string).collect();
    if !content.is_empty()
        && !content.ends_with(['\n', '\r'])
        && let Some(last) = lines.last_mut()
    {
        last.push('\n');
    }
    lines
        .into_iter()
        .map(|text| {
            if let Some(kind) = header_kind(&text) {
                section = kind;
            }
            (section, text)
        })
        .collect()
}

/// Extracts the first HTTP or HTTPS URL from a Markdown line.
pub fn extract_link(line: &str) -> Option<String> {
    let start = URL_PREFIXES
        .iter()
        .filter_map(|prefix| line.find(prefix))
        .min()?;
    let end = line[start..]
        .char_indices()
        .find(|(_, c)| c.is_whitespace() || ")]>'\",".contains(*c))
        .map(|(i, _)| start + i)
        .unwrap_or(line.len());
    if end == start {
        None
    } else {
        Some(line[start..end].to_string())
    }
}

fn write_map(path: &Path, map: &[(Section, String)]) -> std::io::Result<()> {
    let mut out = String::new();
    for (_, line) in map {
        out.push_str(line);
    }
    fs::write(path, out)
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
    let target_header = match target {
        Section::Downloaded => "## Downloaded",
        Section::Failed => "## Failed",
        Section::Pending => "## Not downloaded yet",
        Section::Other => return Ok(()),
    };
    if let Some(header) = map
        .iter()
        .position(|(s, l)| *s == target && header_kind(l) == Some(target))
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
    write_map(&path, &map)
}

/// Removes duplicate links, preferring downloaded links over failed and pending links.
pub fn cleanup_markdown(path: impl AsRef<Path>) -> std::io::Result<()> {
    let path = expand_path(path.as_ref().to_string_lossy().as_ref());
    let map = parse_markdown(&path);
    if map.is_empty() {
        return Ok(());
    }
    let downloaded: HashSet<String> = map
        .iter()
        .filter(|(s, _)| *s == Section::Downloaded)
        .filter_map(|(_, l)| extract_link(l))
        .collect();
    let failed: HashSet<String> = map
        .iter()
        .filter(|(s, _)| *s == Section::Failed)
        .filter_map(|(_, l)| extract_link(l))
        .collect();
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
    write_map(&path, &result)
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
            already |= line.contains("has already been downloaded");
            gone |= line.contains("HTTP Error 410") || line.contains("410 Gone");
            println!("{line}");
        }
    }
    if let Some(stderr) = child.stderr.take() {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            gone |= line.contains("HTTP Error 410") || line.contains("410 Gone");
            eprintln!("{line}");
        }
    }
    (
        child.wait().map(|s| s.success()).unwrap_or(false),
        already,
        gone,
    )
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
