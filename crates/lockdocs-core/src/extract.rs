//! Turn a package's files into entries: API symbols (signature + doc comment)
//! from TypeScript declarations, JavaScript, Python, Rust and Go sources, and
//! prose sections from READMEs, changelogs and docs folders.

use crate::locate::Source;
use crate::markdown;
use crate::Eco;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use tree_sitter::{Node, Parser};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Kind {
    Function,
    Method,
    Class,
    Interface,
    Type,
    Const,
    Enum,
    Struct,
    Trait,
    Macro,
    Module,
    Field,
    Alias,
    Prose,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Function => "function",
            Kind::Method => "method",
            Kind::Class => "class",
            Kind::Interface => "interface",
            Kind::Type => "type",
            Kind::Const => "const",
            Kind::Enum => "enum",
            Kind::Struct => "struct",
            Kind::Trait => "trait",
            Kind::Macro => "macro",
            Kind::Module => "module",
            Kind::Field => "field",
            Kind::Alias => "re-export",
            Kind::Prose => "doc",
        }
    }
    pub fn is_container(self) -> bool {
        matches!(self, Kind::Class | Kind::Interface | Kind::Struct | Kind::Trait | Kind::Enum | Kind::Module)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub kind: Kind,
    /// Short name (`object`, `spawn`), or the heading path for prose.
    pub name: String,
    /// Qualified path: `z.object`, `tokio::task::spawn`, `pydantic.main.BaseModel.model_dump`.
    pub path: String,
    /// File relative to the package root.
    pub file: String,
    pub line: u32,
    /// Declaration text without the body.
    pub sig: String,
    /// Doc comment, or the prose section text.
    pub doc: String,
    /// For re-exports: the local name this entry points at.
    pub alias_of: Option<String>,
    /// Lives in a legacy compatibility copy (`zod/v3` inside zod 4,
    /// `pydantic/v1` inside pydantic 2): ranked below the current API.
    #[serde(default)]
    pub legacy: bool,
}

/// A path segment `vN` with N below the package's major version.
fn legacy_file(file: &str, major: u64) -> bool {
    major > 1
        && file.split('/').any(|seg| {
            seg.len() >= 2 && seg.starts_with('v') && seg[1..].chars().all(|c| c.is_ascii_digit()) && seg[1..].parse::<u64>().is_ok_and(|n| n < major)
        })
}

// ---------------------------------------------------------------- file selection

const SKIP_DIRS: &[&str] = &[
    "node_modules",
    "compiled",
    ".git",
    "test",
    "tests",
    "__tests__",
    "__test__",
    "testdata",
    "test_data",
    "benches",
    "bench",
    "benchmarks",
    "fixtures",
    "__fixtures__",
    "__pycache__",
    "examples",
    "example",
    "_examples",
    "coverage",
    ".github",
    "vendor",
    "internal",
    "__snapshots__",
];

const MAX_FILES: usize = 25_000;
const MAX_FILE_BYTES: u64 = 3 * 1024 * 1024;

fn is_doc_name(name: &str) -> bool {
    let u = name.to_ascii_uppercase();
    [
        "README",
        "CHANGELOG",
        "CHANGES",
        "HISTORY",
        "MIGRATION",
        "MIGRATING",
        "UPGRADING",
        "UPGRADE",
        "RELEASES",
        "NEWS",
        "GUIDE",
        "API",
    ]
    .iter()
    .any(|p| u.starts_with(p))
}

fn doc_ext(ext: &str) -> bool {
    matches!(ext, "md" | "mdx" | "markdown" | "rst")
}

fn walk(dir: &Path, rel: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth > 14 || out.len() >= MAX_FILES {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = rd.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let name = e.file_name().to_string_lossy().to_string();
        let Ok(ft) = e.file_type() else { continue };
        let r = rel.join(&name);
        if ft.is_dir() {
            if SKIP_DIRS.contains(&name.as_str()) || name.starts_with('.') {
                continue;
            }
            walk(&e.path(), &r, depth + 1, out);
        } else if ft.is_file() || ft.is_symlink() {
            out.push(r);
        }
        if out.len() >= MAX_FILES {
            return;
        }
    }
}

