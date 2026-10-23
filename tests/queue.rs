use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use yt_dlp_4me_rusty::{
    Section, cleanup_markdown, extract_link, move_link_to_section, parse_markdown,
};

fn temp_file(content: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "yt-dlp-test-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::write(&path, content).unwrap();
    path
}

#[test]
fn extracts_markdown_url() {
    assert_eq!(
        extract_link("- [Video](https://example.com/a)"),
        Some("https://example.com/a".into())
    );
}

#[test]
fn parses_sections_and_missing_newline() {
    let path = temp_file("# Q\n## Downloaded\nhttps://x");
    let map = parse_markdown(&path);
    assert_eq!(map[2].0, Section::Downloaded);
    assert!(map[2].1.ends_with('\n'));
    let _ = fs::remove_file(path);
}

#[test]
fn cleanup_prioritizes_downloaded() {
    let path = temp_file(
        "## Downloaded\n- https://x\n## Failed\n- https://x\n## Not downloaded yet\n- https://x\n- https://y\n",
    );
    cleanup_markdown(&path).unwrap();
    let text = fs::read_to_string(&path).unwrap();
    assert_eq!(text.matches("https://x").count(), 1);
    assert!(text.contains("https://y"));
    let _ = fs::remove_file(path);
}

#[test]
fn moves_link_and_creates_section() {
    let path = temp_file("## Not downloaded yet\n- https://x\n");
    move_link_to_section(&path, "https://x", Section::Downloaded, Section::Pending).unwrap();
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.contains("## Downloaded\n- https://x"));
    assert!(!text.contains("Not downloaded yet\n- https://x"));
    let _ = fs::remove_file(path);
}
