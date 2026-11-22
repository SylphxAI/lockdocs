import { createContext, createEffect, createSignal, onCleanup, onMount, Show, useContext, type ParentProps } from "solid-js";
import { useLocation } from "@solidjs/router";
import { clearAnalyticsCookies, gpc } from "@/lib/analytics-browser";
import { analyticsPage } from "@/lib/analytics-page";
import { beginCheckoutParams } from "@/lib/analytics-events";
import { campaignCookie, campaignMaxAge, checkoutCampaign } from "@/lib/checkout-campaign";
import { consent, preferenceKey, preferenceLifetime, savedChoice, type Choice } from "@/lib/analytics-policy";

type Page = NonNullable<ReturnType<typeof analyticsPage>>;
type Tag = Window & { startAnalytics?: (settings: ReturnType<typeof consent>, page: Page) => void; gtag?: (...args: unknown[]) => void };
const home = (origin: string) => ({ page_location: origin + "/", page_referrer: "", page_title: "Lockdocs" });
const CookieContext = createContext<{ checkoutChoice: () => Choice; beginCheckout: (currency: string) => void; open: (button: HTMLButtonElement) => void; configured: () => boolean }>();
export const useCookieSettings = () => useContext(CookieContext)!;

function preference() {
  try { return savedChoice(localStorage.getItem(preferenceKey)); } catch { return null; }
}

