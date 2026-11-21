#[test]
fn unavailable_repository_keeps_docs_site_files() {
    let f = fake(Codeload::PrivateWithSite, false, Some("v1.2.3"));
    let out = tempfile::tempdir().unwrap();
    let m = build(&client(&f, MAX_ARCHIVE), &dep("react", "19.0.0"), &repo(None), false, out.path()).unwrap();
    assert!(m.files >= 1 && m.site.is_some(), "{m:?}");
    assert_eq!(std::fs::read_to_string(out.path().join("react.dev/src/content/learn/a.md")).unwrap(), "# learn");
}

#[test]
fn automatic_build_includes_the_docs_site_but_stays_unchecked() {
    let f = fake(Codeload::PrivateWithSite, false, Some("v1.2.3"));
    let out = tempfile::tempdir().unwrap();
    let m = build(&client(&f, MAX_ARCHIVE), &dep("react", "19.0.0"), &repo(None), true, out.path()).unwrap();
    assert!(m.files >= 1 && m.site.is_some(), "{m:?}");
    // Explicit `lockdocs fetch` still completes what the automatic budget cannot.
    assert!(!m.docs_sites_checked);
    assert!(out.path().join("react.dev/src/content/learn/a.md").exists());
}

#[test]
fn automatic_docs_site_failure_keeps_the_package_docs() {
    let f = fake(Codeload::Ok, false, Some("v1.2.3"));
    let out = tempfile::tempdir().unwrap();
    // The fake serves no site repository, so the lookup fails; the package docs stay.
    let m = build(&client(&f, MAX_ARCHIVE), &dep("react", "1.2.3"), &repo(None), true, out.path()).unwrap();
    assert_eq!(m.files, 2, "{m:?}");
    assert!(m.site.is_none());
}

#[test]
fn partial_legacy_fallback_failure_removes_site_but_preserves_package_docs() {
    for major in [16, 17, 18] {
        let version = format!("{major}.0.0");
        let f = fake(Codeload::PartialLegacy, false, Some(&format!("v{version}")));
        let out = tempfile::tempdir().unwrap();
        let m = build(&client(&f, MAX_ARCHIVE), &dep("react", &version), &repo(None), true, out.path()).unwrap();
        assert_eq!(m.files, 2, "{m:?}");
        assert!(m.site.is_none() && m.pages.is_empty() && !m.docs_sites_checked, "{m:?}");
        assert!(m.note.as_deref().unwrap_or("").contains("docs site skipped"), "{m:?}");
        assert!(out.path().join("README.md").exists());
        assert!(out.path().join("docs/guide.md").exists());
        assert_eq!(requests(&f, "/raw/reactjs/legacy.reactjs.org/"), 2);
        assert!(!out.path().join("legacy.reactjs.org").exists());
    }
}

#[test]
fn unavailable_repository_without_docs_site_is_a_clear_error() {
    let f = fake(Codeload::Private, false, Some("v1.2.3"));
    let out = tempfile::tempdir().unwrap();
    let e = format!(
        "{:#}",
        build(&client(&f, MAX_ARCHIVE), &dep("react", "19.0.0"), &repo(None), false, out.path()).unwrap_err()
    );
    assert!(e.contains("private repository: not supported"), "{e}");
}

#[test]
fn older_react_majors_use_the_official_archives() {
    for name in ["react", "react-dom", "@types/react"] {
        for (major, branch) in [(16, "16.reactjs.org"), (17, "17.reactjs.org"), (18, "main")] {
            let f = fake(Codeload::Ok, false, None);
            let out = tempfile::tempdir().unwrap();
            let got = site_docs(&client(&f, MAX_ARCHIVE), &dep(name, &format!("{major}.0.0")), None, out.path()).unwrap();
            let got = got.expect("older React docs must be available");
            assert_eq!(got.files, 1);
            assert!(got.label.contains(&format!("legacy.reactjs.org@{branch}")));
            assert!(got.label.contains(&format!("official archive for major {major}")));
            assert!(out.path().join("legacy.reactjs.org/content/docs/hooks-reference.md").exists());
            assert!(!out.path().join("legacy.reactjs.org/content/blog/new-api.md").exists());
            assert_eq!(requests(&f, "/reactjs/react.dev/tar.gz/"), 0);
        }
    }
}

