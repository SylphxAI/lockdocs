import { randomBytes } from "node:crypto";
import { documentPolicy } from "@/lib/content-security-policy";
import { createHandler, StartServer } from "@solidjs/start/server";
import { getRequestEvent } from "solid-js/web";
import { defaultLocale, pageLocale } from "@/lib/locales";
import { signalRequestGpc } from "@/lib/signal-privacy";

export default createHandler(() => <StartServer document={({ assets, children, scripts }) => {
  const request = getRequestEvent()?.request;
  const lang = request ? pageLocale(new URL(request.url).pathname) : defaultLocale;
  return <html lang={lang} data-campaign-gpc={request && signalRequestGpc(request.headers) ? "1" : undefined}><head><meta charset="utf-8"/><meta name="viewport" content="width=device-width, initial-scale=1"/><meta name="theme-color" content="#06080C"/><link rel="manifest" href="/manifest.webmanifest"/><link rel="icon" href="/icon.svg"/><link rel="apple-touch-icon" href="/apple-touch-icon-180.png"/>{assets}</head><body><div id="app">{children}</div>{scripts}</body></html>;
}} />, event => {
  // Even a clean URL carries the document’s GPC state into SPA navigation.
  event.response.headers.append("Vary", "Sec-GPC");
  // Campaign-bearing links also vary by cookie choice.
  if (new URL(event.request.url).search) event.response.headers.set("Cache-Control", "private, no-store");
  const nonce = randomBytes(18).toString("base64");
  event.response.headers.set("Content-Security-Policy", documentPolicy(nonce, import.meta.env.DEV));
  return { nonce };
});
