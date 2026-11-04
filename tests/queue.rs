use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use yt_dlp_4me_rusty::{
    MarkdownFile, Mode, QueueFile, Section, TextFile, cleanup_markdown, cleanup_queue,
    cleanup_text, download_video, expand_path, extract_link, move_link_to_section, parse_markdown,
    parse_text, process_queue,
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

fn temp_path(prefix: &str, suffix: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "{prefix}-{}-{suffix}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

fn fake_downloader(output: &str, success: bool) -> PathBuf {
    let path = temp_path("yt-dlp-fake", "sh");
    fs::write(
        &path,
        format!(
            "#!/bin/sh\nprintf '%s\\n' '{}'\nexit {}\n",
            output,
            if success { 0 } else { 1 }
        ),
    )
    .unwrap();
    let mut permissions = fs::metadata(&path).unwrap().permissions();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        permissions.set_mode(0o755);
        fs::set_permissions(&path, permissions).unwrap();
    }
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
fn processes_links_from_tilde_path() {
    let name = format!(
        "yt-dlp-tilde-queue-{}.md",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let home = std::env::var_os("HOME").unwrap();
    let path = PathBuf::from(&home).join(&name);
    fs::write(
        &path,
        "## Not downloaded yet\n- https://example.com/video\n",
    )
    .unwrap();
    let tilde_path = format!("~/{name}");
    let output = std::env::temp_dir().join(format!("yt-dlp-output-{}", name));

    let (downloaded, failed, skipped) = process_queue(
        PathBuf::from(&tilde_path).as_path(),
        &output,
        Mode::Download,
        PathBuf::from("/bin/false").as_path(),
    )
    .unwrap();

    assert!(downloaded.is_empty());
    assert_eq!(failed, vec!["https://example.com/video"]);
    assert!(skipped.is_empty());
    let _ = fs::remove_file(path);
    let _ = fs::remove_dir_all(output);
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

#[test]
fn process_queue_moves_successful_downloads() {
    let queue = temp_file("## Not downloaded yet\n- https://success\n");
    let output = temp_path("yt-dlp-output", "dir");
    let executable = fake_downloader("download complete", true);
    let result = process_queue(&queue, &output, Mode::Download, &executable).unwrap();
    assert_eq!(result.0, vec!["https://success"]);
    assert!(result.1.is_empty());
    assert!(result.2.is_empty());
    assert!(
        fs::read_to_string(&queue)
            .unwrap()
            .contains("## Downloaded\n- https://success")
    );
    let _ = fs::remove_file(queue);
    let _ = fs::remove_file(executable);
    let _ = fs::remove_dir_all(output);
}

#[test]
fn process_queue_removes_successful_plain_text_urls_and_keeps_failures() {
    let queue = temp_path("yt-dlp-text-queue", ".txt");
    let failed_queue = temp_path("yt-dlp-text-failed-queue", ".txt");
    let output = temp_path("yt-dlp-text-output", "dir");
    let successful_executable = fake_downloader("download complete", true);
    let failed_executable = fake_downloader("download failed", false);
    fs::write(&queue, "phone note\nhttps://example.com/success\n").unwrap();
    fs::write(&failed_queue, "https://example.com/failure\n").unwrap();

    let result = process_queue(&queue, &output, Mode::Download, &successful_executable).unwrap();
    assert_eq!(result.0, vec!["https://example.com/success"]);
    assert!(
        !fs::read_to_string(&queue)
            .unwrap()
            .contains("https://example.com/success")
    );
    assert!(fs::read_to_string(&queue).unwrap().contains("phone note\n"));

    let failed = process_queue(&failed_queue, &output, Mode::Download, &failed_executable).unwrap();
    assert!(failed.0.is_empty());
    assert_eq!(failed.1, vec!["https://example.com/failure"]);
    assert!(
        fs::read_to_string(&failed_queue)
            .unwrap()
            .contains("https://example.com/failure")
    );

    let _ = fs::remove_file(queue);
    let _ = fs::remove_file(failed_queue);
    let _ = fs::remove_file(successful_executable);
    let _ = fs::remove_file(failed_executable);
    let _ = fs::remove_dir_all(output);
}

#[test]
fn process_queue_supports_verify_and_retry_modes() {
    let verify_queue = temp_file("## Downloaded\n- https://verify\n");
    let retry_queue = temp_file("## Failed\n- https://retry\n");
    let output = temp_path("yt-dlp-output", "dir");
    let executable = fake_downloader("ok", true);

    let verified = process_queue(&verify_queue, &output, Mode::Verify, &executable).unwrap();
    assert_eq!(verified.0, vec!["https://verify"]);
    assert!(verified.1.is_empty());

    let retried = process_queue(&retry_queue, &output, Mode::Retry, &executable).unwrap();
    assert_eq!(retried.0, vec!["https://retry"]);
    assert!(
        fs::read_to_string(&retry_queue)
            .unwrap()
            .contains("## Downloaded\n- https://retry")
    );

    let _ = fs::remove_file(verify_queue);
    let _ = fs::remove_file(retry_queue);
    let _ = fs::remove_file(executable);
    let _ = fs::remove_dir_all(output);
}

#[test]
fn process_queue_handles_duplicate_success_and_failure_links() {
    let success_queue = temp_file("## Not downloaded yet\n- https://same\n- https://same\n");
    let failure_queue = temp_file("## Not downloaded yet\n- https://bad\n- https://bad\n");
    let output = temp_path("yt-dlp-duplicate-output", "dir");
    let success = fake_downloader("ok", true);
    let failure = fake_downloader("bad", false);

    let result = process_queue(&success_queue, &output, Mode::Download, &success).unwrap();
    assert_eq!(result.0, vec!["https://same"]);
    assert!(result.1.is_empty());
    let result = process_queue(&failure_queue, &output, Mode::Download, &failure).unwrap();
    assert_eq!(result.1, vec!["https://bad"]);

    let _ = fs::remove_file(success_queue);
    let _ = fs::remove_file(failure_queue);
    let _ = fs::remove_file(success);
    let _ = fs::remove_file(failure);
    let _ = fs::remove_dir_all(output);
}

#[test]
fn verify_mode_stops_after_duplicate_and_text_modes_are_download_only() {
    let verify_queue = temp_file("## Downloaded\n- https://same\n- https://same\n");
    let text_queue = temp_path("yt-dlp-uppercase", ".TXT");
    let output = temp_path("yt-dlp-mode-output", "dir");
    let executable = fake_downloader("ok", true);
    fs::write(&text_queue, "https://text\n").unwrap();

    let verified = process_queue(&verify_queue, &output, Mode::Verify, &executable).unwrap();
    assert_eq!(verified.0, vec!["https://same"]);
    let retried_text = process_queue(&text_queue, &output, Mode::Retry, &executable).unwrap();
    assert!(retried_text.0.is_empty());
    assert_eq!(parse_text(&text_queue).len(), 1);
    cleanup_queue(&text_queue).unwrap();

    let _ = fs::remove_file(verify_queue);
    let _ = fs::remove_file(text_queue);
    let _ = fs::remove_file(executable);
    let _ = fs::remove_dir_all(output);
}

#[test]
fn download_video_detects_existing_and_gone_results() {
    let output = temp_path("yt-dlp-output", "dir");
    let already = fake_downloader("[download] video has already been downloaded", true);
    let gone = fake_downloader("ERROR: HTTP Error 410: Gone", false);

    assert_eq!(
        download_video("https://already", &output, &already),
        (true, true, false)
    );
    assert_eq!(
        download_video("https://gone", &output, &gone),
        (false, false, true)
    );

    let _ = fs::remove_file(already);
    let _ = fs::remove_file(gone);
    let _ = fs::remove_dir_all(output);
}

#[test]
fn download_video_detects_stderr_and_spawn_failure() {
    let output = temp_path("yt-dlp-output", "dir");
    let stderr = temp_path("yt-dlp-stderr", "sh");
    fs::write(
        &stderr,
        "#!/bin/sh\nprintf '%s\\n' '410 Gone' >&2\nexit 1\n",
    )
    .unwrap();
    let mut permissions = fs::metadata(&stderr).unwrap().permissions();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        permissions.set_mode(0o755);
        fs::set_permissions(&stderr, permissions).unwrap();
    }
    assert_eq!(
        download_video("https://stderr", &output, &stderr),
        (false, false, true)
    );
    assert_eq!(
        download_video(
            "https://missing",
            &output,
            Path::new("/tmp/yt-dlp-no-such-executable")
        ),
        (false, false, false)
    );
    let _ = fs::remove_file(stderr);
    let _ = fs::remove_dir_all(output);
}
