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

fn clean_heading(h: &str) -> String {
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
    fn rst_headings() {
        let rst = "Title\n=====\n\nBody\n\nSub\n---\n\nx\n";
        let s = split(rst, "doc");
        assert_eq!(s.len(), 2);
        assert_eq!(s[1].heading, "doc › Title › Sub");
    }
}
