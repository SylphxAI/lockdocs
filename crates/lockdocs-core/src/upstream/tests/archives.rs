fn names(v: &[(String, Vec<u8>)]) -> Vec<&str> {
    v.iter().map(|(p, _)| p.as_str()).collect()
}

#[test]
fn tar_extraction_keeps_only_docs_paths() {
    let big = vec![b'x'; (MAX_FILE + 1) as usize];
    let tgz = tarball(&[
        ("README.md", b"# readme"),
        ("CHANGELOG.md", b"# changes"),
        ("LICENSE", b"mit"),
        ("src/lib.rs", b"fn main() {}"),
        ("docs/index.md", b"# docs"),
        ("docs/guide/a.mdx", b"a"),
        ("docs/img.png", b"png"),
        ("docs/blog/post.md", b"blog"),
        ("docs/huge.md", &big),
        ("packages/pkg/docs/b.md", b"b"),
        ("packages/other/docs/c.md", b"c"), // another package: not opened
        ("node_modules/x/docs/n.md", b"n"),
        ("src/deep/er/est/docs/z.md", b"z"),
    ]);
    let got = extract_pkg_docs(&tgz[..], &dep("pkg", "1.0.0"), &repo(None)).unwrap();
    assert_eq!(
        names(&got),
        vec!["CHANGELOG.md", "README.md", "docs/guide/a.mdx", "docs/index.md", "packages/pkg/docs/b.md",]
    );
    assert_eq!(got.iter().find(|(p, _)| p == "README.md").unwrap().1, b"# readme");
    // The symlink never appears, and nothing outside docs roots is read.
    assert!(!names(&got).iter().any(|p| p.contains("link") || p.starts_with("src/")));
}

#[test]
fn tar_extraction_prefers_english_and_package_readme() {
    let tgz = tarball(&[
        ("docs/en/a.md", b"a"),
        ("docs/fr/a.md", b"fr"),
        ("docs/de/a.md", b"de"),
        ("docs/top.md", b"t"),
        ("README.md", b"root"),
        ("packages/pkg/README.md", b"pkg"),
        ("packages/pkg/src/x.md", b"no"),
        ("packages/other/README.md", b"other"),
    ]);
    let got = extract_pkg_docs(&tgz[..], &dep("pkg", "1.0.0"), &repo(Some("packages/pkg"))).unwrap();
    assert_eq!(names(&got), vec!["README.md", "docs/en/a.md", "docs/top.md", "packages/pkg/README.md"]);
}

#[test]
fn tar_extraction_rejects_unsafe_and_corrupt_archives() {
    // A path that escapes the folder is skipped, not written anywhere.
    let mut b = tar::Builder::new(Vec::new());
    let mut h = tar::Header::new_gnu();
    h.set_size(1);
    h.set_entry_type(tar::EntryType::Regular);
    h.as_old_mut().name[..16].copy_from_slice(b"r-abc/../evil.md");
    h.set_cksum();
    b.append(&h, &b"x"[..]).unwrap();
    let tar = b.into_inner().unwrap();
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    std::io::Write::write_all(&mut gz, &tar).unwrap();
    let tgz = gz.finish().unwrap();
    assert!(read_tar(&tgz[..], 1 << 20, |_, _| true).unwrap().is_empty());
    assert!(write_file(Path::new("/nonexistent-root"), "../x.md", b"x").is_err());
    // Truncated or non-archive bodies are errors, never "no docs".
    let ok = tarball(&[("docs/a.md", &[b'a'; 4096])]);
    assert!(extract_pkg_docs(&ok[..ok.len() / 2], &dep("pkg", "1.0.0"), &repo(None)).is_err());
    assert!(extract_pkg_docs(&b"<html>not found</html>"[..], &dep("pkg", "1.0.0"), &repo(None)).is_err());
}

#[test]
fn hardlinks_and_absolute_paths_are_skipped() {
    let mut b = tar::Builder::new(Vec::new());
    let mut h = tar::Header::new_gnu();
    h.set_entry_type(tar::EntryType::Link);
    h.set_size(0);
    b.append_link(&mut h, "r-abc/docs/hard.md", "r-abc/README.md").unwrap();
    let mut h = tar::Header::new_gnu();
    h.set_size(1);
    h.set_mode(0o644);
    h.set_entry_type(tar::EntryType::Regular);
    let mut raw = [0u8; 100];
    raw[..11].copy_from_slice(b"/abs/doc.md");
    h.as_old_mut().name = raw;
    h.set_cksum();
    b.append(&h, &b"x"[..]).unwrap();
    let mut h = tar::Header::new_gnu();
    h.set_size(1);
    h.set_mode(0o644);
    h.set_entry_type(tar::EntryType::Regular);
    b.append_data(&mut h, "r-abc/docs/ok.md", &b"y"[..]).unwrap();
    let tar = b.into_inner().unwrap();
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    std::io::Write::write_all(&mut gz, &tar).unwrap();
    let tgz = gz.finish().unwrap();
    let got = read_tar(&tgz[..], 1000, |_, _| true).unwrap();
    assert_eq!(names(&got), vec!["docs/ok.md"]);
}

