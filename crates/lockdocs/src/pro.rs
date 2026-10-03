//! lockdocs Pro: the licence policy and the gate. Everything free stays free:
//! only the entry points listed in `docs/capabilities.md` as LD-PRO-* call
//! `require`, and a free call never reaches it.

use std::time::Duration;

use mcp_kit::licence::{self, Licence, LicencePolicy, ProRequired};

/// Marker for the key list below. No token verifies while it is the only entry.
pub const KEY_PLACEHOLDER: &str = "PLACEHOLDER-lockdocs-pro-issuer-public-key-not-issued-yet";

/// Base URL of the in-terminal checkout.
#[allow(dead_code)]
pub const CHECKOUT_BASE: &str = "https://buy.sylphx.com";

/// The lockdocs Pro licence policy.
///
/// TODO(Services S1): replace `KEY_PLACEHOLDER` with the issued lockdocs-pro
/// Ed25519 public key (base64url). Until then this list contains no valid key,
/// so every token is invalid and nothing Pro unlocks.
pub const POLICY: LicencePolicy<'static> = LicencePolicy {
    product: "lockdocs",
    tier: "Pro",
    require_product: true,
    accepted_plans: &["pro"],
    public_keys: &[KEY_PLACEHOLDER],
    env_var: "LOCKDOCS_LICENCE_TOKEN",
    file_name: "licence",
    // Published pricing page (docs/pro.md). The price itself is `docs/pricing.json`.
    upgrade_url: "https://sylphxai.github.io/lockdocs/pro",
    // set when buy.sylphx.com serves /api/v1/claims and a Money sandbox token passes activate
    checkout_base: None,
};

pub const UPGRADE_REPORT: &str = "Upgrade report";
pub const PRIVATE_SOURCES: &str = lockdocs_core::private::FEATURE;

/// Gate one Pro feature against a policy.
pub fn require(policy: &LicencePolicy, feature: &str) -> Result<Licence, ProRequired> {
    licence::require(policy, feature)
}

/// A renewal line for a licence in its last 30 days, else `None`. The upgrade
/// report appends it so a lapsing seat is noticed before it stops working.
pub fn renewal_note(policy: &LicencePolicy, licence: &Licence) -> Option<String> {
    licence
        .expires_soon(Duration::from_secs(30 * 24 * 3600))
        .then(|| format!("Your lockdocs Pro licence expires soon. Renew: {}", policy.upgrade_url))
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