fn ext_of(p: &Path) -> String {
    let s = p.file_name().map(|f| f.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    for e in [".d.ts", ".d.mts", ".d.cts"] {
        if s.ends_with(e) {
            return e[1..].to_string();
        }
    }
    p.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default()
}

fn in_docs_dir(p: &Path) -> bool {
    p.components().any(|c| {
        matches!(
            c.as_os_str().to_string_lossy().to_ascii_lowercase().as_str(),
            "docs" | "doc" | "documentation" | "guide" | "guides"
        )
    })
}

/// A unit of work: one file to read, how to parse it, and how to label it.
struct Job {
    abs: PathBuf,
    rel: String,
    how: How,
}

#[derive(Clone, Copy, PartialEq)]
enum How {
    Prose,
    Metadata,
    Ts,
    Python,
    Rust,
    Go,
}

/// Plan which files to read for a package.
fn plan(eco: Eco, src: &Source, extra_types: Option<&Path>) -> Vec<Job> {
    let mut rels = Vec::new();
    match &src.files {
        Some(list) => rels.extend(list.iter().cloned()),
        None => walk(&src.dir, Path::new(""), 0, &mut rels),
    }
    let mut jobs = Vec::new();
    let names: HashSet<String> = rels.iter().map(|r| r.to_string_lossy().replace('\\', "/")).collect();
    let mut has_dts = false;
    for r in &rels {
        let e = ext_of(r);
        if e.starts_with("d.") {
            has_dts = true;
        }
    }
    let rs_crate_src = eco == Eco::Cargo;
    for r in rels {
        let rel = r.to_string_lossy().replace('\\', "/");
        if rel.split('/').any(|seg| SKIP_DIRS.contains(&seg) && src.files.is_some()) {
            continue;
        }
        let abs = src.dir.join(&r);
        let fname = r.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default();
        let e = ext_of(&r);
        let depth = rel.matches('/').count();
        let how = if doc_ext(&e) || (e.is_empty() || e == "txt") && is_doc_name(&fname) && depth == 0 {
            if is_doc_name(&fname) || in_docs_dir(&r) || depth == 0 {
                Some(How::Prose)
            } else {
                None
            }
        } else {
            match eco {
                Eco::Npm => match e.as_str() {
                    "d.ts" => Some(How::Ts),
                    "d.mts" | "d.cts" => {
                        let base = rel.trim_end_matches(".d.mts").trim_end_matches(".d.cts");
                        if names.contains(&format!("{base}.d.ts")) {
                            None
                        } else {
                            Some(How::Ts)
                        }
                    }
                    "ts" | "mts" | "cts" | "tsx" if !has_dts => Some(How::Ts),
                    "js" | "mjs" | "cjs" if !has_dts && !fname.contains(".min.") => Some(How::Ts),
                    _ => None,
                },
                Eco::PyPI => match e.as_str() {
                    "py" => Some(How::Python),
                    "pyi" => {
                        let base = rel.trim_end_matches(".pyi");
                        if names.contains(&format!("{base}.py")) {
                            None
                        } else {
                            Some(How::Python)
                        }
                    }
                    _ => None,
                },
                Eco::Cargo => (e == "rs" && rs_crate_src && rel.starts_with("src/")).then_some(How::Rust),
                Eco::Go => (e == "go" && !fname.ends_with("_test.go")).then_some(How::Go),
            }
        };
        if let Some(how) = how {
            jobs.push(Job { abs, rel, how });
        }
    }
    if let Some(meta) = &src.metadata {
        jobs.push(Job {
            abs: meta.clone(),
            rel: "README (package metadata)".into(),
            how: How::Metadata,
        });
    }
    // Types that ship separately (`@types/react` for `react`).
    if let (Some(types), false) = (extra_types, has_dts) {
        let mut rels = Vec::new();
        walk(types, Path::new(""), 0, &mut rels);
        let label = types
            .iter()
            .rev()
            .take(2)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .map(|s| s.to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        for r in rels {
            if ext_of(&r) == "d.ts" {
                let rel = format!("{label}/{}", r.to_string_lossy().replace('\\', "/"));
                jobs.push(Job {
                    abs: types.join(&r),
                    rel,
                    how: How::Ts,
                });
            }
        }
    }
    jobs
}

/// Extract every entry for a package.
pub fn extract(eco: Eco, pkg_name: &str, src: &Source, extra_types: Option<&Path>) -> Vec<Entry> {
    let major: u64 = src.version.trim_start_matches('v').split('.').next().and_then(|m| m.parse().ok()).unwrap_or(0);
    let jobs = plan(eco, src, extra_types);
    let crate_name = if eco == Eco::Cargo {
        rust_crate_name(&src.dir).unwrap_or_else(|| pkg_name.replace('-', "_"))
    } else {
        String::new()
    };
    let mut all: Vec<Entry> = jobs
        .par_iter()
        .flat_map_iter(|job| {
            let Ok(meta) = std::fs::metadata(&job.abs) else { return Vec::new() };
            if meta.len() > MAX_FILE_BYTES && job.how != How::Prose {
                return Vec::new();
            }
            let Ok(bytes) = std::fs::read(&job.abs) else { return Vec::new() };
            let text = String::from_utf8_lossy(&bytes);
            let mut out = Vec::new();
            match job.how {
                How::Prose => prose(&text, &job.rel, 0, &mut out),
                How::Metadata => {
                    let (body, line) = markdown::metadata_body(&text);
                    prose(&body, &job.rel, line - 1, &mut out)
                }
                How::Ts => ts_file(&text, &job.rel, &mut out),
                How::Python => py_file(&text, &job.rel, &mut out),
                How::Rust => rs_file(&text, &job.rel, &crate_name, &mut out),
                How::Go => go_file(&text, &job.rel, pkg_name, &mut out),
            }
            out
        })
        .collect();
    for e in &mut all {
        e.legacy = legacy_file(&e.file, major);
    }
    // Drop exact duplicates (the same declaration shipped twice).
    let mut seen = HashSet::new();
    all.retain(|e| seen.insert((e.path.clone(), e.sig.clone(), e.doc.len())));
    all
}

fn rust_crate_name(dir: &Path) -> Option<String> {
    let t = std::fs::read_to_string(dir.join("Cargo.toml")).ok()?;
    let v: toml::Table = toml::from_str(&t).ok()?;
    let lib = v.get("lib").and_then(|l| l.get("name")).and_then(|n| n.as_str());
    let pkg = v.get("package").and_then(|p| p.get("name")).and_then(|n| n.as_str());
    lib.or(pkg).map(|n| n.replace('-', "_"))
}

fn prose(text: &str, rel: &str, line_off: u32, out: &mut Vec<Entry>) {
    let title = rel.rsplit('/').next().unwrap_or(rel);
    let title = if rel.contains("package metadata") { "README" } else { title };
    for s in markdown::split(text, title) {
        out.push(Entry {
            kind: Kind::Prose,
            name: s.heading.clone(),
            path: s.heading,
            file: rel.to_string(),
            line: s.line + line_off,
            sig: String::new(),
            doc: s.text,
            alias_of: None,
            legacy: false,
        });
    }
}

// ---------------------------------------------------------------- helpers

thread_local! {
    static PARSERS: RefCell<Vec<(&'static str, Parser)>> = const { RefCell::new(Vec::new()) };
}

/// Parse with a pooled parser. The parser is taken out of the pool while in
/// use, so `f` may parse again (Rust macro bodies do).
fn parse_with<R>(lang: &'static str, text: &str, f: impl FnOnce(Node, &[u8]) -> R) -> Option<R> {
    let pooled = PARSERS.with(|ps| {
        let mut ps = ps.borrow_mut();
        ps.iter().position(|(l, _)| *l == lang).map(|i| ps.swap_remove(i).1)
    });
    let mut parser = match pooled {
        Some(p) => p,
        None => {
            let mut p = Parser::new();
            let language: tree_sitter::Language = match lang {
                "ts" => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
                "tsx" => tree_sitter_typescript::LANGUAGE_TSX.into(),
                "py" => tree_sitter_python::LANGUAGE.into(),
                "rs" => tree_sitter_rust::LANGUAGE.into(),
                _ => tree_sitter_go::LANGUAGE.into(),
            };
            p.set_language(&language).ok()?;
            p
        }
    };
    let tree = parser.parse(text, None);
    PARSERS.with(|ps| ps.borrow_mut().push((lang, parser)));
    let tree = tree?;
    Some(f(tree.root_node(), text.as_bytes()))
}

fn txt<'a>(n: Node, src: &'a [u8]) -> &'a str {
    n.utf8_text(src).unwrap_or("")
}

fn compact(s: &str, cap: usize) -> String {
    let mut out = String::with_capacity(s.len().min(cap + 8));
    let mut space = false;
    for c in s.chars() {
        if c.is_whitespace() {
            space = true;
            continue;
        }
        if space && !out.is_empty() {
            out.push(' ');
        }
        space = false;
        out.push(c);
        if out.len() >= cap {
            out.push_str(" …");
            break;
        }
    }
    out.replace("( ", "(").replace(", )", ")").replace(",)", ")").replace(" )", ")")
}

/// Drop `# comments` from Python source lines (outside string literals).
fn strip_py_comments(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for line in s.lines() {
        let mut quote: Option<char> = None;
        let mut cut = line.len();
        for (i, c) in line.char_indices() {
            match (quote, c) {
                (None, '#') => {
                    cut = i;
                    break;
                }
                (None, '"' | '\'') => quote = Some(c),
                (Some(q), c) if c == q => quote = None,
                _ => {}
            }
        }
        out.push_str(&line[..cut]);
        out.push('\n');
    }
    out
}

/// Source text from `n` up to (not including) its body child.
fn head(n: Node, src: &[u8], body_field: &str, cap: usize) -> String {
    let end = n.child_by_field_name(body_field).map_or(n.end_byte(), |b| b.start_byte());
    let s = std::str::from_utf8(&src[n.start_byte()..end]).unwrap_or("");
    compact(s.trim_end().trim_end_matches(['{', ':', '=']).trim_end(), cap)
}

fn cap_doc(s: String) -> String {
    const CAP: usize = 4000;
    if s.len() <= CAP {
        return s;
    }
    let mut i = CAP;
    while !s.is_char_boundary(i) {
        i -= 1;
    }
    format!("{} …", &s[..i])
}

fn dedent(lines: &[&str]) -> String {
    let indent = lines
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start().len())
        .min()
        .unwrap_or(0);
    let v: Vec<&str> = lines.iter().map(|l| if l.len() >= indent { &l[indent..] } else { l.trim_start() }).collect();
    v.join("\n").trim().to_string()
}

