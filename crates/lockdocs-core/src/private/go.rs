//! Go private modules: `GOPROXY`, `GOPRIVATE`, `GONOPROXY`, `GONOSUMDB` (from
//! the environment or the `go env` file) and `~/.netrc`. A module a private
//! proxy serves is read from that proxy; a private module without one is read
//! straight from its git host at the version's tag, like `go` does.

use super::*;

const PUBLIC_PROXIES: &[&str] = &["proxy.golang.org", "goproxy.io", "goproxy.cn"];

/// A Go setting: the environment, then the `go env -w` file.
fn var(env: &Env, name: &str) -> Option<String> {
    if let Some(v) = env.get(name) {
        return Some(v);
    }
    let file = env.get("GOENV").map(PathBuf::from).or_else(|| env.home_file(".config/go/env"))?;
    read(&file)?
        .lines()
        .find_map(|l| l.strip_prefix(&format!("{name}=")).map(|v| v.trim().to_string()).filter(|v| !v.is_empty()))
}

fn glob(pat: &str, s: &str) -> bool {
    let (p, t): (Vec<char>, Vec<char>) = (pat.chars().collect(), s.chars().collect());
    fn m(p: &[char], t: &[char]) -> bool {
        match p.first() {
            None => t.is_empty(),
            Some('*') => (0..=t.len()).any(|i| m(&p[1..], &t[i..])),
            Some('?') => !t.is_empty() && m(&p[1..], &t[1..]),
            Some(c) => t.first() == Some(c) && m(&p[1..], &t[1..]),
        }
    }
    m(&p, &t)
}

/// Go's pattern rule: each comma-separated glob matches a prefix of the module
/// path made of as many elements as the glob has.
pub fn matches_patterns(patterns: &str, module: &str) -> bool {
    patterns.split(',').map(str::trim).filter(|p| !p.is_empty()).any(|p| {
        let n = p.split('/').count();
        let prefix: Vec<&str> = module.split('/').take(n).collect();
        prefix.len() == n && glob(p, &prefix.join("/"))
    })
}

fn private_module(env: &Env, name: &str) -> bool {
    ["GOPRIVATE", "GONOPROXY", "GONOSUMDB"]
        .iter()
        .any(|v| var(env, v).is_some_and(|p| matches_patterns(&p, name)))
}

/// Is this module private by the user's Go settings (process environment)?
pub fn is_private_module(name: &str) -> bool {
    direct(&Env::process(), name)
}

/// Does Go skip proxies for this module (so it is read from its git host)?
fn direct(env: &Env, name: &str) -> bool {
    ["GOPRIVATE", "GONOPROXY"]
        .iter()
        .any(|v| var(env, v).is_some_and(|p| matches_patterns(&p, name)))
}

pub fn plan(dep: &Dep, root: &Path, env: &Env) -> Plan {
    let _ = root;
    let private = private_module(env, &dep.name);
    let mut creds = Creds::default();
    let mut candidates = Vec::new();
    let proxies = var(env, "GOPROXY").unwrap_or_default();
    for p in proxies.split([',', '|']).map(str::trim) {
        let Some(u) = Url::parse(p) else { continue };
        if PUBLIC_PROXIES.contains(&u.host.as_str()) {
            continue;
        }
        let cred = match &u.userinfo {
            Some((user, Some(pw))) => Some(Credential::new(u.origin(), Auth::Basic(user.clone(), pw.clone()))),
            _ => netrc_for(env, &u),
        };
        if let Some(c) = cred {
            creds.push(c);
        }
        candidates.push(Candidate {
            base: u.clean().trim_end_matches('/').to_string(),
            private: true,
            definitive: private,
            pinned: None,
        });
    }
    if private && direct(env, &dep.name) {
        let host = dep.name.split('/').next().unwrap_or_default();
        if let Some(u) = Url::parse(&format!("https://{host}/")) {
            candidates.push(Candidate {
                base: u.origin().to_string(),
                private: true,
                definitive: true,
                pinned: Some("direct".into()),
            });
        }
    }
    if candidates.first().is_none_or(|c| !c.definitive) {
        candidates.push(Candidate {
            base: "https://proxy.golang.org".into(),
            private: false,
            definitive: false,
            pinned: None,
        });
    }
    Plan { candidates, creds }
}

