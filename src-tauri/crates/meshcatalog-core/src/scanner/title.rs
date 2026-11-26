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
