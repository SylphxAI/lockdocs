#[test]
fn legacy_candidate_commit_cache_cannot_be_trusted_online() {
    let mut legacy = manifest();
    legacy.format = 3;
    legacy.docs_sites_checked = true;
    assert!(needs_refresh(&legacy, false));
    assert!(needs_refresh(&legacy, true));
    let rejected = enrich_cached(Some((PathBuf::new(), legacy)), false, || Err(anyhow::anyhow!("exact tag revalidation failed")));
    assert!(rejected.is_err());
}

#[test]
fn format_4_cache_is_trusted_automatically_and_refreshed_on_fetch() {
    let mut v4 = manifest();
    v4.format = 4;
    v4.docs_sites_checked = true;
    assert!(!needs_refresh(&v4, false));
    assert!(needs_refresh(&v4, true));
    let kept = enrich_cached(Some((PathBuf::new(), v4)), false, || Err(anyhow::anyhow!("offline"))).unwrap();
    assert_eq!(kept.files, 1);
}

#[test]
fn enrichment_policy_upgrades_once_and_preserves_opted_in_sites() {
    let automatic = manifest();
    assert!(needs_refresh(&automatic, true));
    assert!(!needs_refresh(&automatic, false));
    let mut explicit = automatic.clone();
    explicit.docs_sites_checked = true;
    explicit.site = Some("github.com/o/site@resolved (major-version docs)".into());
    explicit.files = 2;
    let upgraded = enrich_cached(Some((PathBuf::new(), automatic)), true, || Ok(explicit.clone())).unwrap();
    assert_eq!(upgraded.files, 2);
    for policy in [false, true] {
        let reused = enrich_cached(Some((PathBuf::new(), upgraded.clone())), policy, || panic!("needless network refresh")).unwrap();
        assert_eq!(reused.site, explicit.site);
    }
}

#[test]
fn failed_downloads_are_not_empty_docs_or_published_partial_success() {
    let old = manifest();
    let preserved = enrich_cached(Some((PathBuf::new(), old.clone())), true, || {
        Err(anyhow::anyhow!("reading the archive: corrupt or truncated archive"))
    })
    .unwrap();
    assert_eq!(preserved.commit, old.commit);
    assert_eq!(preserved.files, old.files);
    assert!(!preserved.docs_sites_checked);
    assert!(preserved.note.unwrap().contains("previous complete cache retained"));
    assert!(enrich_cached(None, true, || Err(anyhow::anyhow!("download failed"))).is_err());
}
