import { createEffect, createSignal, on, onCleanup, onMount } from "solid-js";
import { useLocation, useSearchParams } from "@solidjs/router";
import { getRequestEvent } from "solid-js/web";
import { mint } from "@sylphx/sdk/ids";
import { createSignalBrowser, resolveEffectiveConsent, type SignalAttribution, type SignalConsent } from "@sylphx/sdk/signal";
import { migrateSignalChoice, saveSignalChoice } from "@/lib/signal-browser";
import { readSignalChoice, signalChoiceKey, signalRequestGpc, signalStorageKey, type StoredChoice } from "@/lib/signal-privacy";
import { analyticsPage } from "@/lib/analytics-page";
import { behavioralParams, beginCheckoutParams, knownCheckoutSurface, pricingCheckoutSurface, type CheckoutSurface, type TrackEvent } from "@/lib/analytics-events";
import { consumeSignup } from "@/lib/signup-analytics";
import { useSiteLanguage } from "@/lib/site-language";
import { localizedPath, defaultLocale } from "@/lib/locales";
import { validGoogleAdsId, validGoogleAdsPurchaseLabel } from "@/lib/google-ads";
import { createCookieGoogleAnalytics, type Ads } from "./cookie-google-analytics";

// Signal merges attribution before caller properties. Undefined values suppress
// those keys in the serialized event without changing campaign capture or links.
const omitSignalAttribution: Record<Exclude<keyof SignalAttribution, "capturedAt">, undefined> = {
  gclid: undefined, gbraid: undefined, wbraid: undefined, fbclid: undefined,
  utm_id: undefined, utm_source: undefined, utm_medium: undefined, utm_campaign: undefined,
  utm_term: undefined, utm_content: undefined, utm_source_platform: undefined,
  utm_creative_format: undefined, utm_marketing_tactic: undefined,
};

