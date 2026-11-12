use std::fs;

use yt_dlp_4me_rusty::{
    MediaStatus, RepairStatus, RepairStrategy, check_video_with_tools, check_videos_with_tools,
    classify_media, repair_video_with_tools,
};

#[path = "common/mod.rs"]
mod common;

use common::{fake_media_tools, temp_path};

#[test]
fn media_check_distinguishes_repairable_and_unrepairable_corruption() {
    let video = temp_path("video", "mp4");
    fs::write(&video, "input").unwrap();
    let (probe, ffmpeg) =
        fake_media_tools("mov,mp4", "[h264] Invalid NAL unit size (-1 > 2).", false);
    assert!(matches!(
        check_video_with_tools(&video, &probe, &ffmpeg).unwrap(),
        MediaStatus::Unrepairable(_)
    ));
    let (probe_ok, ffmpeg_ok) = fake_media_tools("mpegts", "", true);
    assert!(matches!(
        check_video_with_tools(&video, &probe_ok, &ffmpeg_ok).unwrap(),
        MediaStatus::NeedsRepair(_)
    ));
    for path in [video, probe, ffmpeg, probe_ok, ffmpeg_ok] {
        let _ = fs::remove_file(path);
    }
}

#[test]
fn media_classifier_selects_explicit_strategies() {
    assert!(matches!(
        classify_media("mpegts", true, true, ""),
        MediaStatus::NeedsRepair(issue)
            if issue.reason() == "MPEG-TS container"
                && issue.strategy() == RepairStrategy::StreamCopy
    ));
    assert!(matches!(
        classify_media(
            "mov,mp4",
            true,
            true,
            "Application provided invalid, non monotonically increasing dts"
        ),
        MediaStatus::NeedsRepair(issue) if issue.strategy() == RepairStrategy::StreamCopy
    ));
    assert!(matches!(
        classify_media("mov,mp4", true, false, "missing picture in access unit"),
        MediaStatus::Unrepairable(issue)
            if issue.strategy() == RepairStrategy::Redownload
    ));
    assert!(matches!(
        classify_media("mov,mp4", false, false, ""),
        MediaStatus::NeedsRepair(issue) if issue.reason() == "media decode failed"
    ));
}

#[test]
fn h264_bitstream_diagnostics_require_redownload() {
    let diagnostics = [
        "non-existing PPS 6 referenced",
        "non-existing SPS 32 referenced in buffering period",
        "sps_id 32 out of range",
        "pps_id 1388 out of range",
        "SEI type 47 size 215 truncated at 155",
        "Picture timing SEI payload too large",
        "illegal POC type 27",
        "illegal reordering_of_pic_nums_idc 27",
        "reference count 1 overflow",
        "reference overflow (pps)",
        "too many reference frames 32",
        "log2_max_frame_num_minus4 out of range (0-12): 32",
        "log2_max_poc_lsb (32) is out of range",
        "crop values invalid 902 0 2 0 / 16 16",
        "chroma_log2_weight_denom 8 is out of range",
        "luma_log2_weight_denom 8 is out of range",
        "cpb_count 32 invalid",
        "Clipping illegal num_reorder_frames 32",
        "delta scale 12 is invalid",
    ];

    for diagnostic in diagnostics {
        assert!(
            matches!(
                classify_media("mov,mp4", true, false, diagnostic),
                MediaStatus::Unrepairable(issue)
                    if issue.strategy() == RepairStrategy::Redownload
            ),
            "diagnostic should require redownload: {diagnostic}"
        );
    }

    assert!(matches!(
        classify_media("mov,mp4", true, false, "SEI type 5 is present"),
        MediaStatus::NeedsRepair(issue) if issue.strategy() == RepairStrategy::StreamCopy
    ));
}

#[test]
fn timestamp_warnings_are_repairable_and_validated_after_remux() {
    let video = temp_path("timestamp-video", "mp4");
    fs::write(&video, "input").unwrap();
    let warning = "[null] Application provided invalid, non monotonically increasing dts";
    let (probe, ffmpeg) = fake_media_tools("mov,mp4", warning, true);

    assert!(matches!(
        check_video_with_tools(&video, &probe, &ffmpeg).unwrap(),
        MediaStatus::NeedsRepair(issue)
            if issue.reason().contains("non monotonically increasing dts")
    ));
    assert_eq!(
        repair_video_with_tools(&video, &probe, &ffmpeg).unwrap(),
        RepairStatus::Repaired
    );
    assert_eq!(fs::read_to_string(&video).unwrap(), "repaired");

    for path in [video, probe, ffmpeg] {
        let _ = fs::remove_file(path);
    }
}

#[test]
fn failed_validation_keeps_the_original_media_file() {
    let video = temp_path("invalid-repair-video", "mp4");
    fs::write(&video, "original").unwrap();
    let (probe, ffmpeg) = fake_media_tools("mpegts", "decode failed", false);

    assert!(matches!(
        repair_video_with_tools(&video, &probe, &ffmpeg).unwrap(),
        RepairStatus::Failed(reason) if reason.contains("failed validation")
    ));
    assert_eq!(fs::read_to_string(&video).unwrap(), "original");

    for path in [video, probe, ffmpeg] {
        let _ = fs::remove_file(path);
    }
}

#[test]
fn media_scan_repairs_video_and_keeps_unrepairable_file_unchanged() {
    let directory = temp_path("media-dir", "dir");
    fs::create_dir(&directory).unwrap();
    let video = directory.join("clip.MP4");
    fs::write(&video, "input").unwrap();
    let (probe, ffmpeg) = fake_media_tools("mpegts", "", true);
    let scanned = check_videos_with_tools(&directory, &probe, &ffmpeg).unwrap();
    assert_eq!(scanned.len(), 1);
    assert!(matches!(scanned[0].1, MediaStatus::NeedsRepair(_)));
    assert_eq!(
        repair_video_with_tools(&video, &probe, &ffmpeg).unwrap(),
        RepairStatus::Repaired
    );
    assert_eq!(fs::read_to_string(&video).unwrap(), "repaired");
    for path in [directory, probe, ffmpeg] {
        if path.is_dir() {
            let _ = fs::remove_dir_all(path);
        } else {
            let _ = fs::remove_file(path);
        }
    }
}
