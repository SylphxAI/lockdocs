//! Split Markdown, reStructuredText and plain text into heading-scoped
//! sections small enough to rank and quote.

#[derive(Debug, Clone, PartialEq)]
pub struct Section {
    /// Heading path, e.g. `Basic usage › Objects`.
    pub heading: String,
    /// 1-based line of the section start.
    pub line: u32,
    pub text: String,
}

const MAX_CHARS: usize = 1800;
const RST_UNDERLINE: &[char] = &['=', '-', '~', '^', '*', '+', '#', '"', '`', '\''];

fn is_underline(line: &str, prev: &str) -> Option<char> {
    let t = line.trim_end();
    let c = t.chars().next()?;
    if !RST_UNDERLINE.contains(&c) || t.len() < 3 || !t.chars().all(|x| x == c) {
        return None;
    }
    let p = prev.trim();
    if p.is_empty() || p.len() > 120 || t.len() + 2 < p.chars().count() {
        return None;
    }
    Some(c)
}

/// Drop lines that are only badges or HTML wrappers: they cost tokens and carry no API.
fn noise(line: &str) -> bool {
    let t = line.trim();
    (t.starts_with("[![") || t.starts_with("<img") || t.starts_with("<a href") && t.contains("<img"))
        || (t.starts_with("<p") || t.starts_with("</p") || t.starts_with("<div") || t.starts_with("</div") || t.starts_with("<br")) && t.len() < 60
        || t.starts_with("<!--") && t.ends_with("-->")
}

pub fn split(text: &str, title: &str) -> Vec<Section> {
    let lines: Vec<&str> = text.lines().collect();
    let mut out: Vec<Section> = Vec::new();
    let mut stack: Vec<(usize, String)> = Vec::new();
    let mut rst_levels: Vec<char> = Vec::new();
    let mut cur = String::new();
    let mut cur_line = 1u32;
    let mut fence: Option<String> = None;
    let heading = |stack: &Vec<(usize, String)>| {
        let mut parts: Vec<&str> = vec![title];
        parts.extend(stack.iter().map(|(_, h)| h.as_str()));
        parts.join(" › ")
    };
    let flush = |out: &mut Vec<Section>, cur: &mut String, head: String, line: u32| {
        let body = cur.trim();
        if !body.is_empty() {
            push_sized(out, &head, line, body);
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
                flush(&mut out, &mut cur, heading(&stack), cur_line);
                stack.retain(|(l, _)| *l < level);
                stack.push((level, clean_heading(&h)));
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
                flush(&mut out, &mut cur, heading(&stack), cur_line);
                stack.retain(|(l, _)| *l < level);
                stack.push((level, clean_heading(line.trim())));
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
    flush(&mut out, &mut cur, heading(&stack), cur_line);
    out
}

/// Headings that name a part of a page, not a topic (`Parameters`,
/// `Returns`, `Examples`): the topic is the heading above them.
pub fn generic_heading(h: &str) -> bool {
    let h = h.trim().trim_end_matches(':').to_ascii_lowercase();
    [
        "parameters",
        "parameter",
        "params",
        "arguments",
        "args",
        "returns",
        "return value",
        "return type",
        "props",
        "options",
        "examples",
        "example",
        "usage",
        "reference",
        "overview",
        "notes",
        "caveats",
        "good to know",
        "type",
        "types",
        "description",
        "syntax",
        "signature",
        "see also",
        "troubleshooting",
        "introduction",
        "summary",
        "details",
    ]
    .contains(&h.as_str())
}

/// The heading an entry is about: its last heading, or the one above it
/// when the last is generic (`generateMetadata › Parameters` -> `generateMetadata`).
pub fn topic_heading(name: &str) -> &str {
    let mut it = name.rsplit(" › ");
    let last = it.next().unwrap_or("");
    if generic_heading(last) {
        it.next().unwrap_or(last)
    } else {
        last
    }
}

fn clean_heading(h: &str) -> String {
    // MDX heading ids: `Usage {/*usage*/}`, `Usage {#usage}`.
    let mut h = h.to_string();
    for (open, close) in [("{/*", "*/}"), ("{#", "}")] {
        while let Some(i) = h.find(open) {
            match h[i..].find(close) {
                Some(j) => h.replace_range(i..i + j + close.len(), ""),
                None => break,
            }
        }
    }
    let h = h.as_str();
    // `[text](url)` -> text, drop inline code ticks and HTML tags.
    let mut s = String::new();
    let mut in_tag = false;
    let mut in_url = false;
    let mut prev = ' ';
    for c in h.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            '(' if prev == ']' => in_url = true,
            ')' if in_url => in_url = false,
            '[' | ']' | '`' | '*' => {}
            _ if in_tag || in_url => {}
            _ => s.push(c),
        }
        prev = c;
    }
    s.trim().to_string()
}

