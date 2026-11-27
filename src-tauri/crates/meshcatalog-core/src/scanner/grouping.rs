use std::{
    collections::{BTreeMap, BTreeSet},
    sync::LazyLock,
};

use regex::Regex;

use super::{
    exclusions::supported_extension,
    title::{is_pose_code, is_pose_explicit},
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
            | "lys"
            | "3mf"
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

pub(crate) fn absorbable_candidate(name: &str) -> bool {
    technical(name) || is_pose_explicit(name) || is_pose_code(name)
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
        incomplete_children.insert(WalkCoverage::key(parent));
        if absorbable_candidate(name) || supported_extension(std::path::Path::new(name)).is_some() {
            let mut anc = parent;
            loop {
                result.incomplete_packs.insert(WalkCoverage::key(anc));
                if anc.is_empty() {
                    break;
                }
                let seg = anc.rsplit('/').next().unwrap_or(anc);
                if !absorbable_candidate(seg) {
                    break;
                }
                anc = parent_path(anc);
            }
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
        let mut frozen_descendant = false;
        let mut anc = path;
        while !parent_path(anc).is_empty() {
            let seg = anc.rsplit('/').next().unwrap_or(anc);
            if !absorbable_candidate(seg) {
                break;
            }
            let parent = parent_path(anc);
            if frozen_packs.contains(parent) {
                frozen_descendant = true;
                break;
            }
            anc = parent;
        }
        if frozen_descendant {
            continue;
        }
        let mut members: Vec<_> = dir
            .files
            .iter()
            .copied()
            .filter(|f| f.ext != "zip")
            .collect();
        let has_direct_non_zip = !members.is_empty();
        let mut absorbed_dirs = Vec::new();
        let mut absorbed_direct = 0usize;
        if !path.is_empty() {
            for &child in &dir.children {
                if can_absorb_subtree(
                    child,
                    &dirs,
                    coverage,
                    &incomplete_children,
                    false,
                    has_direct_non_zip,
                ) {
                    absorbed_direct += 1;
                    collect_subtree(child, &dirs, &mut members, &mut absorbed_dirs);
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
        result.folded.extend(absorbed_dirs.iter().copied());
        let leaf = !path.is_empty()
            && !technical(dir.name)
            && !direct_zip
            && !coverage.unobserved(path)
            && !incomplete_children.contains(WalkCoverage::key(path).as_str())
            && absorbed_direct == dir.children.len();
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

fn can_absorb_subtree(
    node: &str,
    dirs: &BTreeMap<&str, Directory<'_>>,
    coverage: &WalkCoverage,
    incomplete_children: &BTreeSet<String>,
    under_tech_or_pose: bool,
    parent_has_direct_non_zip_files: bool,
) -> bool {
    let d = &dirs[node];
    if d.opaque
        || coverage.unobserved(node)
        || incomplete_children.contains(&WalkCoverage::key(node))
        || d.files.iter().any(|f| f.ext == "zip")
    {
        return false;
    }
    let tech_or_explicit = technical(d.name) || is_pose_explicit(d.name);
    let code = is_pose_code(d.name);
    if !tech_or_explicit && !code {
        return false;
    }
    if code
        && !under_tech_or_pose
        && !parent_has_direct_non_zip_files
        && !d
            .children
            .iter()
            .any(|&c| technical(dirs[c].name) || is_pose_explicit(dirs[c].name))
    {
        return false;
    }
    d.children
        .iter()
        .all(|&c| can_absorb_subtree(c, dirs, coverage, incomplete_children, true, false))
}

fn collect_subtree<'a>(
    node: &'a str,
    dirs: &BTreeMap<&'a str, Directory<'a>>,
    members: &mut Vec<Member<'a>>,
    absorbed_dirs: &mut Vec<&'a str>,
) {
    let d = &dirs[node];
    members.extend(&d.files);
    absorbed_dirs.push(node);
    for &c in &d.children {
        collect_subtree(c, dirs, members, absorbed_dirs);
    }
}
