//! On-disk cache location and a stable hash for cache keys.

use std::path::PathBuf;

/// `LOCKDOCS_CACHE`, else the OS cache directory, else a temp dir.
pub fn dir() -> PathBuf {
    if let Some(p) = std::env::var_os("LOCKDOCS_CACHE").filter(|p| !p.is_empty()) {
        return PathBuf::from(p);
    }
    dirs::cache_dir().unwrap_or_else(std::env::temp_dir).join("lockdocs")
}

/// FNV-1a 64: stable across builds and platforms.
pub fn hash(parts: &[&str]) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for p in parts {
        for b in p.as_bytes().iter().chain(std::iter::once(&0u8)) {
            h ^= *b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
    }
    format!("{h:016x}")
}

/// Total bytes and file count under the cache directory.
pub fn usage() -> (u64, usize) {
    fn walk(p: &std::path::Path, acc: &mut (u64, usize)) {
        if let Ok(rd) = std::fs::read_dir(p) {
            for e in rd.flatten() {
                match e.file_type() {
                    Ok(t) if t.is_dir() => walk(&e.path(), acc),
                    Ok(_) => {
                        acc.0 += e.metadata().map(|m| m.len()).unwrap_or(0);
                        acc.1 += 1;
                    }
                    _ => {}
                }
            }
        }
    }
    let mut acc = (0, 0);
    walk(&dir(), &mut acc);
    acc
}
