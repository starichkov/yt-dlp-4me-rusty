use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use yt_dlp_4me_rusty::{Mode, cleanup_queue, download_video, parse_text, process_queue};

#[path = "common/mod.rs"]
mod common;

use common::{companion_path, fake_downloader, temp_file, temp_path};

#[test]
fn redownload_mode_uses_reliable_format_and_moves_link_to_downloaded() {
    let queue = temp_file("## Needs redownload\n- https://example.com/broken\n");
    let output = temp_path("yt-dlp-redownload-output", "dir");
    let arguments = temp_path("yt-dlp-redownload-arguments", "txt");
    let executable = temp_path("yt-dlp-redownload", "sh");
    fs::write(
        &executable,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\n",
            arguments.display()
        ),
    )
    .unwrap();
    let mut permissions = fs::metadata(&executable).unwrap().permissions();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        permissions.set_mode(0o755);
        fs::set_permissions(&executable, permissions).unwrap();
    }

    let result = process_queue(&queue, &output, Mode::Redownload, &executable).unwrap();

    assert_eq!(result.0, vec!["https://example.com/broken"]);
    assert!(
        fs::read_to_string(&queue)
            .unwrap()
            .contains("## Downloaded\n- https://example.com/broken")
    );
    let args = fs::read_to_string(&arguments).unwrap();
    assert!(args.lines().any(|line| line == "-f"));
    assert!(args.lines().any(|line| line == "bv*+ba/b"));

    for path in [queue, arguments, executable] {
        let _ = fs::remove_file(path);
    }
    let _ = fs::remove_dir_all(output);
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
    let failed_queue = companion_path(&queue, "failed");
    let output = temp_path("yt-dlp-text-output", "dir");
    let successful_executable = fake_downloader("download complete", true);
    let failed_executable = fake_downloader("download failed", false);
    fs::write(&queue, "phone note\nhttps://example.com/success\n").unwrap();

    let result = process_queue(&queue, &output, Mode::Download, &successful_executable).unwrap();
    assert_eq!(result.0, vec!["https://example.com/success"]);
    assert!(
        !fs::read_to_string(&queue)
            .unwrap()
            .contains("https://example.com/success")
    );
    assert!(fs::read_to_string(&queue).unwrap().contains("phone note\n"));

    fs::write(&queue, "phone note\nhttps://example.com/failure\n").unwrap();
    let failed = process_queue(&queue, &output, Mode::Download, &failed_executable).unwrap();
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
fn plain_text_retry_and_verify_use_companion_state_files() {
    let queue = temp_path("yt-dlp-text-state", ".txt");
    let failed = companion_path(&queue, "failed");
    let done = companion_path(&queue, "done");
    let output = temp_path("yt-dlp-text-state-output", "dir");
    let executable = fake_downloader("ok", true);
    fs::write(&queue, "pending note\nhttps://pending\n").unwrap();
    fs::write(&failed, "failed note\nhttps://retry\n").unwrap();
    fs::write(&done, "done note\nhttps://verify\n").unwrap();

    let downloaded = process_queue(&queue, &output, Mode::Download, &executable).unwrap();
    assert_eq!(downloaded.0, vec!["https://pending"]);
    assert!(
        !fs::read_to_string(&queue)
            .unwrap()
            .contains("https://pending")
    );
    assert!(
        fs::read_to_string(&done)
            .unwrap()
            .contains("https://pending")
    );

    let retried = process_queue(&queue, &output, Mode::Retry, &executable).unwrap();
    assert_eq!(retried.0, vec!["https://retry"]);
    assert!(
        !fs::read_to_string(&failed)
            .unwrap()
            .contains("https://retry")
    );
    assert!(fs::read_to_string(&done).unwrap().contains("https://retry"));

    let verified = process_queue(&queue, &output, Mode::Verify, &executable).unwrap();
    assert_eq!(verified.0, vec!["https://verify"]);
    assert!(
        fs::read_to_string(&done)
            .unwrap()
            .contains("https://verify")
    );

    let _ = fs::remove_file(queue);
    let _ = fs::remove_file(failed);
    let _ = fs::remove_file(done);
    let _ = fs::remove_file(executable);
    let _ = fs::remove_dir_all(output);
}

