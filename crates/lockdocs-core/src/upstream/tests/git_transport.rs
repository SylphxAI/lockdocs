#[test]
fn ls_refs_round_trip_and_release_pick() {
    let c1 = "1111111111111111111111111111111111111111";
    let c2 = "2222222222222222222222222222222222222222";
    let tagobj = "3333333333333333333333333333333333333333";
    let req = String::from_utf8(ls_refs_request(&["refs/tags/v1.2.3".into(), "refs/tags/1.2.3".into()])).unwrap();
    assert!(req.starts_with("0014command=ls-refs\n00010009peel\n"));
    assert!(req.contains("0020ref-prefix refs/tags/v1.2.3\n") && req.ends_with("0000"));
    let mut resp = String::new();
    for l in [
        format!("{tagobj} refs/tags/v1.2.3 peeled:{c1}\n"),
        format!("{c2} refs/tags/v1.2.3-beta.1\n"),
        format!("{c2} refs/tags/1.2.3\n"),
    ] {
        resp.push_str(&format!("{:04x}{l}", l.len() + 4));
    }
    resp.push_str("0000");
    let refs = parse_ls_refs(resp.as_bytes()).unwrap();
    assert_eq!(refs.len(), 3);
    let cands = tag_candidates(&dep("pkg", "1.2.3"));
    // Annotated tag: the peeled commit, never the tag object; prefix matches are not exact.
    assert_eq!(pick_release(&cands, &refs), Some(("v1.2.3".into(), c1.into())));
    // Candidate order decides, a lightweight tag is its own commit.
    assert_eq!(pick_release(&["1.2.3".into(), "v1.2.3".into()], &refs), Some(("1.2.3".into(), c2.into())));
    // Only a prefix match (a prerelease) is not a release.
    assert_eq!(pick_release(&["v1.2.3".into()], &refs[1..2]), None);
    assert_eq!(pick_release(&cands, &[]), None);
    // Never a branch of the same name, and never a malformed id.
    let branch = [GitRef {
        name: "refs/heads/v1.2.3".into(),
        oid: c1.into(),
        peeled: None,
    }];
    assert_eq!(pick_release(&cands, &branch), None);
    let bad = [GitRef {
        name: "refs/tags/v1.2.3".into(),
        oid: "xyz".into(),
        peeled: None,
    }];
    assert_eq!(pick_release(&cands, &bad), None);
    assert!(parse_ls_refs(b"0010ERR denied\n0000").is_err());
    assert!(parse_ls_refs(b"00ffshort").is_err());
    assert!(parse_ls_refs(b"zzzz").is_err());
}

#[test]
fn credentialed_hook_rejected_token_is_an_error_but_anonymous_401_is_not() {
    let f = fake(Codeload::BadToken, false, Some("v1.2.3"));
    let e = with_token(|| {
        format!(
            "{:#}",
            list_refs(&credentialed_client(&f), &repo(None), &["refs/tags/v1.2.3".into()]).unwrap_err()
        )
    });
    assert!(e.contains("rejected GITHUB_TOKEN") && e.contains("401"), "{e}");
    // Anonymous requests never send the header, so the same server answers normally.
    assert!(list_refs(&client(&f, MAX_ARCHIVE), &repo(None), &["refs/tags/v1.2.3".into()])
        .unwrap()
        .is_some());
}

#[test]
fn credentialed_hook_codeload_404_retries_anonymously() {
    let f = fake(Codeload::MissingWithAuth, false, Some("v1.2.3"));
    let got = with_token(|| codeload(&credentialed_client(&f), &repo(None), SHA).unwrap());
    assert!(got.is_some());
    let seen = f.seen.lock().unwrap().clone();
    let tar: Vec<_> = seen.iter().filter(|l| l.contains("/tar.gz/")).collect();
    assert_eq!(tar.len(), 2);
    assert!(tar[0].ends_with(" AUTH") && !tar[1].ends_with(" AUTH"), "{tar:?}");
}

