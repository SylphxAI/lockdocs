use super::super::testutil::{serve, tgz, write};
use super::super::{Auth, Url};
use super::{download, plan, Env, Http};
use crate::{Dep, Eco};

fn dep(name: &str, v: &str) -> Dep {
    Dep {
        eco: Eco::Npm,
        name: name.into(),
        version: v.into(),
        direct: true,
        from: "package-lock.json".into(),
    }
}

#[test]
fn scoped_registry_and_token_from_project_npmrc() {
    let t = tempfile::tempdir().unwrap();
    write(
        &t.path().join(".npmrc"),
        "@acme:registry=https://npm.acme.example/repo/\n//npm.acme.example/repo/:_authToken=${ACME_TOKEN}\n//other.example/:_authToken=OTHER-SECRET\n",
    );
    let env = Env::fixed(&[("ACME_TOKEN", "SECRET-ACME")], None);
    let p = plan(&dep("@acme/ui", "1.0.0"), t.path(), &env);
    assert!(p.definitely_private());
    assert_eq!(p.candidates[0].base, "https://npm.acme.example/repo");
    let u = Url::parse("https://npm.acme.example/repo/@acme%2Fui").unwrap();
    assert_eq!(p.creds.for_url(&u).unwrap().header(), "Bearer SECRET-ACME");
    // The other host's token never applies to the acme host, nor the reverse.
    assert!(p.creds.for_url(&Url::parse("https://npm.acme.example/other").unwrap()).is_none());
    assert_eq!(
        p.creds.for_url(&Url::parse("https://other.example/x").unwrap()).unwrap().header(),
        "Bearer OTHER-SECRET"
    );
    assert!(p.creds.for_url(&Url::parse("https://registry.npmjs.org/x").unwrap()).is_none());
    // An unscoped package stays public.
    let q = plan(&dep("left-pad", "1.0.0"), t.path(), &env);
    assert!(!q.any_private());
}

#[test]
fn user_file_env_and_basic_auth() {
    let home = tempfile::tempdir().unwrap();
    let proj = tempfile::tempdir().unwrap();
    write(
        &home.path().join(".npmrc"),
        "registry=https://mirror.example/npm\n_auth=ignored\n//mirror.example/npm/:_auth=dXNlcjpwdw==\n",
    );
    let env = Env::fixed(&[("NPM_CONFIG_@x:registry", "https://x.example/")], Some(home.path()));
    let p = plan(&dep("left-pad", "1.0.0"), proj.path(), &env);
    // A default-registry mirror is private but not definitive: public stays the fallback.
    assert!(p.any_private() && !p.definitely_private());
    assert_eq!(p.candidates.len(), 2);
    let u = Url::parse("https://mirror.example/npm/left-pad").unwrap();
    assert_eq!(p.creds.for_url(&u).unwrap().header(), Auth::Basic("user".into(), "pw".into()).header());
    let q = plan(&dep("@x/y", "1.0.0"), proj.path(), &env);
    assert_eq!(q.candidates[0].base, "https://x.example");
}

#[test]
fn lockfile_resolved_url_wins() {
    let t = tempfile::tempdir().unwrap();
    write(
        &t.path().join("package-lock.json"),
        r#"{"lockfileVersion":3,"packages":{"node_modules/@acme/ui":{"version":"1.2.3","resolved":"https://art.acme.example/api/npm/@acme/ui/-/ui-1.2.3.tgz"}}}"#,
    );
    let p = plan(&dep("@acme/ui", "1.2.3"), t.path(), &Env::fixed(&[], None));
    assert!(p.definitely_private());
    assert_eq!(
        p.candidates[0].pinned.as_deref(),
        Some("https://art.acme.example/api/npm/@acme/ui/-/ui-1.2.3.tgz")
    );
    // A different version has no pin.
    assert!(!plan(&dep("@acme/ui", "9.9.9"), t.path(), &Env::fixed(&[], None)).any_private());
}

#[test]
fn downloads_with_the_token_only_from_its_host() {
    let tarball = tgz(&[("package/README.md", b"# private"), ("package/package.json", b"{}")]);
    let other = serve(move |_, _| (200, "application/gzip".into(), tarball.clone(), vec![]));
    let other_base = other.base.clone();
    let reg = serve(move |p, auth| match (p, auth) {
        ("/ui/1.0.0", Some("Bearer SECRET-REG")) => (
            200,
            "application/json".into(),
            format!(r#"{{"dist":{{"tarball":"{other_base}/ui.tgz"}}}}"#).into_bytes(),
            vec![],
        ),
        _ => (401, "text/plain".into(), vec![], vec![]),
    });
    let t = tempfile::tempdir().unwrap();
    write(
        &t.path().join(".npmrc"),
        &format!("registry={}\n//{}/:_authToken=SECRET-REG\n", reg.base, reg.host),
    );
    let p = plan(&dep("ui", "1.0.0"), t.path(), &Env::fixed(&[], None));
    let out = tempfile::tempdir().unwrap();
    assert!(download(&Http::new(p.creds), &dep("ui", "1.0.0"), &p.candidates[0], out.path()).unwrap());
    assert!(out.path().join("README.md").is_file());
    // The tarball host is a different origin: it got no Authorization.
    let seen = other.seen.lock().unwrap().clone();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].1, None);
}
