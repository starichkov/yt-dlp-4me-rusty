use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use yt_dlp_4me_rusty::{
    MediaStatus, Mode, RepairStatus, check_videos_with_tools, cleanup_queue, expand_path,
    process_queue, repair_video_with_tools,
};

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!(
            "Usage: {} <queue_file.md|queue_file.txt> <output_directory> [--verify|--retry|--redownload|--check-media|--repair-media] [--yt-dlp-path PATH] [--ffprobe-path PATH] [--ffmpeg-path PATH]",
            args.first().unwrap_or(&"yt-dlp-4me-rusty".into())
        );
        std::process::exit(2);
    }
    let input = expand_path(&args[1]);
    let output = expand_path(&args[2]);
    if !input.is_file() {
        eprintln!("Error: Input file '{}' not found.", args[1]);
        std::process::exit(1);
    }
    let mode = if args.iter().any(|a| a == "--verify") {
        Mode::Verify
    } else if args.iter().any(|a| a == "--retry") {
        Mode::Retry
    } else if args.iter().any(|a| a == "--redownload") {
        Mode::Redownload
    } else {
        Mode::Download
    };
    let executable = args
        .iter()
        .position(|a| a == "--yt-dlp-path")
        .and_then(|i| args.get(i + 1))
        .map(|s| expand_path(s))
        .unwrap_or_else(|| PathBuf::from("yt-dlp"));
    let check_media = args.iter().any(|a| a == "--check-media");
    let repair_media = args.iter().any(|a| a == "--repair-media");
    let ffprobe = args
        .iter()
        .position(|a| a == "--ffprobe-path")
        .and_then(|i| args.get(i + 1))
        .map(|s| expand_path(s))
        .unwrap_or_else(|| PathBuf::from("ffprobe"));
    let ffmpeg = args
        .iter()
        .position(|a| a == "--ffmpeg-path")
        .and_then(|i| args.get(i + 1))
        .map(|s| expand_path(s))
        .unwrap_or_else(|| PathBuf::from("ffmpeg"));
    if let Err(e) = fs::create_dir_all(&output) {
        eprintln!("Error creating output directory: {e}");
        std::process::exit(1);
    }
    if check_media || repair_media {
        if let Err(e) = process_media(&output, repair_media, &ffprobe, &ffmpeg) {
            eprintln!("Error checking media in '{}': {e}", output.display());
            std::process::exit(1);
        }
        if mode != Mode::Verify {
            return;
        }
    }
    if let Err(e) = cleanup_queue(&input) {
        eprintln!("Error reading input file '{}': {e}", args[1]);
        std::process::exit(1);
    }
    let (ok, bad, skipped) = match process_queue(&input, &output, mode, &executable) {
        Ok(result) => result,
        Err(e) => {
            eprintln!("Error processing input file '{}': {e}", args[1]);
            std::process::exit(1);
        }
    };
    println!(
        "\n========================================\nDOWNLOAD SUMMARY\n========================================\nSuccessfully downloaded: {}\nAlready existed: {}\nFailed: {}\n========================================",
        ok.len(),
        skipped.len(),
        bad.len()
    );
    let _ = std::io::stdout().flush();
}

fn process_media(
    output: &Path,
    repair: bool,
    ffprobe: &Path,
    ffmpeg: &Path,
) -> std::io::Result<()> {
    let videos = check_videos_with_tools(output, ffprobe, ffmpeg)?;
    let mut ok = 0;
    let mut repaired = 0;
    let mut failed = 0;
    for (path, status) in videos {
        match status {
            MediaStatus::Ok => {
                ok += 1;
                println!("OK: {}", path.display());
            }
            MediaStatus::Unrepairable(reason) => {
                failed += 1;
                println!("UNREPAIRABLE: {} ({reason})", path.display());
            }
            MediaStatus::NeedsRepair(reason) if repair => {
                match repair_video_with_tools(&path, ffprobe, ffmpeg)? {
                    RepairStatus::Repaired => {
                        repaired += 1;
                        println!("REPAIRED: {} ({reason})", path.display());
                    }
                    RepairStatus::Unrepairable(reason) | RepairStatus::Failed(reason) => {
                        failed += 1;
                        println!("FAILED: {} ({reason})", path.display());
                    }
                }
            }
            MediaStatus::NeedsRepair(reason) => {
                failed += 1;
                println!("NEEDS REPAIR: {} ({reason})", path.display());
            }
        }
    }
    println!("Media summary: {ok} OK, {repaired} repaired, {failed} needing attention.");
    Ok(())
}
