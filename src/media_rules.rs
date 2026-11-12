//! Pure classification rules for media diagnostics.
//!
//! The rules in this module translate known `ffmpeg` diagnostics into a repair
//! strategy and define which diagnostics are acceptable after a repair. Keeping
//! this policy free of process execution makes it straightforward to test and
//! extend when new media failures are encountered.

use super::media::{MediaIssue, MediaStatus, RepairStrategy};

/// A diagnostic marker and the policy associated with it.
struct DiagnosticRule {
    /// Case-insensitive text identifying the diagnostic.
    marker: &'static str,
    /// The action selected when the diagnostic is found.
    strategy: RepairStrategy,
    /// Whether this diagnostic may remain after a successful repair.
    valid_after_repair: bool,
}

/// Known diagnostics ordered by classification precedence.
const DIAGNOSTIC_RULES: &[DiagnosticRule] = &[
    DiagnosticRule {
        marker: "non-existing pps",
        strategy: RepairStrategy::Redownload,
        valid_after_repair: false,
    },
    DiagnosticRule {
        marker: "non-existing sps",
        strategy: RepairStrategy::Redownload,
        valid_after_repair: false,
    },
    DiagnosticRule {
        marker: "sps_id",
        strategy: RepairStrategy::Redownload,
        valid_after_repair: false,
    },
    DiagnosticRule {
        marker: "pps_id",
        strategy: RepairStrategy::Redownload,
        valid_after_repair: false,
    },
    DiagnosticRule {
        marker: "picture timing sei payload too large",
        strategy: RepairStrategy::Redownload,
        valid_after_repair: false,
    },
    DiagnosticRule {
        marker: "illegal poc type",
        strategy: RepairStrategy::Redownload,
        valid_after_repair: false,
    },
    DiagnosticRule {
        marker: "illegal reordering_of_pic_nums_idc",
        strategy: RepairStrategy::Redownload,
        valid_after_repair: false,
    },
    DiagnosticRule {
        marker: "reference count",
        strategy: RepairStrategy::Redownload,
        valid_after_repair: false,
    },
    DiagnosticRule {
        marker: "reference overflow",
        strategy: RepairStrategy::Redownload,
        valid_after_repair: false,
    },
    DiagnosticRule {
        marker: "too many reference frames",
        strategy: RepairStrategy::Redownload,
        valid_after_repair: false,
    },
    DiagnosticRule {
        marker: "log2_max_frame_num_minus4 out of range",
        strategy: RepairStrategy::Redownload,
        valid_after_repair: false,
    },
    DiagnosticRule {
        marker: "log2_max_poc_lsb",
        strategy: RepairStrategy::Redownload,
        valid_after_repair: false,
    },
    DiagnosticRule {
        marker: "crop values invalid",
        strategy: RepairStrategy::Redownload,
        valid_after_repair: false,
    },
    DiagnosticRule {
        marker: "chroma_log2_weight_denom",
        strategy: RepairStrategy::Redownload,
        valid_after_repair: false,
    },
    DiagnosticRule {
        marker: "luma_log2_weight_denom",
        strategy: RepairStrategy::Redownload,
        valid_after_repair: false,
    },
    DiagnosticRule {
        marker: "cpb_count",
        strategy: RepairStrategy::Redownload,
        valid_after_repair: false,
    },
    DiagnosticRule {
        marker: "clipping illegal num_reorder_frames",
        strategy: RepairStrategy::Redownload,
        valid_after_repair: false,
    },
    DiagnosticRule {
        marker: "delta scale",
        strategy: RepairStrategy::Redownload,
        valid_after_repair: false,
    },
    DiagnosticRule {
        marker: "missing picture in access unit",
        strategy: RepairStrategy::Redownload,
        valid_after_repair: false,
    },
    DiagnosticRule {
        marker: "invalid nal unit size",
        strategy: RepairStrategy::Redownload,
        valid_after_repair: false,
    },
    DiagnosticRule {
        marker: "non monotonically increasing dts",
        strategy: RepairStrategy::StreamCopy,
        valid_after_repair: true,
    },
];

/// Classifies known `ffmpeg` diagnostics into a media issue.
pub(crate) fn classify_diagnostics(diagnostics: &str) -> Option<MediaIssue> {
    let lower = diagnostics.to_ascii_lowercase();
    if lower.contains("sei type") && lower.contains("truncated") {
        return Some(MediaIssue::new(diagnostics, RepairStrategy::Redownload));
    }
    DIAGNOSTIC_RULES
        .iter()
        .find(|rule| lower.contains(rule.marker))
        .map(|rule| MediaIssue::new(diagnostics, rule.strategy))
}

/// Combines probe, decode, and diagnostic results into a media status.
pub(crate) fn classify_media(
    format: &str,
    probe_succeeded: bool,
    decode_succeeded: bool,
    diagnostics: &str,
) -> MediaStatus {
    if let Some(issue) = classify_diagnostics(diagnostics) {
        return if issue.strategy() == RepairStrategy::Redownload {
            MediaStatus::Unrepairable(issue)
        } else {
            MediaStatus::NeedsRepair(issue)
        };
    }
    if format.contains("mpegts") {
        return MediaStatus::NeedsRepair(MediaIssue::new(
            "MPEG-TS container",
            RepairStrategy::StreamCopy,
        ));
    }
    if !probe_succeeded || !decode_succeeded || !diagnostics.is_empty() {
        return MediaStatus::NeedsRepair(MediaIssue::new(
            if diagnostics.is_empty() {
                "media decode failed"
            } else {
                diagnostics
            },
            RepairStrategy::StreamCopy,
        ));
    }
    MediaStatus::Ok
}

/// Reports whether all remaining diagnostics are allowed after repair.
pub(crate) fn diagnostics_are_valid_after_repair(diagnostics: &str) -> bool {
    diagnostics
        .lines()
        .filter(|line| !line.trim().is_empty())
        .all(|line| {
            let lower = line.to_ascii_lowercase();
            DIAGNOSTIC_RULES
                .iter()
                .any(|rule| rule.valid_after_repair && lower.contains(rule.marker))
        })
}
