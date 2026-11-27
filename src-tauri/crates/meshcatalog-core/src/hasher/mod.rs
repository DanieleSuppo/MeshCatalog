//! Content hashing and model fingerprints.

mod blake3_hasher;

pub(crate) use blake3_hasher::content_hash;

pub(crate) fn fingerprint(kind: &str, mut members: Vec<(String, i64)>) -> Option<String> {
    if members.is_empty() {
        return None;
    }
    if kind != "folder_pack" {
        return members.into_iter().next().map(|(hash, _)| hash);
    }
    members.sort_unstable();
    let mut hasher = blake3::Hasher::new();
    for (hash, size) in members {
        // Hashes are fixed width; binary sizes give each member an unambiguous boundary.
        hasher.update(hash.as_bytes());
        hasher.update(&size.to_le_bytes());
    }
    Some(hasher.finalize().to_hex().to_string())
}
