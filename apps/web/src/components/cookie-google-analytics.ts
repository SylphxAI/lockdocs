import { createSignal, type Accessor, type Setter } from "solid-js";
import type { createSignalBrowser, SignalConsent } from "@sylphx/sdk/signal";
import { analyticsPage } from "@/lib/analytics-page";
import { clearAnalyticsCookies } from "@/lib/signal-browser";
import { consumeSignup } from "@/lib/signup-analytics";
import { localizedPath, defaultLocale } from "@/lib/locales";
import {
  claimedPurchases, claimedPurchasesKey, currencyCodePattern, fullAdsConsent, maxClaimedPurchases,
  purchaseConversionPath, transactionIdPattern,
} from "@/lib/google-ads";

type Page = NonNullable<ReturnType<typeof analyticsPage>>;
type Tag = Window & { gtag?: (...args: unknown[]) => void; dataLayer?: IArguments[] };
export type Ads = { googleAdsId: string; googleAdsPurchaseLabel: string };
function startTag(frame: HTMLIFrameElement, measurement: string | undefined, ads: Ads | undefined, consent: SignalConsent, page: Page, landingUrl: string) {
  const id = measurement ?? ads?.googleAdsId;
  if (!id) return false;
  const client = frame.contentWindow as Tag | null;
  const doc = frame.contentDocument;
  if (!client || !doc?.documentElement.hasAttribute("data-analytics-frame")) return false;
  // Even a tag-internal event that ignores our page_location must fall back
  // to the real, sanitized landing page, not the tag transport URL.
  try { client.history.replaceState(null, "", landingUrl); } catch { return false; }
  const commands: IArguments[] = [];
  client.dataLayer = commands;
  client.gtag = function () { commands.push(arguments); };
  client.gtag("consent", "default", consent);
  // Supply scrubbed page context before the tag executes. Suppress both the
  // bootstrap and config page views: only the parent emits a page_view.
  client.gtag("set", { ...page, send_page_view: false });
  client.gtag("js", new Date());
  if (measurement) client.gtag("config", measurement, {
    send_page_view: false,
    allow_google_signals: consent.ad_user_data === "granted",
    allow_ad_personalization_signals: consent.ad_personalization === "granted",
    cookie_domain: "none",
  });
  if (ads && fullAdsConsent(consent)) client.gtag("config", ads.googleAdsId, { send_page_view: false });
  const script = doc.createElement("script");
  script.async = true;
  script.referrerPolicy = "no-referrer";
  script.src = "https://www.googletagmanager.com/gtag/js?id=" + id;
  doc.head.append(script);
  return true;
}
const home = (origin: string) => ({ page_location: origin + "/", page_referrer: "", page_title: "Lockdocs" });
export type PurchaseDestinations = { signal: boolean; ads: boolean };

type GoogleAnalyticsOptions = {
  pathname: Accessor<string>;
  href: (path: string) => string;
  id: Accessor<string | undefined>;
  ads: Accessor<Ads | undefined>;
  settings: Accessor<SignalConsent | undefined>;
  setSettings: Setter<SignalConsent | undefined>;
  currentConsent: () => SignalConsent;
  syncChoice: () => void;
  capture: () => ReturnType<ReturnType<typeof createSignalBrowser>["capture"]> | undefined;
  purchaseSignalReady: () => boolean;
  forwardSignup: (params: { method: string; entry_surface: string }) => void;
};

