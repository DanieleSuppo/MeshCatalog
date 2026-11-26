use std::{
    fs::{self, ReadDir},
    io,
    path::{Path, PathBuf},
};

use tokio_util::sync::CancellationToken;

use crate::error::IpcError;

use super::{
    attr_guard::{check_directory, EntryMetadata, EntryMetadataInspector},
    exclusions::{is_excluded, supported_extension},
};

pub(crate) enum InventoryEntry {
    Directory {
        rel_path: String,
        name: String,
        opaque: bool,
    },
    File {
        rel_path: String,
        name: String,
        ext: String,
        metadata: EntryMetadata,
    },
    OtherFile {
        parent: String,
    },
}

pub(crate) struct WalkIssue {
    pub path: PathBuf,
    pub error: io::Error,
    pub at_root: bool,
}

pub(crate) fn walk_inventory(
    root: &Path,
    inspector: &impl EntryMetadataInspector,
    cancel: &CancellationToken,
    mut visit: impl FnMut(Result<InventoryEntry, WalkIssue>) -> Result<(), IpcError>,
) -> Result<(), IpcError> {
    walk(
        root,
        inspector,
        cancel,
        &mut |path| fs::read_dir(path),
        &mut visit,
    )
}

fn walk(
    root: &Path,
    inspector: &impl EntryMetadataInspector,
    cancel: &CancellationToken,
    read_directory: &mut impl FnMut(&Path) -> io::Result<ReadDir>,
    visit: &mut impl FnMut(Result<InventoryEntry, WalkIssue>) -> Result<(), IpcError>,
) -> Result<(), IpcError> {
    if cancel.is_cancelled() {
        return Ok(());
    }
    let issue = |path: &Path, error| {
        Err(WalkIssue {
            path: path.to_owned(),
            error,
            at_root: path == root,
        })
    };
    let entries = match check_source_directory(root, inspector).and_then(|()| read_directory(root))
    {
        Ok(entries) => entries,
        Err(error) => return visit(issue(root, error)),
    };
    visit(Ok(InventoryEntry::Directory {
        rel_path: String::new(),
        name: String::new(),
        opaque: false,
    }))?;
    let mut stack = vec![(root.to_owned(), String::new(), entries)];
    while !cancel.is_cancelled() {
        let Some((parent, parent_rel, entries)) = stack.last_mut() else {
            break;
        };
        let entry = match entries.next() {
            Some(Ok(entry)) => entry,
            Some(Err(error)) => {
                visit(issue(parent, error))?;
                if parent == root {
                    break;
                }
                continue;
            }
            None => {
                stack.pop();
                continue;
            }
        };
        let name = entry.file_name();
        if is_excluded(&name) {
            continue;
        }
        let path = entry.path();
        let Some(name) = name.to_str() else {
            visit(issue(
                &path,
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Nome non rappresentabile in Unicode",
                ),
            ))?;
            continue;
        };
        let metadata = match inspector.inspect(&path) {
            Ok(metadata) => metadata,
            Err(error) => {
                visit(issue(&path, error))?;
                continue;
            }
        };
        if metadata.flags.is_symlink {
            continue;
        }
        let rel_path = if parent_rel.is_empty() {
            name.to_owned()
        } else {
            format!("{parent_rel}/{name}")
        };
        if metadata.is_dir {
            let opaque = metadata.flags.is_placeholder();
            // A junction can have the reparse bit without being reported as a symlink.
            if metadata.flags.is_reparse_point() && !opaque {
                continue;
            }
            visit(Ok(InventoryEntry::Directory {
                rel_path: rel_path.clone(),
                name: name.to_owned(),
                opaque,
            }))?;
            // WalkDir opens directories before its filter runs; gate enumeration here instead.
            if !opaque {
                match read_directory(&path) {
                    Ok(entries) => stack.push((path, rel_path, entries)),
                    Err(error) => visit(issue(&path, error))?,
                }
            }
        } else if metadata.is_file {
            if let Some(ext) = supported_extension(&path) {
                visit(Ok(InventoryEntry::File {
                    rel_path,
                    name: name.to_owned(),
                    ext,
                    metadata,
                }))?;
            } else if parent_rel.is_empty() {
                visit(Ok(InventoryEntry::OtherFile {
                    parent: parent_rel.clone(),
                }))?;
            }
        }
    }
    Ok(())
}

pub(crate) fn check_source_directory(
    root: &Path,
    inspector: &impl EntryMetadataInspector,
) -> io::Result<()> {
    if root.file_name().is_some_and(is_excluded) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Cartella di servizio esclusa",
        ));
    }
    check_directory(root, inspector)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::attr_guard::{MockMetadataInspector, RawWin32AttrFlags};

    #[test]
    fn guard_runs_before_enumeration_of_root_and_children() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        for name in [
            ".dropbox.cache",
            "placeholder",
            "symlink",
            "junction",
            "resident",
        ] {
            fs::create_dir(root.join(name)).unwrap();
        }
        let inspector = MockMetadataInspector::new([
            (
                root.join("placeholder"),
                RawWin32AttrFlags {
                    file_attributes: 0x0040_0400,
                    is_symlink: false,
                },
            ),
            (
                root.join("symlink"),
                RawWin32AttrFlags {
                    file_attributes: 0x0400,
                    is_symlink: true,
                },
            ),
            (
                root.join("junction"),
                RawWin32AttrFlags {
                    file_attributes: 0x0400,
                    is_symlink: false,
                },
            ),
        ]);
        let mut enumerations = Vec::new();
        let mut read_directory = |path: &Path| {
            enumerations.push(path.to_owned());
            fs::read_dir(path)
        };
        for rejected in ["placeholder", "symlink", "junction", ".dropbox.cache"] {
            walk(
                &root.join(rejected),
                &inspector,
                &CancellationToken::new(),
                &mut read_directory,
                &mut |_| Ok(()),
            )
            .unwrap();
        }
        let mut directories = Vec::new();
        walk(
            &root,
            &inspector,
            &CancellationToken::new(),
            &mut read_directory,
            &mut |entry| {
                if let Ok(InventoryEntry::Directory { name, opaque, .. }) = entry {
                    directories.push((name, opaque));
                }
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(enumerations, [root.clone(), root.join("resident")]);
        directories.sort();
        assert_eq!(
            directories,
            [
                (String::new(), false),
                ("placeholder".into(), true),
                ("resident".into(), false)
            ]
        );
    }
}
