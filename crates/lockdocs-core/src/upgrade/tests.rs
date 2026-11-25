use super::call_sites::mentions;
use super::{diff, Change};
use crate::extract::{Entry, Kind};

fn e(kind: Kind, path: &str, sig: &str, doc: &str) -> Entry {
    Entry {
        kind,
        name: path.rsplit(['.', ':']).next().unwrap().to_string(),
        path: path.into(),
        file: "index.d.ts".into(),
        line: 1,
        sig: sig.into(),
        doc: doc.into(),
        alias_of: None,
        legacy: false,
        prose_imports: Vec::new(),
        guide_modes: Vec::new(),
        prose_parents: Vec::new(),
    }
}

#[test]
fn classifies_changes() {
    let old = vec![
        e(Kind::Function, "p.gone", "function gone(): void", "Does the thing."),
        e(
            Kind::Function,
            "p.oldName",
            "function oldName(a: string): number",
            "Parse a number out of text.",
        ),
        e(Kind::Function, "p.sig", "function sig(a: string): void", ""),
        e(Kind::Function, "p.dep", "function dep(): void", "Fine."),
        e(Kind::Function, "p.same", "function same(): void", ""),
        e(Kind::Function, "p.kept", "function kept(): void", ""),
    ];
    let mut kept_alias = e(Kind::Alias, "p.kept", "export { kept }", "");
    kept_alias.alias_of = Some("kept".into());
    let new = vec![
        e(
            Kind::Function,
            "p.newName",
            "function newName(a: string): number",
            "Parse a number out of text.",
        ),
        e(Kind::Function, "p.sig", "function sig(a: string, b: number): void", ""),
        e(Kind::Function, "p.dep", "function dep(): void", "@deprecated use other"),
        e(Kind::Function, "p.same", "function   same():  void", ""),
        kept_alias,
    ];
    let d = diff(&old, &new);
    let by = |p: &str| d.iter().find(|x| x.old.path == p).map(|x| x.change);
    assert_eq!(by("p.gone"), Some(Change::Removed));
    assert_eq!(by("p.oldName"), Some(Change::Renamed));
    assert_eq!(d.iter().find(|x| x.old.path == "p.oldName").unwrap().new.as_ref().unwrap().path, "p.newName");
    assert_eq!(by("p.sig"), Some(Change::SignatureChanged));
    assert_eq!(by("p.dep"), Some(Change::Deprecated));
    assert_eq!(by("p.same"), None, "whitespace is not a change");
    assert_eq!(by("p.kept"), None, "a re-export is not a removal");
}

#[test]
fn mentions_respects_boundaries_and_members() {
    assert!(mentions("z.object({})", "object", true));
    assert!(!mentions("const objects = 1", "object", false));
    assert!(!mentions("object(1)", "object", true));
    assert!(mentions("use x::spawn;", "spawn", true));
    assert!(mentions("import { oldName } from 'p'", "oldName", false));
}
