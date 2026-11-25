use super::{active_python_prefixes, ancestors, no_system_python, tilde, Source};
use crate::{norm_name, Dep, Eco};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

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
        if no_system_python() {
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
    for v in active_python_prefixes() {
        glob_site_packages(Path::new(&v), &mut out);
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

pub(super) fn locate(dep: &Dep, root: &Path) -> Option<Source> {
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
