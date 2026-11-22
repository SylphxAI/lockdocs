import { randomBytes } from "node:crypto";
import type { APIEvent } from "@solidjs/start/server";
import { sendsGpc } from "@/lib/checkout-attribution";
import { measurementId } from "@/lib/analytics-config";

// Run the tag in a blank document rather than the app document. It has no links,
// forms or client-side navigation to measure. The parent supplies scrubbed context.
export const GET = ({ request }: APIEvent) => {
  const id = measurementId();
  if (!id || sendsGpc(request)) return new Response(null, { status: 404, headers: { "Cache-Control": "no-store" } });
  const nonce = randomBytes(18).toString("base64");
  return new Response(
    '<!doctype html><meta charset="utf-8"><title>Lockdocs analytics</title><script nonce="' + nonce + '">' +
    'window.startAnalytics = function (consent, page) {' +
      'delete window.startAnalytics;' +
      'if (window.parent === window || consent.analytics_storage !== "granted" || navigator.globalPrivacyControl === true) return;' +
      'window.dataLayer = [];' +
      'window.gtag = function () { window.dataLayer.push(arguments); };' +
      'gtag("consent", "default", {...consent, ad_personalization: "denied"});' +
      // The stream's config can bootstrap before the queued config command.
      // Set global defaults before startup as well as disabling config views.
      'gtag("set", {...page, send_page_view: false});' +
      'gtag("js", new Date());' +
      'gtag("config", "' + id + '", {send_page_view: false, allow_google_signals: false, allow_ad_personalization_signals: false,' +
        'cookie_domain: "none"});' +
      'const tag = document.createElement("script"); tag.async = true; tag.referrerPolicy = "no-referrer";' +
      'tag.src = "https://www.googletagmanager.com/gtag/js?id=' + id + '"; document.head.append(tag);' +
    '};</script>',
    { headers: { "Cache-Control": "no-store", "Content-Type": "text/html; charset=utf-8", "Referrer-Policy": "no-referrer",
      "X-Frame-Options": "SAMEORIGIN",
      "Content-Security-Policy": "frame-ancestors 'self'; base-uri 'none'; object-src 'none'; form-action 'none'; script-src 'nonce-" + nonce + "' https://www.googletagmanager.com" } },
  );
};
