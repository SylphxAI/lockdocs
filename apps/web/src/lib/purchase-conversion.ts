import { createHash, createHmac, timingSafeEqual } from "node:crypto";
import { resolveEffectiveConsent } from "@sylphx/sdk/signal";
import { googleAdsConfig, signalConfig } from "./analytics-config";
import { appOrigin, getSession, sameOrigin } from "./auth";
import { boundedJson } from "./bounded-json";
import { claimedPurchases, fullAdsConsent, liveCheckoutSessionId, maxClaimedPurchases, purchaseConversionPath } from "./google-ads";
import { cookie } from "./http";
import { verifiedPaidCheckout } from "./money";
import { signalRequestPrivacy } from "./signal-privacy";
import { signalPurchaseCompleted } from "./signal-server";

const claimedCookie = "__Secure-lockdocs-ads-claimed";
const privateHeaders = { "Cache-Control": "private, no-store", "Referrer-Policy": "no-referrer" };
const empty = (status: number) => new Response(null, { status, headers: privateHeaders });

function sign(key: string, user: string, payload: string) {
  return createHmac("sha256", key).update("lockdocs-ads-claimed:v1:" + user + ":" + payload).digest("hex");
}

function readClaims(request: Request, key: string, user: string) {
  const raw = cookie(request, claimedCookie);
  if (!raw || raw.length > maxClaimedPurchases * 33 + 64) return [];
  const [payload, mac, ...extra] = raw.split(".");
  if (extra.length || !/^[a-f0-9]{64}$/.test(mac ?? "") ||
      !timingSafeEqual(Buffer.from(mac, "hex"), Buffer.from(sign(key, user, payload), "hex"))) return [];
  return claimedPurchases(payload);
}

export async function claimPurchaseConversion(request: Request) {
  const origin = appOrigin();
  if (!origin || !sameOrigin(request, origin)) return empty(403);
  const session = await getSession(request);
  if (!session) return empty(401);
  const privacy = signalRequestPrivacy(request);
  const consent = resolveEffectiveConsent(privacy, request.headers);
  if (consent.analytics_storage !== "granted" ||
      (privacy.storedChoice !== "analytics_only" && privacy.storedChoice !== "granted")) return empty(403);
  const body = await boundedJson(request, 1024);
  const adsConsent = body?.ads_consent === true && privacy.storedChoice === "granted" && fullAdsConsent(consent);
  // Older Ads callers also granted analytics. New Ads-only follow-ups explicitly
  // opt out here because the separate Signal claim already ran.
  const analyticsConsent = body?.analytics_consent === true || (adsConsent && body?.analytics_consent === undefined);
  if (!analyticsConsent && !adsConsent) return empty(403);
  const sid = body?.session_id;
  const key = process.env.SYLPHX_MONEY_API_KEY;
  const sendSignal = analyticsConsent && !!signalConfig();
  const claimAds = adsConsent && !!googleAdsConfig();
  if (!liveCheckoutSessionId(sid) || !key || (!sendSignal && !claimAds)) return empty(204);

  const checkout = await verifiedPaidCheckout(session, sid);
  if (!checkout) return empty(204);
  // Replayed checkout returns reuse the same Signal event ID across devices.
  if (sendSignal) await signalPurchaseCompleted(checkout, privacy, request.headers).catch(() => {});
  if (!claimAds) return empty(204);
  const transactionId = createHash("sha256").update("lockdocs-ads-purchase:" + checkout.name).digest("hex").slice(0, 32);
  const claimed = readClaims(request, key, session.id);
  if (claimed.includes(transactionId)) return empty(204);
  const payload = [...claimed, transactionId].slice(-maxClaimedPurchases).join(",");
  return Response.json({ transactionId, value: checkout.value, currency: checkout.currency }, {
    headers: { ...privateHeaders, "Set-Cookie": claimedCookie + "=" + payload + "." + sign(key, session.id, payload) +
      "; Max-Age=7776000; Path=" + purchaseConversionPath + "; Secure; HttpOnly; SameSite=Lax" },
  });
}
