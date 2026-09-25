//! A project: its lockfiles and the exact dependency versions they pin.

use crate::lockfile::{self, LOCKFILES};
use crate::{locate, norm_name, Dep, Eco};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Project {
    pub root: PathBuf,
    /// Lockfiles read, relative to `root` when inside it.
    pub lockfiles: Vec<String>,
    pub deps: Vec<Dep>,
    pub warnings: Vec<String>,
}

/// A package reference: `zod`, `zod@3.23.8`, `npm:zod`, `pypi:pydantic@2.9.2`,
/// `github.com/gin-gonic/gin`, `@tanstack/react-query@5`.
#[derive(Debug, Clone, PartialEq)]
pub struct Spec {
    pub eco: Option<Eco>,
    pub name: String,
    pub version: Option<String>,
}

impl Spec {
    pub fn parse(s: &str) -> Spec {
        let s = s.trim();
        let (eco, rest) = match s.split_once(':') {
            Some((e, r)) if Eco::parse(e).is_some() && !e.contains('/') => (Eco::parse(e), r),
            _ => (None, s),
        };
        let start = if rest.starts_with('@') { 1 } else { 0 };
        let (name, version) = match rest[start..].find('@') {
            Some(i) => (&rest[..i + start], Some(rest[i + start + 1..].trim_start_matches('v').to_string())),
            None => match rest.find("==") {
                Some(i) => (&rest[..i], Some(rest[i + 2..].to_string())),
                None => (rest, None),
            },
        };
        let version = version.filter(|v| !v.is_empty());
        Spec {
            eco,
            name: name.to_string(),
            version,
        }
    }

    fn name_matches(&self, d: &Dep) -> bool {
        if let Some(e) = self.eco {
            if e != d.eco {
                return false;
            }
        }
        if norm_name(d.eco, &d.name) == norm_name(d.eco, &self.name) {
            return true;
        }
        // Go: `gin` or `gin-gonic/gin` for `github.com/gin-gonic/gin` (also `/v2` majors).
        if d.eco == Eco::Go {
            let segs: Vec<&str> = d.name.split('/').collect();
            let base = match segs.last() {
                Some(l) if l.len() >= 2 && l.starts_with('v') && l[1..].chars().all(|c| c.is_ascii_digit()) && segs.len() > 1 => segs[segs.len() - 2],
                Some(l) => l,
                None => "",
            };
            return base == self.name || d.name.ends_with(&format!("/{}", self.name));
        }
        false
    }

    /// Does the dependency's version satisfy the requested one? `3` matches
    /// `3.23.8`; `3.23` matches `3.23.8`.
    pub fn version_matches(&self, v: &str) -> bool {
        match &self.version {
            None => true,
            Some(want) => {
                let v = v.trim_start_matches('v');
                v == want || v.starts_with(&format!("{want}.")) || v.starts_with(&format!("{want}-"))
            }
        }
    }

    pub fn is_exact(&self) -> bool {
        self.version.as_ref().is_some_and(|v| v.split('.').count() >= 3 || self.eco == Some(Eco::Go))
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
                        d.from = rel.clone();
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

    pub fn direct(&self) -> impl Iterator<Item = &Dep> {
        self.deps.iter().filter(|d| d.direct)
    }

    /// Dependencies matching a spec, best first (direct, then version match).
    pub fn find(&self, spec: &Spec) -> Vec<&Dep> {
        let mut v: Vec<&Dep> = self.deps.iter().filter(|d| spec.name_matches(d)).collect();
        v.sort_by_key(|d| (!spec.version_matches(&d.version), !d.direct));
        v
    }

    /// Names that look like `query` (for "did you mean").
    pub fn similar(&self, query: &str, n: usize) -> Vec<String> {
        let q = query.to_ascii_lowercase();
        let mut v: Vec<String> = self
            .deps
            .iter()
            .filter(|d| {
                let name = d.name.to_ascii_lowercase();
                name.contains(&q) || (q.len() >= 3 && q.contains(name.rsplit('/').next().unwrap_or(&name)))
            })
            .map(|d| format!("{}:{}@{}", d.eco, d.name, d.version))
            .collect();
        v.dedup();
        v.truncate(n);
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn specs() {
        assert_eq!(
            Spec::parse("zod"),
            Spec {
                eco: None,
                name: "zod".into(),
                version: None
            }
        );
        assert_eq!(
            Spec::parse("npm:@tanstack/react-query@5.1.0"),
            Spec {
                eco: Some(Eco::Npm),
                name: "@tanstack/react-query".into(),
                version: Some("5.1.0".into())
            }
        );
        assert_eq!(Spec::parse("pydantic==2.9.2").version.as_deref(), Some("2.9.2"));
        assert_eq!(Spec::parse("github.com/gin-gonic/gin@v1.10.0").version.as_deref(), Some("1.10.0"));
        let s = Spec::parse("zod@3");
        assert!(s.version_matches("3.23.8") && !s.version_matches("4.0.1") && !s.version_matches("30.0.0"));
        let d = Dep {
            eco: Eco::Go,
            name: "github.com/go-chi/chi/v5".into(),
            version: "v5.1.0".into(),
            direct: true,
            from: "go.mod".into(),
        };
        assert!(Spec::parse("chi").name_matches(&d));
        let d = Dep {
            eco: Eco::PyPI,
            name: "Typing_Extensions".into(),
            version: "4".into(),
            direct: true,
            from: "x".into(),
        };
        assert!(Spec::parse("typing-extensions").name_matches(&d));
    }
}
