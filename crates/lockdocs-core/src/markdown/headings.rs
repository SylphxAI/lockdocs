const RST_UNDERLINE: &[char] = &['=', '-', '~', '^', '*', '+', '#', '"', '`', '\''];

pub(super) fn is_underline(line: &str, prev: &str) -> Option<char> {
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
pub(super) fn noise(line: &str) -> bool {
    let t = line.trim();
    (t.starts_with("[![") || t.starts_with("<img") || t.starts_with("<a href") && t.contains("<img"))
        || (t.starts_with("<p") || t.starts_with("</p") || t.starts_with("<div") || t.starts_with("</div") || t.starts_with("<br")) && t.len() < 60
        || t.starts_with("<!--") && t.ends_with("-->")
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

pub(super) fn clean_heading(h: &str) -> String {
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

/// An HTML heading starting at `lines[0]`, spanning up to 4 lines:
/// (level, plain text, lines used).
pub(super) fn html_heading(lines: &[&str]) -> Option<(usize, String, usize)> {
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
