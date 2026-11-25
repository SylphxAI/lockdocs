use super::super::{Auth, Env, Url};
use super::{credential_fill, credentials, git_header, parse_repo_url, Host};

fn host(u: &str) -> Host {
    Host::of(Url::parse(u).unwrap().origin())
}

#[test]
fn repo_urls_on_other_hosts() {
    let p = |u: &str| parse_repo_url(u).map(|(o, p)| (o.to_string(), p));
    assert_eq!(
        p("git+https://git.acme.example/team/lib.git"),
        Some(("https://git.acme.example".into(), "team/lib".into()))
    );
    assert_eq!(
        p("https://gitlab.acme.example/grp/sub/lib"),
        Some(("https://gitlab.acme.example".into(), "grp/sub/lib".into()))
    );
    assert_eq!(p("git@bitbucket.org:ws/repo.git"), Some(("https://bitbucket.org".into(), "ws/repo".into())));
    assert_eq!(p("https://github.com/o/r"), None);
    // A docs site is not a repository.
    assert_eq!(p("https://foo.readthedocs.io/en/latest"), None);
    assert_eq!(p("https://example.com/o/r"), None);
}

#[test]
fn env_tokens_belong_to_their_own_host() {
    let e = Env::fixed(
        &[
            ("GH_TOKEN", "SECRET-GH"),
            ("GITLAB_TOKEN", "SECRET-GL"),
            ("GITLAB_HOST", "https://gitlab.acme.example"),
        ],
        None,
    );
    let gh = credentials(&host("https://github.com/"), &e, false);
    assert!(gh.for_url(&Url::parse("https://codeload.github.com/o/r/tar.gz/x").unwrap()).is_some());
    assert!(gh.for_url(&Url::parse("https://git.acme.example/o/r").unwrap()).is_none());
    // GH_TOKEN is never offered to another GitHub-style host.
    assert!(credentials(&host("https://github.acme.example/"), &e, false).0.is_empty());
    let gl = credentials(&host("https://gitlab.acme.example/"), &e, false);
    assert_eq!(
        gl.for_url(&Url::parse("https://gitlab.acme.example/api/v4/x").unwrap()).unwrap().header(),
        "Bearer SECRET-GL"
    );
    assert!(credentials(&host("https://gitlab.other.example/"), &e, false).0.is_empty());
    assert!(!format!("{gh:?}{gl:?}").contains("SECRET"));
}

#[test]
fn git_endpoint_uses_basic_with_the_host_user() {
    let h = host("https://gitlab.acme.example/");
    assert_eq!(git_header(&h, &Auth::Bearer("t".into())), Auth::Basic("oauth2".into(), "t".into()).header());
    assert_eq!(
        git_header(&h, &Auth::Basic("u".into(), "p".into())),
        Auth::Basic("u".into(), "p".into()).header()
    );
}

#[test]
fn archive_urls() {
    assert_eq!(
        host("https://github.com/").archive_url("o/r", "v1.0.0"),
        "https://api.github.com/repos/o/r/tarball/v1.0.0"
    );
    assert_eq!(
        host("https://gitlab.x.example/").archive_url("g/s/r", "v1"),
        "https://gitlab.x.example/api/v4/projects/g%2Fs%2Fr/repository/archive.tar.gz?sha=v1"
    );
    assert_eq!(
        host("https://bitbucket.org/").archive_url("w/r", "v1"),
        "https://bitbucket.org/w/r/get/v1.tar.gz"
    );
    assert_eq!(
        host("https://git.x.example/").archive_url("o/r", "v1"),
        "https://git.x.example/api/v3/repos/o/r/tarball/v1"
    );
}

#[test]
fn credential_fill_reads_the_helper_for_that_host() {
    if std::process::Command::new("git").arg("--version").output().is_err() {
        return;
    }
    let o = Url::parse("https://git.acme.example/").unwrap().origin();
    let got = credential_fill(
        &o,
        &[
            ("GIT_CONFIG_GLOBAL", "/dev/null"),
            ("GIT_CONFIG_SYSTEM", "/dev/null"),
            ("GIT_CONFIG_COUNT", "1"),
            ("GIT_CONFIG_KEY_0", "credential.helper"),
            ("GIT_CONFIG_VALUE_0", "!f() { echo username=me; echo password=SECRET-FILL; }; f"),
        ],
    );
    assert_eq!(got, Some(("me".into(), "SECRET-FILL".into())));
}
