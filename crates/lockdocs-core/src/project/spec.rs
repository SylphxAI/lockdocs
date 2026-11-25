//! Package spec parsing and matching.

use super::Spec;
use crate::{norm_name, Dep, Eco};

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

    pub(super) fn name_matches(&self, d: &Dep) -> bool {
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