export function CookieSettings(props: ParentProps) {
  const route = useLocation();
  const [id, setId] = createSignal<string>();
  const [choice, setChoice] = createSignal<Choice>("reject");
  const [open, setOpen] = createSignal(false);
  const [protectedByGpc, setProtectedByGpc] = createSignal(false);
  const [active, setActive] = createSignal(false);
  let tag: HTMLIFrameElement | undefined;
  let trigger: HTMLButtonElement | undefined;
  let panel: HTMLElement | undefined;
  let priced = false;
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

  function apply(next: Choice) {
    if (gpc()) next = "reject";
    if (next === "reject" || (next !== choice() && tag)) stop();
    if (next !== "all" && document.cookie.split(";").some(entry => entry.trimStart().startsWith(campaignCookie + "="))) {
      setCampaignCookie("", 0);
    }
    setChoice(next);
    if (next === "reject") return;
    if (tag || !id()) return;
    const frame = document.createElement("iframe");
    tag = frame;
    frame.title = "Analytics";
    frame.hidden = true;
    frame.referrerPolicy = "no-referrer";
    frame.sandbox.add("allow-scripts", "allow-same-origin");
    frame.addEventListener("load", () => {
      const client = frame.contentWindow as Tag | null;
      if (frame !== tag || next !== currentChoice() || !client?.startAnalytics) return;
      const { origin } = window.location;
      client.startAnalytics(consent(next), analyticsPage(origin + route.pathname, document.referrer, origin) ?? home(origin));
      setActive(Boolean(client.gtag));
    }, { once: true });
    frame.src = "/api/analytics/tag";
    document.body.append(frame);
  }

  function resolve() {
    const saved = preference();
    const protectedBySignal = gpc();
    setProtectedByGpc(protectedBySignal);
    apply(protectedBySignal ? "reject" : saved ?? "reject");
    setOpen(!saved);
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
      if (id() && (event.key === preferenceKey || event.key === null)) {
        resolve();
      }
    }, options);
    window.addEventListener("pageshow", event => {
      if (event.persisted && id()) resolve();
      syncEngagement();
    }, options);
    window.addEventListener("focus", syncEngagement, options);
    window.addEventListener("blur", () => endEngagement(), options);
    window.addEventListener("pagehide", () => endEngagement(), options);
    document.addEventListener("visibilitychange", syncEngagement, options);
    void fetch("/api/analytics/config", { cache: "no-store", credentials: "omit", signal: controller.signal })
      .then(response => response.ok ? response.json() : null)
      .then(config => {
        if (controller.signal.aborted || !/^G-[A-Z0-9]{10}$/.test(config?.measurementId ?? "")) return;
        setId(config.measurementId);
        return resolve();
      }).catch(() => {});
    onCleanup(() => { controller.abort(); stop(); });
  });

  function currentChoice(): Choice {
    if (gpc()) return "reject";
    // An absent, expired, unreadable or changed preference is never consent.
    return preference() === choice() ? choice() : "reject";
  }

  const checkoutChoice = (): Choice => active() ? currentChoice() : "reject";

  function setCampaignCookie(value: string, maxAge: number) {
    document.cookie = campaignCookie + "=" + value + "; Max-Age=" + maxAge + "; Path=/; Secure; SameSite=Lax";
  }

  createEffect(() => {
    const query = route.search;
    if (currentChoice() !== "all") return;
    const clicks = checkoutCampaign(new URLSearchParams(query));
    if (Object.keys(clicks).length) {
      setCampaignCookie(encodeURIComponent(new URLSearchParams(clicks).toString()), campaignMaxAge);
    }
  });

  function emit(event: string, params: Record<string, unknown>) {
    if (checkoutChoice() === "reject") return;
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
    if (!measuredPage || checkoutChoice() === "reject" || document.hidden || !document.hasFocus()) {
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
    if (checkoutChoice() === "reject") return;
    const { origin } = window.location;
    const page = analyticsPage(origin + pathname, previousLocation || document.referrer, origin);
    if (measuredPage && measuredPage.page_location === page?.page_location) return;
    endEngagement();
    measuredPage = page;
    previousLocation = page?.page_location ?? "";
    (tag?.contentWindow as Tag | null)?.gtag?.("set", page ?? home(origin));
    if (page) {
      emit("page_view", page);
      syncEngagement();
    }
  });

  function sendPricing(event: "pricing_view" | "begin_checkout", params: Record<string, unknown> = {}) {
    emit(event, {
      ...params,
      page_location: window.location.origin + "/pricing", page_referrer: "", page_title: "Lockdocs",
    });
  }

  function beginCheckout(currency: string) {
    const params = beginCheckoutParams(currency);
    if (params) sendPricing("begin_checkout", params);
  }

  createEffect(() => {
    const pricing = route.pathname === "/pricing";
    if (!pricing) priced = false;
    if (pricing && active() && !priced) { priced = true; sendPricing("pricing_view"); }
  });

  function choose(next: Choice) {
    if (gpc() && next !== "reject") return;
    try { localStorage.setItem(preferenceKey, JSON.stringify({ choice: next, expires: Date.now() + preferenceLifetime })); }
    catch { /* Refuse analytics if this choice cannot be saved and read back. */ }
    apply(preference() === next ? next : "reject");
    close();
  }

  function close() { setOpen(false); trigger?.focus(); trigger = undefined; }
  function showFrom(button: HTMLButtonElement) {
    if (!id()) return;
    trigger = button;
    setOpen(true);
    queueMicrotask(() => panel?.focus());
  }

  return <CookieContext.Provider value={{ checkoutChoice, beginCheckout, open: showFrom, configured: () => Boolean(id()) }}>
    {props.children}
    <Show when={id() && open()}><section id="cookie-settings" class="cookie-panel" aria-labelledby="cookie-heading" tabIndex={-1}
      ref={panel} onKeyDown={event => { if (event.key === "Escape" && trigger) close(); }}>
      <div class="cookie-copy">
        <h2 id="cookie-heading">Cookie settings</h2>
        <p><Show when={protectedByGpc()} fallback="Optional cookies measure usage and ads.">
          <span role="status">Global Privacy Control blocks optional cookies.</span>
        </Show>{" "}<a href="/legal/privacy">Privacy policy</a></p>
      </div>
      <div class="cookie-actions">
        <button type="button" class="cookie-choice" onClick={() => choose("reject")}>Reject optional</button>
        <button type="button" class="cookie-choice" disabled={protectedByGpc()} onClick={() => choose("analytics")}>Analytics only</button>
        <button type="button" class="cookie-choice" disabled={protectedByGpc()} onClick={() => choose("all")}>Allow analytics &amp; ads</button>
      </div>
    </section></Show>
  </CookieContext.Provider>;
}

export function CookieSettingsButton() {
  const settings = useCookieSettings();
  return <button class="footer-cookie" type="button" disabled={!settings.configured()} aria-controls="cookie-settings" onClick={event => settings.open(event.currentTarget)}>Cookie settings</button>;
}