fn clean_block_comment(s: &str) -> String {
    let s = s
        .trim_start_matches("/**")
        .trim_start_matches("/*!")
        .trim_start_matches("/*")
        .trim_end_matches("*/");
    let lines: Vec<&str> = s
        .lines()
        .map(|l| {
            let t = l.trim_start();
            match t.strip_prefix('*') {
                Some(r) => r.strip_prefix(' ').unwrap_or(r),
                None => l,
            }
        })
        .collect();
    cap_doc(dedent(&lines))
}

fn entry(kind: Kind, name: &str, path: String, file: &str, line: u32, sig: String, doc: String) -> Entry {
    Entry {
        kind,
        name: name.to_string(),
        path,
        file: file.to_string(),
        line,
        sig,
        doc,
        alias_of: None,
        legacy: false,
    }
}

// ---------------------------------------------------------------- TypeScript / JavaScript

fn ts_file(text: &str, rel: &str, out: &mut Vec<Entry>) {
    let lang = if rel.ends_with(".tsx") || rel.ends_with(".jsx") { "tsx" } else { "ts" };
    let is_js = rel.ends_with(".js") || rel.ends_with(".mjs") || rel.ends_with(".cjs");
    parse_with(lang, text, |root, src| {
        let mut w = TsWalker {
            src,
            file: rel,
            out,
            js: is_js,
        };
        w.block(root, "");
    });
}

struct TsWalker<'a> {
    src: &'a [u8],
    file: &'a str,
    out: &'a mut Vec<Entry>,
    js: bool,
}

