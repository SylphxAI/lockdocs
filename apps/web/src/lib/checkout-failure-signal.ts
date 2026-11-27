import { createHash } from "node:crypto";
import { resolveEffectiveConsent, type SignalPrivacy } from "@sylphx/sdk/signal";
import { signalConfig } from "./analytics-config";
import type { CheckoutSurface } from "./analytics-events";
import { checkoutIntent } from "./checkout-intent";

// Money answered the create call and refused it, so no checkout exists for this intent.
// Timeouts, network errors, unreadable success bodies, 408, 409 and 5xx leave the outcome
// unknown: Money may still hold a session for the idempotency key. They are never reported.
const refusals = new Map([
  [400, "rejected"], [404, "rejected"], [412, "rejected"], [422, "rejected"],
  [401, "refused"], [403, "refused"], [429, "rate_limited"],
] as const);

export function checkoutRefusal(status: number) {
  return refusals.get(status as never);
}

// Optional analytics. Call it only with the status of Money's create answer, after input and sign-in
// checks have passed. It needs the same explicit analytics grant as purchase claims, Sec-GPC or an
// unready browser blocks it, and it never throws, so it can't change the buyer's response.
export async function signalCheckoutFailed(moneyStatus: number, intent: string | undefined, surface: CheckoutSurface,
  privacy: SignalPrivacy, headers: Headers) {
  try {
    const status = checkoutRefusal(moneyStatus);
    const config = signalConfig();
    if (!status || !config || !checkoutIntent(intent) ||
        (privacy.storedChoice !== "granted" && privacy.storedChoice !== "analytics_only") ||
        resolveEffectiveConsent(privacy, headers).analytics_storage !== "granted") return;
    // A retry reuses the browser intent and Money's idempotency key, so Signal keeps one record.
    const eventId = createHash("sha256").update("lockdocs:checkout_failed:" + intent).digest("hex");
    const response = await fetch(new URL("/v1/analytics/events:batch", config.signalApiOrigin), {
      method: "POST", redirect: "error", cache: "no-store", credentials: "omit", signal: AbortSignal.timeout(10_000),
      headers: { "Content-Type": "application/json", "X-Sylphx-Key": config.signalKey },
      body: JSON.stringify({ consent: "granted", events: [{ event_id: eventId, distinct_id: eventId, event_name: "checkout_failed",
        properties: { status, entry_surface: surface } }] }),
    });
    await response.body?.cancel();
  } catch { /* Signal transport failures can't change the checkout refusal. */ }
}