#[test]
fn automatic_mode_sends_no_authorization_header() {
    let f = fake(Codeload::Ok, false, Some("v1.2.3"));
    let a = with_token(|| {
        let a = agent(true, None);
        assert_eq!(a.max_archive, MAX_ARCHIVE_AUTOMATIC);
        assert_eq!(agent(false, None).max_archive, MAX_ARCHIVE);
        let mut c = client(&f, MAX_ARCHIVE);
        c.authenticated = a.authenticated;
        let out = tempfile::tempdir().unwrap();
        build(&c, &dep("pkg", "1.2.3"), &repo(None), true, out.path()).unwrap();
        a
    });
    assert!(!a.authenticated);
    assert!(requests(&f, "/o/r/tar.gz/") == 1);
    assert_eq!(authed_requests(&f), 0);
}

#[test]
fn explicit_mode_sends_no_token_to_git_codeload_or_raw() {
    // Archive path: git and codeload only.
    let f = fake(Codeload::Ok, false, Some("v1.2.3"));
    with_token(|| {
        let out = tempfile::tempdir().unwrap();
        build(&authed_client(&f), &dep("pkg", "1.2.3"), &repo(None), false, out.path()).unwrap();
    });
    assert!(requests(&f, "/o/r/tar.gz/") == 1 && requests(&f, "/o/r.git/") >= 1);
    assert_eq!(authed_paths(&f), Vec::<String>::new());
    // Per-file path: git, REST and raw. Only REST carries the token, as in 0.4.0.
    let f = fake(Codeload::Fail, false, Some("v1.2.3"));
    with_token(|| {
        let out = tempfile::tempdir().unwrap();
        build(&authed_client(&f), &dep("pkg", "1.2.3"), &repo(None), false, out.path()).unwrap();
    });
    assert!(requests(&f, "/raw/") >= 1 && requests(&f, "/repos/") >= 1);
    let authed = authed_paths(&f);
    assert!(!authed.is_empty() && authed.iter().all(|p| p.starts_with("/repos/")), "{authed:?}");
}

#[test]
fn public_repository_fetches_anonymously_without_a_token() {
    let f = fake(Codeload::Ok, false, Some("v1.2.3"));
    let out = tempfile::tempdir().unwrap();
    let m = build(&authed_client(&f), &dep("pkg", "1.2.3"), &repo(None), false, out.path()).unwrap();
    assert_eq!(m.via.as_deref(), Some("codeload"));
    assert_eq!(authed_requests(&f), 0);
}

#[test]
fn private_repository_fails_clearly_even_with_a_token() {
    let f = fake(Codeload::Private, false, Some("v1.2.3"));
    let e = with_token(|| {
        let out = tempfile::tempdir().unwrap();
        format!(
            "{:#}",
            build(&authed_client(&f), &dep("pkg", "1.2.3"), &repo(None), false, out.path()).unwrap_err()
        )
    });
    assert!(e.contains("private repository: not supported"), "{e}");
    assert!(!e.contains("Pro") && !e.contains("coming"), "{e}");
    assert_eq!(authed_requests(&f), 0);
    assert_eq!(requests(&f, "/o/r/tar.gz/"), 0);
}

#[test]
fn basic_auth_header_is_base64() {
    assert_eq!(basic("a"), "Basic eC1hY2Nlc3MtdG9rZW46YQ==");
    assert_eq!(basic("ab"), "Basic eC1hY2Nlc3MtdG9rZW46YWI=");
    assert_eq!(basic("abc"), "Basic eC1hY2Nlc3MtdG9rZW46YWJj");
}

#[test]
fn automatic_client_is_anonymous_and_bounded() {
    let mut client = agent(true, None);
    assert!(!client.authenticated);
    assert!(client.deadline.is_some());
    client.deadline = Some(std::time::Instant::now());
    assert!(client.check_deadline().is_err());
    assert!(agent(false, None).authenticated);
}
