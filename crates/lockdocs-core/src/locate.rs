//! Find a dependency's installed sources on this machine: node_modules,
//! Python site-packages, the Cargo registry and the Go module cache.

use crate::{norm_name, Dep, Eco};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Where a package's files are.
#[derive(Debug, Clone)]
pub struct Source {
    /// Package root (for Python: the site-packages directory).
    pub dir: PathBuf,
    /// Exact file list when the ecosystem records one (Python RECORD).
    pub files: Option<Vec<PathBuf>>,
    /// Python METADATA path (its body is the README).
    pub metadata: Option<PathBuf>,
    /// The version found on disk (may differ from the lockfile).
    pub version: String,
    /// Human label: `node_modules/zod`, `~/.cargo/registry/...`, `fetched: registry.npmjs.org`.
    pub label: String,
    pub fetched: bool,
}

/// Ancestor directories from `root` up to (but not past) the home directory.
fn ancestors(root: &Path) -> Vec<PathBuf> {
    let home = dirs::home_dir();
    let mut out = Vec::new();
    for a in root.ancestors().take(8) {
        if Some(a.to_path_buf()) == home && !out.is_empty() {
            break;
        }
        out.push(a.to_path_buf());
    }
    out
}

pub fn locate(dep: &Dep, root: &Path) -> Option<Source> {
    match dep.eco {
        Eco::Npm => npm(dep, root),
        Eco::PyPI => python(dep, root),
        Eco::Cargo => cargo(dep, root),
        Eco::Go => go(dep, root),
    }
}

// ---------------------------------------------------------------- npm

fn npm_version(dir: &Path) -> Option<String> {
    let t = std::fs::read_to_string(dir.join("package.json")).ok()?;
    let v: serde_json::Value = serde_json::from_str(&t).ok()?;
    v.get("version")?.as_str().map(String::from)
}

/// Installed version of an npm package reachable from `root`.
pub fn npm_installed(name: &str, root: &Path) -> Option<(PathBuf, String)> {
    for a in ancestors(root) {
        let d = a.join("node_modules").join(name);
        if let Some(v) = npm_version(&d) {
            return Some((d, v));
        }
    }
    None
}

fn npm(dep: &Dep, root: &Path) -> Option<Source> {
    let mut fallback = None;
    for a in ancestors(root) {
        let nm = a.join("node_modules");
        if !nm.is_dir() {
            continue;
        }
        let d = nm.join(&dep.name);
        if let Some(v) = npm_version(&d) {
            if v == dep.version {
                return Some(src(d, &a, v));
            }
            fallback.get_or_insert((d, a.clone(), v));
        }
        // pnpm's virtual store keeps every version side by side.
        let store = nm.join(".pnpm");
        if store.is_dir() {
            let prefix = format!("{}@{}", dep.name.replace('/', "+"), dep.version);
            if let Ok(rd) = std::fs::read_dir(&store) {
                for e in rd.flatten() {
                    let f = e.file_name().to_string_lossy().to_string();
                    if f == prefix || f.starts_with(&format!("{prefix}_")) || f.starts_with(&format!("{prefix}(")) {
                        let d = e.path().join("node_modules").join(&dep.name);
                        if let Some(v) = npm_version(&d) {
                            return Some(src(d, &a, v));
                        }
                    }
                }
            }
        }
    }
    let (d, a, v) = fallback?;
    Some(src(d, &a, v))
}

fn src(dir: PathBuf, base: &Path, version: String) -> Source {
    let label = dir.strip_prefix(base).map(|p| p.display().to_string()).unwrap_or_else(|_| tilde(&dir));
    Source {
        dir,
        files: None,
        metadata: None,
        version,
        label: label.replace('\\', "/"),
        fetched: false,
    }
}

pub fn tilde(p: &Path) -> String {
    if let Some(h) = dirs::home_dir() {
        if let Ok(r) = p.strip_prefix(&h) {
            return format!("~/{}", r.display()).replace('\\', "/");
        }
    }
    p.display().to_string().replace('\\', "/")
}

// ---------------------------------------------------------------- python

fn glob_site_packages(prefix: &Path, out: &mut Vec<PathBuf>) {
    let win = prefix.join("Lib").join("site-packages");
    if win.is_dir() {
        out.push(win);
    }
    for lib in ["lib", "lib64"] {
        if let Ok(rd) = std::fs::read_dir(prefix.join(lib)) {
            for e in rd.flatten() {
                if e.file_name().to_string_lossy().starts_with("python") {
                    let sp = e.path().join("site-packages");
                    if sp.is_dir() && !out.contains(&sp) {
                        out.push(sp);
                    }
                }
            }
        }
    }
}

fn system_site_packages() -> &'static Vec<PathBuf> {
    static SYS: OnceLock<Vec<PathBuf>> = OnceLock::new();
    SYS.get_or_init(|| {
        if std::env::var("LOCKDOCS_NO_SYSTEM_PYTHON").is_ok() {
            return Vec::new();
        }
        let code = "import site,sys\nfor p in site.getsitepackages()+[site.getusersitepackages()]: print(p)";
        for py in ["python3", "python"] {
            if let Ok(o) = std::process::Command::new(py).args(["-c", code]).output() {
                if o.status.success() {
                    return String::from_utf8_lossy(&o.stdout).lines().map(PathBuf::from).filter(|p| p.is_dir()).collect();
                }
            }
        }
        Vec::new()
    })
}

