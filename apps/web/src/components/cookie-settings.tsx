import { createContext, createEffect, createSignal, on, onCleanup, onMount, Show, useContext, type ParentProps } from "solid-js";
import { useLocation } from "@solidjs/router";
import { createSignalBrowser, resolveEffectiveConsent, type SignalConsent } from "@sylphx/sdk/signal";
import { clearAnalyticsCookies, migrateSignalChoice, saveSignalChoice } from "@/lib/signal-browser";
import { readSignalChoice, signalChoiceKey, signalStorageKey, type StoredChoice } from "@/lib/signal-privacy";
import { analyticsPage } from "@/lib/analytics-page";
import { beginCheckoutParams } from "@/lib/analytics-events";
import { consumeSignup } from "@/lib/signup-analytics";
import { useSiteLanguage } from "@/lib/site-language";
import { localizedPath, defaultLocale } from "@/lib/locales";

type Page = NonNullable<ReturnType<typeof analyticsPage>>;
type Tag = Window & { gtag?: (...args: unknown[]) => void; dataLayer?: IArguments[] };
function startTag(frame: HTMLIFrameElement, id: string, consent: SignalConsent, page: Page) {
  const client = frame.contentWindow as Tag | null;
  const doc = frame.contentDocument;
  if (!client || !doc?.documentElement.hasAttribute("data-analytics-frame")) return false;
  // Even a tag-internal event that ignores our page_location must fall back
  // to the real, sanitized landing page, not the tag transport URL.
  try { client.history.replaceState(null, "", page.page_location); } catch { return false; }
  const commands: IArguments[] = [];
  client.dataLayer = commands;
  client.gtag = function () { commands.push(arguments); };
  client.gtag("consent", "default", consent);
  // Supply scrubbed page context before the tag executes. Suppress both the
  // bootstrap and config page views: only the parent emits a page_view.
  client.gtag("set", { ...page, send_page_view: false });
  client.gtag("js", new Date());
  client.gtag("config", id, {
    send_page_view: false,
    allow_google_signals: consent.ad_user_data === "granted",
    allow_ad_personalization_signals: consent.ad_personalization === "granted",
    cookie_domain: "none",
  });
  const script = doc.createElement("script");
  script.async = true;
  script.referrerPolicy = "no-referrer";
  script.src = "https://www.googletagmanager.com/gtag/js?id=" + id;
  doc.head.append(script);
  return true;
}
const home = (origin: string) => ({ page_location: origin + "/", page_referrer: "", page_title: "Lockdocs" });
const CookieContext = createContext<{ beginCheckout: (form: HTMLFormElement, currency: string) => void; open: (button: HTMLButtonElement) => void; configured: () => boolean }>();
export const useCookieSettings = () => useContext(CookieContext)!;

export function CookieSettings(props: ParentProps) {
  const route = useLocation();
  const { href, text } = useSiteLanguage();
  const [id, setId] = createSignal<string>();
  const [settings, setSettings] = createSignal<SignalConsent>();
  const [open, setOpen] = createSignal(false);
  const [protectedByGpc, setProtectedByGpc] = createSignal(false);
  const [active, setActive] = createSignal(false);
  let signal: ReturnType<typeof createSignalBrowser> | undefined;
  let unpersistedDenial = false;
  let serverGpc = false;
  let tag: HTMLIFrameElement | undefined;
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
    setActive(false);
    measuredPage = null;
    previousLocation = "";
    endEngagement(false);
    clearAnalyticsCookies();
  }

  function currentConsent() {
    // GPC can change after first paint; the SDK always rechecks navigator GPC.
    return signal?.getConsent() ?? resolveEffectiveConsent();
  }

  function analyticsAllowed() {
    return !!settings() && active() && currentConsent().analytics_storage === "granted";
  }

  function apply() {
    const next = currentConsent();
    const previous = settings();
    if (next.analytics_storage === "denied" || (tag && next.ad_storage !== previous?.ad_storage)) stop();
    setSettings(next);
    const measurement = id();
    if (next.analytics_storage !== "granted" || tag || !measurement) return;
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
      if (consent.analytics_storage !== "granted" || !landing || !startTag(frame, measurement, consent, landing)) {
        stop();
        return;
      }
      setActive(true);
      void consumeSignup(analyticsAllowed, params => emit("sign_up", params)).catch(() => {});
    }, { once: true });
    frame.src = "/api/analytics/tag";
    document.body.append(frame);
  }

  function syncChoice() {
    const saved = readSignalChoice(document.cookie);
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
    if (prompt) setOpen(saved === undefined && !gpc && currentConsent().analytics_storage === "denied");
  }

  createEffect(() => {
    if (!id() || !open() || !panel) return;
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
        if (controller.signal.aborted || !/^G-[A-Z0-9]{10}$/.test(config?.measurementId ?? "")) return;
        unpersistedDenial = migrateSignalChoice() === "denied";
        syncChoice();
        serverGpc = config.gpc === true;
        // Pass an explicit unknown sentinel so Signal doesn't do a second geo
        // lookup from a source the checkout server cannot consult.
        signal = createSignalBrowser({ storageKey: signalStorageKey, region: typeof config.region === "string" ? config.region : "XX", gpc: serverGpc });
        await signal.ready;
        if (controller.signal.aborted) return;
        setId(config.measurementId);
        resolve(true);
      }).catch(() => {});
    onCleanup(() => { controller.abort(); signal?.dispose(); stop(); });
  });

  createEffect(() => {
    void route.search;
    if (settings()?.analytics_storage !== "granted") return;
    // The router updates its search before window.location; Signal reads the latter.
    queueMicrotask(() => { syncChoice(); signal?.capture(); });
  });

  // Start GA if the first allowed page is reached by client navigation.
  createEffect(on(() => route.pathname, () => resolve(), { defer: true }));

  function emit(event: string, params: Record<string, unknown>) {
    if (!analyticsAllowed()) return;
    (tag?.contentWindow as Tag | null)?.gtag?.("event", event, { ...params, send_to: id() });
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

  function beginCheckout(form: HTMLFormElement, currency: string) {
    resolve();
    const blocked = form.elements.namedItem("signal_blocked");
    const touch = form.elements.namedItem("signal_touch");
    const allowed = currentConsent().analytics_storage === "granted";
    if (blocked instanceof HTMLInputElement) blocked.disabled = allowed;
    if (touch instanceof HTMLInputElement) {
      touch.disabled = !allowed;
      touch.value = allowed ? JSON.stringify(signal?.capture() ?? {}) : "";
    }
    const params = beginCheckoutParams(currency);
    if (params) sendPricing("begin_checkout", params);
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
    if (!id()) return;
    trigger = button;
    setOpen(true);
    queueMicrotask(() => panel?.focus());
  }

  return <CookieContext.Provider value={{ beginCheckout, open: showFrom, configured: () => Boolean(id()) }}>
    {props.children}
    <Show when={id() && open()}><section id="cookie-settings" class="cookie-panel" aria-labelledby="cookie-heading" tabIndex={-1}
      ref={panel} onKeyDown={event => { if (event.key === "Escape" && trigger) close(); }}>
      <div class="cookie-copy">
        <h2 id="cookie-heading">{text("Cookie settings", "Cookie 設定")}</h2>
        <p><Show when={protectedByGpc()} fallback={text("Optional cookies measure usage and can personalize ads.", "選用 Cookie 用於分析使用狀況與廣告成效，並可提供個人化廣告。")}>
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
