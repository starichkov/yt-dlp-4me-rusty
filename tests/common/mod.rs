#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static TEMP_PATH_COUNTER: AtomicU64 = AtomicU64::new(0);

pub fn temp_file(content: &str) -> PathBuf {
    let path = temp_path("yt-dlp-test", "tmp");
    fs::write(&path, content).unwrap();
    path
}

pub fn temp_path(prefix: &str, suffix: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "{prefix}-{}-{}-{}-{suffix}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        TEMP_PATH_COUNTER.fetch_add(1, Ordering::Relaxed)
    ))
}

pub fn companion_path(path: &Path, state: &str) -> PathBuf {
    path.with_file_name(format!(
        "{}.{}.{}",
        path.file_stem().unwrap().to_string_lossy(),
        state,
        path.extension().unwrap().to_string_lossy()
    ))
}

pub fn fake_downloader(output: &str, success: bool) -> PathBuf {
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
    make_executable(&path);
    path
}

pub fn fake_media_tools(
    probe_output: &str,
    decode_output: &str,
    decode_success: bool,
) -> (PathBuf, PathBuf) {
    let probe = temp_path("ffprobe-fake", "sh");
    let ffmpeg = temp_path("ffmpeg-fake", "sh");
    fs::write(
        &probe,
        format!("#!/bin/sh\nprintf '%s\\n' '{}'\n", probe_output),
    )
    .unwrap();
    fs::write(
        &ffmpeg,
        format!(
            "#!/bin/sh\nif [ \"$7\" = \"-\" ]; then printf '%s' '{}' >&2; exit {}; fi\nprintf repaired > \"${{11}}\"\n",
            decode_output,
            if decode_success { 0 } else { 1 }
        ),
    )
    .unwrap();
    make_executable(&probe);
    make_executable(&ffmpeg);
    (probe, ffmpeg)
}

fn make_executable(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mut permissions = fs::metadata(path).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions).unwrap();
    }
}
