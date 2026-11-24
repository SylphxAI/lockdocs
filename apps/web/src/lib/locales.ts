import { canonicalOrigin } from "./indexing";

export const defaultLocale = "en";
const traditionalChinese = "zh-TW";
export type Locale = typeof defaultLocale | typeof traditionalChinese;
const chinesePrefix = "/" + traditionalChinese;
const translatedPaths = ["/", "/docs", "/playground", "/pricing"] as const;
export const chinesePaths = translatedPaths.map(path => chinesePrefix + (path === "/" ? "" : path));

export function pagePath(pathname: string) {
  return pathname.replace(/\/+$/, "") || "/";
}

export function pageLocale(pathname: string): Locale {
  return chinesePaths.includes(pagePath(pathname)) ? traditionalChinese : defaultLocale;
}

export function localizedPath(pathname: string, locale: Locale) {
  const path = pagePath(pathname);
  const translated = chinesePaths.indexOf(path);
  if (translated !== -1) return locale === traditionalChinese ? path : translatedPaths[translated];
  const index = translatedPaths.findIndex(candidate => candidate === path);
  return locale === traditionalChinese && index !== -1 ? chinesePaths[index] : path;
}

export function pageLinks(pathname: string) {
  const english = localizedPath(pathname, defaultLocale);
  const chinese = localizedPath(english, traditionalChinese);
  return {
    canonical: canonicalOrigin + pagePath(pathname),
    alternates: [
      { locale: defaultLocale, href: canonicalOrigin + english },
      ...(chinese === english ? [] : [{ locale: traditionalChinese, href: canonicalOrigin + chinese }]),
      { locale: "x-default", href: canonicalOrigin + english },
    ],
  };
}