impl TsWalker<'_> {
    fn doc(&self, n: Node) -> String {
        let mut p = n.prev_sibling();
        while let Some(c) = p {
            if c.kind() == "comment" {
                let t = txt(c, self.src);
                if t.starts_with("/**") && c.end_position().row + 2 >= n.start_position().row {
                    return clean_block_comment(t);
                }
                if t.starts_with("//") {
                    p = c.prev_sibling();
                    continue;
                }
            }
            break;
        }
        String::new()
    }

    fn line(&self, n: Node) -> u32 {
        n.start_position().row as u32 + 1
    }

    fn block(&mut self, node: Node, prefix: &str) {
        let mut cur = node.walk();
        let children: Vec<Node> = node.named_children(&mut cur).collect();
        for c in children {
            match c.kind() {
                "export_statement" => {
                    let doc = self.doc(c);
                    if let Some(d) = c.child_by_field_name("declaration") {
                        self.decl(d, prefix, doc, c);
                    } else {
                        self.exports(c, prefix);
                    }
                }
                "ambient_declaration" => {
                    let doc = self.doc(c);
                    let mut cc = c.walk();
                    let inner: Vec<Node> = c.named_children(&mut cc).collect();
                    for d in inner {
                        self.decl(d, prefix, doc.clone(), c);
                    }
                }
                "expression_statement" => {
                    let mut cc = c.walk();
                    let inner: Vec<Node> = c.named_children(&mut cc).collect();
                    for d in inner {
                        if matches!(d.kind(), "internal_module" | "module") {
                            let doc = self.doc(c);
                            self.decl(d, prefix, doc, c);
                        }
                    }
                }
                _ => {
                    let doc = self.doc(c);
                    // In JS only documented top-level declarations are API.
                    if !self.js || !doc.is_empty() || !prefix.is_empty() {
                        self.decl(c, prefix, doc, c);
                    }
                }
            }
        }
    }

    fn exports(&mut self, n: Node, prefix: &str) {
        let mut cur = n.walk();
        let children: Vec<Node> = n.named_children(&mut cur).collect();
        for c in children {
            match c.kind() {
                "export_clause" => {
                    let mut cc = c.walk();
                    let specs: Vec<Node> = c.named_children(&mut cc).collect();
                    for s in specs {
                        let (Some(name), alias) = (s.child_by_field_name("name"), s.child_by_field_name("alias")) else {
                            continue;
                        };
                        let Some(alias) = alias else { continue };
                        let (from, to) = (txt(name, self.src), txt(alias, self.src));
                        if from == to || to == "default" {
                            continue;
                        }
                        let mut e = entry(
                            Kind::Alias,
                            to,
                            format!("{prefix}{to}"),
                            self.file,
                            self.line(s),
                            compact(txt(n, self.src), 200),
                            String::new(),
                        );
                        e.alias_of = Some(from.to_string());
                        self.out.push(e);
                    }
                }
                "namespace_export" => {
                    if let Some(id) = c.named_child(0) {
                        let name = txt(id, self.src);
                        self.out.push(entry(
                            Kind::Module,
                            name,
                            format!("{prefix}{name}"),
                            self.file,
                            self.line(n),
                            compact(txt(n, self.src), 200),
                            self.doc(n),
                        ));
                    }
                }
                _ => {}
            }
        }
    }

    fn decl(&mut self, d: Node, prefix: &str, doc: String, outer: Node) {
        let src = self.src;
        let name_of = |n: Node| n.child_by_field_name("name").map(|x| txt(x, src).to_string());
        let line = self.line(outer);
        match d.kind() {
            "ambient_declaration" => {
                let mut cur = d.walk();
                let inner: Vec<Node> = d.named_children(&mut cur).collect();
                for x in inner {
                    self.decl(x, prefix, doc.clone(), outer);
                }
            }
            "function_signature" | "function_declaration" | "generator_function_declaration" => {
                let Some(name) = name_of(d) else { return };
                let sig = head(outer, src, "__none__", 600).trim_end_matches(';').to_string();
                let sig = if d.child_by_field_name("body").is_some() {
                    head(d, src, "body", 600)
                } else {
                    sig
                };
                self.out
                    .push(entry(Kind::Function, &name, format!("{prefix}{name}"), self.file, line, sig, doc));
            }
            "class_declaration" | "abstract_class_declaration" | "class" => {
                let Some(name) = name_of(d) else { return };
                self.out.push(entry(
                    Kind::Class,
                    &name,
                    format!("{prefix}{name}"),
                    self.file,
                    line,
                    head(d, src, "body", 400),
                    doc,
                ));
                if let Some(body) = d.child_by_field_name("body") {
                    self.members(body, &format!("{prefix}{name}."));
                }
            }
            "interface_declaration" => {
                let Some(name) = name_of(d) else { return };
                self.out.push(entry(
                    Kind::Interface,
                    &name,
                    format!("{prefix}{name}"),
                    self.file,
                    line,
                    head(d, src, "body", 400),
                    doc,
                ));
                if let Some(body) = d.child_by_field_name("body") {
                    self.members(body, &format!("{prefix}{name}."));
                }
            }
            "type_alias_declaration" => {
                let Some(name) = name_of(d) else { return };
                self.out.push(entry(
                    Kind::Type,
                    &name,
                    format!("{prefix}{name}"),
                    self.file,
                    line,
                    compact(txt(d, src), 700),
                    doc,
                ));
            }
            "enum_declaration" => {
                let Some(name) = name_of(d) else { return };
                self.out.push(entry(
                    Kind::Enum,
                    &name,
                    format!("{prefix}{name}"),
                    self.file,
                    line,
                    compact(txt(d, src), 500),
                    doc,
                ));
            }
            "lexical_declaration" | "variable_declaration" => {
                let kw = txt(d, src).split_whitespace().next().unwrap_or("const").to_string();
                let mut cur = d.walk();
                let decls: Vec<Node> = d.named_children(&mut cur).filter(|n| n.kind() == "variable_declarator").collect();
                for v in decls {
                    let Some(nn) = v.child_by_field_name("name") else { continue };
                    if nn.kind() != "identifier" {
                        continue;
                    }
                    let name = txt(nn, src).to_string();
                    let value = v.child_by_field_name("value");
                    let is_fn = value.is_some_and(|x| matches!(x.kind(), "arrow_function" | "function_expression" | "function"))
                        || v.child_by_field_name("type").is_some_and(|t| txt(t, src).contains("=>"));
                    let sig = match value {
                        Some(val) if matches!(val.kind(), "arrow_function" | "function_expression" | "function") => {
                            let body_start = val.child_by_field_name("body").map_or(val.end_byte(), |b| b.start_byte());
                            let s = std::str::from_utf8(&src[v.start_byte()..body_start]).unwrap_or("");
                            compact(&format!("{kw} {}", s.trim_end().trim_end_matches("=>").trim_end()), 600)
                        }
                        Some(val) if val.end_byte() - val.start_byte() > 300 => {
                            let s = std::str::from_utf8(&src[v.start_byte()..val.start_byte()]).unwrap_or("");
                            compact(&format!("{kw} {s} …"), 600)
                        }
                        _ => compact(&format!("{kw} {}", txt(v, src)), 600),
                    };
                    let kind = if is_fn { Kind::Function } else { Kind::Const };
                    self.out.push(entry(kind, &name, format!("{prefix}{name}"), self.file, line, sig, doc.clone()));
                }
            }
            "internal_module" | "module" => {
                let Some(nn) = d.child_by_field_name("name") else { return };
                let raw = txt(nn, src);
                let quoted = raw.starts_with('"') || raw.starts_with('\'');
                let name = raw.trim_matches(['"', '\'']).to_string();
                let new_prefix = if quoted { prefix.to_string() } else { format!("{prefix}{name}.") };
                if !quoted {
                    self.out.push(entry(
                        Kind::Module,
                        &name,
                        format!("{prefix}{name}"),
                        self.file,
                        line,
                        head(d, src, "body", 200),
                        doc,
                    ));
                }
                if let Some(body) = d.child_by_field_name("body") {
                    self.block(body, &new_prefix);
                }
            }
            _ => {}
        }
    }

    fn members(&mut self, body: Node, prefix: &str) {
        let src = self.src;
        let mut cur = body.walk();
        let children: Vec<Node> = body.named_children(&mut cur).collect();
        for m in children {
            let kind = match m.kind() {
                "method_definition" | "method_signature" | "abstract_method_signature" => Kind::Method,
                "public_field_definition" | "property_signature" => Kind::Field,
                _ => continue,
            };
            let t = txt(m, src);
            if t.starts_with("private ") || t.starts_with("protected ") || t.contains(" private ") && kind == Kind::Field {
                continue;
            }
            let Some(nn) = m.child_by_field_name("name") else { continue };
            let name = txt(nn, src).trim_matches(['"', '\'']).to_string();
            if name.starts_with('#') || name.starts_with('_') || name.starts_with('[') {
                continue;
            }
            let doc = self.doc(m);
            let sig = if m.child_by_field_name("body").is_some() {
                head(m, src, "body", 500)
            } else {
                compact(t.trim_end_matches(';'), 500)
            };
            self.out.push(entry(
                kind,
                &name,
                format!("{prefix}{name}"),
                self.file,
                m.start_position().row as u32 + 1,
                sig,
                doc,
            ));
        }
    }
}

// ---------------------------------------------------------------- Python

fn py_module(rel: &str) -> String {
    let r = rel.trim_end_matches(".pyi").trim_end_matches(".py");
    let r = r.trim_end_matches("/__init__");
    r.replace('/', ".")
}

