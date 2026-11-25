use super::archive::untar_gz_with;
use super::{cached_for, fetched_dir, fetched_dir_for, safe_rel, unzip_with, Limits};
use crate::{Dep, Eco};
use std::path::{Path, PathBuf};

fn write(p: &Path, text: &str) {
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, text).unwrap();
}

#[test]
fn rejects_escaping_paths() {
    assert_eq!(safe_rel(Path::new("package/README.md"), true), Some(PathBuf::from("README.md")));
    assert_eq!(safe_rel(Path::new("package/../../etc/passwd"), true), None);
    assert_eq!(safe_rel(Path::new("/abs/x"), false), None);
}

#[test]
fn two_sources_publishing_the_same_version_do_not_share_a_cache_dir() {
    // The public default and each configured private registry are distinct
    // sources: two registries publishing the same name@version must not
    // return the copy fetched first. Keying on the resolved source keeps
    // them apart, and a cache written for one source is invisible to another.
    let dep = Dep {
        eco: Eco::PyPI,
        name: "acme-lib".into(),
        version: "2.0.0".into(),
        direct: true,
        from: "uv.lock".into(),
    };
    let craft = tempfile::tempdir().unwrap();
    write(
        &craft.path().join("uv.lock"),
        "version = 1\n[[package]]\nname = \"acme-lib\"\nversion = \"2.0.0\"\nsource = { registry = \"https://craft.example/simple\" }\n",
    );
    let npm = tempfile::tempdir().unwrap();
    write(
        &npm.path().join("uv.lock"),
        "version = 1\n[[package]]\nname = \"acme-lib\"\nversion = \"2.0.0\"\nsource = { registry = \"https://npm.example/simple\" }\n",
    );
    let a = fetched_dir_for(&dep, Some(craft.path()));
    let b = fetched_dir_for(&dep, Some(npm.path()));
    let public = fetched_dir(&dep);
    assert_ne!(a, b, "two private registries shared a cache directory");
    assert_ne!(a, public, "a private registry shared the public cache directory");
    assert_ne!(b, public);
    // The public slot is the same whenever no private source applies.
    assert_eq!(fetched_dir(&dep), fetched_dir_for(&dep, None));
    // The same source resolves to the same directory across projects.
    let craft_again = tempfile::tempdir().unwrap();
    write(
        &craft_again.path().join("uv.lock"),
        &std::fs::read_to_string(craft.path().join("uv.lock")).unwrap(),
    );
    assert_eq!(a, fetched_dir_for(&dep, Some(craft_again.path())));
    // A cache written for one source is not returned for another.
    std::fs::create_dir_all(&a).unwrap();
    std::fs::write(a.join(".lockdocs-complete"), b"").unwrap();
    assert!(cached_for(&dep, Some(craft.path())).is_some());
    assert!(cached_for(&dep, Some(npm.path())).is_none());
    assert!(cached_for(&dep, None).is_none());
}

fn zip_of(files: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let files: Vec<(&str, &[u8])> = files.iter().map(|(n, d)| (*n, d.as_slice())).collect();
    crate::private::testutil::zip(&files)
}

#[test]
fn archive_bombs_are_rejected() {
    let dir = std::env::temp_dir().join(format!("lockdocs-bomb-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let tiny = Limits {
        file: 1000,
        total: 2500,
        entries: 5,
    };
    // Highly compressible members: total unpacked size is capped.
    let big: Vec<(&str, Vec<u8>)> = vec![("a.md", vec![b'x'; 900]), ("b.md", vec![b'x'; 900]), ("c.md", vec![b'x'; 900])];
    let err = unzip_with(&zip_of(&big), &dir, false, None, tiny).unwrap_err();
    assert!(err.to_string().contains("unpacks to more than"), "{err}");
    // Entry count is capped.
    let many: Vec<(String, Vec<u8>)> = (0..6).map(|i| (format!("f{i}.md"), b"x".to_vec())).collect();
    let many: Vec<(&str, Vec<u8>)> = many.iter().map(|(n, d)| (n.as_str(), d.clone())).collect();
    let err = unzip_with(&zip_of(&many), &dir, false, None, tiny).unwrap_err();
    assert!(err.to_string().contains("more than 5 entries"), "{err}");
    // An oversized member is skipped, the rest unpacks.
    let mixed: Vec<(&str, Vec<u8>)> = vec![("huge.md", vec![b'x'; 5000]), ("ok.md", b"hello".to_vec())];
    unzip_with(&zip_of(&mixed), &dir, false, None, tiny).unwrap();
    assert!(dir.join("ok.md").is_file() && !dir.join("huge.md").exists());
    // tar.gz: the same total cap.
    let mut tar = tar::Builder::new(flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default()));
    for n in ["package/a.md", "package/b.md", "package/c.md"] {
        let data = vec![b'y'; 900];
        let mut h = tar::Header::new_gnu();
        h.set_size(data.len() as u64);
        h.set_mode(0o644);
        h.set_cksum();
        tar.append_data(&mut h, n, &data[..]).unwrap();
    }
    let gz = tar.into_inner().unwrap().finish().unwrap();
    let err = untar_gz_with(&gz, &dir.join("t"), tiny).unwrap_err();
    assert!(err.to_string().contains("unpacks to more than"), "{err}");
    let _ = std::fs::remove_dir_all(&dir);
}
