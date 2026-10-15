//! lockdocs Pro: the licence policy and the gate. Everything free stays free:
//! only the entry points listed in `docs/capabilities.md` as LD-PRO-* call
//! `require`, and a free call never reaches it.

use std::time::Duration;

use mcp_kit::licence::{self, Licence, LicencePolicy, ProRequired};

/// The lockdocs-pro issuer public key (base64url, raw Ed25519). Public, not a secret.
pub const ISSUER_PUBLIC_KEY: &str = "3WKM0pn_td25hztpKOVCSFZvE8zs06uXrMWU1p5O4so";

/// Base URL of the in-terminal checkout.
#[allow(dead_code)]
pub const CHECKOUT_BASE: &str = "https://buy.sylphx.com";

/// Yearly price per seat, quoted in the free upgrade preview. The pricing page
/// reads the same value from `docs/.vitepress/config.ts`; a test keeps them equal.
pub const PRICE: &str = "US$120";

/// The lockdocs Pro licence policy.
pub const POLICY: LicencePolicy<'static> = LicencePolicy {
    product: "lockdocs",
    tier: "Pro",
    require_product: true,
    accepted_plans: &["pro"],
    public_keys: &[ISSUER_PUBLIC_KEY],
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
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine as _;
    use ed25519_dalek::{Signer, SigningKey};

    fn token(key: &SigningKey, payload: &str) -> String {
        let sig = key.sign(payload.as_bytes());
        format!("{}.{}", URL_SAFE_NO_PAD.encode(payload), URL_SAFE_NO_PAD.encode(sig.to_bytes()))
    }

    #[test]
    fn issuer_key_is_pinned_and_decodes_to_32_bytes() {
        assert_eq!(POLICY.public_keys, &[ISSUER_PUBLIC_KEY]);
        // Every sold Pro token is signed by this key. Changing or dropping it
        // breaks those tokens: a new key is added next to it, never in its place.
        assert!(POLICY.public_keys.contains(&"3WKM0pn_td25hztpKOVCSFZvE8zs06uXrMWU1p5O4so"));
        assert_eq!(URL_SAFE_NO_PAD.decode(ISSUER_PUBLIC_KEY).unwrap().len(), 32);
        for t in ["", "x.y", "e30.AAAA"] {
            assert!(POLICY.verify(t).is_err());
        }
    }

    #[test]
    fn token_from_a_listed_key_verifies_and_another_key_does_not() {
        let listed = SigningKey::from_bytes(&[7; 32]);
        let other = SigningKey::from_bytes(&[8; 32]);
        let public: &'static str = Box::leak(URL_SAFE_NO_PAD.encode(listed.verifying_key().to_bytes()).into_boxed_str());
        let keys: &'static [&'static str] = Box::leak(vec![public].into_boxed_slice());
        let policy = LicencePolicy { public_keys: keys, ..POLICY };
        let payload = r#"{"plan":"pro","issuedAt":1,"product":"lockdocs"}"#;
        assert!(policy.verify(&token(&listed, payload)).is_ok());
        assert!(policy.verify(&token(&other, payload)).is_err());
        // The pinned production key refuses a token from a key it does not list.
        assert!(POLICY.verify(&token(&listed, payload)).is_err());
    }
}
