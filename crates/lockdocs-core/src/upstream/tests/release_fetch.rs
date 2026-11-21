/// Exercise GitLab's real smart-HTTP and archive endpoints without credentials.
#[test]
#[ignore = "requires network access to gitlab.com"]
fn real_gitlab_release_docs() {
    let origin = crate::private::Url::parse("https://gitlab.com/").unwrap().origin();
    let host = crate::private::git::Host::of(origin.clone());
    let client = agent(false, Some((Some(host), crate::private::Creds::default())));
    let repo = Repo {
        owner: "gitlab-org".into(),
        name: "cli".into(),
        subdir: None,
        host: Some(origin),
    };
    let out = tempfile::tempdir().unwrap();
    let manifest = build(&client, &dep("glab", "1.45.0"), &repo, false, out.path()).unwrap();
    assert_eq!(manifest.tag.as_deref(), Some("v1.45.0"));
    assert!(manifest.commit.as_ref().is_some_and(|c| c.len() == 40));
    assert!(manifest.files > 0);
    assert!(std::fs::read_to_string(out.path().join("README.md")).unwrap().contains("GitLab"));
    println!(
        "GitLab readback: {} tag={} commit={} files={} bytes={}",
        manifest.repo,
        manifest.tag.unwrap(),
        manifest.commit.unwrap(),
        manifest.files,
        manifest.bytes
    );
}

#[test]
fn fetch_with_rest_api_blocked_succeeds_through_codeload() {
    let f = fake(Codeload::Ok, true, Some("v1.2.3"));
    let (m, out) = run_build(&f, MAX_ARCHIVE);
    let m = m.unwrap();
    assert_eq!(
        (m.tag.as_deref(), m.commit.as_deref(), m.via.as_deref()),
        (Some("v1.2.3"), Some(SHA), Some("codeload"))
    );
    assert_eq!(m.files, 2);
    assert_eq!(std::fs::read_to_string(out.path().join("docs/guide.md")).unwrap(), "# guide");
    assert!(!out.path().join("src").exists());
    // Neither the REST API nor raw file requests were made.
    assert_eq!(requests(&f, "/repos/") + requests(&f, "/raw/"), 0);
    assert_eq!(requests(&f, &format!("/o/r/tar.gz/{SHA}")), 1);
}

#[test]
fn over_cap_archive_falls_back_to_per_file_for_that_repository() {
    let f = fake(Codeload::Ok, false, Some("v1.2.3"));
    let (m, out) = run_build(&f, 64);
    let m = m.unwrap();
    assert_eq!(m.via.as_deref(), Some("rest"));
    assert!(m.files >= 2 && out.path().join("README.md").is_file() && out.path().join("docs/guide.md").is_file());
    assert!(requests(&f, "/repos/") > 0 && requests(&f, "/raw/") > 0);
}

#[test]
fn codeload_failure_falls_back_and_both_failing_reports_both() {
    let f = fake(Codeload::Fail, false, Some("v1.2.3"));
    assert_eq!(run_build(&f, MAX_ARCHIVE).0.unwrap().via.as_deref(), Some("rest"));
    let f = fake(Codeload::Fail, true, Some("v1.2.3"));
    let e = format!("{:#}", run_build(&f, MAX_ARCHIVE).0.unwrap_err());
    assert!(e.contains("codeload") && e.contains("fallback failed"), "{e}");
}

#[test]
fn missing_tag_is_a_clear_note_and_downloads_nothing() {
    let f = fake(Codeload::Ok, true, None);
    let m = run_build(&f, MAX_ARCHIVE).0.unwrap();
    let note = m.note.unwrap();
    assert!(
        note.contains("no git tag found for 1.2.3") && note.contains("v1.2.3") && note.contains("1.2.3,"),
        "{note}"
    );
    assert_eq!((m.tag, m.files), (None, 0));
    assert_eq!(requests(&f, "/o/r/tar.gz/") + requests(&f, "/repos/"), 0);
}

#[test]
fn slow_codeload_under_a_short_deadline_falls_back_to_rest() {
    let f = fake(Codeload::Slow, false, Some("v1.2.3"));
    let mut c = client(&f, MAX_ARCHIVE);
    // 17 s left: the stream gets 2 s, the rest of the budget serves the fallback.
    c.deadline = Some(std::time::Instant::now() + std::time::Duration::from_secs(17));
    let out = tempfile::tempdir().unwrap();
    let m = build(&c, &dep("pkg", "1.2.3"), &repo(None), true, out.path()).unwrap();
    assert_eq!(m.via.as_deref(), Some("rest"));
}

#[test]
fn git_origins_are_rejected_before_repository_or_tag_requests() {
    let source = Source {
        dir: PathBuf::new(),
        files: None,
        metadata: None,
        version: "1.2.3".into(),
        label: "checkout".into(),
        fetched: false,
    };
    for (eco, from) in [(Eco::Cargo, "Cargo.lock (git)"), (Eco::PyPI, "uv.lock (git)")] {
        let dep = Dep {
            eco,
            name: "git-fixture".into(),
            version: "1.2.3".into(),
            direct: true,
            from: from.into(),
        };
        for result in [
            fetch(&dep, &source),
            fetch_automatic(&dep, &source),
            crate::fetch::fetch(&dep).map(|_| manifest()),
        ] {
            assert!(result.unwrap_err().to_string().contains("git dependency"));
        }
    }
}
