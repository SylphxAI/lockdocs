import { validGoogleAdsId, validGoogleAdsPurchaseLabel } from "./google-ads";
import { httpsOrigin } from "./http";

function signalApiOrigin() {
  return httpsOrigin(process.env.SYLPHX_API_URL?.trim() || "https://api.sylphx.com");
}

export function signalConfig() {
  const signalKey = process.env.SYLPHX_PUBLISHABLE_KEY?.trim();
  const origin = signalApiOrigin();
  return signalKey?.startsWith("sylphx_pk_") && origin ? { signalKey, signalApiOrigin: origin } : null;
}

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
