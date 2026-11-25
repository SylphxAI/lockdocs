use crate::private::{Candidate, Http, Url};
use crate::{Dep, Eco};
use anyhow::{bail, Result};
use std::path::Path;

fn file_version(name: &str) -> Option<(String, String)> {
    let lower = name.to_string();
    if let Some(stem) = lower.strip_suffix(".whl") {
        let mut it = stem.split('-');
        return Some((it.next()?.to_string(), it.next()?.to_string()));
    }
    for ext in [".tar.gz", ".zip"] {
        if let Some(stem) = lower.strip_suffix(ext) {
            let (n, v) = stem.rsplit_once('-')?;
            return Some((n.to_string(), v.to_string()));
        }
    }
    None
}

/// `(filename, url)` pairs of a simple-index page (PEP 691 JSON or PEP 503 HTML).
pub fn parse_simple(body: &[u8], page_url: &str) -> Vec<(String, String)> {
    let text = String::from_utf8_lossy(body);
    let base = Url::parse(page_url);
    let resolve = |href: &str| -> Option<String> {
        let href = href.split('#').next().unwrap_or(href).replace("&amp;", "&");
        match &base {
            Some(b) => b.join(&href).map(|u| u.clean()),
            None => None,
        }
    };
    if text.trim_start().starts_with('{') {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
            return Vec::new();
        };
        return v
            .get("files")
            .and_then(|f| f.as_array())
            .into_iter()
            .flatten()
            .filter_map(|f| Some((f.get("filename")?.as_str()?.to_string(), resolve(f.get("url")?.as_str()?)?)))
            .collect();
    }
    let mut out = Vec::new();
    let mut rest = &*text;
    while let Some(i) = rest.find("<a ") {
        rest = &rest[i..];
        let Some(end) = rest.find('>') else { break };
        let tag = &rest[..end];
        let after = &rest[end + 1..];
        let href = tag.find("href=").and_then(|h| {
            let v = &tag[h + 5..];
            let q = v.chars().next()?;
            (q == '"' || q == '\'').then(|| v[1..].split(q).next().unwrap_or("").to_string())
        });
        let label = after.split("</a>").next().unwrap_or("").trim().to_string();
        if let (Some(h), false) = (href, label.is_empty()) {
            if let Some(u) = resolve(&h) {
                out.push((label, u));
            }
        }
        rest = after;
    }
    out
}

/// Pick the artifact for `version`: a pure wheel, any wheel, then an sdist.
/// Returns the advertised filename and its URL together: the filename is the
/// only reliable format signal, since a private index may serve the artifact
/// from a signed or opaque download URL with no extension.
pub fn pick(files: &[(String, String)], name: &str, version: &str) -> Option<(String, String)> {
    let want = crate::norm_name(Eco::PyPI, name);
    let matching: Vec<&(String, String)> = files
        .iter()
        .filter(|(f, _)| file_version(f).is_some_and(|(n, v)| crate::norm_name(Eco::PyPI, &n) == want && v == version))
        .collect();
    matching
        .iter()
        .find(|(f, _)| f.ends_with("-none-any.whl"))
        .or_else(|| matching.iter().find(|(f, _)| f.ends_with(".whl")))
        .or_else(|| matching.iter().find(|(f, _)| f.ends_with(".tar.gz") || f.ends_with(".zip")))
        .map(|(f, u)| (f.clone(), u.clone()))
}

/// Unpack a downloaded artifact using its advertised filename, not its URL: a
/// wheel behind a `?query=` or an opaque endpoint still unpacks as a wheel.
pub fn unpack(filename: &str, bytes: &[u8], dir: &Path) -> Result<()> {
    let f = filename.split(['?', '#']).next().unwrap_or(filename).to_ascii_lowercase();
    if f.ends_with(".whl") {
        crate::fetch::unzip(bytes, dir, false, None)
    } else if f.ends_with(".zip") {
        crate::fetch::unzip(bytes, dir, true, None)
    } else {
        crate::fetch::untar_gz(bytes, dir)
    }
}

/// Download one version from a private index. Ok(false) when it has none.
pub fn download(http: &Http, dep: &Dep, c: &Candidate, dir: &Path) -> Result<bool> {
    let page = format!("{}/{}/", c.base.trim_end_matches('/'), crate::norm_name(Eco::PyPI, &dep.name));
    let r = http.get(&page, Some("application/vnd.pypi.simple.v1+json, text/html;q=0.5"))?;
    match r.status {
        200 => {}
        404 | 410 => return Ok(false),
        401 | 403 => bail!(
            "{} refused the request (HTTP {}); check the index credential",
            Url::parse(&page).map_or_else(String::new, |u| u.origin().to_string()),
            r.status
        ),
        s => bail!("{}: HTTP {s}", r.url),
    }
    let files = parse_simple(&r.body, &r.url);
    let Some((filename, url)) = pick(&files, &dep.name, &dep.version) else {
        return Ok(false);
    };
    let Some(bytes) = http.get_ok(&url, None)? else { return Ok(false) };
    unpack(&filename, &bytes, dir)?;
    Ok(true)
}
