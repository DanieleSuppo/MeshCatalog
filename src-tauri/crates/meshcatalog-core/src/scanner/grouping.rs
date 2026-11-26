use std::{
    collections::{BTreeMap, BTreeSet},
    sync::LazyLock,
};

use regex::Regex;

use super::{
    exclusions::supported_extension,
    walker::{InventoryEntry, WalkCoverage},
};

static SCALE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^(?:\d+(?:[.,]\d+)?\s*mm|1[_-]\d+)$").unwrap());

pub(crate) fn is_image(ext: &str) -> bool {
    matches!(ext, "jpg" | "jpeg" | "png" | "webp")
}

pub(crate) fn parent_path(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(parent, _)| parent)
}

pub(crate) fn technical(name: &str) -> bool {
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
    incomplete_packs: BTreeSet<String>,
}

impl Grouping<'_> {
    pub fn incomplete_pack(&self, path: &str) -> bool {
        !self.incomplete_packs.is_empty()
            && self.incomplete_packs.contains(&WalkCoverage::key(path))
    }
}

pub(crate) fn group<'a>(
    entries: &'a [InventoryEntry],
    coverage: &WalkCoverage,
    legacy_root_children: &BTreeSet<String>,
    was_pack: impl Fn(&str) -> bool,
) -> Grouping<'a> {
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
            InventoryEntry::UnobservedLink { .. } => {}
        }
    }
    let mut result = Grouping::default();
    let mut incomplete_children = BTreeSet::new();
    for gap in coverage.gaps().filter(|path| !path.is_empty()) {
        let parent = parent_path(gap);
        let name = gap.rsplit('/').next().unwrap_or(gap);
        incomplete_children.insert(parent);
        if technical(name) || supported_extension(std::path::Path::new(name)).is_some() {
            result.incomplete_packs.insert(parent.to_owned());
        }
        // A technical child with a gap cannot be proven terminal or folded.
        if !parent.is_empty() && technical(parent.rsplit('/').next().unwrap_or(parent)) {
            result
                .incomplete_packs
                .insert(parent_path(parent).to_owned());
        }
    }
    let mut frozen_packs = BTreeSet::new();
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
        // A partial scan must not dismantle a legacy root pack before a complete regrouping.
        if path.is_empty() && !legacy_root_children.is_empty() {
            result.incomplete_packs.insert(String::new());
        }
        if !legacy_root_children.is_empty()
            && !path.is_empty()
            && legacy_root_children.contains(&WalkCoverage::key(path))
        {
            continue;
        }
        // Defer regrouping the historical pack and its potentially absorbed members.
        if was_pack(path) && (coverage.unobserved(path) || result.incomplete_pack(path)) {
            frozen_packs.insert(path);
            continue;
        }
        if !parent_path(path).is_empty()
            && frozen_packs.contains(parent_path(path))
            && technical(dir.name)
        {
            continue;
        }
        let mut members: Vec<_> = dir
            .files
            .iter()
            .copied()
            .filter(|f| f.ext != "zip")
            .collect();
        let mut absorbed = Vec::new();
        if !path.is_empty() {
            for &child in &dir.children {
                let child_dir = &dirs[child];
                if !child_dir.opaque
                    && !coverage.unobserved(child)
                    && !incomplete_children.contains(WalkCoverage::key(child).as_str())
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
        let leaf = !path.is_empty()
            && !technical(dir.name)
            && !direct_zip
            && !coverage.unobserved(path)
            && !incomplete_children.contains(WalkCoverage::key(path).as_str())
            && absorbed.len() == dir.children.len();
        let indirect_leaf = leaf && parent_path(members[0].path) != path;
        let model = if members.len() >= 2 || indirect_leaf {
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
