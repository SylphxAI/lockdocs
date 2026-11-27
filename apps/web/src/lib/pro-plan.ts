import { billingCurrencies, proPrice } from "./analytics-events";

// One source for the Lockdocs Pro price and plan terms shown on pricing and
// in the sign-in purchase summary.
export type BillingCurrency = typeof billingCurrencies[number];
export const pricePrefixes: Record<BillingCurrency, string> = { USD: "US$", GBP: "£", EUR: "€", HKD: "HK$" };
export const annualAmount = (code: BillingCurrency) => Number(proPrice.unit_amounts[code]) / 100;
export const annualPrice = (code: BillingCurrency, numberLocale: string | readonly string[]) =>
  pricePrefixes[code] + new Intl.NumberFormat(numberLocale).format(annualAmount(code));
export const perYear = ["/yr", "／年"] as const;
export const annualBilling = ["Billed annually upfront · Tax included", "年費一次付清，已含稅"] as const;
