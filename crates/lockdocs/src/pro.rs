//! lockdocs Pro: the licence policy and the gate. Everything free stays free:
//! only the entry points listed in `docs/capabilities.md` as LD-PRO-* call
//! `require`, and a free call never reaches it.

use mcp_kit::licence::{self, Licence, LicencePolicy, ProRequired};

/// Marker for the key list below. No token verifies while it is the only entry.
pub const KEY_PLACEHOLDER: &str = "PLACEHOLDER-lockdocs-pro-issuer-public-key-not-issued-yet";

/// The lockdocs Pro licence policy.
///
/// TODO(Services S1): replace `KEY_PLACEHOLDER` with the issued lockdocs-pro
/// Ed25519 public key (base64url). Until then this list contains no valid key,
/// so every token is invalid and nothing Pro unlocks.
pub const POLICY: LicencePolicy<'static> = LicencePolicy {
    product: "lockdocs",
    require_product: true,
    accepted_plans: &["pro"],
    public_keys: &[KEY_PLACEHOLDER],
    env_var: "LOCKDOCS_LICENCE_TOKEN",
    file_name: "licence",
    // Published pricing page (docs/pro.md). The price itself is `docs/pricing.json`.
    upgrade_url: "https://sylphxai.github.io/lockdocs/pro",
};

pub const UPGRADE_REPORT: &str = "Upgrade report";

/// Gate one Pro feature against a policy.
pub fn require(policy: &LicencePolicy, feature: &str) -> Result<Licence, ProRequired> {
    licence::require(policy, feature)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholder_key_verifies_nothing() {
        assert_eq!(POLICY.public_keys, &[KEY_PLACEHOLDER]);
        for t in ["", "x.y", "e30.AAAA"] {
            assert!(POLICY.verify(t).is_err());
        }
    }
}
