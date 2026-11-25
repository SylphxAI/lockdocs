//! Split Markdown, reStructuredText and plain text into heading-scoped
//! sections small enough to rank and quote.

mod headings;
mod imports;
mod mdx;

#[cfg(test)]
mod tests;

use headings::{clean_heading, is_underline, noise};
use imports::{collect_example_imports, example_imports, section_modes};

pub use headings::{generic_heading, topic_heading};
pub(crate) use imports::import_base;
pub use mdx::{clean_mdx, metadata_body};

#[derive(Debug, Clone, PartialEq)]
pub struct Section {
    /// Heading path, e.g. `Basic usage › Objects`.
    pub heading: String,
    /// 1-based line of the section start.
    pub line: u32,
    pub text: String,
    /// Effective example imports, inherited by package before section chunking.
    pub imports: Vec<String>,
    pub guide_modes: Vec<String>,
    /// Lines of nonempty ancestor sections in this page, outermost first.
    pub parents: Vec<u32>,
}

const MAX_CHARS: usize = 1800;

struct Scope {
    level: usize,
    title: String,
    imports: Vec<String>,
    guide_modes: Vec<String>,
    line: Option<u32>,
}

pub fn split(text: &str, title: &str) -> Vec<Section> {
    split_scoped(text, title, None)
}

/// Fetched reference pages can show the import setup once and omit it in
/// following sections. An explicit section import overrides the page default.
pub(crate) fn split_scoped(text: &str, title: &str, package: Option<&str>) -> Vec<Section> {
    let imports = package.map_or_else(Vec::new, |package| {
        collect_example_imports(text, true)
            .into_iter()
            .filter(|import| import_base(import) == package)
            .collect()
    });
    let lines: Vec<&str> = text.lines().collect();
    let mut out: Vec<Section> = Vec::new();
    let mut stack = vec![Scope {
        level: 0,
        title: title.to_string(),
        imports,
        guide_modes: Vec::new(),
        line: None,
    }];
    let mut rst_levels: Vec<char> = Vec::new();
    let mut cur = String::new();
    let mut cur_line = 1u32;
    let mut fence: Option<String> = None;
    let flush = |out: &mut Vec<Section>, cur: &mut String, stack: &mut Vec<Scope>, line: u32| {
        let body = cur.trim();
        if !body.is_empty() {
            let own = example_imports(body);
            let scope = stack.last_mut().unwrap();
            for import in &own {
                scope.imports.retain(|old| import_base(old) != import_base(import));
            }
            scope.imports.extend(own);
            scope.imports.sort();
            scope.imports.dedup();
            if let Some(modes) = section_modes(body) {
                scope.guide_modes = modes;
            }
            let section = Section {
                heading: stack.iter().map(|s| s.title.as_str()).collect::<Vec<_>>().join(" › "),
                line,
                text: body.to_string(),
                imports: stack.last().unwrap().imports.clone(),
                guide_modes: stack.last().unwrap().guide_modes.clone(),
                parents: stack[..stack.len() - 1].iter().filter_map(|s| s.line).collect(),
            };
            push_sized(out, &section);
            stack.last_mut().unwrap().line = Some(line);
        }
        cur.clear();
    };
    let is_md = lines.iter().any(|l| l.starts_with("# ") || l.starts_with("## "));
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let t = line.trim_start();
        if let Some(f) = &fence {
            if t.starts_with(f.as_str()) {
                fence = None;
            }
            cur.push_str(line);
            cur.push('\n');
            i += 1;
            continue;
        }
        if t.starts_with("```") || t.starts_with("~~~") {
            fence = Some(t[..3].to_string());
            cur.push_str(line);
            cur.push('\n');
            i += 1;
            continue;
        }
        // ATX heading
        if line.starts_with('#') {
            let level = line.chars().take_while(|c| *c == '#').count();
            let rest = &line[level..];
            if level <= 6 && (rest.is_empty() || rest.starts_with(' ')) {
                let h = rest.trim().trim_end_matches('#').trim().to_string();
                flush(&mut out, &mut cur, &mut stack, cur_line);
                stack.retain(|s| s.level < level);
                stack.push(Scope {
                    level,
                    title: clean_heading(&h),
                    imports: stack.last().unwrap().imports.clone(),
                    guide_modes: stack.last().unwrap().guide_modes.clone(),
                    line: None,
                });
                cur_line = i as u32 + 1;
                i += 1;
                continue;
            }
        }
        // Setext / reST underline heading
        if i + 1 < lines.len() && !line.trim().is_empty() {
            let under = is_underline(lines[i + 1], line).filter(|c| !is_md || *c == '=' || *c == '-');
            if let Some(c) = under {
                let level = if is_md {
                    if c == '=' {
                        1
                    } else {
                        2
                    }
                } else {
                    match rst_levels.iter().position(|x| *x == c) {
                        Some(p) => p + 1,
                        None => {
                            rst_levels.push(c);
                            rst_levels.len()
                        }
                    }
                };
                flush(&mut out, &mut cur, &mut stack, cur_line);
                stack.retain(|s| s.level < level);
                stack.push(Scope {
                    level,
                    title: clean_heading(line.trim()),
                    imports: stack.last().unwrap().imports.clone(),
                    guide_modes: stack.last().unwrap().guide_modes.clone(),
                    line: None,
                });
                cur_line = i as u32 + 1;
                i += 2;
                continue;
            }
        }
        if !noise(line) {
            cur.push_str(line);
            cur.push('\n');
        }
        i += 1;
    }
    flush(&mut out, &mut cur, &mut stack, cur_line);
    out
}

/// Split long sections at blank lines outside code fences.
#[allow(clippy::explicit_counter_loop)]
fn push_sized(out: &mut Vec<Section>, section: &Section) {
    let body = &section.text;
    let line = section.line;
    if body.len() <= MAX_CHARS {
        out.push(Section {
            heading: section.heading.clone(),
            line,
            text: body.to_string(),
            imports: section.imports.clone(),
            guide_modes: section.guide_modes.clone(),
            parents: section.parents.clone(),
        });
        return;
    }
    let mut buf = String::new();
    let mut start = line;
    let mut ln = line;
    let mut in_fence = false;
    for l in body.lines() {
        let t = l.trim_start();
        if t.starts_with("```") || t.starts_with("~~~") {
            in_fence = !in_fence;
        }
        if !in_fence && l.trim().is_empty() && buf.len() > MAX_CHARS * 2 / 3 {
            out.push(Section {
                heading: section.heading.clone(),
                line: start,
                text: buf.trim().to_string(),
                imports: section.imports.clone(),
                guide_modes: section.guide_modes.clone(),
                parents: section.parents.clone(),
            });
            buf.clear();
            start = ln + 1;
        } else if buf.len() > MAX_CHARS * 3 {
            // A giant fence or paragraph: cut hard.
            out.push(Section {
                heading: section.heading.clone(),
                line: start,
                text: buf.trim().to_string(),
                imports: section.imports.clone(),
                guide_modes: section.guide_modes.clone(),
                parents: section.parents.clone(),
            });
            buf.clear();
            start = ln;
        }
        buf.push_str(l);
        buf.push('\n');
        ln += 1;
    }
    if !buf.trim().is_empty() {
        out.push(Section {
            heading: section.heading.clone(),
            line: start,
            text: buf.trim().to_string(),
            imports: section.imports.clone(),
            guide_modes: section.guide_modes.clone(),
            parents: section.parents.clone(),
        });
    }
}
