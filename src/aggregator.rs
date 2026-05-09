use crate::DiskEntry;

const OTHERS_LABEL: &str = "Autres";

pub fn aggregate(mut entries: Vec<DiskEntry>, max_slices: usize) -> Vec<DiskEntry> {
    entries.sort_by(|a, b| b.bytes.cmp(&a.bytes));
    let leftover_bytes: u64 = entries.iter().skip(max_slices).map(|e| e.bytes).sum();
    let mut top: Vec<DiskEntry> = entries.into_iter().take(max_slices).collect();
    if leftover_bytes > 0 {
        top.push(DiskEntry::bucket(OTHERS_LABEL, leftover_bytes));
    }
    top
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str, bytes: u64) -> DiskEntry {
        DiskEntry::folder(name, bytes)
    }

    #[test]
    fn empty_disk_gives_empty_pie() {
        let pie = aggregate(vec![], 5);
        assert!(pie.is_empty());
    }

    #[test]
    fn fewer_folders_than_max_slices_returns_all_sorted_by_size_desc() {
        let folders = vec![
            folder("Photos", 10),
            folder("Videos", 30),
            folder("Documents", 20),
        ];
        let pie = aggregate(folders, 5);
        assert_eq!(
            pie,
            vec![
                folder("Videos", 30),
                folder("Documents", 20),
                folder("Photos", 10),
            ]
        );
    }

    #[test]
    fn exactly_max_slices_folders_returns_all_no_others_bucket() {
        let folders = vec![
            folder("Photos", 10),
            folder("Videos", 30),
            folder("Documents", 20),
        ];
        let pie = aggregate(folders, 3);
        assert!(pie.iter().all(|e| e.name != OTHERS_LABEL));
        assert_eq!(pie.len(), 3);
    }

    #[test]
    fn smaller_folders_beyond_max_slices_collapse_into_autres() {
        let folders = vec![
            folder("Videos", 100),
            folder("Documents", 50),
            folder("Photos", 30),
            folder("Music", 20),
            folder("Cache", 10),
        ];
        let pie = aggregate(folders, 2);
        assert_eq!(
            pie,
            vec![
                folder("Videos", 100),
                folder("Documents", 50),
                DiskEntry::bucket(OTHERS_LABEL, 30 + 20 + 10),
            ]
        );
    }

    #[test]
    fn autres_bucket_is_always_the_last_slice() {
        let folders = vec![
            folder("Cache", 1),
            folder("Videos", 1_000),
            folder("Documents", 100),
        ];
        let pie = aggregate(folders, 1);
        assert_eq!(pie.last().unwrap().name, OTHERS_LABEL);
    }

    #[test]
    fn autres_bucket_is_marked_as_a_synthetic_bucket_not_a_folder() {
        let folders = vec![folder("Videos", 100), folder("Documents", 50)];
        let pie = aggregate(folders, 1);
        let autres = pie.last().unwrap();
        assert_eq!(autres.name, OTHERS_LABEL);
        assert!(!autres.is_drillable(), "Autres must not be drillable");
    }

    #[test]
    fn max_slices_zero_collapses_every_folder_into_autres() {
        let folders = vec![folder("Documents", 10), folder("Videos", 20)];
        let pie = aggregate(folders, 0);
        assert_eq!(pie, vec![DiskEntry::bucket(OTHERS_LABEL, 30)]);
    }

    #[test]
    fn empty_folders_beyond_cutoff_do_not_create_an_autres_slice() {
        let folders = vec![
            folder("Videos", 100),
            folder("EmptyDir1", 0),
            folder("EmptyDir2", 0),
        ];
        let pie = aggregate(folders, 1);
        assert_eq!(pie, vec![folder("Videos", 100)]);
    }

    #[test]
    fn aggregating_preserves_each_entry_kind_for_kept_slices() {
        let entries = vec![
            DiskEntry::folder("Documents", 100),
            DiskEntry::file("song.mp3", 50),
        ];
        let pie = aggregate(entries, 5);
        assert_eq!(pie[0].kind, crate::EntryKind::Folder);
        assert_eq!(pie[1].kind, crate::EntryKind::File);
    }
}
