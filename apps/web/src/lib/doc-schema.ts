import type { DocPage } from "./docs";
import { localizedPath, pageLinks, type Locale } from "./locales";
import { lockdocsApplication, lockdocsSoftware, usdProOffer } from "./storefront-schema";

const sylphx = { "@type": "Organization", name: "Sylphx Limited" } as const;

export function docMetadata(page: DocPage, locale: Locale) {
  const title = locale === "en" ? page.title : "文件總覽";
  const description = locale === "en" ? page.description : "學會安裝 Lockdocs、設定 MCP，並查閱專案實際安裝版本的 API 文件。";
  const { canonical: url } = pageLinks(localizedPath(page.route, locale));
  const common = { "@context": "https://schema.org", url, description, inLanguage: locale };
  const schema = page.route === "/pro"
    ? lockdocsApplication(url, [usdProOffer])
    : page.route === "/docs"
      ? { ...common, "@type": "CollectionPage", name: title }
      : {
        ...common, "@type": "TechArticle",
        headline: page.route === "/compare" ? "Lockdocs vs Context7" : title,
        author: sylphx, publisher: sylphx, about: lockdocsSoftware,
      };
  return { title: title + " · Lockdocs", description, schema };
}
