#![cfg(unix)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

fn unique_path(prefix: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "{prefix}-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

fn fake_downloader(success: bool) -> PathBuf {
    let path = unique_path("yt-dlp-cli-fake");
    fs::write(
        &path,
        format!(
            "#!/bin/sh\nprintf '%s\\n' '{}'\nexit {}\n",
            if success {
                "download complete"
            } else {
                "download failed"
            },
            if success { 0 } else { 1 }
        ),
    )
    .unwrap();
    let mut permissions = fs::metadata(&path).unwrap().permissions();
    use std::os::unix::fs::PermissionsExt;
    permissions.set_mode(0o755);
    fs::set_permissions(&path, permissions).unwrap();
    path
}

fn run_cli(args: &[&Path]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_yt-dlp-4me-rusty"))
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn downloads_pending_link_and_creates_output_directory() {
    let queue = unique_path("yt-dlp-cli-queue");
    let output = unique_path("yt-dlp-cli-output");
    let executable = fake_downloader(true);
    fs::write(
        &queue,
        "# Queue\n\n## Not downloaded yet\n- https://example.com/video\n",
    )
    .unwrap();

    let result = run_cli(&[&queue, &output, Path::new("--yt-dlp-path"), &executable]);

    assert!(result.status.success());
    assert!(String::from_utf8_lossy(&result.stdout).contains("Successfully downloaded: 1"));
    assert!(output.is_dir());
    let queue_text = fs::read_to_string(&queue).unwrap();
    assert!(queue_text.contains("## Downloaded\n- https://example.com/video"));
    assert!(!queue_text.contains("Not downloaded yet\n- https://example.com/video"));

    let _ = fs::remove_file(queue);
    let _ = fs::remove_file(executable);
    let _ = fs::remove_dir_all(output);
}

#[test]
fn verify_and_retry_flags_select_the_expected_sections() {
    let verify_queue = unique_path("yt-dlp-cli-verify");
    let retry_queue = unique_path("yt-dlp-cli-retry");
    let verify_output = unique_path("yt-dlp-cli-verify-output");
    let retry_output = unique_path("yt-dlp-cli-retry-output");
    let executable = fake_downloader(true);
    fs::write(
        &verify_queue,
        "## Downloaded\n- https://example.com/verify\n",
    )
    .unwrap();
    fs::write(&retry_queue, "## Failed\n- https://example.com/retry\n").unwrap();

    let verify = run_cli(&[
        &verify_queue,
        &verify_output,
        Path::new("--verify"),
        Path::new("--yt-dlp-path"),
        &executable,
    ]);
    let retry = run_cli(&[
        &retry_queue,
        &retry_output,
        Path::new("--retry"),
        Path::new("--yt-dlp-path"),
        &executable,
    ]);

    assert!(verify.status.success());
    assert!(String::from_utf8_lossy(&verify.stdout).contains("Successfully downloaded: 1"));
    assert!(retry.status.success());
    assert!(String::from_utf8_lossy(&retry.stdout).contains("Successfully downloaded: 1"));
    assert!(
        fs::read_to_string(&retry_queue)
            .unwrap()
            .contains("## Downloaded\n- https://example.com/retry")
    );

    let _ = fs::remove_file(verify_queue);
    let _ = fs::remove_file(retry_queue);
    let _ = fs::remove_file(executable);
    let _ = fs::remove_dir_all(verify_output);
    let _ = fs::remove_dir_all(retry_output);
}

#[test]
fn reports_downloader_failure_and_moves_link_to_failed() {
    let queue = unique_path("yt-dlp-cli-failure-queue");
    let output = unique_path("yt-dlp-cli-failure-output");
    let executable = fake_downloader(false);
    fs::write(
        &queue,
        "## Not downloaded yet\n- https://example.com/video\n",
    )
    .unwrap();

    let result = run_cli(&[&queue, &output, Path::new("--yt-dlp-path"), &executable]);

    assert!(result.status.success());
    assert!(String::from_utf8_lossy(&result.stdout).contains("Failed: 1"));
    assert!(
        fs::read_to_string(&queue)
            .unwrap()
            .contains("## Failed\n- https://example.com/video")
    );

    let _ = fs::remove_file(queue);
    let _ = fs::remove_file(executable);
    let _ = fs::remove_dir_all(output);
}

#[test]
fn processes_plain_text_queue_and_reports_missing_downloader() {
    let queue = std::env::temp_dir().join(format!(
        "yt-dlp-cli-text-queue-{}.TXT",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let output = unique_path("yt-dlp-cli-text-output");
    fs::write(&queue, "phone note\nhttps://example.com/video\n").unwrap();

    let result = run_cli(&[
        &queue,
        &output,
        Path::new("--yt-dlp-path"),
        Path::new("/tmp/yt-dlp-cli-no-such-executable"),
    ]);

    assert!(result.status.success());
    assert!(String::from_utf8_lossy(&result.stdout).contains("Failed: 1"));
    assert_eq!(fs::read_to_string(&queue).unwrap(), "phone note\n");
    let failed = queue.with_file_name(format!(
        "{}.failed.TXT",
        queue.file_stem().unwrap().to_string_lossy()
    ));
    assert!(failed.exists());

    let _ = fs::remove_file(queue);
    let _ = fs::remove_file(failed);
    let _ = fs::remove_dir_all(output);
}

#[test]
fn reports_output_directory_creation_failure() {
    let queue = unique_path("yt-dlp-cli-output-error-queue");
    let output_file = unique_path("yt-dlp-cli-output-error-file");
    fs::write(
        &queue,
        "## Not downloaded yet\n- https://example.com/video\n",
    )
    .unwrap();
    fs::write(&output_file, "not a directory").unwrap();

    let result = run_cli(&[&queue, &output_file]);

    assert_eq!(result.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&result.stderr).contains("Error creating output directory"));

    let _ = fs::remove_file(queue);
    let _ = fs::remove_file(output_file);
}

#[test]
fn reports_queue_cleanup_failure() {
    let queue = unique_path("yt-dlp-cli-cleanup-error-queue").with_extension("txt");
    let done = queue.with_file_name(format!(
        "{}.done.txt",
        queue.file_stem().unwrap().to_string_lossy()
    ));
    let output = unique_path("yt-dlp-cli-cleanup-error-output");
    fs::write(&queue, "https://example.com/video\n").unwrap();
    fs::create_dir(&done).unwrap();

    let result = run_cli(&[&queue, &output]);

    assert_eq!(result.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&result.stderr).contains("Error reading input file"));

    let _ = fs::remove_file(queue);
    let _ = fs::remove_dir_all(done);
    let _ = fs::remove_dir_all(output);
}

#[test]
fn reports_missing_arguments_and_input_file() {
    let missing_arguments = Command::new(env!("CARGO_BIN_EXE_yt-dlp-4me-rusty"))
        .arg("only-one-argument")
        .output()
        .unwrap();
    assert_eq!(missing_arguments.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&missing_arguments.stderr).contains("Usage:"));

    let missing_input = run_cli(&[
        Path::new("/tmp/yt-dlp-cli-does-not-exist.md"),
        Path::new("/tmp/yt-dlp-cli-output"),
    ]);
    assert_eq!(missing_input.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&missing_input.stderr).contains("not found"));
}
