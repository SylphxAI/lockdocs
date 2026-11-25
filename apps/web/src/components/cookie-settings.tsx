import { createContext, createEffect, createSignal, on, onCleanup, onMount, Show, useContext, type ParentProps } from "solid-js";
import { useLocation, useSearchParams } from "@solidjs/router";
import { getRequestEvent } from "solid-js/web";
import { mint } from "@sylphx/sdk/ids";
import { createSignalBrowser, resolveEffectiveConsent, type SignalConsent } from "@sylphx/sdk/signal";
import { clearAnalyticsCookies, migrateSignalChoice, saveSignalChoice } from "@/lib/signal-browser";
import { readSignalChoice, signalChoiceKey, signalRequestGpc, signalStorageKey, type StoredChoice } from "@/lib/signal-privacy";
import { CampaignBlocked } from "@/lib/campaign-navigation";
import { analyticsPage } from "@/lib/analytics-page";
import { behavioralParams, beginCheckoutParams, knownCheckoutSurface, pricingCheckoutSurface, type CheckoutSurface, type ProSurface, type TrackEvent } from "@/lib/analytics-events";
import { consumeSignup } from "@/lib/signup-analytics";
import { useSiteLanguage } from "@/lib/site-language";
import { localizedPath, defaultLocale } from "@/lib/locales";

import {
  claimedPurchases, claimedPurchasesKey, currencyCodePattern, fullAdsConsent, maxClaimedPurchases,
  purchaseConversionPath, transactionIdPattern, validGoogleAdsId, validGoogleAdsPurchaseLabel,
} from "@/lib/google-ads";

type Page = NonNullable<ReturnType<typeof analyticsPage>>;
type Tag = Window & { gtag?: (...args: unknown[]) => void; dataLayer?: IArguments[] };
type Ads = { googleAdsId: string; googleAdsPurchaseLabel: string };
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
type PurchaseDestinations = { signal: boolean; ads: boolean };
const CookieContext = createContext<{
  ready: () => boolean;
  trackEvent: TrackEvent;
  beginCheckout: (form: HTMLFormElement, currency: string, surface: CheckoutSurface) => void;
  open: (button: HTMLButtonElement) => void;
  configured: () => boolean;
  adsReady: () => boolean;
  purchaseSignalReady: () => boolean;
  claimPurchase: (sid: string, destinations: PurchaseDestinations, abort: AbortSignal) => Promise<void>;
}>();
export const useCookieSettings = () => useContext(CookieContext)!;

// Count an offer when it comes into view with analytics active, once per mount.
// No observer or replay of pre-consent impressions while analytics is disabled.
export function useProImpression(surface: ProSurface) {
  const { ready, trackEvent } = useCookieSettings();
  const [element, setElement] = createSignal<HTMLElement>();
  let seen = false;
  createEffect(() => {
    const target = element();
    if (!target || seen || !ready()) return;
    const observer = new IntersectionObserver(entries => {
      if (entries.some(entry => entry.isIntersecting && entry.intersectionRatio >= 0.25) &&
        trackEvent("paywall_view", { entry_surface: surface })) {
        seen = true;
        observer.disconnect();
      }
    }, { threshold: 0.25 });
    observer.observe(target);
    onCleanup(() => observer.disconnect());
  });
  return (target: HTMLElement) => { setElement(target); };
}

