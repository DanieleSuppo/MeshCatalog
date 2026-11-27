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
const IO_REPARSE_TAG_CLOUD: u32 = 0x9000_001a;
const IO_REPARSE_TAG_CLOUD_MASK: u32 = 0x0000_f000;
const NAME_SURROGATE_BIT: u32 = 0x2000_0000;

#[derive(Debug, Clone, Copy)]
pub struct RawWin32AttrFlags {
    pub file_attributes: u32,
    pub is_symlink: bool,
    pub reparse_tag: Option<u32>,
}

impl RawWin32AttrFlags {
    pub fn is_placeholder(self) -> bool {
        self.file_attributes
            & (FILE_ATTRIBUTE_OFFLINE
                | FILE_ATTRIBUTE_RECALL_ON_OPEN
                | FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS)
            != 0
    }

    pub(crate) fn is_link(self) -> bool {
        self.is_symlink
            || self
                .reparse_tag
                .is_some_and(|tag| tag & NAME_SURROGATE_BIT != 0)
    }

    pub(crate) fn allows_resident_access(self) -> bool {
        !self.is_link()
            && !self.is_placeholder()
            && (self.file_attributes & FILE_ATTRIBUTE_REPARSE_POINT == 0
                || self
                    .reparse_tag
                    .is_some_and(|tag| tag & !IO_REPARSE_TAG_CLOUD_MASK == IO_REPARSE_TAG_CLOUD))
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
    #[cfg(not(windows))]
    fn inspect(&self, path: &Path) -> io::Result<EntryMetadata> {
        let metadata = fs::symlink_metadata(path)?;
        Ok(EntryMetadata {
            flags: RawWin32AttrFlags {
                file_attributes: 0,
                is_symlink: metadata.file_type().is_symlink(),
                reparse_tag: None,
            },
            is_dir: metadata.is_dir(),
            is_file: metadata.is_file(),
            size_bytes: metadata.len(),
            mtime_unix: metadata.modified().ok().map(unix_seconds),
        })
    }

    #[cfg(windows)]
    fn inspect(&self, path: &Path) -> io::Result<EntryMetadata> {
        use std::{
            os::windows::ffi::OsStrExt,
            path::{Component, Prefix},
        };
        use windows_sys::Win32::{
            Foundation::INVALID_HANDLE_VALUE,
            Storage::FileSystem::{
                FindClose, FindFirstFileW, GetFileAttributesW, FILE_ATTRIBUTE_DIRECTORY,
                INVALID_FILE_ATTRIBUTES, WIN32_FIND_DATAW,
            },
        };

        // FindFirstFileW accepts patterns. Never let a source path turn into one.
        let invalid_name =
            |name: &std::ffi::OsStr| name.encode_wide().any(|c| matches!(c, 0 | 42 | 63 | 58));
        if path.components().any(|part| match part {
            Component::Prefix(prefix) => match prefix.kind() {
                Prefix::Disk(_) | Prefix::VerbatimDisk(_) => false,
                Prefix::UNC(host, share) | Prefix::VerbatimUNC(host, share) => {
                    invalid_name(host) || invalid_name(share)
                }
                _ => true, // Device namespaces are not filesystem sources.
            },
            Component::Normal(name) => invalid_name(name),
            Component::ParentDir => true,
            _ => false,
        }) {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let wide: Vec<_> = path.as_os_str().encode_wide().chain([0]).collect();
        let (file_attributes, tag, size_bytes, mtime_unix) = if path.parent().is_none() {
            // FindFirstFileW cannot query a volume root with a trailing backslash.
            // Without a tag, a reparse root fails closed like any unknown reparse point.
            let attrs = unsafe { GetFileAttributesW(wide.as_ptr()) };
            if attrs == INVALID_FILE_ATTRIBUTES {
                return Err(io::Error::last_os_error());
            }
            (attrs, None, 0, None)
        } else {
            let mut data = WIN32_FIND_DATAW::default();
            // This handle is a find handle, not a file data handle. For links the
            // returned attributes and tag describe the link, not its target.
            let handle = unsafe { FindFirstFileW(wide.as_ptr(), &mut data) };
            if handle == INVALID_HANDLE_VALUE {
                return Err(io::Error::last_os_error());
            }
            unsafe { FindClose(handle) };
            let attrs = data.dwFileAttributes;
            let tag = (attrs & FILE_ATTRIBUTE_REPARSE_POINT != 0).then_some(data.dwReserved0);
            let size = (u64::from(data.nFileSizeHigh) << 32) | u64::from(data.nFileSizeLow);
            let written = data.ftLastWriteTime;
            let ticks =
                (u64::from(written.dwHighDateTime) << 32) | u64::from(written.dwLowDateTime);
            let mtime = (ticks != 0).then(|| (ticks / 10_000_000) as i64 - 11_644_473_600);
            (attrs, tag, size, mtime)
        };
        let is_dir = file_attributes & FILE_ATTRIBUTE_DIRECTORY != 0;
        Ok(EntryMetadata {
            flags: RawWin32AttrFlags {
                file_attributes,
                is_symlink: false,
                reparse_tag: tag,
            },
            is_dir,
            is_file: !is_dir,
            size_bytes,
            mtime_unix,
        })
    }
}

/// Only open a data stream after rechecking every directory and the file itself.
pub(crate) fn open_resident(
    path: &Path,
    expected: &EntryMetadata,
    inspector: &impl EntryMetadataInspector,
) -> io::Result<fs::File> {
    open_resident_with(path, expected, inspector, |path| fs::File::open(path))
}

fn open_resident_with<T>(
    path: &Path,
    expected: &EntryMetadata,
    inspector: &impl EntryMetadataInspector,
    open: impl FnOnce(&Path) -> io::Result<T>,
) -> io::Result<T> {
    check_directory(
        path.parent()
            .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?,
        inspector,
    )?;
    let current = inspector.inspect(path)?;
    if !current.is_file
        || !current.flags.allows_resident_access()
        || current.size_bytes != expected.size_bytes
        || current.mtime_unix != expected.mtime_unix
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Il file è cambiato o non è più residente",
        ));
    }
    open(path)
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
    if !path.is_absolute()
        || path
            .components()
            .any(|part| part == std::path::Component::ParentDir)
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Percorso non assoluto",
        ));
    }
    // A trailing slash can make symlink_metadata follow a directory link.
    let ancestors: Vec<_> = path.components().as_path().ancestors().collect();
    for prefix in ancestors.into_iter().rev() {
        let metadata = inspector.inspect(prefix)?;
        if !metadata.is_dir || !metadata.flags.allows_resident_access() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Non è una cartella locale accessibile senza seguire collegamenti o placeholder",
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn tags_gate_root_ancestors_and_the_actual_open_callback() {
        let temp = tempfile::tempdir().unwrap();
        let ancestor = temp.path().canonicalize().unwrap().join("ancestor");
        let root = ancestor.join("root");
        let file = root.join("asset.zip");
        fs::create_dir_all(&root).unwrap();
        fs::write(&file, b"data").unwrap();
        let expected = OsMetadataInspector.inspect(&file).unwrap();
        for (tag, extra, allowed) in [
            (Some(0x9000_001a), 0, true),
            (Some(0x9000_d01a), 0, true),
            (Some(0x9000_001a), FILE_ATTRIBUTE_OFFLINE, false),
            (Some(0x9000_001a), FILE_ATTRIBUTE_RECALL_ON_OPEN, false),
            (
                Some(0x9000_001a),
                FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS,
                false,
            ),
            (Some(0xa000_0003), 0, false), // mount point / junction
            (Some(0xa000_000c), 0, false), // symbolic link
            (Some(0xb000_001a), 0, false), // lookalike name surrogate
            (Some(0x8000_001b), 0, false), // unknown non-surrogate
            (Some(0), 0, false),
            (None, 0, false),
        ] {
            for path in [&ancestor, &root, &file] {
                let inspector = MockMetadataInspector::new([(
                    path.clone(),
                    RawWin32AttrFlags {
                        file_attributes: FILE_ATTRIBUTE_REPARSE_POINT | extra,
                        is_symlink: false,
                        reparse_tag: tag,
                    },
                )]);
                if path != &file {
                    assert_eq!(
                        check_directory(&root, &inspector).is_ok(),
                        allowed,
                        "{path:?}, {tag:?}, {extra:#x}"
                    );
                }
                let opened = Cell::new(0);
                let result = open_resident_with(&file, &expected, &inspector, |_| {
                    opened.set(opened.get() + 1);
                    Ok(())
                });
                assert_eq!(result.is_ok(), allowed, "{path:?}, {tag:?}, {extra:#x}");
                assert_eq!(opened.get(), usize::from(allowed));
            }
        }

        struct Deny<'a>(&'a Path);
        impl EntryMetadataInspector for Deny<'_> {
            fn inspect(&self, path: &Path) -> io::Result<EntryMetadata> {
                if path == self.0 {
                    return Err(io::ErrorKind::PermissionDenied.into());
                }
                OsMetadataInspector.inspect(path)
            }
        }
        for path in [&ancestor, &root, &file] {
            assert!(
                open_resident_with(&file, &expected, &Deny(path), |_| -> io::Result<()> {
                    panic!("metadata lookup failed before open")
                })
                .is_err()
            );
        }
    }
}