fn py_file(text: &str, rel: &str, out: &mut Vec<Entry>) {
    let module = py_module(rel);
    parse_with("py", text, |root, src| {
        if let Some(doc) = py_docstring(root, src) {
            for s in markdown::split(&doc, &module) {
                out.push(Entry {
                    kind: Kind::Prose,
                    name: s.heading.clone(),
                    path: s.heading,
                    file: rel.to_string(),
                    line: s.line,
                    sig: String::new(),
                    doc: s.text,
                    alias_of: None,
                    legacy: false,
                });
            }
        }
        py_block(root, src, &module, false, rel, out);
    });
}

fn py_docstring(block: Node, src: &[u8]) -> Option<String> {
    let first = block.named_child(0)?;
    let first = if first.kind() == "comment" { first.next_named_sibling()? } else { first };
    if first.kind() != "expression_statement" {
        return None;
    }
    let s = first.named_child(0)?;
    if s.kind() != "string" {
        return None;
    }
    let raw = txt(s, src);
    let raw = raw.trim_start_matches(['r', 'R', 'u', 'U', 'b', 'B']);
    let inner = raw
        .strip_prefix("\"\"\"")
        .and_then(|x| x.strip_suffix("\"\"\""))
        .or_else(|| raw.strip_prefix("'''").and_then(|x| x.strip_suffix("'''")))
        .or_else(|| raw.strip_prefix('"').and_then(|x| x.strip_suffix('"')))
        .or_else(|| raw.strip_prefix('\'').and_then(|x| x.strip_suffix('\'')))?;
    let lines: Vec<&str> = inner.lines().collect();
    if lines.is_empty() {
        return None;
    }
    // inspect.cleandoc: first line as is, the rest dedented.
    let first_line = lines[0].trim();
    let rest = dedent(&lines[1..]);
    let gap = if lines.get(1).is_some_and(|l| l.trim().is_empty()) { "\n\n" } else { "\n" };
    let doc = if rest.is_empty() {
        first_line.to_string()
    } else if first_line.is_empty() {
        rest
    } else {
        format!("{first_line}{gap}{rest}")
    };
    Some(cap_doc(doc))
}

fn py_block(block: Node, src: &[u8], prefix: &str, in_class: bool, rel: &str, out: &mut Vec<Entry>) {
    let mut cur = block.walk();
    let children: Vec<Node> = block.named_children(&mut cur).collect();
    for c in children {
        let (def, decorators) = if c.kind() == "decorated_definition" {
            let mut dc = c.walk();
            let decs: Vec<String> = c
                .named_children(&mut dc)
                .filter(|n| n.kind() == "decorator")
                .map(|n| compact(txt(n, src), 80))
                .collect();
            match c.child_by_field_name("definition") {
                Some(d) => (d, decs),
                None => continue,
            }
        } else {
            (c, Vec::new())
        };
        let Some(nn) = def.child_by_field_name("name") else { continue };
        let name = txt(nn, src);
        let dunder_ok = matches!(name, "__init__" | "__call__");
        if name.starts_with('_') && !dunder_ok {
            continue;
        }
        let line = c.start_position().row as u32 + 1;
        let mut sig = decorators.join(" ");
        match def.kind() {
            "function_definition" => {
                if !sig.is_empty() {
                    sig.push(' ');
                }
                let end = def.child_by_field_name("body").map_or(def.end_byte(), |b| b.start_byte());
                let raw = strip_py_comments(std::str::from_utf8(&src[def.start_byte()..end]).unwrap_or(""));
                sig.push_str(&compact(raw.trim_end().trim_end_matches(':').trim_end(), 700));
                let doc = def.child_by_field_name("body").and_then(|b| py_docstring(b, src)).unwrap_or_default();
                let kind = if in_class { Kind::Method } else { Kind::Function };
                out.push(entry(kind, name, format!("{prefix}.{name}"), rel, line, sig, doc));
            }
            "class_definition" => {
                if !sig.is_empty() {
                    sig.push(' ');
                }
                sig.push_str(&head(def, src, "body", 400));
                let body = def.child_by_field_name("body");
                let doc = body.and_then(|b| py_docstring(b, src)).unwrap_or_default();
                let path = format!("{prefix}.{name}");
                out.push(entry(Kind::Class, name, path.clone(), rel, line, sig, doc));
                if let Some(b) = body {
                    py_block(b, src, &path, true, rel, out);
                }
            }
            _ => {}
        }
    }
}

// ---------------------------------------------------------------- Rust

fn rs_module(rel: &str, crate_name: &str) -> String {
    let r = rel.strip_prefix("src/").unwrap_or(rel).trim_end_matches(".rs");
    let mut segs: Vec<&str> = r.split('/').collect();
    if matches!(segs.last(), Some(&"mod") | Some(&"lib") | Some(&"main")) {
        segs.pop();
    }
    let mut p = crate_name.to_string();
    for s in segs {
        if !s.is_empty() {
            p.push_str("::");
            p.push_str(s);
        }
    }
    p
}

fn rs_file(text: &str, rel: &str, crate_name: &str, out: &mut Vec<Entry>) {
    let module = rs_module(rel, crate_name);
    rs_parse(text, rel, &module, 0, 0, out);
}

fn rs_parse(text: &str, rel: &str, module: &str, line_off: u32, depth: u32, out: &mut Vec<Entry>) {
    parse_with("rs", text, |root, src| {
        let mut inner_doc = Vec::new();
        let mut w = RsWalker {
            src,
            file: rel,
            line_off,
            depth,
            out: Vec::new(),
        };
        w.block(root, module, None, &mut inner_doc);
        if !inner_doc.is_empty() {
            let doc = inner_doc.join("\n");
            for s in markdown::split(&doc, module) {
                out.push(Entry {
                    kind: Kind::Prose,
                    name: s.heading.clone(),
                    path: s.heading,
                    file: rel.to_string(),
                    line: s.line + line_off,
                    sig: String::new(),
                    doc: s.text,
                    alias_of: None,
                    legacy: false,
                });
            }
        }
        out.append(&mut w.out);
    });
}

struct RsWalker<'a> {
    src: &'a [u8],
    file: &'a str,
    line_off: u32,
    depth: u32,
    out: Vec<Entry>,
}

fn is_pub(n: Node, src: &[u8]) -> bool {
    let mut cur = n.walk();
    let r = n
        .named_children(&mut cur)
        .any(|c| c.kind() == "visibility_modifier" && txt(c, src).trim() == "pub");
    r
}

