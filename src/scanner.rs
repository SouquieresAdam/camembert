use crate::DiskEntry;
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Maps an absolute folder path to its first-level disk entries.
/// Populated as a side-effect of scanning ancestors, so drilling into a previously
/// visited subtree is a cache hit (no filesystem traversal needed).
pub type FolderCache = HashMap<PathBuf, Vec<DiskEntry>>;

pub fn scan_first_level(root: &Path) -> io::Result<Vec<DiskEntry>> {
    let mut throwaway_cache = FolderCache::new();
    scan_first_level_cached(root, &mut throwaway_cache)
}

/// Lists immediate children of `root` with their kind, but **without** computing
/// any sizes (every `bytes` is 0). A single `read_dir`, no recursion — fast
/// enough to call before kicking off a real scan, so the TUI can show the names
/// of the new level immediately while the recursive size walk runs.
pub fn list_first_level(root: &Path) -> io::Result<Vec<DiskEntry>> {
    let mut entries = Vec::new();
    for dir_entry in fs::read_dir(root)?.flatten() {
        let file_type = match dir_entry.file_type() {
            Ok(t) => t,
            Err(_) => continue,
        };
        if file_type.is_symlink() {
            continue;
        }
        let name = dir_entry.file_name().to_string_lossy().into_owned();
        let entry = if file_type.is_file() {
            DiskEntry::file(name, 0)
        } else if file_type.is_dir() {
            DiskEntry::folder(name, 0)
        } else {
            continue;
        };
        entries.push(entry);
    }
    Ok(entries)
}

/// Progress events emitted during a scan to keep a TUI informed in real time.
pub enum ScanProgress {
    /// Walking just descended into this folder. Useful as a "currently scanning"
    /// status line — typically the deepest path in the recursion.
    Entered(PathBuf),
    /// An immediate child of the *root* of the scan completed and now has its
    /// total size known. Useful to fill in the legend incrementally.
    TopLevelDone { name: String, bytes: u64 },
}

pub fn scan_first_level_cached(
    root: &Path,
    cache: &mut FolderCache,
) -> io::Result<Vec<DiskEntry>> {
    scan_first_level_cached_with_progress(root, cache, |_| {})
}

/// Same as `scan_first_level_cached` but invokes `on_progress` as the recursion
/// makes progress: once per descent into a folder (`Entered`) and once per
/// top-level child of `root` that finishes (`TopLevelDone`).
pub fn scan_first_level_cached_with_progress<F>(
    root: &Path,
    cache: &mut FolderCache,
    mut on_progress: F,
) -> io::Result<Vec<DiskEntry>>
where
    F: FnMut(ScanProgress),
{
    if let Some(cached) = cache.get(root) {
        return Ok(cached.clone());
    }
    // Surface the top-level read_dir error so callers can show a meaningful message.
    // Inner-folder failures during recursion are tolerated and reported as 0 bytes.
    fs::read_dir(root)?;
    on_progress(ScanProgress::Entered(root.to_path_buf()));
    let entries = walk_top_level(root, cache, &mut on_progress);
    cache.insert(root.to_path_buf(), entries.clone());
    Ok(entries)
}

/// Walks the top level of the scan: in addition to the usual walk, emits a
/// `TopLevelDone` event after each immediate child finishes (so a TUI can fill
/// in the legend incrementally as sizes settle).
fn walk_top_level<F>(folder: &Path, cache: &mut FolderCache, on_progress: &mut F) -> Vec<DiskEntry>
where
    F: FnMut(ScanProgress),
{
    let read_dir = match fs::read_dir(folder) {
        Ok(rd) => rd,
        Err(_) => return Vec::new(),
    };

    let mut entries = Vec::new();
    for dir_entry in read_dir.flatten() {
        let file_type = match dir_entry.file_type() {
            Ok(t) => t,
            Err(_) => continue,
        };
        if file_type.is_symlink() {
            continue;
        }

        let path = dir_entry.path();
        let name = dir_entry.file_name().to_string_lossy().into_owned();
        let entry = if file_type.is_file() {
            let bytes = dir_entry.metadata().map(|m| m.len()).unwrap_or(0);
            on_progress(ScanProgress::TopLevelDone { name: name.clone(), bytes });
            DiskEntry::file(name, bytes)
        } else if file_type.is_dir() {
            let sub_entries = if let Some(cached) = cache.get(&path) {
                cached.clone()
            } else {
                let computed = walk_and_cache(&path, cache, on_progress);
                cache.insert(path, computed.clone());
                computed
            };
            let total: u64 = sub_entries.iter().map(|e| e.bytes).sum();
            on_progress(ScanProgress::TopLevelDone { name: name.clone(), bytes: total });
            DiskEntry::folder(name, total)
        } else {
            continue;
        };

        entries.push(entry);
    }
    entries
}