#[test]
fn plain_text_retry_failure_stays_failed_and_verify_failure_moves_to_failed() {
    let queue = temp_path("yt-dlp-text-failure-state", ".txt");
    let failed = companion_path(&queue, "failed");
    let done = companion_path(&queue, "done");
    let output = temp_path("yt-dlp-text-failure-output", "dir");
    let executable = fake_downloader("failed", false);
    fs::write(&failed, "https://retry-failure\n").unwrap();

    let retried = process_queue(&queue, &output, Mode::Retry, &executable).unwrap();
    assert_eq!(retried.1, vec!["https://retry-failure"]);
    assert!(
        fs::read_to_string(&failed)
            .unwrap()
            .contains("https://retry-failure")
    );

    fs::write(&done, "https://verify-failure\n").unwrap();
    let verified = process_queue(&queue, &output, Mode::Verify, &executable).unwrap();
    assert_eq!(verified.1, vec!["https://verify-failure"]);
    assert!(
        !fs::read_to_string(&done)
            .unwrap()
            .contains("https://verify-failure")
    );
    assert!(
        fs::read_to_string(&failed)
            .unwrap()
            .contains("https://verify-failure")
    );

    let _ = fs::remove_file(queue);
    let _ = fs::remove_file(failed);
    let _ = fs::remove_file(done);
    let _ = fs::remove_file(executable);
    let _ = fs::remove_dir_all(output);
}

#[test]
fn plain_text_modes_ignore_missing_state_files() {
    let queue = temp_path("yt-dlp-text-missing-state", ".txt");
    let output = temp_path("yt-dlp-text-missing-state-output", "dir");
    let executable = fake_downloader("unused", true);
    fs::write(&queue, "https://pending\n").unwrap();

    let retried = process_queue(&queue, &output, Mode::Retry, &executable).unwrap();
    let verified = process_queue(&queue, &output, Mode::Verify, &executable).unwrap();
    assert!(retried.0.is_empty());
    assert!(retried.1.is_empty());
    assert!(retried.2.is_empty());
    assert!(verified.0.is_empty());
    assert!(verified.1.is_empty());
    assert!(verified.2.is_empty());

    let _ = fs::remove_file(queue);
    let _ = fs::remove_file(executable);
    let _ = fs::remove_dir_all(output);
}

#[test]
fn process_queue_reports_text_state_read_errors() {
    let queue = temp_path("yt-dlp-text-state-read-error", ".txt");
    let done = companion_path(&queue, "done");
    let output = temp_path("yt-dlp-text-state-read-error-output", "dir");
    let executable = fake_downloader("ok", true);
    fs::write(&queue, "https://pending\n").unwrap();
    fs::create_dir(&done).unwrap();

    let result = process_queue(&queue, &output, Mode::Download, &executable);
    assert!(result.is_err());

    let _ = fs::remove_file(queue);
    let _ = fs::remove_dir_all(done);
    let _ = fs::remove_file(executable);
    let _ = fs::remove_dir_all(output);
}

#[test]
fn cleanup_text_queue_is_no_op_when_all_state_files_are_missing() {
    let queue = temp_path("yt-dlp-text-no-state", ".txt");

    cleanup_queue(&queue).unwrap();
    assert!(!queue.exists());

    let _ = fs::remove_file(queue);
}

#[test]
fn process_queue_reports_already_downloaded_links_as_skipped() {
    let queue = temp_file("## Not downloaded yet\n- https://already\n");
    let output = temp_path("yt-dlp-existing-output", "dir");
    let executable = fake_downloader("has already been downloaded", true);

    let result = process_queue(&queue, &output, Mode::Download, &executable).unwrap();
    assert!(result.0.is_empty());
    assert!(result.1.is_empty());
    assert_eq!(result.2, vec!["https://already"]);

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
