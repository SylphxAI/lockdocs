//! Project lockfile discovery and installed-package fallbacks.

use super::Project;
use crate::lockfile::{self, LOCKFILES};
use crate::{locate, norm_name, Dep, Eco};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Does `dir` declare dependencies of `eco` (a manifest without its lockfile)?
fn declares(dir: &Path, eco: Eco) -> bool {
    match eco {
        Eco::Npm => dir.join("package.json").is_file(),
        Eco::Cargo => dir.join("Cargo.toml").is_file(),
        Eco::Go => dir.join("go.mod").is_file(),
        Eco::PyPI => ["pyproject.toml", "setup.py", "setup.cfg", "Pipfile"].iter().any(|f| dir.join(f).is_file()),
    }
}

impl Project {
    pub fn load(root: &Path) -> Project {
        let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
        let mut p = Project {
            root: root.clone(),
            lockfiles: Vec::new(),
            deps: Vec::new(),
            warnings: Vec::new(),
        };
        let mut have: HashSet<Eco> = HashSet::new();
        // The project directory, then ancestors (monorepo roots) for
        // ecosystems not found yet.
        let home = dirs::home_dir();
        for (depth, dir) in root.ancestors().take(6).enumerate() {
            if depth > 0 && Some(dir.to_path_buf()) == home {
                break;
            }
            if depth == 1 {
                // Above the project, only look for the lockfile of an ecosystem the
                // project declares (a workspace member's package.json, Cargo.toml...).
                for eco in Eco::all() {
                    if !declares(&root, eco) {
                        have.insert(eco);
                    }
                }
                if have.len() == 4 {
                    break;
                }
            }
            p.read_dir(dir, &mut have);
            if depth == 0 {
                p.fallbacks(dir, &mut have);
            }
            if have.len() == 4 || dir.join(".git").exists() {
                break;
            }
        }
        // Keep the first occurrence of each (eco, name, version).
        let mut seen = HashSet::new();
        p.deps.retain(|d| seen.insert((d.eco, norm_name(d.eco, &d.name), d.version.clone())));
        // Direct first, then by name.
        p.deps.sort_by(|a, b| b.direct.cmp(&a.direct).then(a.eco.cmp(&b.eco)).then(a.name.cmp(&b.name)));
        p
    }

    fn rel(&self, path: &Path) -> String {
        path.strip_prefix(&self.root)
            .map(|r| r.display().to_string())
            .unwrap_or_else(|_| path.display().to_string())
            .replace('\\', "/")
    }

    fn read_dir(&mut self, dir: &Path, have: &mut HashSet<Eco>) {
        let mut found_here: HashSet<Eco> = HashSet::new();
        for (name, eco) in LOCKFILES {
            if have.contains(eco) || found_here.contains(eco) {
                continue;
            }
            let path = dir.join(name);
            let Ok(text) = std::fs::read_to_string(&path) else { continue };
            match lockfile::parse(name, &text, dir) {
                Some(Ok(mut deps)) => {
                    let rel = self.rel(&path);
                    for d in &mut deps {
                        d.from = if d.from.ends_with("(git)") { format!("{rel} (git)") } else { rel.clone() };
                    }
                    self.deps.extend(deps);
                    self.lockfiles.push(rel);
                    found_here.insert(*eco);
                }
                Some(Err(e)) => self.warnings.push(format!("could not parse {}: {e}", self.rel(&path))),
                None => {}
            }
        }
        if !have.contains(&Eco::PyPI) && !found_here.contains(&Eco::PyPI) {
            let mut reqs: Vec<PathBuf> = std::fs::read_dir(dir)
                .map(|rd| {
                    rd.flatten()
                        .map(|e| e.path())
                        .filter(|p| p.file_name().is_some_and(|n| lockfile::is_requirements(&n.to_string_lossy())))
                        .collect()
                })
                .unwrap_or_default();
            reqs.sort();
            for path in reqs {
                let Ok(text) = std::fs::read_to_string(&path) else { continue };
                let rel = self.rel(&path);
                let deps = lockfile::requirements(&text, &rel);
                if !deps.is_empty() {
                    self.deps.extend(deps);
                    self.lockfiles.push(rel);
                    found_here.insert(Eco::PyPI);
                }
            }
        }
        have.extend(found_here);
    }

    /// No lockfile, but installed packages: read the install itself.
    fn fallbacks(&mut self, dir: &Path, have: &mut HashSet<Eco>) {
        if !have.contains(&Eco::Npm) && dir.join("package.json").is_file() {
            let direct = lockfile::package_json_direct(dir);
            let mut n = 0;
            let mut names: Vec<&String> = direct.iter().collect();
            names.sort();
            for name in names {
                if let Some((_, v)) = locate::npm_installed(name, dir) {
                    self.deps.push(Dep {
                        eco: Eco::Npm,
                        name: name.clone(),
                        version: v,
                        direct: true,
                        from: "node_modules".into(),
                    });
                    n += 1;
                }
            }
            if n > 0 {
                have.insert(Eco::Npm);
                self.lockfiles.push("package.json + node_modules".into());
            }
        }
        let is_py = ["pyproject.toml", "setup.py", "setup.cfg"].iter().any(|f| dir.join(f).is_file()) || dir.join(".venv").is_dir();
        if !have.contains(&Eco::PyPI) && is_py {
            let direct = lockfile::pyproject_direct(dir);
            let mut n = 0;
            for sp in locate::site_packages(dir).into_iter().take(1) {
                for (name, v, _) in locate::dist_infos(&sp) {
                    let is_direct = direct.is_empty() || direct.contains(&name);
                    self.deps.push(Dep {
                        eco: Eco::PyPI,
                        name,
                        version: v,
                        direct: is_direct,
                        from: "site-packages".into(),
                    });
                    n += 1;
                }
            }
            if n > 0 {
                have.insert(Eco::PyPI);
                self.lockfiles.push("site-packages".into());
            }
        }
    }
}