export function createCookieConsent(setOpen: (open: boolean) => void) {
  const route = useLocation();
  const [query] = useSearchParams();
  const { href } = useSiteLanguage();
  const [id, setId] = createSignal<string>();
  const [ads, setAds] = createSignal<Ads>();
  const [ingestEnabled, setIngestEnabled] = createSignal(false);
  const configured = () => Boolean(id() || ads() || ingestEnabled());
  const [settings, setSettings] = createSignal<SignalConsent>();
  const [protectedByGpc, setProtectedByGpc] = createSignal(false);
  let signal: ReturnType<typeof createSignalBrowser> | undefined;
  let signalPath: string | undefined;
  const checkoutAttempts = new Set<string>();
  let eventScope: string | undefined;
  let routeVisit = 0;
  let unpersistedDenial = false;
  const requestHeaders = getRequestEvent()?.request.headers;
  const documentGpc = import.meta.env.SSR
    ? signalRequestGpc(requestHeaders)
    : document.documentElement.dataset.campaignGpc === "1";
  let serverGpc = documentGpc;
  const [campaignDenied, setCampaignDenied] = createSignal(readSignalChoice(
    import.meta.env.SSR ? requestHeaders?.get("cookie") ?? "" : document.cookie,
  ) === "denied");
  const campaignBlocked = () => documentGpc || protectedByGpc() || signalRequestGpc() || campaignDenied();
  const google = createCookieGoogleAnalytics({
    pathname: () => route.pathname, href, id, ads, settings, setSettings, currentConsent, syncChoice,
    capture: () => signal?.capture(), purchaseSignalReady, forwardSignup,
  });

  function currentConsent() {
    // GPC can change after first paint; the SDK always rechecks navigator GPC.
    const consent = signal?.getConsent() ?? resolveEffectiveConsent();
    // A regional GA default is not an explicit grant to the Ads destination.
    if (!ads() || (!unpersistedDenial && readSignalChoice(document.cookie) === "granted")) return consent;
    return { ...consent, ad_storage: "denied", ad_user_data: "denied", ad_personalization: "denied" } as const;
  }

  function syncChoice() {
    const saved = readSignalChoice(document.cookie);
    setCampaignDenied(unpersistedDenial || saved === "denied");
    // localStorage is the SDK's browser input; mirror the expiring cookie into
    // it so expiration, cookie deletion and checkout cannot diverge.
    try {
      const choice = unpersistedDenial ? "denied" : saved;
      const local = localStorage.getItem(signalChoiceKey);
      if (choice === undefined && local !== null) localStorage.removeItem(signalChoiceKey);
      else if (choice !== undefined && choice !== local) localStorage.setItem(signalChoiceKey, choice);
    } catch { /* Signal denies when its storage cannot be read. */ }
    return saved;
  }

  function resolve(prompt = false) {
    if (!signal) return;
    const saved = syncChoice();
    // Ask the SDK whether even an explicit grant would be denied by GPC.
    const gpc = resolveEffectiveConsent({ storedChoice: "granted", gpc: serverGpc }).analytics_storage === "denied";
    setProtectedByGpc(gpc);
    google.apply();
    queueMicrotask(syncSignalPage);
    if (prompt) setOpen(saved === undefined && !gpc && currentConsent().analytics_storage === "denied");
  }

  onMount(() => {
    const controller = new AbortController();
    const options = { signal: controller.signal };
    window.addEventListener("storage", event => {
      if (event.key === signalChoiceKey || event.key === null) resolve();
    }, options);
    window.addEventListener("pageshow", event => {
      if (event.persisted) resolve();
      google.syncEngagement();
    }, options);
    window.addEventListener("focus", () => { resolve(); google.syncEngagement(); }, options);
    window.addEventListener("blur", () => google.endEngagement(), options);
    window.addEventListener("pagehide", () => google.endEngagement(), options);
    document.addEventListener("visibilitychange", () => {
      if (!document.hidden) resolve();
      google.syncEngagement();
    }, options);
    void fetch("/api/analytics/config", { cache: "no-store", credentials: "omit", signal: controller.signal })
      .then(response => response.ok ? response.json() : null)
      .then(async config => {
        if (controller.signal.aborted) return;
        const ga = typeof config?.measurementId === "string" && /^G-[A-Z0-9]{10}$/.test(config.measurementId)
          ? config.measurementId as string : undefined;
        const adsId: unknown = config?.googleAdsId;
        const label: unknown = config?.googleAdsPurchaseLabel;
        const configuredAds = validGoogleAdsId(adsId) && validGoogleAdsPurchaseLabel(label)
          ? { googleAdsId: adsId, googleAdsPurchaseLabel: label } : undefined;
        const key: unknown = config?.signalKey;
        const baseUrl: unknown = config?.signalApiOrigin;
        const ingest = typeof key === "string" && key.startsWith("sylphx_pk_")
          ? { apiKey: key, baseUrl: typeof baseUrl === "string" ? baseUrl : "https://api.sylphx.com" } : undefined;
        if (!ga && !configuredAds && !ingest) return;
        unpersistedDenial = migrateSignalChoice() === "denied";
        syncChoice();
        serverGpc = documentGpc || config.gpc === true;
        // Pass an explicit unknown sentinel so Signal doesn't do a second geo
        // lookup from a source the checkout server cannot consult.
        signal = createSignalBrowser({ storageKey: signalStorageKey, region: typeof config.region === "string" ? config.region : "XX", gpc: serverGpc, ingest });
        await signal.ready;
        if (controller.signal.aborted) return;
        setId(ga);
        setAds(configuredAds);
        setIngestEnabled(!!ingest);
        resolve(true);
      }).catch(() => {});
    onCleanup(() => { controller.abort(); signal?.dispose(); signal = undefined; google.stop(); });
  });

  function signalAllowed() {
    return !!signal && ingestEnabled() && currentConsent().analytics_storage === "granted";
  }

  function syncSignalPage() {
    if (!signal) return;
    syncChoice();
    signal.capture();
    const origin = window.location.origin;
    const page = analyticsPage(origin + route.pathname, "", origin);
    const path = page ? new URL(page.page_location).pathname : undefined;
    if (!signalAllowed()) {
      signalPath = undefined;
      checkoutAttempts.clear();
      eventScope = undefined;
      return;
    }
    if (signalPath === path) return;
    signalPath = path;
    checkoutAttempts.clear();
    routeVisit++;
    if (!path) return;
    // Never queue a pending SDK page view: on grant the SDK re-reads the raw
    // browser URL, which can have private query data. Emit the safe path on grant.
    void signal.pageView(path);
    if (localizedPath(path, defaultLocale) === "/pricing") {
      const entry_surface = knownCheckoutSurface(query.entry_surface) ?? pricingCheckoutSurface;
      forwardSignal("pricing_view", { path, entry_surface }, signalEventId("pricing_view:" + routeVisit));
    }
    if (!id()) void consumeSignup(signalAllowed, forwardSignup).catch(() => {});
  }

  createEffect(on(() => [route.pathname, route.search], () => {
    // The router publishes before history commits; capture needs window.location.
    queueMicrotask(syncSignalPage);
  }, { defer: true }));

  // Start GA if the first allowed page is reached by client navigation.
  createEffect(on(() => route.pathname, () => resolve(), { defer: true }));

  function purchaseSignalReady() {
    if (settings()?.analytics_storage !== "granted" || !signalAllowed()) return false;
    const choice = readSignalChoice(document.cookie);
    return choice === "analytics_only" || choice === "granted";
  }

  function forwardSignal(eventName: string, properties: Record<string, unknown>, eventId?: string) {
    if (!signalAllowed()) return false;
    void signal?.track({ eventName, properties, eventId });
    return true;
  }

  // Allocate an anonymous document scope only after consent. Repeated calls
  // for the same logical event reuse the same ID within that scope.
  function signalEventId(logicalEvent: string) {
    return (eventScope ??= mint("evt").toString()) + ":" + logicalEvent;
  }

  function forwardSignup(params: { method: string; entry_surface: string }) {
    // consumeSignup atomically claims the verified login marker across tabs.
    if (signalAllowed()) forwardSignal("sign_up", params, signalEventId("sign_up:" + params.method + ":" + params.entry_surface));
    google.emit("sign_up", params);
  }

  const trackEvent: TrackEvent = (event, params, options) => {
    try {
      const saved = syncChoice();
      const pricingBuy = event === "pricing_buy_clicked";
      if ((pricingBuy || options?.requireExplicitConsent) && (unpersistedDenial || (saved !== "analytics_only" && saved !== "granted"))) return false;
      const safe = behavioralParams(event, params);
      if (!safe) return false;
      if (pricingBuy) return forwardSignal(event, { ...omitSignalAttribution, ...safe });
      const sent = forwardSignal(event, safe);
      return google.emit(event, safe) || sent;
    } catch { return false; } // Analytics failures must not interrupt the UI or a native form submit.
  };

  createEffect(google.measurePage);

  function beginCheckout(form: HTMLFormElement, currency: string, surface: CheckoutSurface) {
    resolve();
    const blocked = form.elements.namedItem("signal_blocked");
    const touch = form.elements.namedItem("signal_touch");
    const allowed = currentConsent().analytics_storage === "granted";
    if (blocked instanceof HTMLInputElement) blocked.disabled = allowed;
    if (touch instanceof HTMLInputElement) {
      touch.disabled = !allowed;
      touch.value = allowed ? JSON.stringify(signal?.capture() ?? {}) : "";
    }
    const params = beginCheckoutParams(currency, surface);
    if (params) {
      google.sendPricing("begin_checkout", params);
      const { value, plan_sku, entry_surface } = params;
      const attempt = currency + ":" + entry_surface;
      if (signalAllowed() && !checkoutAttempts.has(attempt)) {
        checkoutAttempts.add(attempt);
        forwardSignal("begin_checkout", { value, currency, plan_sku, entry_surface },
          signalEventId("begin_checkout:" + routeVisit + ":" + attempt));
      }
    }
  }

  function choose(next: StoredChoice) {
    if (!signal || (protectedByGpc() && next !== "denied")) return;
    unpersistedDenial = !saveSignalChoice(next);
    signal.setConsent(unpersistedDenial ? "denied" : next);
    resolve();
    return true;
  }

  return {
    configured, protectedByGpc, campaignBlocked, choose, beginCheckout, trackEvent,
    ready: () => id() ? google.analyticsAllowed() : settings()?.analytics_storage === "granted" && signalAllowed(),
    adsReady: google.adsReady, purchaseSignalReady, claimPurchase: google.claimPurchase,
  };
}
