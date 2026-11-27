use std::{ffi::OsStr, path::Path};

pub fn is_excluded(name: &OsStr) -> bool {
    let name = name.to_string_lossy();
    name.starts_with("._")
        || [
            "$RECYCLE.BIN",
            "System Volume Information",
            ".dropbox.cache",
            ".git",
            "Thumbs.db",
            ".DS_Store",
            "__MACOSX",
        ]
        .iter()
        .any(|excluded| name.eq_ignore_ascii_case(excluded))
}

pub fn supported_extension(path: &Path) -> Option<String> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    match ext.as_str() {
        "stl" | "3mf" | "lys" | "lyt" | "obj" | "ply" | "glb" | "step" | "stp" | "gcode"
        | "zip" | "jpg" | "jpeg" | "png" | "webp" => Some(ext),
        _ => None,
    }
}
