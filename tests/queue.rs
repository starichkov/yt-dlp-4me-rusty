use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use yt_dlp_4me_rusty::{
    MarkdownFile, Mode, QueueFile, Section, cleanup_markdown, download_video, expand_path,
    extract_link, move_link_to_section, parse_markdown, process_queue,
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
fn preserves_relative_paths() {
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
