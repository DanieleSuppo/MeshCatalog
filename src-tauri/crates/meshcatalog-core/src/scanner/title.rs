use std::sync::LazyLock;

use regex::Regex;

static PREFIX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^\s*(?:\[[^\]]+\][\s_.-]*|(?:19|20)\d{2}[\s_.-]+(?:0[1-9]|1[0-2])(?:[\s_.-]+(?:0[1-9]|[12]\d|3[01]))?[\s_.-]+)").unwrap()
});
static NOISE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^(?:pre-?supported|presupp|supported|unsupported|\d+(?:[.,]\d+)?\s*mm|1[_-]\d+|v\d+|stls?|3mf|lys|lyt|obj|ply|glb|step|stp|gcode|zip)$").unwrap()
});
static SCALES: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\d+(?:[.,]\d+)?\s*mm|1[_-]\d+").unwrap());
static POSE_EXPLICIT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^(?:(?:pose|poses|variant|variants|var|version|ver)[\s_.-]*(?:[a-z]|\d{1,2})?|v\d{1,2})$").unwrap()
});
static POSE_CODE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(?:[A-Za-z]|\d{1,2})$").unwrap());

pub(crate) fn is_pose_explicit(name: &str) -> bool {
    POSE_EXPLICIT.is_match(name.trim())
}

pub(crate) fn is_pose_code(name: &str) -> bool {
    POSE_CODE.is_match(name.trim())
}

pub(crate) fn is_bare_pose_title(title: &str) -> bool {
    is_pose_code(title) || is_pose_explicit(title)
}

pub(crate) fn contextual_display_title(
    folder_rel_path: &str,
    raw_name: &str,
    is_technical_dir: impl Fn(&str) -> bool,
) -> String {
    let clean = display_title(raw_name);
    if !folder_rel_path.is_empty() && is_bare_pose_title(&clean) {
        if let Some(seg) = folder_rel_path
            .rsplit('/')
            .find(|seg| !seg.is_empty() && !is_technical_dir(seg) && !is_bare_pose_title(seg))
        {
            let parent = display_title(seg);
            return format!("{parent} - {clean}");
        }
    }
    clean
}

pub(crate) fn display_title(raw: &str) -> String {
    let mut title = raw;
    while let Some(prefix) = PREFIX.find(title) {
        title = &title[prefix.end()..];
    }
    // Match scales before splitting underscores and decimal dots.
    let separator = |c: char| c.is_whitespace() || matches!(c, '_' | '.');
    let mut unscaled = String::with_capacity(title.len());
    let mut end = 0;
    for scale in SCALES.find_iter(title) {
        if title[..scale.start()]
            .chars()
            .next_back()
            .is_none_or(separator)
            && title[scale.end()..].chars().next().is_none_or(separator)
        {
            unscaled.push_str(&title[end..scale.start()]);
            end = scale.end();
        }
    }
    unscaled.push_str(&title[end..]);
    let title = unscaled.replace(['_', '.'], " ");
    let clean = title
        .split_whitespace()
        .filter(|token| !NOISE.is_match(token))
        .collect::<Vec<_>>()
        .join(" ");
    if clean.is_empty() {
        raw.to_owned()
    } else {
        clean
    }
}
