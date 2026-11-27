/** The browser mints one ID per checkout attempt. Money's create replays per ID, so a resubmit can't open a second session. */
export const checkoutIntentField = "checkout_intent";
const storageKey = "lockdocs.checkout.intent";

export function checkoutIntent(value: unknown): string | undefined {
  return typeof value === "string" && /^[a-f0-9]{64}$/.test(value) ? value : undefined;
}

function mint() {
  return Array.from(crypto.getRandomValues(new Uint8Array(32)), byte => byte.toString(16).padStart(2, "0")).join("");
}

/**
 * A return from a failed checkout continues the stored attempt for the same currency and surface.
 * A fresh visit, a Stripe cancel return or another currency or surface starts a new attempt.
 */
export function attemptIntent(scope: string, retry: boolean, storage: () => Storage = () => sessionStorage) {
  try {
    const saved = JSON.parse(storage().getItem(storageKey) ?? "null");
    const intent = checkoutIntent(saved?.intent);
    if (retry && intent && saved.scope === scope) return intent;
  } catch { /* Unavailable or stale storage starts a new attempt. */ }
  const intent = mint();
  try { storage().setItem(storageKey, JSON.stringify({ scope, intent })); } catch { /* The page keeps its own intent. */ }
  return intent;
}
