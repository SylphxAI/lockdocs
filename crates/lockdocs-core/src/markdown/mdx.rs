use super::headings::html_heading;

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
