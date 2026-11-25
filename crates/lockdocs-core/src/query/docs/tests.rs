use super::super::ranking::{hybrid_rank, identifiers};
use super::super::render::Pack;
use super::answer::pack_hits;
use crate::bm25::{self, Bm25};
use crate::extract::{Entry, Kind};
use crate::index::PackageIndex;
use crate::{embed, est_tokens, Dep, Eco};
use std::{
    collections::{BTreeSet, HashSet},
    sync::Arc,
};

#[test]
fn hybrid_budget_preserves_exact_symbol_and_semantic_evidence() {
    let entry = |kind, name: &str, line, doc: String| Entry {
        kind,
        name: name.into(),
        path: name.into(),
        file: "reference.md".into(),
        line,
        sig: if kind == Kind::Function { "fn encode_packet()".into() } else { String::new() },
        doc,
        alias_of: None,
        legacy: false,
        prose_imports: vec![],
        guide_modes: vec![],
        prose_parents: vec![],
    };
    let dep = Dep {
        eco: Eco::Cargo,
        name: "codec".into(),
        version: "1.0.0".into(),
        direct: true,
        from: "requested".into(),
    };
    let idx = PackageIndex {
        format: crate::index::FORMAT,
        eco: dep.eco,
        name: dep.name.clone(),
        version: dep.version.clone(),
        source: "fixture".into(),
        fetched: false,
        entries: vec![
            entry(
                Kind::Prose,
                "Wire format",
                1,
                format!("The binary wire layout.\n{}", "Background on framing and layout.\n".repeat(60)),
            ),
            entry(Kind::Prose, "Transport", 2, "A guide to transmitting packets.\n".repeat(50)),
            entry(Kind::Function, "encode_packet", 3, "Writes a length prefix before the payload.".into()),
        ],
        npm_api: None,
        terms: vec![],
        lens: vec![],
        head: vec![],
        head_lens: vec![],
        build_ms: 0,
        upstream: Some("fixture upstream guides".into()),
        upstream_provenance: None,
        embed: "fixture".into(),
        vecs: vec![embed::quantize(&[1.0, 0.0]), embed::quantize(&[1.0, 0.0]), embed::quantize(&[0.0, 1.0])],
    };
    let ready = [(Arc::new(idx), dep, None)];
    // Equal BM25 evidence isolates the symbol-name boost and dense mix.
    let (terms, len) = bm25::tf(&[("encode_packet binary wire", 1)]);
    let bm = Bm25::build(std::iter::repeat_n((terms.as_slice(), len), 3));
    let query = "How does `encode_packet` lay out the binary wire format?";
    let rank = |vector| {
        hybrid_rank(
            &ready,
            &[(0, 0), (0, 1), (0, 2)],
            (&bm, &bm),
            &bm25::query_terms(query),
            &identifiers(query),
            false,
            || vector,
        )
    };
    let (hybrid, anchor) = rank(Some(vec![1.0, 0.0]));
    assert_eq!(hybrid.iter().map(|r| r.2).collect::<Vec<_>>(), [0, 1, 2]);
    let anchor = anchor.expect("literal match must survive fusion as a packing candidate");
    assert!(anchor.literal);
    assert_eq!(anchor.entry, (0, 2));
    assert!(rank(None).1.is_none());

    let scopes = [BTreeSet::from([0, 1, 2])];
    let mut packed = Pack::new(200);
    packed.push_raw("codec@1.0.0 · fixture\n");
    let mut old = Pack::new(200);
    old.push_raw(&packed.text);
    pack_hits(&mut old, &ready, &scopes, hybrid.clone(), None, &HashSet::new());
    assert!(!old.text.contains("fn encode_packet"));
    let (hits, _) = pack_hits(&mut packed, &ready, &scopes, hybrid, Some(anchor), &HashSet::new());
    assert!(packed.text.contains("binary wire layout"));
    assert!(packed.text.contains("fn encode_packet()"), "{}", packed.text);
    assert!(packed.text.contains("truncated to fit"));
    assert!(est_tokens(&packed.text) <= 200);
    assert_eq!(hits[0]["line"], 1);
    assert_eq!(hits[1]["line"], 3);
}
