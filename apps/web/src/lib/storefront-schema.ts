import { canonicalOrigin } from "./indexing";

export function freeOffer(currency: string) {
  return { "@type": "Offer", name: "Lockdocs Free", url: canonicalOrigin + "/guide", price: 0, priceCurrency: currency };
}

export function lockdocsApplication(url: string, offers: Record<string, unknown>[]) {
  return {
    "@context": "https://schema.org",
    "@type": "SoftwareApplication",
    "@id": canonicalOrigin + "/#lockdocs",
    name: "Lockdocs",
    url,
    applicationCategory: "DeveloperApplication",
    operatingSystem: "Linux, macOS, Windows",
    offers,
  };
}