/// Split long sections at blank lines outside code fences.
#[allow(clippy::explicit_counter_loop)]
fn push_sized(out: &mut Vec<Section>, head: &str, line: u32, body: &str) {
    if body.len() <= MAX_CHARS {
        out.push(Section {
            heading: head.to_string(),
            line,
            text: body.to_string(),
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
                heading: head.to_string(),
                line: start,
                text: buf.trim().to_string(),
            });
            buf.clear();
            start = ln + 1;
        } else if buf.len() > MAX_CHARS * 3 {
            // A giant fence or paragraph: cut hard.
            out.push(Section {
                heading: head.to_string(),
                line: start,
                text: buf.trim().to_string(),
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
            heading: head.to_string(),
            line: start,
            text: buf.trim().to_string(),
        });
    }
}

/// Blank out front matter (keeping its `title`), and for MDX the `import` /
/// `export` lines and JSX-only lines. `export const title = "..."` names the
/// page and `export const description` keeps its text. HTML headings
/// (`<h3 id="x"><a href="#x">@import</a></h3>`, one or more lines) become
/// Markdown headings so sections split at them. Line numbers are preserved.
pub fn clean_mdx(text: &str, mdx: bool) -> (String, Option<String>) {
    let lines: Vec<&str> = text.lines().collect();
    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    let mut title = None;
    let mut description = None;
    let mut i = 0;
    if lines.first().map(|l| l.trim_end()) == Some("---") {
        if let Some(end) = lines.iter().skip(1).position(|l| l.trim_end() == "---") {
            for l in &lines[1..=end] {
                if let Some(t) = l.strip_prefix("title:") {
                    title = Some(t.trim().trim_matches(['"', '\'']).to_string()).filter(|t| !t.is_empty());
                }
                if let Some(t) = l.strip_prefix("description:") {
                    description = Some(t.trim().trim_matches(['"', '\'']).to_string()).filter(|t| !t.is_empty());
                }
            }
            // Keep line numbers: the description takes the front matter's first line.
            out.push(description.take().unwrap_or_default());
            for _ in 0..end + 1 {
                out.push(String::new());
            }
            i = end + 2;
        }
    }
    let mut fence = false;
    let mut j = i.min(lines.len());
    while j < lines.len() {
        let l = lines[j];
        let t = l.trim();
        if t.starts_with("```") || t.starts_with("~~~") {
            fence = !fence;
        }
        if !fence {
            if let Some((level, text, used)) = html_heading(&lines[j..]) {
                out.push(format!("{} {text}", "#".repeat(level)));
                out.extend(std::iter::repeat_n(String::new(), used - 1));
                j += used;
                continue;
            }
        }
        if mdx && !fence {
            if let Some(v) = export_const(t, "title") {
                title = title.or(Some(v));
                out.push(String::new());
                j += 1;
                continue;
            }
            if let Some(v) = export_const(t, "description") {
                out.push(v);
                j += 1;
                continue;
            }
        }
        let drop = mdx
            && !fence
            && (t.starts_with("import ")
                || t.starts_with("export ")
                || (t.starts_with('<') && t.ends_with('>') && !t.starts_with("<!--") && !t.contains("</code>")));
        out.push(if drop { String::new() } else { l.to_string() });
        j += 1;
    }
    (out.join("\n"), title)
}

/// `export const <name> = "value";` -> value.
fn export_const(line: &str, name: &str) -> Option<String> {
    let rest = line.strip_prefix("export const ")?.strip_prefix(name)?.trim_start().strip_prefix('=')?.trim();
    let rest = rest.trim_end_matches(';').trim();
    let q = rest.chars().next().filter(|c| matches!(c, '"' | '\'' | '`'))?;
    let v = rest.strip_prefix(q)?.strip_suffix(q)?;
    (!v.is_empty()).then(|| v.to_string())
}

