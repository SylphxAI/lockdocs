use super::config::pip_conf;
use super::{download, parse_simple, pick, plan};
use crate::private::testutil::{serve, tgz, write, zip};
use crate::private::{Auth, Env, Http, Url};
use crate::{Dep, Eco};

fn dep(name: &str, v: &str) -> Dep {
    Dep {
        eco: Eco::PyPI,
        name: name.into(),
        version: v.into(),
        direct: true,
        from: "uv.lock".into(),
    }
}

#[test]
fn pip_conf_and_env_indexes() {
    let (m, e) = pip_conf("[global]\nindex-url = https://pypi.org/simple\nextra-index-url =\n  https://a.example/simple\n  https://b.example/simple\n[other]\nindex-url=https://no.example\n");
    assert_eq!(m.as_deref(), Some("https://pypi.org/simple"));
    assert_eq!(e, ["https://a.example/simple", "https://b.example/simple"]);
    let t = tempfile::tempdir().unwrap();
    let env = Env::fixed(&[("PIP_EXTRA_INDEX_URL", "https://__token__:SECRET-PIP@priv.example/simple")], None);
    let p = plan(&dep("acme-lib", "1.0.0"), t.path(), &env);
    assert!(p.any_private() && !p.definitely_private());
    assert_eq!(p.candidates[0].base, "https://priv.example/simple");
    assert!(!format!("{p:?}").contains("SECRET-PIP"));
    let auth = p.creds.for_url(&Url::parse("https://priv.example/simple/acme-lib/").unwrap()).unwrap();
    assert_eq!(auth.header(), Auth::Basic("__token__".into(), "SECRET-PIP".into()).header());
    assert!(p.creds.for_url(&Url::parse("https://pypi.org/simple/x/").unwrap()).is_none());
}

#[test]
fn uv_index_named_credentials_sources_and_lock() {
    let t = tempfile::tempdir().unwrap();
    write(
        &t.path().join("pyproject.toml"),
        "[tool.uv.sources]\nacme-lib = { index = \"acme\" }\n[[tool.uv.index]]\nname = \"acme\"\nurl = \"https://py.acme.example/simple\"\nexplicit = true\n",
    );
    let env = Env::fixed(&[("UV_INDEX_ACME_USERNAME", "ci"), ("UV_INDEX_ACME_PASSWORD", "SECRET-UV")], None);
    let p = plan(&dep("acme_lib", "2.0.0"), t.path(), &env);
    assert!(p.definitely_private());
    assert_eq!(
        p.creds.for_url(&Url::parse("https://py.acme.example/simple/x/").unwrap()).unwrap().header(),
        Auth::Basic("ci".into(), "SECRET-UV".into()).header()
    );
    // An explicit index serves only the packages pinned to it.
    assert!(!plan(&dep("requests", "2.0.0"), t.path(), &env).any_private());

    let t2 = tempfile::tempdir().unwrap();
    write(
        &t2.path().join("uv.lock"),
        "version = 1\n[[package]]\nname = \"acme-lib\"\nversion = \"2.0.0\"\nsource = { registry = \"https://lock.example/simple\" }\n[[package]]\nname = \"requests\"\nversion = \"2.0.0\"\nsource = { registry = \"https://pypi.org/simple\" }\n",
    );
    let p = plan(&dep("acme-lib", "2.0.0"), t2.path(), &Env::fixed(&[], None));
    assert!(p.definitely_private() && p.candidates[0].base == "https://lock.example/simple");
    assert!(!plan(&dep("requests", "2.0.0"), t2.path(), &Env::fixed(&[], None)).any_private());
}

#[test]
fn netrc_credential_for_the_index_host() {
    let home = tempfile::tempdir().unwrap();
    write(&home.path().join(".netrc"), "machine priv.example login me password SECRET-NETRC\n");
    let t = tempfile::tempdir().unwrap();
    let env = Env::fixed(&[("PIP_INDEX_URL", "https://priv.example/simple")], Some(home.path()));
    let p = plan(&dep("acme-lib", "1.0.0"), t.path(), &env);
    assert!(p.creds.for_url(&Url::parse("https://priv.example/simple/a/").unwrap()).is_some());
    assert!(p.creds.for_url(&Url::parse("https://pypi.org/simple/a/").unwrap()).is_none());
}

#[test]
fn simple_pages_html_and_json() {
    let html = br#"<a href="../../files/acme_lib-1.0.0-py3-none-any.whl#sha256=ab">acme_lib-1.0.0-py3-none-any.whl</a><a href="acme-lib-1.0.0.tar.gz">acme-lib-1.0.0.tar.gz</a>"#;
    let f = parse_simple(html, "https://p.example/simple/acme-lib/");
    assert_eq!(
        pick(&f, "acme-lib", "1.0.0").unwrap(),
        (
            "acme_lib-1.0.0-py3-none-any.whl".to_string(),
            "https://p.example/files/acme_lib-1.0.0-py3-none-any.whl".to_string()
        )
    );
    assert!(pick(&f, "acme-lib", "2.0.0").is_none());
    let json = br#"{"files":[{"filename":"acme-lib-1.0.0.tar.gz","url":"https://f.example/a.tar.gz"}]}"#;
    assert_eq!(
        pick(&parse_simple(json, "https://p.example/simple/acme-lib/"), "acme-lib", "1.0.0").unwrap(),
        ("acme-lib-1.0.0.tar.gz".to_string(), "https://f.example/a.tar.gz".to_string())
    );
}

