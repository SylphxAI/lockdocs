import { validGoogleAdsId, validGoogleAdsPurchaseLabel } from "./google-ads";

export function googleAdsConfig() {
  const googleAdsId = process.env.LOCKDOCS_GOOGLE_ADS_ID;
  const googleAdsPurchaseLabel = process.env.LOCKDOCS_GOOGLE_ADS_PURCHASE_LABEL;
  return validGoogleAdsId(googleAdsId) && validGoogleAdsPurchaseLabel(googleAdsPurchaseLabel)
    ? { googleAdsId, googleAdsPurchaseLabel } : null;
}

export function measurementId() {
  const id = process.env.LOCKDOCS_GA4_MEASUREMENT_ID?.trim();
  return id && /^G-[A-Z0-9]{10}$/.test(id) ? id : null;
}
