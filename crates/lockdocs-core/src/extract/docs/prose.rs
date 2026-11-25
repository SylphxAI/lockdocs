//! Extract Markdown prose and inline referenced examples.

use super::super::plan::{ext_of, is_doc_name, walk_all, EXAMPLE_EXTS};
use super::super::{Entry, Kind};
use crate::markdown;
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

/// Docs sites include example files by reference (`{!.tmp_examples/x.md!}`,
/// `{* ../../docs_src/a/tutorial001.py *}`, `--8<-- "x.py"`). Inline them so
/// the section carries its code.
#[derive(Default)]
pub(in crate::extract) struct Includes {
    /// (lowercase path without extension, absolute path)
    pub(in crate::extract) files: Vec<(String, PathBuf)>,
}

impl Includes {
    pub(in crate::extract) fn scan(dir: &Path) -> Includes {
        let mut rels = Vec::new();
        walk_all(dir, Path::new(""), 0, &mut rels);
        let files = rels
            .into_iter()
            .filter(|r| EXAMPLE_EXTS.contains(&ext_of(r).as_str()))
            .map(|r| {
                let s = r.to_string_lossy().replace('\\', "/").to_ascii_lowercase();
                let stem = s.rsplit_once('.').map_or(s.clone(), |x| x.0.to_string());
                (stem, dir.join(&r))
            })
            .collect();
        Includes { files }
    }

    fn find(&self, reference: &str) -> Option<&PathBuf> {
        let r = reference.trim().trim_matches(['"', '\'']).to_ascii_lowercase();
        let r = r.split_whitespace().next().unwrap_or("");
        let r = r.rsplit_once('.').map_or(r, |x| x.0);
        let segs: Vec<&str> = r
            .split('/')
            .filter(|s| !s.is_empty() && *s != ".." && *s != "." && *s != ".tmp_examples")
            .collect();
        for take in (1..=segs.len().min(3)).rev() {
            let suffix = segs[segs.len() - take..].join("/");
            let hits: Vec<&(String, PathBuf)> = self.files.iter().filter(|(k, _)| *k == suffix || k.ends_with(&format!("/{suffix}"))).collect();
            if hits.len() == 1 {
                return Some(&hits[0].1);
            }
        }
        None
    }

    pub(in crate::extract) fn expand(&self, text: &str) -> String {
        if !(text.contains("{!") || text.contains("{*") || text.contains("--8<--")) {
            return text.to_string();
        }
        let mut out = String::with_capacity(text.len());
        for line in text.lines() {
            let t = line.trim();
            let reference = t
                .strip_prefix("{!")
                .and_then(|x| x.strip_suffix("!}"))
                .or_else(|| t.strip_prefix("{*").and_then(|x| x.strip_suffix("*}")))
                .or_else(|| t.strip_prefix("--8<--"));
            match reference
                .and_then(|r| self.find(r))
                .and_then(|p| std::fs::read_to_string(p).ok().map(|c| (p, c)))
            {
                Some((p, code)) => {
                    let lang = p.extension().map(|e| e.to_string_lossy().to_string()).unwrap_or_default();
                    let code: String = code.chars().take(3000).collect();
                    out.push_str(&format!("```{lang}\n{}\n```\n", code.trim_end()));
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
