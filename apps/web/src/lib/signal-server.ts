import { createHash } from "node:crypto";
import { resolveEffectiveConsent, type SignalPrivacy } from "@sylphx/sdk/signal";
import { signalConfig } from "./analytics-config";
import { beginCheckoutParams, pricingCheckoutSurface, proPrice } from "./analytics-events";

async function reportCheckoutEvent(
  eventName: "checkout_started" | "purchase_completed", id: string,
  { value, currency, plan_sku, entry_surface }: { value: number; currency: string; plan_sku: string; entry_surface: string },
  privacy: SignalPrivacy, headers: Headers,
) {
  const config = signalConfig();
  if (!config || resolveEffectiveConsent(privacy, headers).analytics_storage !== "granted") return;
  const prefix = eventName === "checkout_started" ? "lockdocs-checkout-" : "lockdocs-purchase-";
  const eventId = prefix + createHash("sha256").update(id).digest("hex");
  const response = await fetch(new URL("/v1/analytics/events:batch", config.signalApiOrigin), {
    method: "POST", redirect: "error", cache: "no-store", credentials: "omit", signal: AbortSignal.timeout(10_000),
    headers: { "Content-Type": "application/json", "X-Sylphx-Key": config.signalKey },
    body: JSON.stringify({ consent: "granted", events: [{ event_id: eventId, distinct_id: eventId, event_name: eventName,
      numeric_value: value, properties: { currency, plan_sku, entry_surface } }] }),
  });
  await response.body?.cancel();
  if (!response.ok) throw new Error("Signal checkout ingest unavailable");
}

// Record confirmed checkout creation using the same catalogue as the pricing form.
export async function signalCheckoutStarted(checkoutId: string, currency: string, privacy: SignalPrivacy, headers: Headers) {
  const params = beginCheckoutParams(currency);
  if (params) await reportCheckoutEvent("checkout_started", checkoutId, params, privacy, headers);
}

// Only called with a settled, owner-verified Money checkout. Report the paid
// amount (not today's catalogue amount); Lockdocs offers one purchasable SKU.
export function signalPurchaseCompleted(checkout: { name: string; value: number; currency: string }, privacy: SignalPrivacy, headers: Headers) {
  return reportCheckoutEvent("purchase_completed", checkout.name, {
    value: checkout.value, currency: checkout.currency, plan_sku: proPrice.key, entry_surface: pricingCheckoutSurface,
  }, privacy, headers);
}
