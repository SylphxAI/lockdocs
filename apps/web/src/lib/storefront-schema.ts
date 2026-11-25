import { canonicalOrigin } from "./indexing";
import { proPrice } from "./analytics-events";

export function proOffer(currency: keyof typeof proPrice.unit_amounts, url: string) {
  const price = Number(proPrice.unit_amounts[currency]) / 100;
  return {
    "@type": "Offer",
    name: "Lockdocs Pro",
    description: "One seat with call-site impact reports from lockdocs upgrade and exact-version docs from private package registries.",
    url,
    price,
    priceCurrency: currency,
    priceSpecification: {
      "@type": "UnitPriceSpecification",
      price,
      priceCurrency: currency,
      billingDuration: "P1Y",
      valueAddedTaxIncluded: true,
    },
  };
}

export const usdProOffer = proOffer("USD", canonicalOrigin + "/pricing");

export function freeOffer(currency: string) {
  return { "@type": "Offer", name: "Lockdocs Free", url: canonicalOrigin + "/guide", price: 0, priceCurrency: currency };
}

export const lockdocsSoftware = {
  "@type": "SoftwareApplication",
  "@id": canonicalOrigin + "/#lockdocs",
  name: "Lockdocs",
  applicationCategory: "DeveloperApplication",
  operatingSystem: "Linux, macOS, Windows",
} as const;

export function lockdocsApplication(url: string, offers: Record<string, unknown>[]) {
  return { "@context": "https://schema.org", ...lockdocsSoftware, url, offers };
}
