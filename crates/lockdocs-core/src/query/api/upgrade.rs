//! Version-to-version API differences for symbols used by the project.

use crate::project::Spec;
use crate::query::packages::Resolved;
use crate::query::render::truncate;
use crate::query::{Answer, Engine};
use crate::Dep;

impl Engine {
    /// Upgrade report (lockdocs Pro; the caller checks the licence): the API
    /// difference between the pinned version of `package` and `target`,
    /// limited to symbols this project uses. The target version is read from
    /// the same sources as any other version (installed, cached, or fetched
    /// when fetching is enabled).
    pub fn upgrade_report(&self, package: &str, target: &str, tokens: usize) -> Result<Answer, String> {
        let spec = Spec::parse(package);
        let found = self.project.find(&spec);
        let dep = found
            .iter()
            .find(|d| d.direct)
            .or_else(|| found.first())
            .map(|d| (*d).clone())
            .ok_or_else(|| format!("`{package}` is not pinned in this project's lockfiles; run `resolve` to see packages."))?;
        let target = target.trim().trim_start_matches('v');
        if target.is_empty() {
            return Err("`upgrade_to` needs a target version, e.g. 15.0.0".into());
        }
        if target == dep.version.trim_start_matches('v') {
            return Err(format!("{} is already the pinned version; pick a different target.", dep.id()));
        }
        let old = match self.index(&dep) {
            Resolved::Ready(i, _, _) => i,
            Resolved::Missing(_, e) => return Err(e),
        };
        let target_dep = Dep {
            eco: dep.eco,
            name: dep.name.clone(),
            version: target.to_string(),
            direct: false,
            from: "requested".into(),
        };
        let new = match self.index(&target_dep) {
            Resolved::Ready(i, _, _) => i,
            Resolved::Missing(_, e) => {
                return Err(format!(
                    "{e}\nThe target version's files are needed to compare; add --fetch (or LOCKDOCS_FETCH=1) to download exactly {}.",
                    target_dep.id()
                ))
            }
        };
        let r = crate::upgrade::report(&self.project.root, &old, &new);
        let max = tokens.clamp(200, 20000) * 3;
        let text = if r.text.len() > max {
            format!("{}\n... truncated to the token budget; raise `tokens`.\n", truncate(&r.text, max))
        } else {
            r.text
        };
        Ok(Answer { text, json: r.json })
    }
}
