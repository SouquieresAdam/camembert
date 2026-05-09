pub mod aggregator;
pub mod command;
pub mod drive_picker;
pub mod drives;
pub mod event_map;
pub mod render;
pub mod scanner;
pub mod tui;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntryKind {
    /// A real directory on disk — drillable.
    Folder,
    /// A regular file — not drillable.
    File,
    /// Synthetic aggregate produced by the aggregator (e.g. "Autres") — not drillable.
    Bucket,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiskEntry {
    pub name: String,
    pub bytes: u64,
    pub kind: EntryKind,
}

impl DiskEntry {
    pub fn folder(name: impl Into<String>, bytes: u64) -> Self {
        Self { name: name.into(), bytes, kind: EntryKind::Folder }
    }

    pub fn file(name: impl Into<String>, bytes: u64) -> Self {
        Self { name: name.into(), bytes, kind: EntryKind::File }
    }

    pub fn bucket(name: impl Into<String>, bytes: u64) -> Self {
        Self { name: name.into(), bytes, kind: EntryKind::Bucket }
    }

    pub fn is_drillable(&self) -> bool {
        matches!(self.kind, EntryKind::Folder)
    }
}
