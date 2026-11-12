use std::fs;

use yt_dlp_4me_rusty::{
    MarkdownFile, QueueFile, Section, TextFile, cleanup_text, extract_link, parse_text,
};

#[path = "common/mod.rs"]
mod common;

use common::{temp_file, temp_path};

#[test]
fn plain_text_preserves_crlf_and_adds_final_newline() {
    let path = temp_file("note\r\nhttps://example.com/video");
    let lines = parse_text(&path);
    assert_eq!(lines[0].1, "note\r\n");
    assert_eq!(lines[1].1, "https://example.com/video\n");
    let _ = fs::remove_file(path);
}

#[test]
fn queue_file_lines_mut_can_edit_and_write() {
    let path = temp_file("old\n");
    let mut file = MarkdownFile::read(&path).unwrap();
    file.lines_mut()[0].1 = "new\n".into();
    file.write(&path).unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), "new\n");
    let _ = fs::remove_file(path);
}

#[test]
fn text_file_lines_mut_can_edit_and_write() {
    let path = temp_file("old\n");
    let mut file = TextFile::read(&path).unwrap();
    file.lines_mut()[0].1 = "new\n".into();
    file.write(&path).unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), "new\n");
    let _ = fs::remove_file(path);
}

#[test]
fn reads_markdown_through_file_interface() {
    let path = temp_file("## Downloaded\n- https://example.com/video\n");
    let file = MarkdownFile::read(&path).unwrap();
    assert_eq!(file.lines()[1].0, Section::Downloaded);
    assert_eq!(
        extract_link(&file.lines()[1].1),
        Some("https://example.com/video".into())
    );
    let _ = fs::remove_file(path);
}

#[test]
fn reads_plain_text_inbox_through_file_interface() {
    let path = temp_file("https://example.com/one\nnotes\nhttps://example.com/two");
    let file = TextFile::read(&path).unwrap();
    assert_eq!(file.lines().len(), 3);
    assert!(
        file.lines()
            .iter()
            .all(|(section, _)| *section == Section::Pending)
    );
    assert_eq!(parse_text(&path).len(), 3);
    let _ = fs::remove_file(path);
}

#[test]
fn plain_text_cleanup_keeps_first_url_and_non_url_lines() {
    let path = temp_path("yt-dlp-text-queue", ".txt");
    fs::write(
        &path,
        "keep this\nhttps://example.com/one\nhttps://example.com/one\nhttps://example.com/two\n",
    )
    .unwrap();
    cleanup_text(&path).unwrap();
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "keep this\nhttps://example.com/one\nhttps://example.com/two\n"
    );
    let _ = fs::remove_file(path);
}
