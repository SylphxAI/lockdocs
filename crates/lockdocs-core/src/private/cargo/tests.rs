use super::super::testutil::{serve, tgz, write};
use super::super::{Creds, Env, Http, Url};
use super::{dl_url, download, plan};
use crate::{Dep, Eco};

fn dep(name: &str, v: &str) -> Dep {
    Dep {
        eco: Eco::Cargo,
        name: name.into(),
        version: v.into(),
        direct: true,
        from: "Cargo.lock".into(),
    }
}

#[test]
fn lock_source_config_and_token() {
    let t = tempfile::tempdir().unwrap();
    write(&t.path().join("Cargo.lock"), "version = 4\n[[package]]\nname = \"acme-core\"\nversion = \"0.3.0\"\nsource = \"sparse+https://cargo.acme.example/index/\"\n[[package]]\nname = \"serde\"\nversion = \"1.0.0\"\nsource = \"registry+https://github.com/rust-lang/crates.io-index\"\n");
    write(
        &t.path().join(".cargo/config.toml"),
        "[registries.acme]\nindex = \"sparse+https://cargo.acme.example/index/\"\n",
    );
    let home = tempfile::tempdir().unwrap();
    write(&home.path().join(".cargo/credentials.toml"), "[registries.acme]\ntoken = \"SECRET-CRED\"\n");
    let env = Env::fixed(&[], Some(home.path()));
    let p = plan(&dep("acme-core", "0.3.0"), t.path(), &env);
    assert!(p.definitely_private());
    assert_eq!(p.candidates[0].base, "https://cargo.acme.example/index");
    let auth = p.creds.for_url(&Url::parse("https://cargo.acme.example/index/config.json").unwrap()).unwrap();
    assert_eq!(auth.header(), "SECRET-CRED");
    assert!(p.creds.for_url(&Url::parse("https://index.crates.io/config.json").unwrap()).is_none());
    assert!(!plan(&dep("serde", "1.0.0"), t.path(), &env).any_private());
    // The environment token wins over the credentials file.
    let env2 = Env::fixed(&[("CARGO_REGISTRIES_ACME_TOKEN", "SECRET-ENV")], Some(home.path()));
    let p2 = plan(&dep("acme-core", "0.3.0"), t.path(), &env2);
    assert_eq!(
        p2.creds.for_url(&Url::parse("https://cargo.acme.example/index/x").unwrap()).unwrap().header(),
        "SECRET-ENV"
    );
}

#[test]
fn manifest_registry_key_without_lock() {
    let t = tempfile::tempdir().unwrap();
    write(
        &t.path().join("Cargo.toml"),
        "[package]\nname=\"x\"\n[dependencies]\nacme-core = { version = \"0.3\", registry = \"acme\" }\n",
    );
    write(
        &t.path().join(".cargo/config.toml"),
        "[registries.acme]\nindex = \"sparse+https://cargo.acme.example/index/\"\n",
    );
    let p = plan(&dep("acme-core", "0.3.0"), t.path(), &Env::fixed(&[], None));
    assert!(p.definitely_private());
}

#[test]
fn git_index_registry_is_refused_clearly() {
    let t = tempfile::tempdir().unwrap();
    write(
        &t.path().join("Cargo.lock"),
        "version = 4\n[[package]]\nname = \"a\"\nversion = \"1.0.0\"\nsource = \"registry+https://git.acme.example/index\"\n",
    );
    let p = plan(&dep("a", "1.0.0"), t.path(), &Env::fixed(&[], None));
    let e = download(&Http::new(Creds::default()), &dep("a", "1.0.0"), &p.candidates[0], t.path())
        .unwrap_err()
        .to_string();
    assert!(e.contains("sparse"), "{e}");
}

#[test]
fn dl_templates() {
    assert_eq!(
        dl_url("https://d.example/api", "acme", "1.0.0").unwrap(),
        "https://d.example/api/acme/1.0.0/download"
    );
    assert_eq!(
        dl_url("https://d.example/{prefix}/{crate}-{version}.crate", "acme", "1.0.0").unwrap(),
        "https://d.example/ac/me/acme-1.0.0.crate"
    );
    assert_eq!(
        dl_url("https://d.example/{lowerprefix}/{crate}", "ab", "1.0.0").unwrap(),
        "https://d.example/2/ab"
    );
    assert!(dl_url("https://d.example/{sha256-checksum}", "ab", "1").is_none());
}

#[test]
fn downloads_with_the_raw_token() {
    let krate = tgz(&[("acme-core-0.3.0/README.md", b"# acme"), ("acme-core-0.3.0/Cargo.toml", b"[package]")]);
    let m = serve(move |p, auth| {
        if auth != Some("SECRET-TOK") {
            return (401, "text/plain".into(), vec![], vec![]);
        }
        match p {
            "/index/config.json" => (200, "application/json".into(), br#"{"dl":"/dl/{crate}/{version}.crate"}"#.to_vec(), vec![]),
            "/dl/acme-core/0.3.0.crate" => (200, "application/gzip".into(), krate.clone(), vec![]),
            _ => (404, "text/plain".into(), vec![], vec![]),
        }
    });
    let t = tempfile::tempdir().unwrap();
    write(
        &t.path().join("Cargo.lock"),
        &format!(
            "version = 4\n[[package]]\nname = \"acme-core\"\nversion = \"0.3.0\"\nsource = \"sparse+{}/index/\"\n",
            m.base
        ),
    );
    let env = Env::fixed(&[("CARGO_REGISTRIES_TEST_TOKEN", "SECRET-TOK")], None);
    write(
        &t.path().join(".cargo/config.toml"),
        &format!("[registries.test]\nindex = \"sparse+{}/index/\"\n", m.base),
    );
    let p = plan(&dep("acme-core", "0.3.0"), t.path(), &env);
    let out = tempfile::tempdir().unwrap();
    assert!(download(&Http::new(p.creds), &dep("acme-core", "0.3.0"), &p.candidates[0], out.path()).unwrap());
    assert!(out.path().join("README.md").is_file());
}
