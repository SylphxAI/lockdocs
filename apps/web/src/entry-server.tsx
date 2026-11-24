import { createHandler, StartServer } from "@solidjs/start/server";
import { getRequestEvent } from "solid-js/web";
import { defaultLocale, pageLocale } from "@/lib/locales";

export default createHandler(() => <StartServer document={({ assets, children, scripts }) => {
  const request = getRequestEvent()?.request;
  const lang = request ? pageLocale(new URL(request.url).pathname) : defaultLocale;
  return <html lang={lang}><head><meta charset="utf-8"/><meta name="viewport" content="width=device-width, initial-scale=1"/><link rel="icon" href="/icon.svg"/>{assets}</head><body><div id="app">{children}</div>{scripts}</body></html>;
}} />);