fn walk_and_cache<F>(folder: &Path, cache: &mut FolderCache, on_progress: &mut F) -> Vec<DiskEntry>
where
    F: FnMut(ScanProgress),
{
    on_progress(ScanProgress::Entered(folder.to_path_buf()));
    let read_dir = match fs::read_dir(folder) {
        Ok(rd) => rd,
        Err(_) => return Vec::new(),
    };

    let mut entries = Vec::new();
    for dir_entry in read_dir.flatten() {
        let file_type = match dir_entry.file_type() {
            Ok(t) => t,
            Err(_) => continue,
        };
        if file_type.is_symlink() {
            continue;
        }

        let path = dir_entry.path();
        let name = dir_entry.file_name().to_string_lossy().into_owned();
        let entry = if file_type.is_file() {
            let bytes = dir_entry.metadata().map(|m| m.len()).unwrap_or(0);
            DiskEntry::file(name, bytes)
        } else if file_type.is_dir() {
            // Reuse a cached subtree if we already have it — this is what makes
            // re-scanning a parent cheap when its children are already cached.
            let sub_entries = if let Some(cached) = cache.get(&path) {
                cached.clone()
            } else {
                let computed = walk_and_cache(&path, cache, on_progress);
                cache.insert(path, computed.clone());
                computed
            };
            let total: u64 = sub_entries.iter().map(|e| e.bytes).sum();
            DiskEntry::folder(name, total)
        } else {
            continue;
        };

        entries.push(entry);
    }
    entries
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn write_file(folder: &Path, name: &str, byte_count: usize) {
        if let Some(parent) = folder.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::create_dir_all(folder).unwrap();
        fs::write(folder.join(name), vec![0u8; byte_count]).unwrap();
    }

    // ---- scan_first_level (cache-less wrapper) ----

    #[test]
    fn scanning_an_empty_folder_returns_no_entries() {
        let root = tempdir().unwrap();
        let entries = scan_first_level(root.path()).unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn scanning_a_folder_with_files_reports_each_files_size() {
        let root = tempdir().unwrap();
        write_file(root.path(), "report.pdf", 1234);
        write_file(root.path(), "song.mp3", 5000);

        let mut entries = scan_first_level(root.path()).unwrap();
        entries.sort_by(|a, b| a.name.cmp(&b.name));

        assert_eq!(
            entries,
            vec![
                DiskEntry::file("report.pdf", 1234),
                DiskEntry::file("song.mp3", 5000),
            ]
        );
    }

    #[test]
    fn scanning_aggregates_a_subfolders_contents_into_a_single_entry() {
        let root = tempdir().unwrap();
        let photos = root.path().join("Photos");
        write_file(&photos, "img1.jpg", 1000);
        write_file(&photos, "img2.jpg", 2000);

        let entries = scan_first_level(root.path()).unwrap();

        assert_eq!(entries, vec![DiskEntry::folder("Photos", 3000)]);
    }

    #[test]
    fn scanning_recurses_through_nested_subfolders() {
        let root = tempdir().unwrap();
        write_file(&root.path().join("project").join("src"), "main.rs", 500);
        write_file(
            &root.path().join("project").join("target").join("debug"),
            "out",
            1500,
        );

        let entries = scan_first_level(root.path()).unwrap();

        assert_eq!(entries, vec![DiskEntry::folder("project", 2000)]);
    }

    #[test]
    fn scanning_keeps_top_level_files_and_folders_separate() {
        let root = tempdir().unwrap();
        write_file(root.path(), "root_file.txt", 100);
        write_file(&root.path().join("Documents"), "cv.pdf", 2000);

        let mut entries = scan_first_level(root.path()).unwrap();
        entries.sort_by(|a, b| a.name.cmp(&b.name));

        assert_eq!(
            entries,
            vec![
                DiskEntry::folder("Documents", 2000),
                DiskEntry::file("root_file.txt", 100),
            ]
        );
    }

    #[test]
    fn scanning_a_nonexistent_path_returns_an_error() {
        let result = scan_first_level(Path::new("Z:/this/path/does/not/exist/disk-camembert-test"));
        assert!(result.is_err(), "expected an io::Error, got {:?}", result);
    }

    #[test]
    fn scanning_a_file_path_instead_of_a_folder_returns_an_error() {
        let root = tempdir().unwrap();
        let solo_file = root.path().join("solo.txt");
        fs::write(&solo_file, b"hi").unwrap();

        let result = scan_first_level(&solo_file);

        assert!(result.is_err(), "expected an io::Error, got {:?}", result);
    }

    // ---- scan_first_level_cached ----

    #[test]
    fn scanning_a_parent_populates_the_cache_for_its_immediate_subfolders() {
        let root = tempdir().unwrap();
        let photos = root.path().join("Photos");
        write_file(&photos, "img1.jpg", 1000);
        write_file(&photos, "img2.jpg", 2000);

        let mut cache = FolderCache::new();
        scan_first_level_cached(root.path(), &mut cache).unwrap();

        let cached_photos = cache.get(&photos).expect("Photos must be cached");
        let mut sorted = cached_photos.clone();
        sorted.sort_by(|a, b| a.name.cmp(&b.name));

        assert_eq!(
            sorted,
            vec![
                DiskEntry::file("img1.jpg", 1000),
                DiskEntry::file("img2.jpg", 2000),
            ]
        );
    }

    #[test]
    fn scanning_populates_the_cache_for_deeply_nested_descendants_too() {
        let root = tempdir().unwrap();
        let nested = root.path().join("project").join("src");
        write_file(&nested, "main.rs", 500);

        let mut cache = FolderCache::new();
        scan_first_level_cached(root.path(), &mut cache).unwrap();

        assert!(cache.contains_key(&root.path().join("project")));
        assert!(cache.contains_key(&nested));
    }

    #[test]
    fn drilling_into_a_cached_child_does_not_need_a_fresh_filesystem_scan() {
        let root = tempdir().unwrap();
        let photos = root.path().join("Photos");
        write_file(&photos, "img.jpg", 5000);

        let mut cache = FolderCache::new();
        scan_first_level_cached(root.path(), &mut cache).unwrap();

        // Wipe the subfolder. The cache must still serve the drill-down.
        fs::remove_dir_all(&photos).unwrap();
        let drilled = scan_first_level_cached(&photos, &mut cache).unwrap();
        assert_eq!(drilled, vec![DiskEntry::file("img.jpg", 5000)]);
    }

    #[test]
    fn a_second_call_for_the_same_root_returns_cached_entries() {
        let root = tempdir().unwrap();
        write_file(root.path(), "a.txt", 100);

        let mut cache = FolderCache::new();
        let first = scan_first_level_cached(root.path(), &mut cache).unwrap();

        // Wipe the directory; cache lookup must still return the original entries.
        fs::remove_dir_all(root.path()).unwrap();
        let second = scan_first_level_cached(root.path(), &mut cache).unwrap();

        assert_eq!(first, second);
    }

    #[test]
    fn cached_folder_total_equals_sum_of_its_first_level_entries() {
        let root = tempdir().unwrap();
        let project = root.path().join("project");
        write_file(&project.join("src"), "main.rs", 500);
        write_file(&project.join("src"), "lib.rs", 300);
        write_file(&project, "README.md", 200);

        let mut cache = FolderCache::new();
        let scan = scan_first_level_cached(root.path(), &mut cache).unwrap();

        let project_total = scan.iter().find(|e| e.name == "project").unwrap().bytes;
        let cached_project_entries = cache.get(&project).unwrap();
        let cached_sum: u64 = cached_project_entries.iter().map(|e| e.bytes).sum();

        assert_eq!(project_total, cached_sum, "parent's reported total must match cached child entries");
    }

    #[test]
    fn cached_scan_of_a_nonexistent_path_returns_an_error() {
        let mut cache = FolderCache::new();
        let result = scan_first_level_cached(
            Path::new("Z:/nope/nope/nope/disk-camembert-test"),
            &mut cache,
        );
        assert!(result.is_err());
    }

    #[test]
    fn scanning_a_parent_reuses_cached_subfolder_data_without_re_walking_the_disk() {
        // This test pre-populates the cache with a FAKE entry for a subfolder.
        // If the recursive walk respects the cache, the parent will report the FAKE size.
        // If it re-walks the disk, it'll report the real (different) size.
        let root = tempdir().unwrap();
        let photos = root.path().join("Photos");
        write_file(&photos, "real.jpg", 100); // real on-disk size: 100 bytes

        let mut cache = FolderCache::new();
        cache.insert(
            photos.clone(),
            vec![DiskEntry::file("fake.jpg", 99_999)],
        );

        let entries = scan_first_level_cached(root.path(), &mut cache).unwrap();

        let photos_entry = entries
            .iter()
            .find(|e| e.name == "Photos")
            .expect("Photos must be in scan output");
        assert_eq!(
            photos_entry.bytes, 99_999,
            "parent scan must reuse cached child data instead of re-walking"
        );
    }

    // ---- list_first_level (instant skeleton) ----

    #[test]
    fn list_first_level_returns_immediate_children_with_zero_bytes() {
        let root = tempdir().unwrap();
        write_file(root.path(), "report.pdf", 1234);
        let photos = root.path().join("Photos");
        write_file(&photos, "img.jpg", 9999);

        let mut entries = list_first_level(root.path()).unwrap();
        entries.sort_by(|a, b| a.name.cmp(&b.name));

        // Immediate children only: "Photos" (folder) and "report.pdf" (file).
        // Sizes must be 0 — list_first_level intentionally skips size computation.
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "Photos");
        assert_eq!(entries[0].bytes, 0);
        assert_eq!(entries[0].kind, crate::EntryKind::Folder);
        assert_eq!(entries[1].name, "report.pdf");
        assert_eq!(entries[1].bytes, 0);
        assert_eq!(entries[1].kind, crate::EntryKind::File);
    }

    #[test]
    fn list_first_level_does_not_recurse_into_subfolders() {
        let root = tempdir().unwrap();
        let nested = root.path().join("a").join("b").join("c");
        write_file(&nested, "deep.txt", 100);

        let entries = list_first_level(root.path()).unwrap();

        // Only "a" should appear; "b" and "c" must not leak through.
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "a");
        assert_eq!(entries[0].bytes, 0);
    }

    #[test]
    fn list_first_level_propagates_io_errors_for_nonexistent_paths() {
        let result = list_first_level(Path::new("Z:\\definitely\\does\\not\\exist"));
        assert!(result.is_err());
    }

    // ---- scan_first_level_cached_with_progress ----

    #[test]
    fn progress_emits_top_level_done_for_each_immediate_child() {
        let root = tempdir().unwrap();
        write_file(root.path(), "report.pdf", 1234);
        let photos = root.path().join("Photos");
        write_file(&photos, "img.jpg", 5000);

        let mut cache = FolderCache::new();
        let mut events: Vec<(String, u64)> = Vec::new();
        scan_first_level_cached_with_progress(root.path(), &mut cache, |p| {
            if let ScanProgress::TopLevelDone { name, bytes } = p {
                events.push((name, bytes));
            }
        })
        .unwrap();

        events.sort();
        assert_eq!(
            events,
            vec![
                ("Photos".to_string(), 5000),
                ("report.pdf".to_string(), 1234),
            ]
        );
    }

    #[test]
    fn progress_entered_path_includes_at_least_each_subfolder_we_descend_into() {
        let root = tempdir().unwrap();
        let docs = root.path().join("Documents");
        let invoices = docs.join("Invoices");
        write_file(&invoices, "jan.pdf", 100);

        let mut cache = FolderCache::new();
        let mut entered: Vec<PathBuf> = Vec::new();
        scan_first_level_cached_with_progress(root.path(), &mut cache, |p| {
            if let ScanProgress::Entered(path) = p {
                entered.push(path);
            }
        })
        .unwrap();

        // Must have descended into root, Documents, and Invoices.
        assert!(entered.iter().any(|p| p == root.path()));
        assert!(entered.iter().any(|p| p == &docs));
        assert!(entered.iter().any(|p| p == &invoices));
    }

    #[test]
    fn progress_does_not_re_enter_subtrees_already_in_cache() {
        // Simulates: user starts in Documents (which got cached deeply), then
        // navigates UP to its parent. The parent scan must skip the Documents
        // subtree entirely instead of re-walking it.
        let root = tempdir().unwrap();
        let documents = root.path().join("Documents");
        let invoices = documents.join("Invoices");
        write_file(&invoices, "jan.pdf", 100);
        let videos = root.path().join("Videos");
        write_file(&videos, "movie.mp4", 5000);

        let mut cache = FolderCache::new();
        cache.insert(
            documents.clone(),
            vec![DiskEntry::folder("Invoices", 100)],
        );
        cache.insert(
            invoices.clone(),
            vec![DiskEntry::file("jan.pdf", 100)],
        );

        let mut entered: Vec<PathBuf> = Vec::new();
        scan_first_level_cached_with_progress(root.path(), &mut cache, |p| {
            if let ScanProgress::Entered(path) = p {
                entered.push(path);
            }
        })
        .unwrap();

        assert!(
            !entered.iter().any(|p| p == &documents),
            "Documents was already cached — should not be re-entered, got {:?}",
            entered
        );
        assert!(
            !entered.iter().any(|p| p == &invoices),
            "Invoices was already cached — should not be re-entered, got {:?}",
            entered
        );
        assert!(
            entered.iter().any(|p| p == &videos),
            "Videos is not cached — should have been entered, got {:?}",
            entered
        );
    }

    #[test]
    fn progress_callback_emits_no_events_for_a_cache_hit() {
        let root = tempdir().unwrap();
        let mut cache = FolderCache::new();
        cache.insert(
            root.path().to_path_buf(),
            vec![DiskEntry::folder("preexisting", 42)],
        );

        let mut event_count = 0usize;
        scan_first_level_cached_with_progress(root.path(), &mut cache, |_| {
            event_count += 1;
        })
        .unwrap();

        assert_eq!(event_count, 0, "cache hit must not emit progress");
    }
}
