import { useLocation } from "@solidjs/router";
import { localizedPath, pageLocale } from "./locales";

export function useSiteLanguage() {
  const location = useLocation();
  const locale = () => pageLocale(location.pathname);
  return {
    locale,
    text: (en: string, zh: string) => locale() === "en" ? en : zh,
    href: (path: string) => localizedPath(path, locale()),
  };
}
