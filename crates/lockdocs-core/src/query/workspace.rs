//! Reuse engines by project root, lockfile state, and network policy.

use super::{Engine, Options, Workspace};
use std::path::Path;
use std::sync::Arc;

fn lock_stamp(root: &Path) -> u64 {
    // Re-read the project when any lockfile in the root changes.
    let mut h: u64 = 0;
    if let Ok(rd) = std::fs::read_dir(root) {
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().to_string();
            if crate::lockfile::LOCKFILES.iter().any(|(f, _)| *f == n) || crate::lockfile::is_requirements(&n) || n == "package.json" || n == "pyproject.toml" {
                if let Ok(m) = e.metadata() {
                    let t = m
                        .modified()
                        .ok()
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map_or(0, |d| d.as_nanos() as u64);
                    h = h.wrapping_mul(31).wrapping_add(t ^ m.len());
                }
            }
        }
    }
    h
}

impl Workspace {
    pub fn engine(&self, root: &Path, opts: &Options) -> Arc<Engine> {
        let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
        let stamp = lock_stamp(&root);
        let mut m = self.engines.lock().unwrap();
        if let Some((s, e)) = m.get(&root) {
            if *s == stamp && e.opts == *opts {
                return e.clone();
            }
        }
        let e = Arc::new(Engine::new(&root, opts.clone()));
        m.insert(root, (stamp, e.clone()));
        e
    }
}