fn strip_line_doc(t: &str) -> Option<(&str, bool)> {
    if let Some(r) = t.strip_prefix("///") {
        if r.starts_with('/') {
            return None;
        }
        return Some((r.strip_prefix(' ').unwrap_or(r).trim_end(), false));
    }
    if let Some(r) = t.strip_prefix("//!") {
        return Some((r.strip_prefix(' ').unwrap_or(r).trim_end(), true));
    }
    None
}

impl RsWalker<'_> {
    fn line(&self, n: Node) -> u32 {
        n.start_position().row as u32 + 1 + self.line_off
    }

    /// `owner`: impl/trait type whose members we are in (`Some((name, is_trait))`).
    fn block(&mut self, node: Node, module: &str, owner: Option<(&str, bool)>, inner_doc: &mut Vec<String>) {
        let src = self.src;
        let mut cur = node.walk();
        let children: Vec<Node> = node.named_children(&mut cur).collect();
        let mut pending: Vec<String> = Vec::new();
        let mut hidden = false;
        let mut exported_macro = false;
        let mut first_line: Option<u32> = None;
        for c in children {
            let k = c.kind();
            match k {
                "line_comment" => {
                    let t = txt(c, src);
                    if let Some((d, inner)) = strip_line_doc(t) {
                        if inner {
                            inner_doc.push(d.to_string());
                        } else {
                            if pending.is_empty() {
                                first_line = Some(self.line(c));
                            }
                            pending.push(d.to_string());
                        }
                    }
                    continue;
                }
                "block_comment" => {
                    let t = txt(c, src);
                    if t.starts_with("/*!") {
                        inner_doc.push(clean_block_comment(t));
                    } else if t.starts_with("/**") && !t.starts_with("/***") {
                        pending.push(clean_block_comment(t));
                    }
                    continue;
                }
                "attribute_item" => {
                    let t = txt(c, src);
                    if t.contains("doc(hidden)") {
                        hidden = true;
                    }
                    if t.contains("macro_export") {
                        exported_macro = true;
                    }
                    // `#[doc = "..."]`
                    if let Some(rest) = t.strip_prefix("#[doc = \"") {
                        pending.push(rest.trim_end_matches("\"]").replace("\\n", "\n"));
                    }
                    continue;
                }
                "inner_attribute_item" => continue,
                _ => {}
            }
            let doc = cap_doc(pending.join("\n").trim().to_string());
            let _ = first_line.take();
            pending.clear();
            let was_hidden = std::mem::take(&mut hidden);
            let was_macro_export = std::mem::take(&mut exported_macro);
            if was_hidden {
                continue;
            }
            let line = self.line(c);
            let sep = "::";
            let in_trait = owner.is_some_and(|o| o.1);
            let public = in_trait || is_pub(c, src);
            let name = c.child_by_field_name("name").map(|n| txt(n, src).to_string());
            let qual = |name: &str| match owner {
                Some((o, _)) => format!("{module}{sep}{o}{sep}{name}"),
                None => format!("{module}{sep}{name}"),
            };
            match k {
                "function_item" | "function_signature_item" => {
                    let (Some(name), true) = (name, public) else { continue };
                    let kind = if owner.is_some() { Kind::Method } else { Kind::Function };
                    self.out.push(entry(
                        kind,
                        &name,
                        qual(&name),
                        self.file,
                        line,
                        head(c, src, "body", 600).trim_end_matches(';').to_string(),
                        doc,
                    ));
                }
                "struct_item" | "enum_item" | "union_item" => {
                    let (Some(name), true) = (name, public) else { continue };
                    let kind = if k == "enum_item" { Kind::Enum } else { Kind::Struct };
                    let full = txt(c, src);
                    let sig = if full.len() <= 900 {
                        compact(full, 900)
                    } else {
                        format!("{} {{ … }}", head(c, src, "body", 400))
                    };
                    self.out.push(entry(kind, &name, qual(&name), self.file, line, sig, doc));
                }
                "trait_item" => {
                    let (Some(name), true) = (name, public) else { continue };
                    self.out
                        .push(entry(Kind::Trait, &name, qual(&name), self.file, line, head(c, src, "body", 400), doc));
                    if let Some(b) = c.child_by_field_name("body") {
                        let mut ignore = Vec::new();
                        self.block(b, module, Some((&name, true)), &mut ignore);
                    }
                }
                "impl_item" => {
                    if c.child_by_field_name("trait").is_some() {
                        continue;
                    }
                    let Some(ty) = c.child_by_field_name("type") else { continue };
                    let ty = txt(ty, src);
                    let ty = ty.split('<').next().unwrap_or(ty).trim().to_string();
                    if let Some(b) = c.child_by_field_name("body") {
                        let mut ignore = Vec::new();
                        self.block(b, module, Some((&ty, false)), &mut ignore);
                    }
                }
                "mod_item" => {
                    let (Some(name), true) = (name, public) else { continue };
                    let path = format!("{module}{sep}{name}");
                    if !doc.is_empty() {
                        self.out
                            .push(entry(Kind::Module, &name, path.clone(), self.file, line, format!("pub mod {name}"), doc));
                    }
                    if let Some(b) = c.child_by_field_name("body") {
                        let mut inner = Vec::new();
                        self.block(b, &path, None, &mut inner);
                    }
                }
                "type_item" | "const_item" | "static_item" => {
                    let (Some(name), true) = (name, public) else { continue };
                    let kind = if k == "type_item" { Kind::Type } else { Kind::Const };
                    self.out.push(entry(
                        kind,
                        &name,
                        qual(&name),
                        self.file,
                        line,
                        compact(txt(c, src).trim_end_matches(';'), 400),
                        doc,
                    ));
                }
                "macro_definition" => {
                    let Some(name) = name else { continue };
                    if !was_macro_export {
                        continue;
                    }
                    // Exported macros live at the crate root.
                    let root = module.split("::").next().unwrap_or(module);
                    let arms = compact(txt(c, src), 300);
                    self.out.push(entry(Kind::Macro, &name, format!("{root}::{name}!"), self.file, line, arms, doc));
                }
                "macro_invocation" if self.depth < 3 => {
                    // cfg_rt! { pub fn spawn ... } and friends: parse the body as items.
                    let Some(tt) = c.named_children(&mut c.walk()).find(|n| n.kind() == "token_tree") else {
                        continue;
                    };
                    let body = txt(tt, src);
                    if body.len() < 4 || !(body.contains("fn ") || body.contains("struct ") || body.contains("mod ") || body.contains("trait ")) {
                        continue;
                    }
                    let inner = &body[1..body.len() - 1];
                    let off = tt.start_position().row as u32 + self.line_off;
                    let mut sub = Vec::new();
                    rs_parse(inner, self.file, module, off, self.depth + 1, &mut sub);
                    // Owner-less items only: prose from nested inner docs stays.
                    self.out.extend(sub);
                }
                _ => {}
            }
        }
    }
}

