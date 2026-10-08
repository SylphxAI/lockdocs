//! The free answer to an upgrade request: what the full report would cover,
//! counted from the same local report, without the list. The licence gate stays
//! in `pro.rs`; this only shapes text, so it makes no network calls of its own.

use mcp_kit::licence::ProRequired;
use serde_json::{json, Value};

pub struct Preview {
    pub text: String,
    pub json: Value,
}

/// Build the preview from the report JSON (`upgrade::report`'s `json`).
pub fn build(report: &Value, required: &ProRequired, price: &str) -> Preview {
    let affected = report["affected"].as_array().cloned().unwrap_or_default();
    let count = |kind: &str| affected.iter().filter(|a| a["change"] == kind).count();
    let (removed, renamed, resigned, deprecated) = (count("removed"), count("renamed"), count("signature_changed"), count("deprecated"));
    let sites: usize = affected.iter().map(|a| a["call_sites"].as_array().map_or(0, Vec::len)).sum();
    let head = format!(
        "{} {} -> {}",
        report["package"].as_str().unwrap_or(""),
        report["from"].as_str().unwrap_or(""),
        report["to"].as_str().unwrap_or("")
    );
    let pro = format!(
        "The full report with each change, its replacement and your call sites is {} {} ({price}/seat/yr): {}",
        required.product, required.tier, required.url
    );
    let text = if affected.is_empty() {
        format!(
            "{head}: none of the APIs your project calls change ({} other API changes exist). {pro}",
            report["unaffected_changes"]
        )
    } else {
        let parts: Vec<String> = [(removed, "removed"), (renamed, "renamed"), (resigned, "re-signed"), (deprecated, "deprecated")]
            .iter()
            .filter(|(n, _)| *n > 0)
            .map(|(n, w)| format!("{n} {w}"))
            .collect();
        let first = &affected[0];
        let site = first["call_sites"][0]["path"]
            .as_str()
            .map(|p| format!(" at {p}:{}", first["call_sites"][0]["line"]))
            .unwrap_or_default();
        format!(
            "{head}: {} of the APIs your project calls change ({}) across {sites} call sites. Sample: {} {}{site}. {pro}",
            affected.len(),
            parts.join(", "),
            first["change"].as_str().unwrap_or("").replace('_', " "),
            first["symbol"].as_str().unwrap_or("")
        )
    };
    let json = json!({
        "pro_required": {"feature": required.feature, "product": required.product, "tier": required.tier, "url": required.url},
        "package": report["package"], "from": report["from"], "to": report["to"],
        "affected": affected.len(),
        "by_change": {"removed": removed, "renamed": renamed, "signature_changed": resigned, "deprecated": deprecated},
        "call_sites": sites,
        "unaffected_changes": report["unaffected_changes"],
        "text": text,
    });
    Preview { text, json }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn required() -> ProRequired {
        ProRequired {
            feature: "Upgrade report".into(),
            product: "lockdocs".into(),
            tier: "Pro".into(),
            url: "https://example.com/pro".into(),
        }
    }

    #[test]
    fn counts_by_change_and_names_the_price_and_link() {
        let r = json!({"package": "next", "from": "14.2.3", "to": "15.0.0", "unaffected_changes": 4, "affected": [
            {"change": "removed", "symbol": "next.a", "call_sites": [{"path": "a.ts", "line": 3}, {"path": "b.ts", "line": 9}]},
            {"change": "renamed", "symbol": "next.b", "call_sites": [{"path": "a.ts", "line": 5}]},
            {"change": "signature_changed", "symbol": "next.c", "call_sites": [{"path": "c.ts", "line": 1}]},
        ]});
        let p = build(&r, &required(), "US$120");
        assert_eq!(
            p.text,
            "next 14.2.3 -> 15.0.0: 3 of the APIs your project calls change (1 removed, 1 renamed, 1 re-signed) across 4 call sites. Sample: removed next.a at a.ts:3. The full report with each change, its replacement and your call sites is lockdocs Pro (US$120/seat/yr): https://example.com/pro"
        );
        assert_eq!(p.json["call_sites"], 4);
        assert_eq!(p.json["pro_required"]["url"], "https://example.com/pro");
    }

    #[test]
    fn nothing_affected_says_so() {
        let r = json!({"package": "p", "from": "1.0.0", "to": "2.0.0", "unaffected_changes": 2, "affected": []});
        let p = build(&r, &required(), "US$120");
        assert!(
            p.text.contains("none of the APIs your project calls change (2 other API changes exist)"),
            "{}",
            p.text
        );
    }

    #[test]
    fn price_matches_the_pricing_page() {
        // Skipped when the docs are absent (the packaged crate).
        let cfg = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/.vitepress/config.ts");
        if let Ok(c) = std::fs::read_to_string(cfg) {
            assert!(c.contains(&format!("price: '{}'", crate::pro::PRICE)), "docs price differs from pro::PRICE");
        }
    }
}