#[test]
fn public_registry_recorded_in_uv_lock_beats_a_private_index() {
    // uv.lock pins the public index, but a private index is also configured
    // and publishes the same name@version. The lockfile must win: otherwise
    // the private artifact shadows the one the project actually resolved.
    let t = tempfile::tempdir().unwrap();
    write(
        &t.path().join("uv.lock"),
        "version = 1\n[[package]]\nname = \"acme-lib\"\nversion = \"2.0.0\"\nsource = { registry = \"https://pypi.org/simple\" }\n",
    );
    let env = Env::fixed(&[("PIP_EXTRA_INDEX_URL", "https://priv.example/simple")], None);
    let p = plan(&dep("acme-lib", "2.0.0"), t.path(), &env);
    assert!(!p.any_private(), "the locked public source must not be shadowed: {p:?}");
    assert_eq!(p.candidates[0].base, "https://pypi.org/simple");
}

#[test]
fn advertised_filename_drives_extraction_not_the_url() {
    // A wheel served from a signed URL with a query string, and one from an
    // opaque endpoint with no extension at all. The advertised filename is
    // the only reliable format signal; the URL must not decide extraction.
    let wheel = zip(&[
        ("acme_lib/__init__.py", b"x = 1"),
        ("acme_lib-1.0.0.dist-info/METADATA", b"Metadata-Version: 2.1\nName: acme-lib\nVersion: 1.0.0\n"),
    ]);
    let body = wheel.clone();
    let m = serve(move |p, _| {
        if p.starts_with("/simple/acme-lib/") {
            (
                200,
                "application/vnd.pypi.simple.v1+json".into(),
                br#"{"files":[{"filename":"acme_lib-1.0.0-py3-none-any.whl","url":"../../artifact/acme.whl?X-Amz-Signature=abc"}]}"#.to_vec(),
                vec![],
            )
        } else if p.starts_with("/artifact/acme.whl") {
            (200, "application/octet-stream".into(), body.clone(), vec![])
        } else {
            (404, "text/plain".into(), vec![], vec![])
        }
    });
    let t = tempfile::tempdir().unwrap();
    let env = Env::fixed(&[("PIP_EXTRA_INDEX_URL", &format!("{}/simple", m.base))], None);
    let p = plan(&dep("acme-lib", "1.0.0"), t.path(), &env);
    let out = tempfile::tempdir().unwrap();
    assert!(download(&Http::new(p.creds), &dep("acme-lib", "1.0.0"), &p.candidates[0], out.path()).unwrap());
    assert!(out.path().join("acme_lib/__init__.py").is_file(), "wheel not unpacked from a signed URL");

    // The opaque endpoint: no extension on the URL, the filename says wheel.
    let wheel2 = zip(&[("acme_pkg/__init__.py", b"y = 2")]);
    let body2 = wheel2.clone();
    let m2 = serve(move |p, _| {
        if p.starts_with("/simple/acme-pkg/") {
            (
                200,
                "application/vnd.pypi.simple.v1+json".into(),
                br#"{"files":[{"filename":"acme_pkg-1.0.0-py3-none-any.whl","url":"../../download/9f3a2b"}]}"#.to_vec(),
                vec![],
            )
        } else if p == "/download/9f3a2b" {
            (200, "application/octet-stream".into(), body2.clone(), vec![])
        } else {
            (404, "text/plain".into(), vec![], vec![])
        }
    });
    let env2 = Env::fixed(&[("PIP_EXTRA_INDEX_URL", &format!("{}/simple", m2.base))], None);
    let p2 = plan(&dep("acme-pkg", "1.0.0"), t.path(), &env2);
    let out2 = tempfile::tempdir().unwrap();
    assert!(download(&Http::new(p2.creds), &dep("acme-pkg", "1.0.0"), &p2.candidates[0], out2.path()).unwrap());
    assert!(out2.path().join("acme_pkg/__init__.py").is_file(), "wheel not unpacked from an opaque URL");
}

#[test]
fn downloads_from_a_private_index_with_basic_auth() {
    let tar = tgz(&[("acme-lib-1.0.0/README.md", b"# acme"), ("acme-lib-1.0.0/acme.py", b"x = 1")]);
    let m = serve(move |p, auth| {
        if auth != Some("Basic dXNlcjpTRUNSRVQtSURY") {
            return (401, "text/plain".into(), vec![], vec![]);
        }
        match p {
            "/simple/acme-lib/" => (
                200,
                "text/html".into(),
                br#"<a href="/files/acme-lib-1.0.0.tar.gz#sha256=00">acme-lib-1.0.0.tar.gz</a>"#.to_vec(),
                vec![],
            ),
            "/files/acme-lib-1.0.0.tar.gz" => (200, "application/gzip".into(), tar.clone(), vec![]),
            _ => (404, "text/plain".into(), vec![], vec![]),
        }
    });
    let t = tempfile::tempdir().unwrap();
    let url = format!("http://user:SECRET-IDX@{}/simple", m.host);
    let env = Env::fixed(&[("PIP_EXTRA_INDEX_URL", &url)], None);
    let p = plan(&dep("acme-lib", "1.0.0"), t.path(), &env);
    let out = tempfile::tempdir().unwrap();
    assert!(download(&Http::new(p.creds), &dep("acme-lib", "1.0.0"), &p.candidates[0], out.path()).unwrap());
    assert_eq!(std::fs::read_to_string(out.path().join("README.md")).unwrap(), "# acme");
}
