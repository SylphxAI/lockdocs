import { billingCurrencies } from "./analytics-events";
import { validateNext } from "./auth-entry";
import type { Locale } from "./locales";
import { annualPrice, type BillingCurrency } from "./pro-plan";

// `path` is a literal union so the Back link stays a statically known page.
export type PurchaseResume = { currency: BillingCurrency; price: string; path: "/pricing" | "/zh-TW/pricing"; query: string };

/**
 * The Pro purchase a signed-out buyer started, read only from the validated
 * return target. The price comes from the catalogue, never from the URL.
 */
export function purchaseResume(next: string, locale: Locale): PurchaseResume | undefined {
  if (validateNext(next) !== next) return;
  const [path, query = ""] = next.split("?", 2);
  if (path !== "/pricing" && path !== "/zh-TW/pricing") return;
  const selected = new URLSearchParams(query).get("currency") ?? "USD";
  const currency = billingCurrencies.find(code => code === selected);
  if (!currency) return;
  // Same rule as the pricing page's server render: the page locale formats the number.
  return { currency, price: annualPrice(currency, locale), path, query };
}
