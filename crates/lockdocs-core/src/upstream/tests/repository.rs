#[test]
fn github_urls() {
    let g = |s: &str| parse_github(s).map(|(o, n)| format!("{o}/{n}"));
    assert_eq!(g("git+https://github.com/vercel/next.js.git").as_deref(), Some("vercel/next.js"));
    assert_eq!(g("https://github.com/pydantic/pydantic").as_deref(), Some("pydantic/pydantic"));
    assert_eq!(g("git@github.com:colinhacks/zod.git").as_deref(), Some("colinhacks/zod"));
    assert_eq!(g("github:tokio-rs/axum").as_deref(), Some("tokio-rs/axum"));
    assert_eq!(g("expressjs/express").as_deref(), Some("expressjs/express"));
    assert_eq!(g("https://gitlab.com/x/y"), None);
    assert_eq!(g("github:../repo"), None);
    assert_eq!(g("github:owner/.."), None);
    assert_eq!(g("github:owner/repo%2f.."), None);
    let d = Dep {
        eco: Eco::PyPI,
        name: "sqlalchemy".into(),
        version: "2.0.36".into(),
        direct: true,
        from: "x".into(),
    };
    let t = tag_candidates(&d);
    assert_eq!(t[0], "v2.0.36");
    assert!(t.contains(&"rel_2_0_36".to_string()));
    assert!(doc_file("docs/01-app/page.mdx") && !doc_file("docs/blog/x.md") && !doc_file("docs/img.png"));
    assert!(later_major_file("src/pages/docs/v4-beta.mdx", 3) && !later_major_file("src/pages/docs/v3-upgrade.mdx", 3));
    assert!(!later_major_file("docs/300-upgrade-guides/upgrading-to-v6.mdx", 6));
    assert!(page_file("src/app/(docs)/docs/installation/(tabs)/using-vite/page.tsx") && !page_file("src/app/(docs)/docs/installation/(tabs)/layout.tsx"));
}