#[test]
fn docs_site_files_follow_the_major_rules() {
    let (docs, pages): (Vec<String>, Vec<String>) = (vec!["src/content/docs/en".into()], vec!["src/app/installation".into()]);
    let versioned = vec!["src/content/api/4x".to_string()];
    let k = |p: &str, ok: bool| site_kind(p, 4, &versioned, ok, &docs, &pages);
    assert_eq!(k("src/content/docs/en/a.md", false), Some(false));
    assert_eq!(k("src/content/docs/en/a.txt", false), None);
    assert_eq!(k("src/content/docs/en/v5-beta.md", false), None);
    assert_eq!(k("src/content/docs/en/v5/migration.md", false), None);
    assert_eq!(k("src/app/installation/v5/page.tsx", false), None);
    assert_eq!(k("src/content/api/4x/req.md", true), Some(false));
    assert_eq!(k("src/content/api/4x/req.md", false), None);
    assert_eq!(k("src/app/installation/page.tsx", false), Some(true));
    assert_eq!(k("src/app/installation/layout.tsx", false), None);
    assert_eq!(k("README.md", true), None);
}

#[test]
fn all_five_docs_sites_exclude_later_major_pages() {
    for (site, major) in DOCS_SITES.iter().zip([18, 4, 3, 5, 1]) {
        let docs: Vec<String> = site.current.iter().map(|p| p.to_string()).collect();
        let pages: Vec<String> = site.pages.iter().map(|p| p.to_string()).collect();
        let versioned: Vec<String> = site.versioned.iter().map(|p| p.replace("{major}", &major.to_string())).collect();
        let k = |p: &str| site_kind(p, major, &versioned, true, &docs, &pages);
        for dir in &docs {
            assert_eq!(k(&format!("{dir}/guide.md")), Some(false), "{}", site.repo.1);
            assert_eq!(k(&format!("{dir}/v{}/guide.mdx", major + 1)), None, "{}", site.repo.1);
            assert_eq!(k(&format!("{dir}/v{major}/guide.md")), Some(false), "{}", site.repo.1);
        }
        for dir in &pages {
            assert_eq!(k(&format!("{dir}/page.tsx")), Some(true));
            assert_eq!(k(&format!("{dir}/v{}/page.tsx", major + 1)), None);
        }
        if !versioned.is_empty() {
            assert_eq!(k("src/content/api/4x/req.md"), Some(false));
            assert_eq!(k("src/content/api/5x/req.md"), None);
        }
    }
}

#[test]
fn latest_major_failures_remain_retryable_and_explicit_retry_adds_site() {
    assert_eq!(latest_major_response(404, "not found", "/version").unwrap(), None);
    assert_eq!(latest_major_response(200, r#"{"version":"2.3.4"}"#, "/version").unwrap(), Some(2));
    for (status, body) in [
        (503, "unavailable"),
        (429, "limited"),
        (200, "not json"),
        (200, "{}"),
        (200, r#"{"version":"invalid"}"#),
    ] {
        assert!(latest_major_response(status, body, "/version").is_err());
    }
    let prior = manifest();
    let failed = enrich_cached(Some((PathBuf::new(), prior.clone())), true, || {
        latest_major_response(503, "unavailable", "/version")?;
        unreachable!("failed latest lookup cannot mark docs-site enrichment complete")
    })
    .unwrap();
    assert_eq!(failed.files, prior.files);
    assert!(!failed.docs_sites_checked);
    assert!(failed.note.as_deref().unwrap().contains("HTTP 503"));
    let retried = enrich_cached(Some((PathBuf::new(), failed)), true, || {
        assert_eq!(latest_major_response(200, r#"{"version":"2.3.4"}"#, "/version")?, Some(2));
        let mut enriched = prior;
        enriched.docs_sites_checked = true;
        enriched.site = Some("github.com/o/site@fixed (major 2)".into());
        enriched.files += 1;
        Ok(enriched)
    })
    .unwrap();
    assert!(retried.docs_sites_checked && retried.site.is_some());
    assert_eq!(retried.files, 2);
    assert!(retried.note.is_none());
}