/// `v1.2.3-0.20240101000000-abcdef123456` -> the commit prefix.
fn pseudo_commit(version: &str) -> Option<&str> {
    let (rest, hash) = version.rsplit_once('-')?;
    let (_, stamp) = rest.rsplit_once('-')?;
    (hash.len() == 12 && hash.bytes().all(|b| b.is_ascii_hexdigit()) && stamp.len() == 14 && stamp.bytes().all(|b| b.is_ascii_digit())).then_some(hash)
}

/// `(repository path, module subdirectory)` of a module path on a git host.
pub fn split_module(module: &str) -> Option<(String, Option<String>)> {
    let mut segs: Vec<&str> = module.split('/').skip(1).collect();
    if let Some(i) = segs.iter().position(|s| s.ends_with(".git")) {
        let repo = segs[..=i].join("/").trim_end_matches(".git").to_string();
        segs.drain(..=i);
        return Some((repo, major_trim(segs)));
    }
    if segs.len() < 2 {
        return None;
    }
    let repo = segs[..2].join("/");
    Some((repo, major_trim(segs[2..].to_vec())))
}

fn major_trim(mut segs: Vec<&str>) -> Option<String> {
    if segs
        .last()
        .is_some_and(|s| s.len() > 1 && s.starts_with('v') && s[1..].bytes().all(|b| b.is_ascii_digit()))
    {
        segs.pop();
    }
    (!segs.is_empty()).then(|| segs.join("/"))
}

/// Download from a private proxy or straight from the git host. Ok(false) when absent.
pub fn download(http: &Http, env: &Env, dep: &Dep, c: &Candidate, dir: &Path) -> Result<bool> {
    if c.pinned.as_deref() == Some("direct") {
        return download_direct(env, dep, c, dir);
    }
    let m = crate::locate::go_escape(&dep.name);
    let v = crate::locate::go_escape(&dep.version);
    let Some(bytes) = http.get_ok(&format!("{}/{m}/@v/{v}.zip", c.base.trim_end_matches('/')), None)? else {
        return Ok(false);
    };
    crate::fetch::unzip(&bytes, dir, false, Some(&format!("{}@{}/", dep.name, dep.version)))?;
    Ok(true)
}

