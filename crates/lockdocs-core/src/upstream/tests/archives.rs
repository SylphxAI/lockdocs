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