/// An HTML heading starting at `lines[0]`, spanning up to 4 lines:
/// (level, plain text, lines used).
fn html_heading(lines: &[&str]) -> Option<(usize, String, usize)> {
    let t = lines.first()?.trim_start();
    let b = t.as_bytes();
    if b.len() < 4 || b[0] != b'<' || !matches!(b[1], b'h' | b'H') || !(b'1'..=b'6').contains(&b[2]) || !matches!(b[3], b'>' | b' ') {
        return None;
    }
    let level = (b[2] - b'0') as usize;
    let close = format!("</h{level}>");
    let mut joined = String::new();
    for (n, l) in lines.iter().take(4).enumerate() {
        joined.push_str(l.trim());
        joined.push(' ');
        if l.to_ascii_lowercase().contains(&close) {
            let text = clean_heading(&joined);
            return (!text.is_empty()).then_some((level, text, n + 1));
        }
    }
    None
}

/// The body of a Python METADATA file (after the RFC 822 headers), which is
/// the package README.
pub fn metadata_body(text: &str) -> (String, u32) {
    let mut line = 1u32;
    let mut rest = text;
    while let Some(i) = rest.find('\n') {
        let l = &rest[..i];
        rest = &rest[i + 1..];
        line += 1;
        if l.trim().is_empty() {
            return (rest.to_string(), line);
        }
    }
    // Older metadata keeps the README in a Description: header.
    (String::new(), line)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sections_follow_headings_and_skip_fences() {
        let md = "# Zod\n\nIntro.\n\n## Objects\n\n```ts\n# not a heading\nz.object({})\n```\n\n### strict\n\nText.\n\nSetext\n------\n\nMore.\n";
        let s = split(md, "README");
        let heads: Vec<&str> = s.iter().map(|x| x.heading.as_str()).collect();
        assert_eq!(
            heads,
            vec![
                "README › Zod",
                "README › Zod › Objects",
                "README › Zod › Objects › strict",
                "README › Zod › Setext"
            ]
        );
        assert!(s[1].text.contains("# not a heading"));
        assert_eq!(s[1].line, 5);
    }

    #[test]
    fn mdx_exports_and_html_headings() {
        let mdx = "import { X } from \"x\";\n\nexport const title = \"Functions and directives\";\nexport const description = \"Custom directives.\";\n\n## Directives\n\nIntro.\n\n<h3 id=\"import-directive\">\n  <a href=\"#import-directive\">@import</a>\n</h3>\n\nUse `@import`.\n\n<h3 id=\"theme\"><a href=\"#theme\">@theme</a></h3>\n\nTokens.\n";
        let (text, title) = clean_mdx(mdx, true);
        assert_eq!(title.as_deref(), Some("Functions and directives"));
        assert_eq!(text.lines().count(), mdx.lines().count());
        let s = split(&text, title.as_deref().unwrap());
        let heads: Vec<&str> = s.iter().map(|x| x.heading.as_str()).collect();
        assert_eq!(
            heads,
            vec![
                "Functions and directives",
                "Functions and directives › Directives",
                "Functions and directives › Directives › @import",
                "Functions and directives › Directives › @theme"
            ]
        );
        assert!(s[0].text.contains("Custom directives."));
        assert_eq!(s[2].line, 10);
    }

    #[test]
    fn heading_ids_and_generic_headings() {
        let s = split("# forwardRef\n\n## Usage {/*usage*/}\n\nText.\n\n### Props {#props}\n\nMore.\n", "doc");
        assert_eq!(s[0].heading, "doc › forwardRef › Usage");
        assert_eq!(topic_heading(&s[1].heading), "Usage");
        assert_eq!(topic_heading("generateMetadata › Parameters"), "generateMetadata");
    }

    #[test]
    fn rst_headings() {
        let rst = "Title\n=====\n\nBody\n\nSub\n---\n\nx\n";
        let s = split(rst, "doc");
        assert_eq!(s.len(), 2);
        assert_eq!(s[1].heading, "doc › Title › Sub");
    }
}
