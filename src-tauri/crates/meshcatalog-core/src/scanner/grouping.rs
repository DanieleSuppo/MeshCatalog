use std::{
    collections::{BTreeMap, BTreeSet},
    sync::LazyLock,
};

use regex::Regex;

use super::walker::InventoryEntry;

static SCALE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^(?:\d+(?:[.,]\d+)?\s*mm|1[_-]\d+)$").unwrap());

pub(crate) fn is_image(ext: &str) -> bool {
    matches!(ext, "jpg" | "jpeg" | "png" | "webp")
}

pub(crate) fn parent_path(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(parent, _)| parent)
}

fn technical(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "stl"
            | "stls"
            | "presupported"
            | "pre-supported"
            | "supported"
            | "unsupported"
            | "images"
            | "renders"
            | "render"
            | "lychee"
            | "files"
    ) || SCALE.is_match(name)
}

#[derive(Default)]
struct Directory<'a> {
    name: &'a str,
    children: Vec<&'a str>,
    files: Vec<Member<'a>>,
    has_other_file: bool,
    opaque: bool,
}

#[derive(Clone, Copy)]
pub(crate) struct Member<'a> {
    pub path: &'a str,
    pub name: &'a str,
    pub ext: &'a str,
    pub cloud: bool,
}

pub(crate) struct Model<'a> {
    pub path: &'a str,
    pub folder: &'a str,
    pub name: &'a str,
    pub kind: &'static str,
    pub members: Vec<Member<'a>>,
}

#[derive(Default)]
pub(crate) struct Grouping<'a> {
    pub models: Vec<Model<'a>>,
    pub orphans: Vec<Member<'a>>,
    pub folded: BTreeSet<&'a str>,
}

pub(crate) fn group(entries: &[InventoryEntry]) -> Grouping<'_> {
    let mut dirs = BTreeMap::<&str, Directory>::new();
    for entry in entries {
        match entry {
            InventoryEntry::Directory {
                rel_path,
                name,
                opaque,
            } => {
                let dir = dirs.entry(rel_path).or_default();
                dir.name = name;
                dir.opaque = *opaque;
                if !rel_path.is_empty() {
                    dirs.entry(parent_path(rel_path))
                        .or_default()
                        .children
                        .push(rel_path);
                }
            }
            InventoryEntry::File {
                rel_path,
                name,
                ext,
                metadata,
            } => {
                dirs.entry(parent_path(rel_path))
                    .or_default()
                    .files
                    .push(Member {
                        path: rel_path,
                        name,
                        ext,
                        cloud: metadata.flags.is_placeholder(),
                    });
            }
            InventoryEntry::OtherFile { parent } => {
                dirs.entry(parent).or_default().has_other_file = true
            }
        }
    }
    let mut result = Grouping::default();
    for (&path, dir) in &dirs {
        if result.folded.contains(path) {
            continue;
        }
        let direct_zip = dir.files.iter().any(|file| file.ext == "zip");
        result
            .models
            .extend(dir.files.iter().filter(|f| f.ext == "zip").map(|f| Model {
                path: f.path,
                folder: path,
                name: f.name,
                kind: "zip_archive",
                members: vec![*f],
            }));
        let mut members: Vec<_> = dir
            .files
            .iter()
            .copied()
            .filter(|f| f.ext != "zip")
            .collect();
        let mut absorbed = Vec::new();
        if !path.is_empty() || !dir.files.is_empty() || dir.has_other_file {
            for &child in &dir.children {
                let child_dir = &dirs[child];
                if !child_dir.opaque
                    && child_dir.children.is_empty()
                    && technical(child_dir.name)
                    && !child_dir.files.iter().any(|f| f.ext == "zip")
                {
                    members.extend(&child_dir.files);
                    absorbed.push(child);
                }
            }
        }
        let shapes = members.iter().filter(|f| !is_image(f.ext)).count();
        if shapes == 0 {
            result
                .orphans
                .extend(dir.files.iter().copied().filter(|f| is_image(f.ext)));
            continue;
        }
        members.sort_unstable_by_key(|f| (is_image(f.ext), f.path));
        result.folded.extend(absorbed.iter().copied());
        let model = if members.len() >= 2 {
            let leaf = !path.is_empty() && !direct_zip && absorbed.len() == dir.children.len();
            if leaf {
                result.folded.insert(path);
            }
            Model {
                path,
                folder: if leaf { parent_path(path) } else { path },
                name: dir.name,
                kind: "folder_pack",
                members,
            }
        } else {
            let file = members[0];
            Model {
                path: file.path,
                folder: path,
                name: file.name,
                kind: "single_file",
                members,
            }
        };
        result.models.push(model);
    }
    result.models.sort_unstable_by_key(|model| model.path);
    result
}
