import type { SignalConsent } from "@sylphx/sdk/signal";

export function validGoogleAdsId(id: unknown): id is string {
  return typeof id === "string" && /^AW-[0-9]{6,15}$/.test(id);
}

export function validGoogleAdsPurchaseLabel(label: unknown): label is string {
  return typeof label === "string" && /^[A-Za-z0-9_-]{1,64}$/.test(label);
}

export function fullAdsConsent(consent: SignalConsent) {
  return consent.ad_storage === "granted" && consent.ad_user_data === "granted";
}

export function liveCheckoutSessionId(id: unknown): id is string {
  return typeof id === "string" && /^cs_live_[A-Za-z0-9_-]{1,124}$/.test(id);
}

export const purchaseConversionPath = "/api/billing/purchase-conversion";
export const claimedPurchasesKey = "lockdocs.ads-purchases.v1";
export const maxClaimedPurchases = 24;
export const transactionIdPattern = /^[a-f0-9]{32}$/;
export const currencyCodePattern = /^[A-Z]{3}$/;

export function claimedPurchases(value: string) {
  return value.split(",").filter(id => transactionIdPattern.test(id)).slice(-maxClaimedPurchases);
}
