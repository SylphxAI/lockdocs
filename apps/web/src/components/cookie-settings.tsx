import { createContext, createEffect, createSignal, onCleanup, onMount, Show, useContext, type ParentProps } from "solid-js";
import { useLocation } from "@solidjs/router";
import { allowsDefault, clearAnalyticsCookies, gpc } from "@/lib/analytics-browser";
import { isPurchaseReference, items, pathByEvent, type FunnelEvent, type Purchase } from "@/lib/analytics-events";
import { consent, preferenceKey, preferenceLifetime, savedChoice, type Choice } from "@/lib/analytics-policy";

type Tag = Window & { startAnalytics?: (settings: ReturnType<typeof consent>) => void; gtag?: (...args: unknown[]) => void };
const CookieContext = createContext<{ active: () => boolean; beginCheckout: () => void; open: (button: HTMLButtonElement) => void; configured: () => boolean }>();
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
  let remembered: Choice | null = null;
  let generation = 0;
  let priced = false;

  function stop() {
    const frame = tag?.contentWindow as Tag | undefined;
    if (frame && id()) Object.assign(frame, { ["ga-disable-" + id()]: true });
    tag?.remove();
    tag = undefined;
    setActive(false);
    clearAnalyticsCookies();
  }

  function apply(next: Choice) {
    if (gpc()) next = "reject";
    if (next === "reject" || (next !== choice() && tag)) stop();
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
      if (frame !== tag || next !== choice() || gpc() || !client?.startAnalytics) return;
      client.startAnalytics(consent(next));
      setActive(Boolean(client.gtag));
    }, { once: true });
    frame.src = "/api/analytics/tag";
    document.body.append(frame);
  }

  async function resolve() {
    const run = ++generation;
    remembered = preference();
    const signal = gpc();
    setProtectedByGpc(signal);
    if (signal || remembered) {
      apply(signal ? "reject" : remembered!);
      setOpen(!remembered);
      return;
    }
    const allow = await allowsDefault();
    if (run !== generation) return;
    apply(allow ? "all" : "reject");
    setProtectedByGpc(gpc());
    setOpen(!allow || gpc());
  }

  onMount(() => {
    const controller = new AbortController();
    const options = { signal: controller.signal };
    window.addEventListener("storage", event => {
      if (id() && (event.key === preferenceKey || event.key === null)) {
        // Stop at once when a choice is cleared, even before another region lookup.
        if (!preference()) apply("reject");
        void resolve();
      }
    }, options);
    window.addEventListener("pageshow", event => { if (event.persisted && id()) void resolve(); }, options);
    void fetch("/api/analytics/config", { cache: "no-store", credentials: "omit", signal: controller.signal })
      .then(response => response.ok ? response.json() : null)
      .then(config => {
        if (controller.signal.aborted || !/^G-[A-Z0-9]{10}$/.test(config?.measurementId ?? "")) return;
        setId(config.measurementId);
        return resolve();
      }).catch(() => {});
    onCleanup(() => { controller.abort(); generation++; stop(); });
  });

  function send(event: FunnelEvent, details?: Purchase) {
    if (!active() || gpc()) return;
    (tag?.contentWindow as Tag | null)?.gtag?.("event", event, {
      ...details, ...(event !== "pricing_view" ? { items } : {}), send_to: id(),
      page_location: window.location.origin + pathByEvent[event], page_referrer: "", page_title: "Lockdocs",
    });
  }

  createEffect(() => {
    const pricing = route.pathname === "/pricing";
    if (!pricing) priced = false;
    if (pricing && active() && !priced) { priced = true; send("pricing_view"); }
  });

  createEffect(() => {
    if (route.pathname !== "/dashboard" || new URLSearchParams(route.search).get("billing") !== "return" || !active()) return;
    const controller = new AbortController();
    let retry: number | undefined;
    async function check(remaining: number) {
      try {
        const response = await fetch("/api/analytics/purchase", { method: "POST", cache: "no-store", signal: controller.signal });
        if (response.status === 202 && remaining > 0) retry = window.setTimeout(() => void check(remaining - 1), 2500);
        if (response.status === 200) {
          const { transaction_id, value, currency } = await response.json();
          if (!controller.signal.aborted && isPurchaseReference(transaction_id) &&
              typeof value === "number" && Number.isFinite(value) && value >= 0 &&
              typeof currency === "string" && /^[A-Z]{3}$/.test(currency)) {
            send("purchase", { transaction_id, value, currency });
          }
        }
      } catch { /* Analytics must never interrupt billing or account access. */ }
    }
    void check(2);
    onCleanup(() => { controller.abort(); window.clearTimeout(retry); });
  });

  function choose(next: Choice) {
    if (gpc() && next !== "reject") return;
    ++generation;
    remembered = next;
    try { localStorage.setItem(preferenceKey, JSON.stringify({ choice: next, expires: Date.now() + preferenceLifetime })); }
    catch { /* Keep this document's choice if storage is unavailable. */ }
    apply(next);
    close();
  }

  function close() { setOpen(false); trigger?.focus(); trigger = undefined; }
  function showFrom(button: HTMLButtonElement) {
    if (!id()) return;
    trigger = button;
    setOpen(true);
    queueMicrotask(() => panel?.focus());
  }

  return <CookieContext.Provider value={{ active: () => active() && !gpc(), beginCheckout: () => send("begin_checkout"), open: showFrom, configured: () => Boolean(id()) }}>
    {props.children}
    <Show when={id() && open()}><section id="cookie-settings" class="cookie-panel" aria-labelledby="cookie-heading" tabIndex={-1}
      ref={panel} onKeyDown={event => { if (event.key === "Escape" && trigger) close(); }}>
      <div><h2 id="cookie-heading">Cookie settings</h2><p>Essential cookies keep the site working. Optional Google Analytics cookies help us measure usage and ad performance. We never enable personalized ads. <a href="/legal/privacy">Privacy policy</a></p>
        <Show when={protectedByGpc()}><p role="status">Your browser’s Global Privacy Control is on. Optional cookies are blocked.</p></Show>
      </div>
      <div class="cookie-actions">
        <button type="button" class="button secondary" onClick={() => choose("reject")}>Reject optional</button>
        <button type="button" class="button secondary" disabled={protectedByGpc()} onClick={() => choose("analytics")}>Analytics only</button>
        <button type="button" class="button secondary" disabled={protectedByGpc()} onClick={() => choose("all")}>Allow analytics &amp; ads</button>
      </div>
    </section></Show>
  </CookieContext.Provider>;
}

export function CookieSettingsButton() {
  const settings = useCookieSettings();
  return <button class="footer-cookie" type="button" disabled={!settings.configured()} aria-controls="cookie-settings" onClick={event => settings.open(event.currentTarget)}>Cookie settings</button>;
}
