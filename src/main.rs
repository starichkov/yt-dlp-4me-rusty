use std::env;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

use yt_dlp_4me_rusty::{Mode, cleanup_markdown, expand_path, process_queue};

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!(
            "Usage: {} <queue_file.md> <output_directory> [--verify|--retry] [--yt-dlp-path PATH]",
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
    } else {
        Mode::Download
    };
    let executable = args
        .iter()
        .position(|a| a == "--yt-dlp-path")
        .and_then(|i| args.get(i + 1))
        .map(|s| expand_path(s))
        .unwrap_or_else(|| PathBuf::from("yt-dlp"));
    if let Err(e) = fs::create_dir_all(&output) {
        eprintln!("Error creating output directory: {e}");
        std::process::exit(1);
    }
    let _ = cleanup_markdown(&input);
    let (ok, bad, skipped) = process_queue(&input, &output, mode, &executable);
    println!(
        "\n========================================\nDOWNLOAD SUMMARY\n========================================\nSuccessfully downloaded: {}\nAlready existed: {}\nFailed: {}\n========================================",
        ok.len(),
        skipped.len(),
        bad.len()
    );
    let _ = std::io::stdout().flush();
}