// ---------------------------------------------------------------- Go

fn exported(name: &str) -> bool {
    name.chars().next().is_some_and(|c| c.is_uppercase())
}

fn go_doc(n: Node, src: &[u8]) -> String {
    let mut lines: Vec<String> = Vec::new();
    let mut p = n.prev_sibling();
    let mut next_row = n.start_position().row;
    while let Some(c) = p {
        if c.kind() != "comment" || c.end_position().row + 1 != next_row {
            break;
        }
        let t = txt(c, src);
        if let Some(r) = t.strip_prefix("//") {
            lines.push(r.strip_prefix(' ').unwrap_or(r).trim_end().to_string());
        } else {
            lines.push(clean_block_comment(t));
        }
        next_row = c.start_position().row;
        p = c.prev_sibling();
    }
    lines.reverse();
    cap_doc(lines.join("\n").trim().to_string())
}

fn go_file(text: &str, rel: &str, module: &str, out: &mut Vec<Entry>) {
    parse_with("go", text, |root, src| {
        let mut cur = root.walk();
        let children: Vec<Node> = root.named_children(&mut cur).collect();
        let mut pkg = module.rsplit('/').next().unwrap_or(module).to_string();
        for c in children {
            let line = c.start_position().row as u32 + 1;
            match c.kind() {
                "package_clause" => {
                    if let Some(id) = c.named_child(0) {
                        pkg = txt(id, src).to_string();
                    }
                    if pkg == "main" {
                        return;
                    }
                    let doc = go_doc(c, src);
                    if !doc.is_empty() {
                        let dir = rel.rsplit_once('/').map_or("", |x| x.0);
                        let title = if dir.is_empty() {
                            format!("package {pkg}")
                        } else {
                            format!("package {pkg} ({dir})")
                        };
                        for s in markdown::split(&doc, &title) {
                            out.push(Entry {
                                kind: Kind::Prose,
                                name: s.heading.clone(),
                                path: s.heading,
                                file: rel.to_string(),
                                line: s.line,
                                sig: String::new(),
                                doc: s.text,
                                alias_of: None,
                                legacy: false,
                            });
                        }
                    }
                }
                "function_declaration" => {
                    let Some(name) = c.child_by_field_name("name").map(|n| txt(n, src)) else {
                        continue;
                    };
                    if !exported(name) {
                        continue;
                    }
                    out.push(entry(
                        Kind::Function,
                        name,
                        format!("{pkg}.{name}"),
                        rel,
                        line,
                        head(c, src, "body", 600),
                        go_doc(c, src),
                    ));
                }
                "method_declaration" => {
                    let Some(name) = c.child_by_field_name("name").map(|n| txt(n, src)) else {
                        continue;
                    };
                    let recv = c.child_by_field_name("receiver").map(|r| txt(r, src)).unwrap_or("");
                    let ty: String = recv
                        .trim_matches(['(', ')'])
                        .split_whitespace()
                        .last()
                        .unwrap_or("")
                        .trim_start_matches('*')
                        .split('[')
                        .next()
                        .unwrap_or("")
                        .to_string();
                    if !exported(name) || !exported(&ty) {
                        continue;
                    }
                    out.push(entry(
                        Kind::Method,
                        name,
                        format!("{pkg}.{ty}.{name}"),
                        rel,
                        line,
                        head(c, src, "body", 600),
                        go_doc(c, src),
                    ));
                }
                "type_declaration" | "const_declaration" | "var_declaration" => {
                    let outer_doc = go_doc(c, src);
                    let mut cc = c.walk();
                    let specs: Vec<Node> = c
                        .named_children(&mut cc)
                        .filter(|n| matches!(n.kind(), "type_spec" | "type_alias" | "const_spec" | "var_spec"))
                        .collect();
                    let single = specs.len() == 1;
                    for s in specs {
                        let names: Vec<String> = if s.kind().starts_with("type") {
                            s.child_by_field_name("name").map(|n| vec![txt(n, src).to_string()]).unwrap_or_default()
                        } else {
                            let mut sc = s.walk();
                            s.children_by_field_name("name", &mut sc).map(|n| txt(n, src).to_string()).collect()
                        };
                        let doc = if single { outer_doc.clone() } else { go_doc(s, src) };
                        let sline = s.start_position().row as u32 + 1;
                        for name in names.iter().filter(|n| exported(n)) {
                            if s.kind().starts_with("type") {
                                let full = txt(s, src);
                                let sig = if full.len() <= 900 {
                                    format!("type {}", compact(full, 900))
                                } else {
                                    format!("type {} …", compact(&full[..full.find('{').unwrap_or(200.min(full.len()))], 300))
                                };
                                let kind = if full.contains("interface") { Kind::Interface } else { Kind::Struct };
                                out.push(entry(kind, name, format!("{pkg}.{name}"), rel, sline, sig, doc.clone()));
                                // Interface methods
                                if let Some(t) = s.child_by_field_name("type").filter(|t| t.kind() == "interface_type") {
                                    let mut ic = t.walk();
                                    let elems: Vec<Node> = t.named_children(&mut ic).collect();
                                    for m in elems {
                                        if !matches!(m.kind(), "method_elem" | "method_spec") {
                                            continue;
                                        }
                                        let Some(mn) = m.child_by_field_name("name").map(|n| txt(n, src)) else {
                                            continue;
                                        };
                                        out.push(entry(
                                            Kind::Method,
                                            mn,
                                            format!("{pkg}.{name}.{mn}"),
                                            rel,
                                            m.start_position().row as u32 + 1,
                                            compact(txt(m, src), 400),
                                            go_doc(m, src),
                                        ));
                                    }
                                }
                            } else {
                                let kw = if c.kind() == "const_declaration" { "const" } else { "var" };
                                out.push(entry(
                                    Kind::Const,
                                    name,
                                    format!("{pkg}.{name}"),
                                    rel,
                                    sline,
                                    compact(&format!("{kw} {}", txt(s, src)), 300),
                                    doc.clone(),
                                ));
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(f: fn(&str, &str, &mut Vec<Entry>), text: &str, rel: &str) -> Vec<Entry> {
        let mut out = Vec::new();
        f(text, rel, &mut out);
        out
    }

    #[test]
    fn typescript_declarations() {
        let src = r#"
/** Creates an object schema. */
export declare function object<T extends ZodRawShape>(shape: T, params?: string): ZodObject<T>;
export declare namespace z {
  /** A string schema. */
  function string(): ZodString;
}
export interface ZodType<T> {
  /** Parse or throw. */
  parse(data: unknown): T;
  readonly _def: Def;
}
export declare class ZodObject<T> extends ZodType<T> {
  /** Disallow unknown keys. */
  strict(message?: string): ZodObject<T>;
  private _cached;
}
declare const objectType: <T>(shape: T) => ZodObject<T>;
export { objectType as obj };
export * as zz from "./external";
"#;
        let e = run(ts_file, src, "lib/index.d.ts");
        let get = |p: &str| {
            e.iter()
                .find(|x| x.path == p)
                .unwrap_or_else(|| panic!("missing {p}: {:#?}", e.iter().map(|x| &x.path).collect::<Vec<_>>()))
        };
        assert_eq!(get("object").doc, "Creates an object schema.");
        assert!(get("object").sig.contains("ZodObject<T>"));
        assert_eq!(get("z.string").doc, "A string schema.");
        assert_eq!(get("ZodType.parse").kind, Kind::Method);
        assert!(e.iter().all(|x| x.name != "_def" && x.name != "_cached"));
        assert_eq!(get("ZodObject.strict").doc, "Disallow unknown keys.");
        assert_eq!(get("objectType").kind, Kind::Function);
        assert_eq!(get("obj").alias_of.as_deref(), Some("objectType"));
        assert_eq!(get("zz").kind, Kind::Module);
        assert_eq!(get("object").line, 3);
    }

    #[test]
    fn legacy_dirs() {
        assert!(legacy_file("v3/types.d.ts", 4));
        assert!(!legacy_file("v4/classic/schemas.d.ts", 4));
        assert!(legacy_file("pydantic/v1/main.py", 2));
        assert!(!legacy_file("pydantic/main.py", 2));
        assert!(!legacy_file("src/v2/x.rs", 1));
    }

    #[test]
    fn python_definitions() {
        let src = "\"\"\"Module docs.\"\"\"\n\nclass BaseModel:\n    \"\"\"Base class.\n\n    More.\n    \"\"\"\n\n    def model_dump(self, *, mode: str = 'python') -> dict:\n        \"\"\"Dump the model.\"\"\"\n        return {}\n\n    @classmethod\n    def model_validate(cls, obj):\n        '''Validate.'''\n\n    def _private(self): pass\n\ndef create_model(name: str, **fields) -> type:\n    return None\n";
        let e = run(py_file, src, "pydantic/main.py");
        let get = |p: &str| e.iter().find(|x| x.path == p).unwrap_or_else(|| panic!("missing {p}"));
        assert_eq!(get("pydantic.main.BaseModel").doc, "Base class.\n\nMore.");
        assert_eq!(
            get("pydantic.main.BaseModel.model_dump").sig,
            "def model_dump(self, *, mode: str = 'python') -> dict"
        );
        assert!(get("pydantic.main.BaseModel.model_validate").sig.starts_with("@classmethod def model_validate"));
        assert!(e.iter().all(|x| x.name != "_private"));
        assert!(e.iter().any(|x| x.kind == Kind::Prose && x.doc == "Module docs."));
        assert_eq!(get("pydantic.main.create_model").kind, Kind::Function);
    }

    #[test]
    fn rust_items_and_macro_bodies() {
        let src = "//! Crate docs.\n//!\n//! # Usage\n//! Call spawn.\n\ncfg_rt! {\n    /// Spawns a new task.\n    #[track_caller]\n    pub fn spawn<F>(future: F) -> JoinHandle<F::Output>\n    where F: Future + Send + 'static {\n        todo!()\n    }\n}\n\n/// A router.\npub struct Router<S = ()> { inner: Inner }\n\nimpl<S> Router<S> {\n    /// Add a route.\n    pub fn route(self, path: &str, m: MethodRouter<S>) -> Self { self }\n    fn private(&self) {}\n}\n\n/// Hidden\n#[doc(hidden)]\npub fn hidden() {}\n\n/// Macro.\n#[macro_export]\nmacro_rules! select { () => {} }\n";
        let mut e = Vec::new();
        rs_file(src, "src/task/spawn.rs", "tokio", &mut e);
        let get = |p: &str| {
            e.iter()
                .find(|x| x.path == p)
                .unwrap_or_else(|| panic!("missing {p}: {:#?}", e.iter().map(|x| &x.path).collect::<Vec<_>>()))
        };
        let spawn = get("tokio::task::spawn::spawn");
        assert_eq!(spawn.doc, "Spawns a new task.");
        assert_eq!(spawn.line, 9);
        assert!(spawn.sig.contains("JoinHandle"));
        assert_eq!(get("tokio::task::spawn::Router::route").doc, "Add a route.");
        assert!(e.iter().all(|x| x.name != "private" && x.name != "hidden"));
        assert_eq!(get("tokio::select!").kind, Kind::Macro);
        assert!(e.iter().any(|x| x.kind == Kind::Prose && x.path.ends_with("Usage")));
    }

    #[test]
    fn go_declarations() {
        let src = "// Package gin implements a web framework.\npackage gin\n\n// Context is the request context.\ntype Context struct {\n\tKeys map[string]any\n}\n\n// JSON serializes obj.\nfunc (c *Context) JSON(code int, obj any) {\n}\n\n// Default returns an Engine.\nfunc Default(opts ...Option) *Engine {\n\treturn nil\n}\n\nfunc internal() {}\n\ntype Handler interface {\n\t// Serve serves.\n\tServe(c *Context)\n}\n";
        let mut e = Vec::new();
        go_file(src, "context.go", "github.com/gin-gonic/gin", &mut e);
        let get = |p: &str| {
            e.iter()
                .find(|x| x.path == p)
                .unwrap_or_else(|| panic!("missing {p}: {:#?}", e.iter().map(|x| &x.path).collect::<Vec<_>>()))
        };
        assert_eq!(get("gin.Context.JSON").doc, "JSON serializes obj.");
        assert_eq!(get("gin.Default").sig, "func Default(opts ...Option) *Engine");
        assert_eq!(get("gin.Context").doc, "Context is the request context.");
        assert_eq!(get("gin.Handler.Serve").doc, "Serve serves.");
        assert!(e.iter().all(|x| x.name != "internal"));
        assert!(e.iter().any(|x| x.kind == Kind::Prose && x.doc.contains("web framework")));
    }
}
