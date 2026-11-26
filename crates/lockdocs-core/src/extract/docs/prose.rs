//! Extract Markdown prose and inline referenced examples.

use super::super::plan::{ext_of, is_doc_name, walk_all, EXAMPLE_EXTS};
use super::super::{Entry, Kind};
use crate::markdown;
use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};

pub(in crate::extract) fn prose(text: &str, rel: &str, line_off: u32, out: &mut Vec<Entry>) {
    prose_titled(text, rel, line_off, None, None, out)
}

pub(in crate::extract) fn prose_titled(text: &str, rel: &str, line_off: u32, given: Option<String>, package: Option<&str>, out: &mut Vec<Entry>) {
    let (text, fm_title) = markdown::clean_mdx(text, rel.ends_with(".mdx"));
    // A page without a title is named after its file: `model_config.md` -> `model config`.
    let file_name = rel.rsplit('/').next().unwrap_or(rel);
    let file_title = if rel.starts_with("upstream:") && !is_doc_name(file_name) {
        file_name
            .rsplit_once('.')
            .map_or(file_name, |x| x.0)
            .trim_start_matches(|c: char| c.is_ascii_digit() || c == '-')
            .replace(['_', '-'], " ")
    } else {
        file_name.to_string()
    };
    let file_title = file_title.as_str();
    let title = if let Some(t) = given {
        t
    } else if rel.contains("package metadata") {
        "README".to_string()
    } else if let Some(t) = fm_title {
        t
    } else {
        file_title.to_string()
    };
    for s in markdown::split_scoped(&text, &title, package) {
        out.push(prose_section(s, rel, line_off));
    }
}

/// A Prose entry for one Markdown section: the heading names it and is its
/// path, and `line_off` shifts the section line into the surrounding file.
pub(in crate::extract) fn prose_section(s: crate::markdown::Section, rel: &str, line_off: u32) -> Entry {
    Entry {
        kind: Kind::Prose,
        name: s.heading.clone(),
        path: s.heading,
        file: rel.to_string(),
        line: s.line + line_off,
        sig: String::new(),
        doc: s.text,
        alias_of: None,
        legacy: false,
        prose_imports: s.imports,
        guide_modes: s.guide_modes,
        prose_parents: s.parents.into_iter().map(|line| line + line_off).collect(),
    }
}

/// Inline example directives using only regular files in the fetched docs tree.
#[derive(Default)]
pub(in crate::extract) struct Includes(BTreeMap<String, PathBuf>);

impl Includes {
    pub(in crate::extract) fn scan(dir: &Path) -> Includes {
        let mut rels = Vec::new();
        walk_all(dir, Path::new(""), 0, &mut rels);
        Includes(
            rels.into_iter()
                .filter(|r| EXAMPLE_EXTS.contains(&ext_of(r).as_str()))
                .filter_map(|r| Some((r.to_str()?.replace(std::path::MAIN_SEPARATOR, "/"), dir.join(&r))))
                .collect(),
        )
    }

    fn find(&self, page: &str, reference: &str) -> Option<&Path> {
        let reference = reference.trim().trim_matches(['"', '\'']);
        if reference.contains(['\\', ':', '\0']) {
            return None;
        }
        let mut path = reference.split('/').peekable();
        let mut parents = 0;
        while matches!(path.peek(), Some(&".." | &".")) {
            parents += usize::from(path.next() == Some(".."));
        }
        if path.peek() == Some(&".tmp_examples") {
            path.next();
        }
        let segs: Vec<_> = path.collect();
        if segs.is_empty() || segs.iter().any(|s| matches!(*s, "" | "." | "..")) {
            return None;
        }
        let tail = segs.join("/");
        let page = page.strip_prefix("upstream:")?;
        let (dir, _) = page.rsplit_once('/').unwrap_or(("", page));
        let dirs: Vec<_> = dir.split('/').filter(|s| !s.is_empty()).collect();
        if parents > dirs.len() {
            return None;
        }
        // The include base can be the page's directory or an enclosing docs
        // directory. Try those bases without ever resolving above the cache.
        let exact = (0..=dirs.len() - parents).rev().find_map(|n| {
            let key = if n == 0 { tail.clone() } else { format!("{}/{tail}", dirs[..n].join("/")) };
            self.0.get(&key)
        });
        if exact.is_some() || parents > 0 {
            return exact.map(PathBuf::as_path);
        }
        // Bare filenames and docs-root-relative paths may name a unique file
        // elsewhere in the cache; never shorten an unmatched path to a basename.
        let suffix = format!("/{tail}");
        let mut hits = self.0.iter().filter(|(p, _)| p.ends_with(&suffix));
        let (_, hit) = hits.next()?;
        hits.next().is_none().then_some(hit.as_path())
    }

