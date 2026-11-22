import { consent, type Choice } from "./analytics-policy";
import { campaignCookie, checkoutCampaign } from "./checkout-campaign";
import { canonicalOrigin } from "./indexing";

export function sendsGpc(request: Request) {
  return request.headers.get("sec-gpc")?.split(",").some(value => value.trim() === "1") ?? false;
}

export function checkoutConsent(form: FormData): Choice {
  const [choice, ...duplicates] = form.getAll("checkout_consent");
  return !duplicates.length && (choice === "analytics" || choice === "all") ? choice : "reject";
}

function measurementCookie(header: string, name: string) {
  const prefix = name + "=";
  const [entry, ...duplicates] = header.split(";").map(part => part.trim()).filter(part => part.startsWith(prefix));
  if (!entry || duplicates.length || entry.length > prefix.length + 800) return "";
  try { return decodeURIComponent(entry.slice(prefix.length)); } catch { return ""; }
}

function gaSessionId(value: string) {
  const gs1 = /^GS1\.\d{1,2}\.([1-9]\d{0,19})(?:\.\d{1,20})*$/.exec(value);
  if (gs1) return gs1[1];
  const gs2 = /^GS2\.\d{1,2}\.([a-z][A-Za-z0-9-]*(?:\$[a-z][A-Za-z0-9-]*)*)$/.exec(value);
  const fields = gs2?.[1].split("$").filter(field => field.startsWith("s"));
  return fields?.length === 1 ? /^s([1-9]\d{0,19})$/.exec(fields[0])?.[1] : undefined;
}

/** Best effort: a pending or failed binding must not hold up the Stripe redirect. */
export async function bindCheckoutAttribution(request: Request, config: { base: string; key: string },
  id: string, choice: Choice, measurementId: string) {
  const settings = consent(choice);
  if (settings.analytics_storage !== "granted" || sendsGpc(request) || !/^[A-Za-z0-9_-]{1,128}$/.test(id)) return;
  try {
    const cookie = request.headers.get("cookie") ?? "";
    const ga_client_id = /^GA1\.\d{1,2}\.(\d{1,20}\.\d{1,20})$/.exec(measurementCookie(cookie, "_ga"))?.[1];
    const ga_session_id = gaSessionId(measurementCookie(cookie, "_ga_" + measurementId.slice(2)));
    const adsAllowed = settings.ad_storage === "granted" && settings.ad_user_data === "granted";
    const ads = adsAllowed ? {
      ...checkoutCampaign(new URLSearchParams(measurementCookie(cookie, campaignCookie))),
      fbc: /^fb\.[1-9]\d?\.\d{13}\.[A-Za-z0-9_-]{1,200}$/.exec(measurementCookie(cookie, "_fbc"))?.[0],
      fbp: /^fb\.[1-9]\d?\.\d{13}\.\d{1,20}$/.exec(measurementCookie(cookie, "_fbp"))?.[0],
    } : {};
    const response = await fetch(new URL("/v1/events/attribution_bindings", config.base), {
      method: "POST",
      headers: { "Content-Type": "application/json", "X-Sylphx-Key": config.key },
      body: JSON.stringify({ checkout_session_id: id, consent: settings, ga_client_id, ga_session_id,
        ...ads, page_location: canonicalOrigin + "/pricing" }),
      cache: "no-store", redirect: "error", signal: AbortSignal.timeout(750),
    });
    if (!response.ok) console.warn("Checkout attribution binding rejected");
    await response.body?.cancel();
  } catch {
    // Provider errors can contain request details. Never log the exception or response.
    console.warn("Checkout attribution binding unavailable");
  }
}