export function CookieSettings(props: ParentProps) {
  const route = useLocation();
  const [query] = useSearchParams();
  const { href, text } = useSiteLanguage();
  const [id, setId] = createSignal<string>();
  const [ads, setAds] = createSignal<Ads>();
  const [ingestEnabled, setIngestEnabled] = createSignal(false);
  const configured = () => Boolean(id() || ads() || ingestEnabled());
  const [settings, setSettings] = createSignal<SignalConsent>();
  const [open, setOpen] = createSignal(false);
  const [protectedByGpc, setProtectedByGpc] = createSignal(false);
  const [active, setActive] = createSignal(false);
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
  let tag: HTMLIFrameElement | undefined;
  let adsConfiguredInTag = false;
  let trigger: HTMLButtonElement | undefined;
  let panel: HTMLElement | undefined;
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

  function currentConsent() {
    // GPC can change after first paint; the SDK always rechecks navigator GPC.
    const consent = signal?.getConsent() ?? resolveEffectiveConsent();
    // A regional GA default is not an explicit grant to the Ads destination.
    if (!ads() || (!unpersistedDenial && readSignalChoice(document.cookie) === "granted")) return consent;
    return { ...consent, ad_storage: "denied", ad_user_data: "denied", ad_personalization: "denied" } as const;
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
    const page = analyticsPage(origin + route.pathname, document.referrer, origin);
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
      const landing = analyticsPage(origin + route.pathname, document.referrer, origin);
      const adsDestination = fullAdsConsent(consent) ? ads() : undefined;
      const landingUrl = landing && new URL(landing.page_location);
      if (adsDestination && landingUrl) {
        // Let the Ads tag see only Signal-validated click IDs, never the raw
        // query or checkout session. GA's page_location stays path-only.
        const touch = signal?.capture();
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
    apply();
    queueMicrotask(syncSignalPage);
    if (prompt) setOpen(saved === undefined && !gpc && currentConsent().analytics_storage === "denied");
  }

  createEffect(() => {
    if (!configured() || !open() || !panel) return;
    const element = panel;
    const root = document.documentElement;
    const reserveSpace = () => root.style.setProperty("--cookie-bar-height", element.getBoundingClientRect().height + "px");
    const observer = new ResizeObserver(reserveSpace);
    reserveSpace();
    observer.observe(element);
    onCleanup(() => {
      observer.disconnect();
      root.style.removeProperty("--cookie-bar-height");
    });
  });

  onMount(() => {
    const controller = new AbortController();
    const options = { signal: controller.signal };
    window.addEventListener("storage", event => {
      if (event.key === signalChoiceKey || event.key === null) resolve();
    }, options);
    window.addEventListener("pageshow", event => {
      if (event.persisted) resolve();
      syncEngagement();
    }, options);
    window.addEventListener("focus", () => { resolve(); syncEngagement(); }, options);
    window.addEventListener("blur", () => endEngagement(), options);
    window.addEventListener("pagehide", () => endEngagement(), options);
    document.addEventListener("visibilitychange", () => {
      if (!document.hidden) resolve();
      syncEngagement();
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
    onCleanup(() => { controller.abort(); signal?.dispose(); signal = undefined; stop(); });
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

  function adsReady() {
    const consent = settings();
    return active() && adsConfiguredInTag && !!consent && fullAdsConsent(consent) && fullAdsConsent(currentConsent());
  }

  function purchaseSignalReady() {
    if (settings()?.analytics_storage !== "granted" || !signalAllowed()) return false;
    const choice = readSignalChoice(document.cookie);
    return choice === "analytics_only" || choice === "granted";
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
    emit("sign_up", params);
  }

  const trackEvent: TrackEvent = (event, params, options) => {
    try {
      const saved = syncChoice();
      if (options?.requireExplicitConsent && (unpersistedDenial || (saved !== "analytics_only" && saved !== "granted"))) return false;
      const safe = behavioralParams(event, params);
      if (!safe) return false;
      const sent = forwardSignal(event, safe);
      return emit(event, safe) || sent;
    } catch { return false; } // Analytics failures must not interrupt the UI or a native form submit.
  };

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

  createEffect(() => {
    // The router publishes the new pathname before browser history is updated.
    const pathname = route.pathname;
    if (!analyticsAllowed()) return;
    const { origin } = window.location;
    const page = analyticsPage(origin + pathname, previousLocation || document.referrer, origin);
    if (measuredPage && measuredPage.page_location === page?.page_location) return;
    endEngagement();
    measuredPage = page;
    previousLocation = page?.page_location ?? "";
    (tag?.contentWindow as Tag | null)?.gtag?.("set", page ?? home(origin));
    if (page) {
      emit("page_view", page);
      if (localizedPath(pathname, defaultLocale) === "/pricing") sendPricing("pricing_view");
      syncEngagement();
    }
  });

  function sendPricing(event: "pricing_view" | "begin_checkout", params: Record<string, unknown> = {}) {
    emit(event, {
      ...params,
      page_location: window.location.origin + href("/pricing"), page_referrer: "", page_title: "Lockdocs",
    });
  }

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
      sendPricing("begin_checkout", params);
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
    close();
  }

  function close() { setOpen(false); trigger?.focus(); trigger = undefined; }
  function showFrom(button: HTMLButtonElement) {
    if (!configured()) return;
    trigger = button;
    setOpen(true);
    queueMicrotask(() => panel?.focus());
  }

  return <CookieContext.Provider value={{ beginCheckout, trackEvent, ready: () => id() ? analyticsAllowed() : settings()?.analytics_storage === "granted" && signalAllowed(), open: showFrom, configured, adsReady, purchaseSignalReady, claimPurchase }}>
    <CampaignBlocked.Provider value={campaignBlocked}>{props.children}</CampaignBlocked.Provider>
    <Show when={configured() && open()}><section id="cookie-settings" class="cookie-panel" aria-labelledby="cookie-heading" tabIndex={-1}
      ref={panel} onKeyDown={event => { if (event.key === "Escape" && trigger) close(); }}>
      <div class="cookie-copy">
        <h2 id="cookie-heading">{text("Cookie settings", "Cookie 設定")}</h2>
        <p><Show when={protectedByGpc()} fallback={text("Optional cookies: analytics and personalized ads.", "選用 Cookie 用於分析與個人化廣告。")}>
          <span role="status">{text("Global Privacy Control blocks optional cookies.", "全域隱私控制（GPC）已封鎖選用 Cookie。")}</span>
        </Show>{" "}<a href="/legal/privacy">{text("Privacy policy", "隱私權政策")}</a></p>
      </div>
      <div class="cookie-actions">
        <button type="button" class="cookie-choice" onClick={() => choose("denied")}>{text("Reject optional", "拒絕選用項目")}</button>
        <button type="button" class="cookie-choice" disabled={protectedByGpc()} onClick={() => choose("analytics_only")}>{text("Analytics only", "僅限分析")}</button>
        <button type="button" class="cookie-choice" disabled={protectedByGpc()} onClick={() => choose("granted")}>{text("Allow analytics & ads", "允許分析與廣告")}</button>
      </div>
    </section></Show>
  </CookieContext.Provider>;
}

export function CookieSettingsButton() {
  const settings = useCookieSettings();
  const { text } = useSiteLanguage();
  return <button class="footer-cookie" type="button" disabled={!settings.configured()} aria-controls="cookie-settings" onClick={event => settings.open(event.currentTarget)}>{text("Cookie settings", "Cookie 設定")}</button>;
}
