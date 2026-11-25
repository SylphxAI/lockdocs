use super::{ancestors, go_modcache, src, tilde, Source};
use crate::Dep;
use std::path::Path;

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

pub(super) fn locate(dep: &Dep, root: &Path) -> Option<Source> {
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
