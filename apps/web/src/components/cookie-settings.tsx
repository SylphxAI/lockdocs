import { createContext, createEffect, createSignal, onCleanup, Show, useContext, type ParentProps } from "solid-js";
import { CampaignBlocked } from "@/lib/campaign-navigation";
import type { CheckoutSurface, ProSurface, TrackEvent } from "@/lib/analytics-events";
import type { StoredChoice } from "@/lib/signal-privacy";
import { useSiteLanguage } from "@/lib/site-language";
import { createCookieConsent } from "./cookie-consent";
import { CookiePanel } from "./cookie-panel";
import type { PurchaseDestinations } from "./cookie-google-analytics";

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
  const [open, setOpen] = createSignal(false);
  const { configured, protectedByGpc, campaignBlocked, choose, ...analytics } = createCookieConsent(setOpen);
  let trigger: HTMLButtonElement | undefined;
  let panel: HTMLElement | undefined;

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

  function close() { setOpen(false); trigger?.focus(); trigger = undefined; }
  function showFrom(button: HTMLButtonElement) {
    if (!configured()) return;
    trigger = button;
    setOpen(true);
    queueMicrotask(() => panel?.focus());
  }

  function chooseAndClose(next: StoredChoice) {
    if (choose(next)) close();
  }

  return <CookieContext.Provider value={{ ...analytics, open: showFrom, configured }}>
    <CampaignBlocked.Provider value={campaignBlocked}>{props.children}</CampaignBlocked.Provider>
    <Show when={configured() && open()}>
      <CookiePanel protectedByGpc={protectedByGpc()} choose={chooseAndClose}
        ref={element => { panel = element; }} onEscape={() => { if (trigger) close(); }} />
    </Show>
  </CookieContext.Provider>;
}

export function CookieSettingsButton() {
  const settings = useCookieSettings();
  const { text } = useSiteLanguage();
  return <button class="footer-cookie" type="button" disabled={!settings.configured()} aria-controls="cookie-settings" onClick={event => settings.open(event.currentTarget)}>{text("Cookie settings", "Cookie 設定")}</button>;
}