fn download_direct(env: &Env, dep: &Dep, c: &Candidate, dir: &Path) -> Result<bool> {
    let Some(origin) = Url::parse(&c.base).map(|u| u.origin()) else {
        return Ok(false);
    };
    let Some((repo, sub)) = split_module(&dep.name) else {
        bail!("{}: cannot tell which repository this module path names", dep.name);
    };
    let host = git::Host::of(origin);
    let version = dep.version.trim_end_matches("+incompatible");
    let reference = match (pseudo_commit(version), &sub) {
        (Some(h), _) => h.to_string(),
        (None, Some(s)) => format!("{s}/{version}"),
        (None, None) => version.to_string(),
    };
    let http = Http::new(git::credentials(&host, env, true));
    let Some(bytes) = http.get_ok(&host.archive_url(&repo, &reference), None)? else {
        return Ok(false);
    };
    crate::fetch::untar_gz_sub(&bytes, dir, sub.as_deref())?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::super::testutil::*;
    use super::*;

    fn dep(name: &str, v: &str) -> Dep {
        Dep {
            eco: Eco::Go,
            name: name.into(),
            version: v.into(),
            direct: true,
            from: "go.mod".into(),
        }
    }

    #[test]
    fn patterns_follow_go_semantics() {
        assert!(matches_patterns("corp.example/*,github.com/acme", "corp.example/x/y"));
        assert!(matches_patterns("corp.example", "corp.example/x/y"));
        assert!(matches_patterns("github.com/acme", "github.com/acme/lib/v2"));
        assert!(!matches_patterns("github.com/acme", "github.com/acmeco/lib"));
        assert!(!matches_patterns("", "a/b"));
        assert!(!matches_patterns("corp.example/x/y/z", "corp.example/x"));
    }

    #[test]
    fn private_proxy_creds_and_direct_modules() {
        let home = tempfile::tempdir().unwrap();
        write(&home.path().join(".netrc"), "machine goproxy.corp.example login ci password SECRET-NETRC\n");
        let env = Env::fixed(
            &[
                ("GOPROXY", "https://goproxy.corp.example/go,https://proxy.golang.org,direct"),
                ("GOPRIVATE", "git.corp.example/*"),
            ],
            Some(home.path()),
        );
        let t = tempfile::tempdir().unwrap();
        let p = plan(&dep("git.corp.example/team/lib", "v1.2.3"), t.path(), &env);
        assert!(p.definitely_private());
        assert_eq!(p.candidates[0].base, "https://goproxy.corp.example/go");
        assert_eq!(p.candidates[1].pinned.as_deref(), Some("direct"));
        assert!(p.creds.for_url(&Url::parse("https://goproxy.corp.example/go/x").unwrap()).is_some());
        assert!(p.creds.for_url(&Url::parse("https://proxy.golang.org/x").unwrap()).is_none());
        // A public module: the private proxy is tried first (non-definitive), public follows.
        let q = plan(&dep("github.com/gin-gonic/gin", "v1.10.0"), t.path(), &env);
        assert!(q.any_private() && !q.definitely_private());
        assert!(!q.candidates.last().unwrap().private);
        // With nothing configured, everything is public.
        assert!(!plan(&dep("github.com/x/y", "v1.0.0"), t.path(), &Env::fixed(&[], None)).any_private());
    }

    #[test]
    fn go_env_file_is_read() {
        let home = tempfile::tempdir().unwrap();
        write(&home.path().join(".config/go/env"), "GOPRIVATE=git.corp.example\nGOFLAGS=-mod=mod\n");
        let env = Env::fixed(&[], Some(home.path()));
        assert!(plan(&dep("git.corp.example/a/b", "v1.0.0"), home.path(), &env).definitely_private());
    }

    #[test]
    fn module_paths_and_versions() {
        assert_eq!(split_module("git.corp.example/team/lib"), Some(("team/lib".into(), None)));
        assert_eq!(split_module("git.corp.example/team/lib/v3"), Some(("team/lib".into(), None)));
        assert_eq!(
            split_module("git.corp.example/team/lib/sub/pkg/v2"),
            Some(("team/lib".into(), Some("sub/pkg".into())))
        );
        assert_eq!(split_module("git.corp.example/g/s/lib.git/mod"), Some(("g/s/lib".into(), Some("mod".into()))));
        assert_eq!(pseudo_commit("v0.0.0-20240101123456-abcdef123456"), Some("abcdef123456"));
        assert_eq!(pseudo_commit("v1.2.3"), None);
    }

    #[test]
    fn downloads_from_a_private_proxy() {
        let zip = {
            use std::io::Write;
            let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
            let o = zip::write::SimpleFileOptions::default();
            w.start_file("corp.example/lib@v1.0.0/README.md", o).unwrap();
            w.write_all(b"# lib").unwrap();
            w.finish().unwrap().into_inner()
        };
        let m = serve(move |p, auth| match (p, auth) {
            ("/go/corp.example/lib/@v/v1.0.0.zip", Some("Basic Y2k6U0VDUkVULVBX")) => (200, "application/zip".into(), zip.clone(), vec![]),
            _ => (401, "text/plain".into(), vec![], vec![]),
        });
        let home = tempfile::tempdir().unwrap();
        write(&home.path().join(".netrc"), "machine 127.0.0.1 login ci password SECRET-PW\n");
        let env = Env::fixed(&[("GOPROXY", &format!("{}/go", m.base)), ("GOPRIVATE", "corp.example")], Some(home.path()));
        let t = tempfile::tempdir().unwrap();
        let d = dep("corp.example/lib", "v1.0.0");
        let p = plan(&d, t.path(), &env);
        let out = tempfile::tempdir().unwrap();
        assert!(download(&Http::new(p.creds), &env, &d, &p.candidates[0], out.path()).unwrap());
        assert_eq!(std::fs::read_to_string(out.path().join("README.md")).unwrap(), "# lib");
    }
}
