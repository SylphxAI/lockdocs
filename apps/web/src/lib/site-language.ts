import { useLocation } from "@solidjs/router";
import { useContext } from "solid-js";
import { CampaignBlocked, withCampaign } from "./campaign-navigation";
import type { CheckoutSurface } from "./analytics-events";
import { pricingPath, signInLocale } from "./auth-entry";
import { localizedPath, pageLocale, pagePath } from "./locales";

export function localeForLocation(pathname: string, search: string) {
  return pagePath(pathname) === "/sign-in" ? signInLocale(search) : pageLocale(pathname);
}

export function useSiteLanguage() {
  const location = useLocation();
  const locale = () => localeForLocation(location.pathname, location.search);
  const campaignBlocked = useContext(CampaignBlocked);
  const carryCampaign = (href: string) => campaignBlocked?.() === false ? withCampaign(href, location.search) : href;
  return {
    carryCampaign,
    proHref: (surface: CheckoutSurface) => carryCampaign(pricingPath("USD", surface, locale())),
    locale,
    text: (en: string, zh: string) => locale() === "en" ? en : zh,
    href: (path: string) => localizedPath(path, locale()),
    campaignHref: (path: string) => carryCampaign(localizedPath(path, locale())),
  };
}
