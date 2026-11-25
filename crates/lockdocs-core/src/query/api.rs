//! Exact API lookup and version-to-version API comparison.

mod matches;
mod render;
mod upgrade;

use super::entries::{npm_import_base, split_symbol};
use super::{Answer, Engine};
use crate::project::Spec;
use crate::{Dep, Eco};

impl Engine {
    pub fn api(&self, symbol: &str, package: Option<&str>, tokens: usize) -> Result<Answer, String> {
        let tokens = tokens.clamp(200, 20000);
        let symbol = symbol.trim().trim_end_matches("()").trim();
        if symbol.is_empty() {
            return Err("symbol is required, e.g. `z.object`, `tokio::spawn`, `BaseModel.model_dump`".into());
        }
        let mut requested_import = None;
        let (deps, rest) = match package.filter(|p| !p.trim().is_empty()) {
            Some(p) => {
                let spec = Spec::parse(p);
                let base = npm_import_base(&spec.name);
                if spec.eco.is_none_or(|e| e == Eco::Npm) && base != spec.name && self.project.find(&spec).is_empty() {
                    let selector = format!("npm:{base}{}", spec.version.as_ref().map(|v| format!("@{v}")).unwrap_or_default());
                    requested_import = Some(spec.name);
                    (self.select(Some(&selector), "")?.0, symbol.to_string())
                } else {
                    (self.select(Some(p), "")?.0, symbol.to_string())
                }
            }
            None => match self
                .npm_import_prefix(symbol)
                .map(|(dep, import, rest)| {
                    requested_import = Some(import);
                    (dep, rest)
                })
                .or_else(|| self.package_prefix(symbol))
            {
                Some((dep, rest)) => (vec![dep], rest),
                None => (self.select(None, symbol)?.0, symbol.to_string()),
            },
        };
        let segs: Vec<String> = split_symbol(&rest);
        let Some(last) = segs.last().map(|l| l.trim_end_matches('!').to_string()) else {
            // Only a package name: give the overview.
            return self.docs(
                Some(&deps.iter().map(|d| format!("{}:{}@{}", d.eco, d.name, d.version)).collect::<Vec<_>>().join(",")),
                "",
                tokens,
            );
        };
        let quals: Vec<String> = segs[..segs.len() - 1].iter().map(|s| s.to_ascii_lowercase()).collect();
        let (ready, missing) = self.resolve_all(&deps);
        if ready.is_empty() {
            return Err(missing.into_iter().map(|(_, e)| e).collect::<Vec<_>>().join("\n"));
        }
        let public_path = segs.join(".");
        let import = requested_import.as_deref();
        let matches = matches::find(&ready, &last, &quals, &public_path, import);
        if let Some(reason) = matches.npm_miss {
            return Ok(render::npm_miss(&ready, matches, &public_path, import, reason, tokens));
        }
        if matches.cands.is_empty() {
            // Nothing by that name: fall back to a docs search scoped to the same packages.
            let pk = ready
                .iter()
                .map(|r| format!("{}:{}@{}", r.0.eco, r.0.name, r.0.version))
                .collect::<Vec<_>>()
                .join(",");
            let mut a = self.docs(Some(&pk), &segs.join(" "), tokens)?;
            a.text = format!(
                "No symbol named `{last}` in {}; closest documentation:\n\n{}",
                ready.iter().map(|r| r.0.id()).collect::<Vec<_>>().join(", "),
                a.text
            );
            return Ok(a);
        }
        Ok(render::exact(&ready, matches, &public_path, import, tokens))
    }

    fn npm_import_prefix(&self, symbol: &str) -> Option<(Dep, String, String)> {
        for d in &self.project.deps {
            if d.eco != Eco::Npm {
                continue;
            }
            let Some(rest) = symbol.strip_prefix(&format!("{}/", d.name)) else {
                continue;
            };
            if let Some((subpath, sym)) = rest.split_once('.') {
                return Some((d.clone(), format!("{}/{subpath}", d.name), sym.to_string()));
            }
        }
        None
    }

    /// `zod.z.object` -> (zod, `z.object`); `tokio::spawn` -> (tokio, `spawn`);
    /// `@tanstack/react-query.useQuery`; `github.com/gin-gonic/gin.Context`.
    fn package_prefix(&self, symbol: &str) -> Option<(Dep, String)> {
        let mut best: Option<(usize, Dep)> = None;
        let bytes = symbol.as_bytes();
        let mut cuts: Vec<usize> = Vec::new();
        for i in 0..bytes.len() {
            if bytes[i] == b'.' || bytes[i] == b'#' || (bytes[i] == b':' && i + 1 < bytes.len() && bytes[i + 1] == b':') {
                cuts.push(i);
            }
        }
        cuts.push(symbol.len());
        for &c in &cuts {
            let cand = &symbol[..c];
            let spec = Spec::parse(cand);
            if spec.version.is_some() {
                continue;
            }
            let found = self.project.find(&spec);
            if let Some(d) = found.first() {
                if best.as_ref().is_none_or(|(bc, _)| c > *bc) {
                    best = Some((c, (*d).clone()));
                }
            }
        }
        let (c, dep) = best?;
        let rest = symbol[c..].trim_start_matches(['.', '#', ':']).to_string();
        Some((dep, rest))
    }
}