// Owns the consent-gated tag document and all commands sent to it.
export function createCookieGoogleAnalytics({
  pathname, href, id, ads, settings, setSettings, currentConsent, syncChoice,
  capture, purchaseSignalReady, forwardSignup,
}: GoogleAnalyticsOptions) {
  const [active, setActive] = createSignal(false);
  let tag: HTMLIFrameElement | undefined;
  let adsConfiguredInTag = false;
  let measuredPage: Page | null = null;
  let previousLocation = "";
  let focusedAt: number | undefined;
  let engagementTimer: number | undefined;

  function stop() {
    const frame = tag?.contentWindow as Tag | undefined;
    if (frame && id()) Object.assign(frame, { ["ga-disable-" + id()]: true });
    tag?.remove();
    tag = undefined;
    adsConfiguredInTag = false;
    setActive(false);
    measuredPage = null;
    previousLocation = "";
    endEngagement(false);
    clearAnalyticsCookies();
  }

  function analyticsAllowed() {
    return Boolean(id() && settings() && active() && currentConsent().analytics_storage === "granted");
  }

  function apply() {
    const next = currentConsent();
    const previous = settings();
    if (next.analytics_storage === "denied" || (tag && fullAdsConsent(next) !== (previous && fullAdsConsent(previous)))) stop();
    setSettings(next);
    const measurement = id();
    if (next.analytics_storage !== "granted" || tag || (!measurement && !(ads() && fullAdsConsent(next)))) return;
    const { origin } = window.location;
    const page = analyticsPage(origin + pathname(), document.referrer, origin);
    if (!page) return;
    // Keep the tag out of the storefront DOM so it cannot auto-measure
    // private paths, queries or form activity.
    const frame = document.createElement("iframe");
    tag = frame;
    frame.title = "Analytics";
    frame.hidden = true;
    frame.referrerPolicy = "no-referrer";
    frame.sandbox.add("allow-scripts", "allow-same-origin");
    frame.addEventListener("load", () => {
      if (frame !== tag) return;
      syncChoice();
      const consent = currentConsent();
      const landing = analyticsPage(origin + pathname(), document.referrer, origin);
      const adsDestination = fullAdsConsent(consent) ? ads() : undefined;
      const landingUrl = landing && new URL(landing.page_location);
      if (adsDestination && landingUrl) {
        // Let the Ads tag see only Signal-validated click IDs, never the raw
        // query or checkout session. GA's page_location stays path-only.
        const touch = capture();
        for (const key of ["gclid", "gbraid", "wbraid"] as const) {
          const value = touch?.[key];
          if (value) landingUrl.searchParams.set(key, value);
        }
      }
      if (consent.analytics_storage !== "granted" || !landing || !landingUrl ||
          !startTag(frame, measurement, adsDestination, consent, landing, landingUrl.href)) {
        stop();
        return;
      }
      adsConfiguredInTag = !!adsDestination;
      setActive(true);
      void consumeSignup(analyticsAllowed, forwardSignup).catch(() => {});
    }, { once: true });
    frame.src = "/api/analytics/tag";
    document.body.append(frame);
  }

  function adsReady() {
    const consent = settings();
    return active() && adsConfiguredInTag && !!consent && fullAdsConsent(consent) && fullAdsConsent(currentConsent());
  }

  async function claimPurchase(sid: string, destinations: PurchaseDestinations, abort: AbortSignal) {
    try {
      syncChoice();
      const client = tag?.contentWindow as Tag | undefined;
      const destination = ads();
      const analyticsConsent = destinations.signal && purchaseSignalReady();
      const adsConsent = destinations.ads && adsReady() && !!client?.gtag && !!destination;
      if ((!analyticsConsent && !adsConsent) || abort.aborted) return;
      const response = await fetch(purchaseConversionPath, {
        method: "POST", credentials: "same-origin", redirect: "error", cache: "no-store", referrerPolicy: "no-referrer", signal: abort,
        headers: { "Content-Type": "application/json" }, body: JSON.stringify({
          session_id: sid, analytics_consent: analyticsConsent, ads_consent: adsConsent,
        }),
      });
      if (response.status !== 200 || !adsConsent || !client?.gtag || !destination) return;
      const purchase = await response.json();
      syncChoice();
      if (abort.aborted || !adsReady() || tag?.contentWindow !== client ||
          typeof purchase?.transactionId !== "string" || !transactionIdPattern.test(purchase.transactionId) ||
          typeof purchase.value !== "number" || !Number.isFinite(purchase.value) || purchase.value <= 0 ||
          typeof purchase.currency !== "string" || !currencyCodePattern.test(purchase.currency)) return;
      try {
        const claimed = claimedPurchases(localStorage.getItem(claimedPurchasesKey) ?? "");
        if (claimed.includes(purchase.transactionId)) return;
        localStorage.setItem(claimedPurchasesKey, [...claimed, purchase.transactionId].slice(-maxClaimedPurchases).join(","));
      } catch { /* The server cookie and Google transaction ID still deduplicate. */ }
      client.gtag("event", "conversion", {
        send_to: destination.googleAdsId + "/" + destination.googleAdsPurchaseLabel,
        value: purchase.value, currency: purchase.currency, transaction_id: purchase.transactionId,
      });
    } catch { /* Optional tracking must not interrupt billing. */ }
  }

  function emit(event: string, params: Record<string, unknown>) {
    if (!analyticsAllowed()) return false;
    const client = tag?.contentWindow as Tag | undefined;
    if (!client?.gtag) return false;
    client.gtag("event", event, { ...params, send_to: id() });
    return true;
  }

  // The hidden frame never takes focus. Use the parent clock so GA4 receives
  // actual post-consent foreground time; gtag still owns the session ID.
  function endEngagement(report = true) {
    window.clearTimeout(engagementTimer);
    engagementTimer = undefined;
    if (focusedAt === undefined) return;
    const elapsed = Math.floor(performance.now() - focusedAt);
    focusedAt = undefined;
    if (report && measuredPage && elapsed > 0) {
      emit("user_engagement", { ...measuredPage, engagement_time_msec: elapsed });
    }
  }

  function syncEngagement() {
    if (!measuredPage || !analyticsAllowed() || document.hidden || !document.hasFocus()) {
      endEngagement();
    } else if (focusedAt === undefined) {
      focusedAt = performance.now();
      engagementTimer = window.setTimeout(() => {
        endEngagement();
        // Let the final visibility or navigation boundary flush the remaining time.
        if (!document.hidden && document.hasFocus()) focusedAt = performance.now();
      }, 10_000);
    }
  }

  function measurePage() {
    // The router publishes the new pathname before browser history is updated.
    const path = pathname();
    if (!analyticsAllowed()) return;
    const { origin } = window.location;
    const page = analyticsPage(origin + path, previousLocation || document.referrer, origin);
    if (measuredPage && measuredPage.page_location === page?.page_location) return;
    endEngagement();
    measuredPage = page;
    previousLocation = page?.page_location ?? "";
    (tag?.contentWindow as Tag | null)?.gtag?.("set", page ?? home(origin));
    if (page) {
      emit("page_view", page);
      if (localizedPath(path, defaultLocale) === "/pricing") sendPricing("pricing_view");
      syncEngagement();
    }
  }

  function sendPricing(event: "pricing_view" | "begin_checkout", params: Record<string, unknown> = {}) {
    emit(event, {
      ...params,
      page_location: window.location.origin + href("/pricing"), page_referrer: "", page_title: "Lockdocs",
    });
  }

  return { apply, stop, analyticsAllowed, adsReady, claimPurchase, emit, endEngagement, syncEngagement, measurePage, sendPricing };
}