    pub(in crate::extract) fn expand(&self, text: &str, page: &str) -> String {
        if self.0.is_empty() || !(text.contains("{!") || text.contains("{*") || text.contains("--8<--")) {
            return text.to_string();
        }
        let mut out = String::with_capacity(text.len());
        let mut fence = None;
        for line in text.lines() {
            let t = line.trim();
            if let Some(c @ ('`' | '~')) = t.chars().next() {
                let len = t.chars().take_while(|&x| x == c).count();
                if len >= 3 {
                    if let Some((marker, width)) = fence {
                        if c == marker && len >= width && t[len..].trim().is_empty() {
                            fence = None;
                        }
                    } else {
                        fence = Some((c, len));
                    }
                }
            }
            let reference = t
                .strip_prefix("{!")
                .and_then(|x| x.strip_suffix("!}"))
                .or_else(|| t.strip_prefix("{*").and_then(|x| x.strip_suffix("*}")))
                .or_else(|| t.strip_prefix("--8<--"));
            match reference.and_then(|r| self.find(page, r)).and_then(|p| read_example(p).map(|code| (p, code))) {
                Some((p, code)) => {
                    let code: String = code.chars().take(3000).collect();
                    if fence.is_some() {
                        out.push_str(code.trim_end());
                        out.push('\n');
                    } else {
                        let lang = p.extension().and_then(|e| e.to_str()).unwrap_or("");
                        out.push_str(&format!("```{lang}\n{}\n```\n", code.trim_end()));
                    }
                }
                None => {
                    out.push_str(line);
                    out.push('\n');
                }
            }
        }
        out
    }
}

fn read_example(path: &Path) -> Option<String> {
    let limit = crate::upstream::MAX_FILE;
    let meta = std::fs::symlink_metadata(path).ok()?;
    if !meta.is_file() || meta.len() > limit {
        return None;
    }
    let mut code = String::new();
    std::fs::File::open(path).ok()?.take(limit + 1).read_to_string(&mut code).ok()?;
    (code.len() as u64 <= limit).then_some(code)
}

/// A docs example file as one code section titled by its file name.
pub(in crate::extract) fn example(text: &str, rel: &str, out: &mut Vec<Entry>) {
    if text.len() > 12_000 || text.trim().is_empty() {
        return;
    }
    let name = rel.rsplit('/').next().unwrap_or(rel);
    let lang = name.rsplit('.').next().unwrap_or("");
    let title = name.rsplit_once('.').map_or(name, |x| x.0).replace(['_', '-'], " ");
    out.push(Entry {
        kind: Kind::Prose,
        name: format!("example › {title}"),
        path: format!("example › {title}"),
        file: rel.to_string(),
        line: 1,
        sig: String::new(),
        doc: format!("```{lang}\n{}\n```", text.trim_end()),
        alias_of: None,
        legacy: false,
        prose_imports: Vec::new(),
        guide_modes: Vec::new(),
        prose_parents: Vec::new(),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn includes_refuse_unselected_unsafe_and_over_budget_files() {
        let cache = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("real.py"), "outside").unwrap();
        std::fs::create_dir(cache.path().join("docs_src")).unwrap();
        std::fs::write(cache.path().join("docs_src/real.py"), "the selected example").unwrap();
        std::fs::File::create(cache.path().join("docs_src/big.py"))
            .unwrap()
            .set_len(crate::upstream::MAX_FILE + 1)
            .unwrap();
        let includes = Includes::scan(cache.path());
        let page = "upstream:docs/en/docs/advanced/events.md";
        assert!(includes.expand("{!../../../docs_src/real.py!}", page).contains("the selected example"));
        for reference in [
            "docs_src/missing.py",
            "src/real.py", // same basename does not make it a match
            "../../../../../docs_src/real.py",
            "/docs_src/real.py",
            "https://example.invalid/docs_src/real.py",
            "docs_src/../docs_src/real.py",
            "docs_src\\real.py",
            "docs_src/big.py",
            outside.path().join("real.py").to_str().unwrap(),
        ] {
            let directive = format!("{{!{reference}!}}\n");
            assert_eq!(includes.expand(&directive, page), directive);
        }
    }

    #[cfg(unix)]
    #[test]
    fn includes_ignore_linked_files_and_directories() {
        use std::os::unix::fs::symlink;
        let cache = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let example = outside.path().join("demo.py");
        std::fs::write(&example, "not part of the release").unwrap();
        symlink(&example, cache.path().join("link.py")).unwrap();
        symlink(outside.path(), cache.path().join("linked_dir")).unwrap();
        let includes = Includes::scan(cache.path());
        let directives = "{!link.py!}\n{!linked_dir/demo.py!}\n";
        assert_eq!(includes.expand(directives, "upstream:docs/guide.md"), directives);
    }
}
