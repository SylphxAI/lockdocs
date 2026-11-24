import { useLocation } from "@solidjs/router";
import { withCampaign } from "./campaign-navigation";
import { localizedPath, pageLocale } from "./locales";

export function useSiteLanguage() {
  const location = useLocation();
  const locale = () => pageLocale(location.pathname);
  const carryCampaign = (href: string) => withCampaign(href, location.search);
  return {
    carryCampaign,
    locale,
    text: (en: string, zh: string) => locale() === "en" ? en : zh,
    href: (path: string) => localizedPath(path, locale()),
    campaignHref: (path: string) => carryCampaign(localizedPath(path, locale())),
  };
}