#[test]
fn tar_extraction_stops_at_the_size_cap() {
    let tgz = tarball(&[("docs/a.md", &[b'a'; 600]), ("docs/b.md", &[b'b'; 600])]);
    let got = read_tar(&tgz[..], 1000, |_, _| true).unwrap();
    assert_eq!(got.len(), 1);
}

// Unmodified blobs from fastapi/fastapi tag 0.88.0, commit
// 612b8ff168beee33fcf0c0bdff9718b4c8b1ac63 (MIT; see fixture license).
const EVENTS_GUIDE: &str = "docs/en/docs/advanced/events.md";
const STARTUP_EXAMPLE: &str = "docs_src/events/tutorial001.py";
const SHUTDOWN_EXAMPLE: &str = "docs_src/events/tutorial002.py";
const FASTAPI_DOCS: &[(&str, &[u8])] = &[
    (EVENTS_GUIDE, include_bytes!("fastapi_088/events.md")),
    (STARTUP_EXAMPLE, include_bytes!("fastapi_088/tutorial001.py")),
    (SHUTDOWN_EXAMPLE, include_bytes!("fastapi_088/tutorial002.py")),
    ("docs/fr/docs/advanced/events.md", b"# Translation"),
    ("src/events/tutorial001.py", b"not documentation"),
];

#[test]
fn fastapi_088_archive_selects_the_guide_and_both_examples() {
    let tgz = tarball(FASTAPI_DOCS);
    let got = extract_pkg_docs(&tgz[..], &dep("fastapi", "0.88.0"), &repo(None)).unwrap();
    assert_eq!(names(&got), [EVENTS_GUIDE, STARTUP_EXAMPLE, SHUTDOWN_EXAMPLE]);
    let cache = tempfile::tempdir().unwrap();
    for ((path, bytes), (_, expected)) in got.iter().zip(FASTAPI_DOCS) {
        assert_eq!(bytes, expected);
        write_file(cache.path(), path, bytes).unwrap();
    }
    assert_fastapi_088_guide(cache.path());
}

#[test]
fn fastapi_088_cache_renders_examples_inside_the_existing_fences() {
    let cache = tempfile::tempdir().unwrap();
    for (path, bytes) in &FASTAPI_DOCS[..3] {
        write_file(cache.path(), path, bytes).unwrap();
    }
    assert_fastapi_088_guide(cache.path());
}

fn assert_fastapi_088_guide(cache: &Path) {
    let src = tempfile::tempdir().unwrap();
    let src = Source {
        dir: src.path().to_owned(),
        files: Some(vec![]),
        metadata: None,
        version: "0.88.0".into(),
        label: "fixture".into(),
        fetched: false,
    };
    let entries = crate::extract::extract(Eco::PyPI, "fastapi", &src, None, Some(cache));
    for (event, highlight, (_, code)) in [("startup", 8, FASTAPI_DOCS[1]), ("shutdown", 6, FASTAPI_DOCS[2])] {
        let section = entries
            .iter()
            .find(|e| e.file == format!("upstream:{EVENTS_GUIDE}") && e.name.ends_with(&format!("{event} event")))
            .unwrap();
        let code = std::str::from_utf8(code).unwrap().trim_end();
        let fence = "```";
        let expected = format!("{fence}Python hl_lines=\"{highlight}\"");
        let expected = format!("{expected}\n{code}\n{fence}");
        assert!(section.doc.contains(&expected), "{}", section.doc);
        assert!(!section.doc.contains("{!"));
        assert_eq!(section.doc.matches(fence).count(), 2);
    }
}

#[test]
fn fastapi_088_rest_fallback_uses_the_same_release_and_files() {
    let f = fake(Codeload::Fail, false, Some("0.88.0"));
    let out = tempfile::tempdir().unwrap();
    let m = build(&client(&f, MAX_ARCHIVE), &dep("fastapi", "0.88.0"), &repo(None), true, out.path()).unwrap();
    assert_eq!((m.tag.as_deref(), m.commit.as_deref(), m.via.as_deref()), (Some("0.88.0"), Some(FASTAPI_SHA), Some("rest")));
    assert_eq!(m.files, 3);
    for (path, bytes) in &FASTAPI_DOCS[..3] {
        assert_eq!(std::fs::read(out.path().join(path)).unwrap(), *bytes);
        assert_eq!(requests(&f, &format!("/raw/o/r/{FASTAPI_SHA}/{path}")), 1);
    }
    assert_eq!(requests(&f, "/raw/"), 3);
    assert_fastapi_088_guide(out.path());
}
