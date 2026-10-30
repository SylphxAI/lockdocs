//! lockdocs Pro: the licence policy and the gate. Everything free stays free:
//! only the entry points listed in `docs/capabilities.md` as LD-PRO-* call
//! `require`, and a free call never reaches it.

use std::time::Duration;

use mcp_kit::licence::{self, Licence, LicencePolicy, MoneyCheckout, ProRequired};

/// The lockdocs-pro issuer public key (base64url, raw Ed25519). Public, not a secret.
pub const ISSUER_PUBLIC_KEY: &str = "3WKM0pn_td25hztpKOVCSFZvE8zs06uXrMWU1p5O4so";

/// Historical and current production Money signers; rotation is additive.
const PUBLIC_KEYS: &[&str] = &[ISSUER_PUBLIC_KEY, "Pd65t04wyYv5wZRz5I9WZb8Imwn9erorqb-Xv-yuBXM"];

/// Test Money signers never unlock ordinary Pro operations.
const TEST_PUBLIC_KEYS: &[&str] = &["FSZx75Ybp9KpOeqCzgXcf2CrmUil8oDXkPz1Z-deEN4"];
const TEST_POLICY: LicencePolicy<'static> = LicencePolicy {
    public_keys: TEST_PUBLIC_KEYS,
    env_var: "LOCKDOCS_TEST_LICENCE_TOKEN",
    file_name: "test-licence",
    tier: "Pro (TEST)",
    ..POLICY
};

/// The lockdocs Pro licence policy.
pub const POLICY: LicencePolicy<'static> = LicencePolicy {
    product: "lockdocs",
    tier: "Pro",
    require_product: true,
    accepted_plans: &["pro"],
    public_keys: PUBLIC_KEYS,
    env_var: "LOCKDOCS_LICENCE_TOKEN",
    file_name: "licence",
    upgrade_url: "https://sylphxai.github.io/lockdocs/pricing",
    // Keel Money configuration is supplied to the Money-aware CLI entry point.
    checkout_base: None,
};

pub const UPGRADE_REPORT: &str = "Upgrade report";
pub const PRIVATE_SOURCES: &str = lockdocs_core::private::FEATURE;

/// The product-owned Money checkout, parsed from the same Keel file the site uses.
pub fn checkout() -> MoneyCheckout {
    MoneyCheckout::from_docs_toml(include_str!("money_docs.toml"), "pro").expect("lockdocs' embedded Keel Money configuration must be valid")
}

/// `lockdocs licence status | activate <token> | buy`; returns the exit code.
pub fn run_licence(arguments: &[String]) -> i32 {
    if arguments.first().is_some_and(|arg| arg == "--test") {
        let arguments = &arguments[1..];
        if !matches!(arguments.first().map(String::as_str), Some("status" | "activate")) {
            eprintln!("usage: lockdocs licence --test status | lockdocs licence --test activate <token>");
            return 2;
        }
        return licence::run_cli(&TEST_POLICY, arguments);
    }
    if arguments.first().is_some_and(|command| command == "buy") {
        licence::run_money_cli(&POLICY, &checkout(), arguments)
    } else {
        licence::run_cli(&POLICY, arguments)
    }
}

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
    fn checkout_policy_and_docs_use_the_live_product() {
        assert_eq!(POLICY.product, "lockdocs");
        assert_eq!(POLICY.checkout_base, None);
        assert_eq!(POLICY.upgrade_url, "https://sylphxai.github.io/lockdocs/pricing");
        let config = include_str!("../../../docs/.vitepress/config.ts");
        assert!(config.contains("checkoutUrl: 'https://sylphxai.github.io/lockdocs/pricing'"));
        let storefront = checkout();
        assert_eq!(storefront.product, "lockdocs_pro_seat_year");
        assert!(storefront.checkout_enabled);
        assert!(storefront.publishable_key.starts_with("sylphx_pk_"));
        assert_eq!(storefront.prices.iter().map(|p| p.quantity).collect::<Vec<_>>(), [1, 5, 10, 25, 50]);
        assert_eq!(
            storefront.prices.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(),
            ["1", "5", "10", "25", "50"]
        );
        let page = include_str!("../../../docs/pro.md");
        assert!(page.contains("Buy lockdocs Pro"));
        assert!(page.contains("lockdocs licence buy --pack 1"));
        assert!(page.contains("starts the device flow"));
    }

    #[test]
    fn issuer_key_is_pinned_and_decodes_to_32_bytes() {
        assert_eq!(POLICY.public_keys, PUBLIC_KEYS);
        assert!(POLICY.public_keys.contains(&"Pd65t04wyYv5wZRz5I9WZb8Imwn9erorqb-Xv-yuBXM"));
        for key in POLICY.public_keys {
            assert_eq!(URL_SAFE_NO_PAD.decode(key).unwrap().len(), 32);
        }
        // Every sold Pro token is signed by this key. Changing or dropping it
        // breaks those tokens: a new key is added next to it, never in its place.
        assert!(POLICY.public_keys.contains(&"3WKM0pn_td25hztpKOVCSFZvE8zs06uXrMWU1p5O4so"));
        assert_eq!(URL_SAFE_NO_PAD.decode(ISSUER_PUBLIC_KEY).unwrap().len(), 32);
        for t in ["", "x.y", "e30.AAAA"] {
            assert!(POLICY.verify(t).is_err());
        }
    }

    #[test]
    fn test_activation_cannot_replace_a_production_licence() {
        assert!(TEST_POLICY.public_keys.iter().all(|key| !POLICY.public_keys.contains(key)));
        assert_ne!(TEST_POLICY.env_var, POLICY.env_var);
        assert_ne!(TEST_POLICY.file_name, POLICY.file_name);
        assert!(TEST_POLICY.tier.contains("TEST"));
        assert_eq!(run_licence(&["--test".into(), "buy".into()]), 2);
        let old = SigningKey::from_bytes(&[21; 32]);
        let new = SigningKey::from_bytes(&[22; 32]);
        let public = [
            URL_SAFE_NO_PAD.encode(old.verifying_key().to_bytes()),
            URL_SAFE_NO_PAD.encode(new.verifying_key().to_bytes()),
        ];
        let keys = [public[0].as_str(), public[1].as_str()];
        let policy = LicencePolicy { public_keys: &keys, ..POLICY };
        let payload = r#"{"plan":"pro","issuedAt":1,"product":"lockdocs"}"#;
        for key in [&old, &new] {
            assert!(policy.verify(&token(key, payload)).is_ok());
            assert!(policy.verify(&token(key, r#"{"plan":"pro","issuedAt":1,"product":"foreign"}"#)).is_err());
            assert!(policy
                .verify(&token(key, r#"{"plan":"pro","issuedAt":1,"product":"lockdocs","expiresAt":1}"#))
                .is_err());
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
