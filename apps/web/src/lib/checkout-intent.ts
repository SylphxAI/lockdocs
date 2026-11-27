/** The browser mints one ID per checkout attempt. Money's create replays per ID, so a resubmit can't open a second session. */
export const checkoutIntentField = "checkout_intent";
const storageKey = "lockdocs.checkout.intent";

// Money replays a key's first non-5xx answer for 24 hours, even after its Stripe session completes. An attempt
// is reused for 30 minutes from its first submit: long enough for retries after a timeout or 5xx, and short enough
// that a replayed open session is still payable, since Stripe sessions last at least 30 minutes (24 hours by default).
export const attemptLifetime = 30 * 60_000;

// The failed-checkout return carries this marker only when Money definitely refused, so no session exists:
// the next submit starts a new attempt instead of replaying the refusal under the same key.
export const finishedAttempt = { name: "attempt", value: "done" } as const;

export function continuesAttempt(billing: unknown, attempt: unknown) {
  return billing === "unavailable" && attempt !== finishedAttempt.value;
}

export type CheckoutAttempt = { scope: string; intent: string; minted: number };

export function checkoutIntent(value: unknown): string | undefined {
  return typeof value === "string" && /^[a-f0-9]{64}$/.test(value) ? value : undefined;
}

function mint() {
  return Array.from(crypto.getRandomValues(new Uint8Array(32)), byte => byte.toString(16).padStart(2, "0")).join("");
}

function current(attempt: Partial<CheckoutAttempt> | null | undefined, scope: string, now: number) {
  const age = now - Number(attempt?.minted);
  return attempt?.scope === scope && checkoutIntent(attempt.intent) && age >= 0 && age < attemptLifetime
    ? attempt as CheckoutAttempt : undefined;
}

/**
 * A resubmit from the same page, or a return from a failed checkout, continues the attempt for the same
 * currency and surface. A fresh visit, a Stripe cancel return, another currency or surface, or an attempt
 * older than `attemptLifetime` starts a new one.
 */
export function attemptIntent(scope: string, retry: boolean, page: { attempt?: CheckoutAttempt },
  storage: () => Storage = () => sessionStorage, now = Date.now()) {
  let attempt = current(page.attempt, scope, now);
  if (!attempt && retry) {
    try { attempt = current(JSON.parse(storage().getItem(storageKey) ?? "null"), scope, now); }
    catch { /* Unavailable or stale storage starts a new attempt. */ }
  }
  if (!attempt) {
    attempt = { scope, intent: mint(), minted: now };
    try { storage().setItem(storageKey, JSON.stringify(attempt)); } catch { /* The page keeps its own attempt. */ }
  }
  page.attempt = attempt;
  return attempt.intent;
}