/// site-packages directories for a project: local virtualenvs first, then the
/// active environment, then the system interpreter.
pub fn site_packages(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for a in ancestors(root).into_iter().take(3) {
        for v in [".venv", "venv", "env", ".env"] {
            let p = a.join(v);
            if p.join("pyvenv.cfg").is_file() {
                glob_site_packages(&p, &mut out);
            }
        }
    }
    for var in ["VIRTUAL_ENV", "CONDA_PREFIX"] {
        if let Ok(v) = std::env::var(var) {
            glob_site_packages(Path::new(&v), &mut out);
        }
    }
    for p in system_site_packages() {
        if !out.contains(p) {
            out.push(p.clone());
        }
    }
    out
}

/// A dist-info directory in `sp` for `name`, with its version.
pub fn dist_infos(sp: &Path) -> Vec<(String, String, PathBuf)> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(sp) else { return out };
    for e in rd.flatten() {
        let f = e.file_name().to_string_lossy().to_string();
        let Some(stem) = f.strip_suffix(".dist-info") else { continue };
        // name-version; names have no '-' after normalization in wheels.
        let Some((n, v)) = stem.rsplit_once('-') else { continue };
        out.push((norm_name(Eco::PyPI, n), v.to_string(), e.path()));
    }
    out
}

fn python(dep: &Dep, root: &Path) -> Option<Source> {
    let want = norm_name(Eco::PyPI, &dep.name);
    let mut fallback = None;
    for sp in site_packages(root) {
        for (n, v, di) in dist_infos(&sp) {
            if n != want {
                continue;
            }
            let mut s = python_source(&sp, &di, v.clone());
            for a in ancestors(root).into_iter().take(3) {
                if let Ok(r) = sp.strip_prefix(&a) {
                    s.label = r.display().to_string().replace('\\', "/");
                    break;
                }
            }
            if v == dep.version {
                return Some(s);
            }
            fallback.get_or_insert(s);
        }
    }
    fallback
}

pub fn python_source(sp: &Path, dist_info: &Path, version: String) -> Source {
    let mut files = Vec::new();
    if let Ok(rec) = std::fs::read_to_string(dist_info.join("RECORD")) {
        for line in rec.lines() {
            let path = line.split(',').next().unwrap_or("");
            if path.is_empty() || path.starts_with("..") || path.contains(".dist-info/") || path.contains("__pycache__") {
                continue;
            }
            files.push(PathBuf::from(path));
        }
    }
    let label = tilde(dist_info.parent().unwrap_or(sp));
    Source {
        dir: sp.to_path_buf(),
        files: Some(files),
        metadata: Some(dist_info.join("METADATA")).filter(|p| p.is_file()),
        version,
        label,
        fetched: false,
    }
}

// ---------------------------------------------------------------- cargo

fn cargo_home() -> Option<PathBuf> {
    std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|h| h.join(".cargo")))
}

fn cargo(dep: &Dep, root: &Path) -> Option<Source> {
    let dirname = format!("{}-{}", dep.name, dep.version);
    for a in ancestors(root).into_iter().take(3) {
        for v in ["vendor", "third_party"] {
            for cand in [a.join(v).join(&dirname), a.join(v).join(&dep.name)] {
                if cand.join("Cargo.toml").is_file() {
                    return Some(src(cand, &a, dep.version.clone()));
                }
            }
        }
    }
    let reg = cargo_home()?.join("registry").join("src");
    for e in std::fs::read_dir(&reg).ok()?.flatten() {
        let d = e.path().join(&dirname);
        if d.join("Cargo.toml").is_file() {
            return Some(Source {
                label: tilde(&d),
                dir: d,
                files: None,
                metadata: None,
                version: dep.version.clone(),
                fetched: false,
            });
        }
    }
    None
}

// ---------------------------------------------------------------- go

/// Go module cache path escaping: `A` -> `!a`.
pub fn go_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c.is_ascii_uppercase() {
            out.push('!');
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

fn go_modcache() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("GOMODCACHE").filter(|p| !p.is_empty()) {
        return Some(PathBuf::from(p));
    }
    if let Some(p) = std::env::var_os("GOPATH").filter(|p| !p.is_empty()) {
        let first = std::env::split_paths(&p).next()?;
        return Some(first.join("pkg").join("mod"));
    }
    dirs::home_dir().map(|h| h.join("go").join("pkg").join("mod"))
}

fn go(dep: &Dep, root: &Path) -> Option<Source> {
    for a in ancestors(root).into_iter().take(3) {
        let v = a.join("vendor").join(&dep.name);
        if v.is_dir() {
            return Some(src(v, &a, dep.version.clone()));
        }
    }
    let d = go_modcache()?.join(format!("{}@{}", go_escape(&dep.name), go_escape(&dep.version)));
    if d.is_dir() {
        return Some(Source {
            label: tilde(&d),
            dir: d,
            files: None,
            metadata: None,
            version: dep.version.clone(),
            fetched: false,
        });
    }
    None
}

#[cfg(test)]
mod tests {
    #[test]
    fn go_escaping() {
        assert_eq!(super::go_escape("github.com/BurntSushi/toml"), "github.com/!burnt!sushi/toml");
    }
}
