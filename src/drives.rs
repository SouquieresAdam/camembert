use std::path::PathBuf;

/// A mounted volume the user can navigate into. `total` and `used` are in
/// bytes; `used == total - available_space` (saturating).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DriveInfo {
    pub path: PathBuf,
    pub total: u64,
    pub used: u64,
}

impl DriveInfo {
    /// Occupation ratio in `0.0..=1.0`. Returns 0.0 for a drive that reports a
    /// zero total (uncommon, defensive).
    pub fn ratio(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            (self.used as f64 / self.total as f64).clamp(0.0, 1.0)
        }
    }
}

/// Enumerates mounted volumes and returns one `DriveInfo` per accessible drive.
/// Skips drive letters that don't have a volume mounted (e.g., empty CD/DVD bays).
pub fn list_drives() -> Vec<DriveInfo> {
    enumerate_drive_paths()
        .into_iter()
        .filter_map(|path| {
            // `fs2` queries the OS for total/available; both calls fail on an
            // unmounted drive letter — we let those drop out via `?`.
            let total = fs2::total_space(&path).ok()?;
            let available = fs2::available_space(&path).ok()?;
            let used = total.saturating_sub(available);
            Some(DriveInfo { path, total, used })
        })
        .collect()
}

#[cfg(windows)]
fn enumerate_drive_paths() -> Vec<PathBuf> {
    // Walk A: through Z: and keep the ones whose root path actually exists.
    // Faster than calling Win32 APIs and good enough — at most 26 stat calls.
    (b'A'..=b'Z')
        .map(|letter| PathBuf::from(format!("{}:\\", letter as char)))
        .filter(|p| p.exists())
        .collect()
}

#[cfg(not(windows))]
fn enumerate_drive_paths() -> Vec<PathBuf> {
    // On Unix-likes there's a single filesystem root the user can navigate
    // from; the picker is mostly a Windows convenience.
    vec![PathBuf::from("/")]
}

/// Builds a Unicode horizontal bar of `width` cells whose filled portion is
/// proportional to `ratio` (0.0..=1.0). Pure — useful in tests and reused by
/// the drive picker.
pub fn progress_bar(ratio: f64, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let clamped = ratio.clamp(0.0, 1.0);
    let filled = (clamped * width as f64).round() as usize;
    let filled = filled.min(width);
    let empty = width - filled;
    let mut s = String::with_capacity(width * 3);
    for _ in 0..filled {
        s.push('█');
    }
    for _ in 0..empty {
        s.push('░');
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- progress_bar ----

    #[test]
    fn progress_bar_at_zero_ratio_is_all_empty_cells() {
        let bar = progress_bar(0.0, 10);
        assert_eq!(bar.chars().count(), 10);
        assert!(bar.chars().all(|c| c == '░'));
    }

    #[test]
    fn progress_bar_at_full_ratio_is_all_filled_cells() {
        let bar = progress_bar(1.0, 10);
        assert_eq!(bar.chars().count(), 10);
        assert!(bar.chars().all(|c| c == '█'));
    }

    #[test]
    fn progress_bar_half_ratio_splits_cells_roughly_evenly() {
        let bar = progress_bar(0.5, 10);
        let filled = bar.chars().filter(|&c| c == '█').count();
        assert_eq!(filled, 5);
    }

    #[test]
    fn progress_bar_clamps_out_of_range_ratios() {
        // Defensive — bytes math can yield ratios slightly above 1.0 because
        // fs2::available_space includes overhead reserved for root.
        assert_eq!(progress_bar(1.5, 4).chars().count(), 4);
        assert!(progress_bar(1.5, 4).chars().all(|c| c == '█'));
        assert!(progress_bar(-0.2, 4).chars().all(|c| c == '░'));
    }

    #[test]
    fn progress_bar_zero_width_returns_empty_string() {
        assert_eq!(progress_bar(0.5, 0), "");
    }

    // ---- DriveInfo::ratio ----

    #[test]
    fn drive_info_ratio_is_used_over_total() {
        let drive = DriveInfo { path: PathBuf::from("X:\\"), total: 100, used: 25 };
        assert!((drive.ratio() - 0.25).abs() < 1e-9);
    }

    #[test]
    fn drive_info_ratio_handles_zero_total_without_dividing_by_zero() {
        let drive = DriveInfo { path: PathBuf::from("X:\\"), total: 0, used: 0 };
        assert_eq!(drive.ratio(), 0.0);
    }

    // ---- list_drives ----

    #[test]
    fn list_drives_returns_at_least_one_drive_pointing_to_an_existing_path() {
        // Any working machine has at least one mounted volume.
        let drives = list_drives();
        assert!(!drives.is_empty(), "expected at least one drive on this host");
        for drive in &drives {
            assert!(drive.path.exists(), "drive root must exist: {}", drive.path.display());
        }
    }

    #[test]
    fn list_drives_reports_a_total_capacity_greater_than_zero_for_each_drive() {
        let drives = list_drives();
        for drive in &drives {
            assert!(
                drive.total > 0,
                "drive {} should report a non-zero capacity",
                drive.path.display()
            );
        }
    }
}
