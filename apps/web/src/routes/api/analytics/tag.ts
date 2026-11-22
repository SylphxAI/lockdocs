import { randomBytes } from "node:crypto";
import { measurementId } from "@/lib/analytics-config";

// Run the tag in a blank document rather than the app document. Its location and
// referrer are controlled, and it has no links, forms or client-side navigation to measure.
export const GET = () => {
  const id = measurementId();
  if (!id) return new Response(null, { status: 404, headers: { "Cache-Control": "no-store" } });
  const nonce = randomBytes(18).toString("base64");
  return new Response(
    '<!doctype html><meta charset="utf-8"><title>Lockdocs analytics</title><script nonce="' + nonce + '">' +
    'window.startAnalytics = function (consent) {' +
      'delete window.startAnalytics;' +
      'if (window.parent === window || consent.analytics_storage !== "granted" || navigator.globalPrivacyControl === true) return;' +
      'window.dataLayer = [];' +
      'window.gtag = function () { window.dataLayer.push(arguments); };' +
      'gtag("consent", "default", {...consent, ad_personalization: "denied"});' +
      'gtag("js", new Date());' +
      'gtag("config", "' + id + '", {send_page_view: false, allow_google_signals: false, allow_ad_personalization_signals: false,' +
        'cookie_domain: "none", page_location: location.origin + "/", page_referrer: "", page_title: "Lockdocs"});' +
      'const tag = document.createElement("script"); tag.async = true; tag.referrerPolicy = "no-referrer";' +
      'tag.src = "https://www.googletagmanager.com/gtag/js?id=' + id + '"; document.head.append(tag);' +
    '};</script>',
    { headers: { "Cache-Control": "no-store", "Content-Type": "text/html; charset=utf-8", "Referrer-Policy": "no-referrer",
      "X-Frame-Options": "SAMEORIGIN",
      "Content-Security-Policy": "frame-ancestors 'self'; base-uri 'none'; object-src 'none'; form-action 'none'; script-src 'nonce-" + nonce + "' https://www.googletagmanager.com" } },
  );
};
