use std::fs;
use std::path::PathBuf;

use yt_dlp_4me_rusty::{
    Section, cleanup_markdown, expand_path, extract_link, move_link_to_section, parse_markdown,
    parse_text,
};

#[path = "common/mod.rs"]
mod common;

use common::temp_file;

#[test]
fn extracts_markdown_url() {
    assert_eq!(
        extract_link("- [Video](https://example.com/a)"),
        Some("https://example.com/a".into())
    );
}

#[test]
fn extracts_first_url_and_handles_punctuation_or_missing_urls() {
    assert_eq!(
        extract_link("prefix https://first.example/a, then http://second.example/b"),
        Some("https://first.example/a".into())
    );
    assert_eq!(
        extract_link("https://example.com/a (label)"),
        Some("https://example.com/a".into())
    );
    assert_eq!(extract_link("no link here"), None);
    assert_eq!(extract_link("https://"), Some("https://".into()));
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
fn parses_markdown_heading_boundaries_case_insensitively() {
    let path =
        temp_file("# dOwNlOaDeD\n### Failed\n## fAiLeD extra\n## Not downloaded yet\n## Unknown\n");
    let lines = parse_markdown(&path);
    assert_eq!(lines[0].0, Section::Downloaded);
    assert_eq!(lines[1].0, Section::Downloaded);
    assert_eq!(lines[2].0, Section::Failed);
    assert_eq!(lines[3].0, Section::Pending);
    assert_eq!(lines[4].0, Section::Other);
    let _ = fs::remove_file(path);
}

#[test]
fn parses_empty_and_invalid_paths_as_empty() {
    let empty = temp_file("");
    assert!(parse_markdown(&empty).is_empty());
    assert!(parse_text(&empty).is_empty());
    assert!(parse_markdown("/tmp/yt-dlp-no-such-markdown").is_empty());
    assert!(parse_text("/tmp/yt-dlp-no-such-text").is_empty());
    let _ = fs::remove_file(empty);
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

#[test]
fn moves_link_into_existing_section_without_losing_neighbors() {
    let path = temp_file(
        "# Queue\n\n## Not downloaded yet\n- https://x\n\n## Downloaded\nintro\n- https://y\n\n## Notes\nkeep\n",
    );
    move_link_to_section(&path, "https://x", Section::Downloaded, Section::Pending).unwrap();
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.contains("## Downloaded\nintro\n- https://y\n- https://x"));
    assert!(text.contains("## Notes\nkeep\n"));
    assert!(!text.contains("Not downloaded yet\n- https://x"));
    let _ = fs::remove_file(path);
}

#[test]
fn moving_missing_or_non_section_link_leaves_queue_unchanged() {
    let path = temp_file("## Pending\n- https://x\n");
    let before = fs::read_to_string(&path).unwrap();
    move_link_to_section(
        &path,
        "https://missing",
        Section::Downloaded,
        Section::Pending,
    )
    .unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), before);
    move_link_to_section(&path, "https://x", Section::Other, Section::Pending).unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), before);
    let _ = fs::remove_file(path);
}

#[test]
fn moving_link_creates_target_before_neighboring_content() {
    let path = temp_file("## Not downloaded yet\n- https://x\n## Notes\nkeep\n");
    move_link_to_section(&path, "https://x", Section::Downloaded, Section::Pending).unwrap();
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.contains("## Notes\nkeep\n"));
    assert!(text.contains("## Downloaded\n- https://x\n"));
    let _ = fs::remove_file(path);
}

#[test]
fn preserves_relative_paths() {
    assert_eq!(
        expand_path("~"),
        PathBuf::from(std::env::var_os("HOME").unwrap())
    );
    assert_eq!(expand_path("queue.md"), PathBuf::from("queue.md"));
    assert_eq!(expand_path("downloads"), PathBuf::from("downloads"));
}

#[test]
fn reports_missing_queue_file() {
    let path = std::env::temp_dir().join("yt-dlp-missing-queue-file.md");
    let error = cleanup_markdown(&path).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
}

#[test]
fn cleanup_empty_markdown_queue_is_a_no_op() {
    let path = temp_file("");
    cleanup_markdown(&path).unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), "");
    let _ = fs::remove_file(path);
}

#[test]
fn cleanup_prioritizes_failed_over_pending_and_preserves_other_content() {
    let path = temp_file(
        "# Queue\n## Failed\n- https://failed\n## Not downloaded yet\n- https://failed\n- https://pending\n## Notes\n- https://failed\n",
    );
    cleanup_markdown(&path).unwrap();
    let text = fs::read_to_string(&path).unwrap();
    assert_eq!(text.matches("https://failed").count(), 2);
    assert!(text.contains("## Failed\n- https://failed\n"));
    assert!(text.contains("## Notes\n- https://failed\n"));
    assert_eq!(text.matches("https://pending").count(), 1);
    let _ = fs::remove_file(path);
}
