use std::{
    collections::HashMap,
    fs, io,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub const FILE_ATTRIBUTE_OFFLINE: u32 = 0x0000_1000;
pub const FILE_ATTRIBUTE_RECALL_ON_OPEN: u32 = 0x0004_0000;
pub const FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS: u32 = 0x0040_0000;
pub const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;

#[derive(Debug, Clone, Copy)]
pub struct RawWin32AttrFlags {
    pub file_attributes: u32,
    pub is_symlink: bool,
}

impl RawWin32AttrFlags {
    pub fn is_placeholder(self) -> bool {
        self.file_attributes
            & (FILE_ATTRIBUTE_OFFLINE
                | FILE_ATTRIBUTE_RECALL_ON_OPEN
                | FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS)
            != 0
    }

    pub(crate) fn is_reparse_point(self) -> bool {
        self.file_attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
}

#[derive(Debug)]
pub struct EntryMetadata {
    pub flags: RawWin32AttrFlags,
    pub is_dir: bool,
    pub is_file: bool,
    pub size_bytes: u64,
    pub mtime_unix: Option<i64>,
}

pub trait EntryMetadataInspector {
    fn inspect(&self, path: &Path) -> io::Result<EntryMetadata>;
}

pub struct OsMetadataInspector;

impl EntryMetadataInspector for OsMetadataInspector {
    fn inspect(&self, path: &Path) -> io::Result<EntryMetadata> {
        let metadata = fs::symlink_metadata(path)?;
        #[cfg(windows)]
        let file_attributes = {
            use std::os::windows::fs::MetadataExt;
            metadata.file_attributes()
        };
        #[cfg(not(windows))]
        let file_attributes = 0;
        Ok(EntryMetadata {
            flags: RawWin32AttrFlags {
                file_attributes,
                is_symlink: metadata.file_type().is_symlink(),
            },
            is_dir: metadata.is_dir(),
            is_file: metadata.is_file(),
            size_bytes: metadata.len(),
            mtime_unix: metadata.modified().ok().map(unix_seconds),
        })
    }
}

/// Only open a data stream after rechecking every directory and the file itself.
pub(crate) fn open_resident(
    path: &Path,
    expected: &EntryMetadata,
    inspector: &impl EntryMetadataInspector,
) -> io::Result<fs::File> {
    check_directory(
        path.parent()
            .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?,
        inspector,
    )?;
    let current = inspector.inspect(path)?;
    if !current.is_file
        || current.flags.is_symlink
        || current.flags.is_placeholder()
        || current.size_bytes != expected.size_bytes
        || current.mtime_unix != expected.mtime_unix
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Il file è cambiato o non è più residente",
        ));
    }
    fs::File::open(path)
}

pub(crate) fn unix_seconds(time: SystemTime) -> i64 {
    match time.duration_since(UNIX_EPOCH) {
        Ok(elapsed) => elapsed.as_secs() as i64,
        Err(error) => {
            let elapsed = error.duration();
            -(elapsed.as_secs() as i64) - i64::from(elapsed.subsec_nanos() != 0)
        }
    }
}

/// Overrides Win32 flags on real fixture entries without needing NTFS.
#[doc(hidden)]
pub struct MockMetadataInspector(HashMap<PathBuf, RawWin32AttrFlags>);

impl MockMetadataInspector {
    pub fn new(overrides: impl IntoIterator<Item = (PathBuf, RawWin32AttrFlags)>) -> Self {
        Self(overrides.into_iter().collect())
    }
}

impl EntryMetadataInspector for MockMetadataInspector {
    fn inspect(&self, path: &Path) -> io::Result<EntryMetadata> {
        let mut metadata = OsMetadataInspector.inspect(path)?;
        if let Some(flags) = self.0.get(path) {
            metadata.flags = *flags;
        }
        Ok(metadata)
    }
}

/// Check each component before resolving a descendant through it.
pub(crate) fn check_directory(
    path: &Path,
    inspector: &impl EntryMetadataInspector,
) -> io::Result<()> {
    if !path.is_absolute() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Percorso non assoluto",
        ));
    }
    // A trailing slash can make symlink_metadata follow a directory link.
    let ancestors: Vec<_> = path.components().as_path().ancestors().collect();
    for prefix in ancestors.into_iter().rev() {
        let metadata = inspector.inspect(prefix)?;
        if !metadata.is_dir
            || metadata.flags.is_symlink
            || metadata.flags.is_placeholder()
            || metadata.flags.is_reparse_point()
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Non è una cartella locale accessibile senza seguire collegamenti o placeholder",
            ));
        }
    }
    Ok(())
}
