use std::fs;
use std::io;
use std::path::Path;

use crate::{Section, expand_path};

/// A file-backed queue representation.
///
/// Implement this trait to add support for another queue file format without
/// changing the queue-processing logic.
pub trait QueueFile {
    /// Reads and parses a queue file.
    fn read(path: &Path) -> io::Result<Self>
    where
        Self: Sized;

    /// Writes the queue representation to a file.
    fn write(&self, path: &Path) -> io::Result<()>;

    /// Returns the queue lines together with their logical sections.
    fn lines(&self) -> &[(Section, String)];

    /// Returns mutable queue lines together with their logical sections.
    fn lines_mut(&mut self) -> &mut Vec<(Section, String)>;
}

/// The Markdown implementation of [`QueueFile`].
#[derive(Clone, Debug, Default)]
pub struct MarkdownFile {
    lines: Vec<(Section, String)>,
}

impl MarkdownFile {
    /// Creates a Markdown file representation from parsed lines.
    pub(crate) fn from_lines(lines: Vec<(Section, String)>) -> Self {
        Self { lines }
    }
}

impl QueueFile for MarkdownFile {
    fn read(path: &Path) -> io::Result<Self> {
        let path = expand_path(path.to_string_lossy().as_ref());
        let content = fs::read_to_string(path)?;
        let mut section = Section::Other;
        let mut lines: Vec<String> = content.split_inclusive('\n').map(str::to_string).collect();
        if !content.is_empty()
            && !content.ends_with(['\n', '\r'])
            && let Some(last) = lines.last_mut()
        {
            last.push('\n');
        }
        Ok(Self::from_lines(
            lines
                .into_iter()
                .map(|text| {
                    if let Some(kind) = header_kind(&text) {
                        section = kind;
                    }
                    (section, text)
                })
                .collect(),
        ))
    }

    fn write(&self, path: &Path) -> io::Result<()> {
        let path = expand_path(path.to_string_lossy().as_ref());
        let content = self
            .lines
            .iter()
            .map(|(_, line)| line.as_str())
            .collect::<String>();
        fs::write(path, content)
    }

    fn lines(&self) -> &[(Section, String)] {
        &self.lines
    }

    fn lines_mut(&mut self) -> &mut Vec<(Section, String)> {
        &mut self.lines
    }
}

/// The plain-text URL inbox implementation of [`QueueFile`].
#[derive(Clone, Debug, Default)]
pub struct TextFile {
    lines: Vec<(Section, String)>,
}

impl TextFile {
    /// Creates a plain-text inbox representation from parsed lines.
    pub(crate) fn from_lines(lines: Vec<(Section, String)>) -> Self {
        Self { lines }
    }
}

impl QueueFile for TextFile {
    fn read(path: &Path) -> io::Result<Self> {
        let path = expand_path(path.to_string_lossy().as_ref());
        let content = fs::read_to_string(path)?;
        let mut lines: Vec<String> = content.split_inclusive('\n').map(str::to_string).collect();
        if !content.is_empty()
            && !content.ends_with(['\n', '\r'])
            && let Some(last) = lines.last_mut()
        {
            last.push('\n');
        }
        Ok(Self::from_lines(
            lines
                .into_iter()
                .map(|line| (Section::Pending, line))
                .collect(),
        ))
    }

    fn write(&self, path: &Path) -> io::Result<()> {
        let path = expand_path(path.to_string_lossy().as_ref());
        let content = self
            .lines
            .iter()
            .map(|(_, line)| line.as_str())
            .collect::<String>();
        fs::write(path, content)
    }

    fn lines(&self) -> &[(Section, String)] {
        &self.lines
    }

    fn lines_mut(&mut self) -> &mut Vec<(Section, String)> {
        &mut self.lines
    }
}

fn header_kind(line: &str) -> Option<Section> {
    let s = line.trim();
    let hashes = s.chars().take_while(|c| *c == '#').count();
    if hashes == 0 || hashes > 2 || s.chars().nth(hashes) == Some('#') {
        return None;
    }
    let name = s[hashes..].trim().to_ascii_lowercase();
    if name.starts_with(crate::DOWNLOADED.to_ascii_lowercase().as_str()) {
        Some(Section::Downloaded)
    } else if name.starts_with(crate::FAILED.to_ascii_lowercase().as_str()) {
        Some(Section::Failed)
    } else if name.starts_with(crate::PENDING.to_ascii_lowercase().as_str()) {
        Some(Section::Pending)
    } else if name.starts_with(crate::REDOWNLOAD.to_ascii_lowercase().as_str()) {
        Some(Section::Redownload)
    } else {
        Some(Section::Other)
    }
}

/// Parses a Markdown queue into `(section, line)` pairs.
pub fn parse_markdown(path: impl AsRef<Path>) -> Vec<(Section, String)> {
    MarkdownFile::read(path.as_ref())
        .map(|file| file.lines)
        .unwrap_or_default()
}

/// Parses a plain-text URL inbox into `(section, line)` pairs.
pub fn parse_text(path: impl AsRef<Path>) -> Vec<(Section, String)> {
    TextFile::read(path.as_ref())
        .map(|file| file.lines)
        .unwrap_or_default()
}

/// Extracts the first HTTP or HTTPS URL from a Markdown line.
pub fn extract_link(line: &str) -> Option<String> {
    const URL_PREFIXES: [&str; 2] = ["http://", "https://"];
    let start = URL_PREFIXES
        .iter()
        .filter_map(|prefix| line.find(prefix))
        .min()?;
    let end = line[start..]
        .char_indices()
        .find(|(_, c)| c.is_whitespace() || ")]>'\",".contains(*c))
        .map(|(i, _)| start + i)
        .unwrap_or(line.len());
    (end != start).then(|| line[start..end].to_string())
}

pub(crate) fn write_lines(path: &Path, lines: Vec<(Section, String)>) -> io::Result<()> {
    MarkdownFile::from_lines(lines).write(path)
}

pub(crate) fn is_header(line: &str, section: Section) -> bool {
    header_kind(line) == Some(section)
}

pub(crate) fn links_in_section(
    lines: &[(Section, String)],
    section: Section,
) -> std::collections::HashSet<String> {
    lines
        .iter()
        .filter(|(current, _)| *current == section)
        .filter_map(|(_, line)| extract_link(line))
        .collect()
}
