import { createHash } from "node:crypto";
import { resolveEffectiveConsent, type SignalPrivacy } from "@sylphx/sdk/signal";
import { signalConfig } from "./analytics-config";

// Record creation of a verified Money checkout, not payment or a conversion.
export async function signalCheckoutStarted(checkoutId: string, privacy: SignalPrivacy, headers: Headers) {
  const config = signalConfig();
  if (!config || resolveEffectiveConsent(privacy, headers).analytics_storage !== "granted") return;
  const id = "lockdocs-checkout-" + createHash("sha256").update(checkoutId).digest("hex");
  const response = await fetch(new URL("/v1/analytics/events:batch", config.signalApiOrigin), {
    method: "POST", redirect: "error", cache: "no-store", credentials: "omit", signal: AbortSignal.timeout(10_000),
    headers: { "Content-Type": "application/json", "X-Sylphx-Key": config.signalKey },
    body: JSON.stringify({ consent: "granted", events: [{ event_id: id, distinct_id: id, event_name: "checkout_started",
      properties: { entry_surface: "pricing" } }] }),
  });
  if (!response.ok) throw new Error("Signal checkout ingest unavailable");
}
