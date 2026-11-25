import type { Page } from "@playwright/test";
import { ga4CollectEvents } from "./ga4-collect";

export const settings = (page: Page) => page.getByRole("region", { name: "Cookie settings" });
export const analyticsFrame = 'iframe[title="Analytics"]';
export const tagPath = "/api/analytics/tag";
export const google = /https:\/\/([^/]+\.)?(googletagmanager\.com|google-analytics\.com|doubleclick\.net|google\.com|googleadservices\.com)\//;

// Exercise the browser-to-collector boundary without sending data to a real
// property. Model the stream's bootstrap page view before its config command,
// then model the observed gtag.js wire format: the first view in the URL,
// later events batched in a POST body. History-based measurement is off.
export const wireTag = String.raw`(() => {
  const globals = {};
  const config = {};
  const sid = Math.floor(Date.now() / 1000);
  const endpoint = "https://www.google-analytics.com/g/collect?";
  const pending = [];
  let first = true;
  function send(event, params) {
    const eventParams = {...globals, ...config, ...params};
    const query = new URLSearchParams({en:event, sid, dl:eventParams.page_location ?? location.href,
      dr:eventParams.page_referrer ?? document.referrer, dt:eventParams.page_title ?? document.title});
    if (eventParams.engagement_time_msec !== undefined) query.set("_et", eventParams.engagement_time_msec);
    for (const [key, value] of Object.entries(params)) {
      if (key === "send_to" || key === "engagement_time_msec" || key.startsWith("page_")) continue;
      if (typeof value === "string") query.set("ep." + key, value);
      if (typeof value === "number") query.set("epn." + key, value);
    }
    const options = {method:"POST", mode:"no-cors", keepalive:true};
    if (first) {
      first = false;
      void fetch(endpoint + query, options);
      return;
    }
    pending.push(query);
    if (pending.length === 1) queueMicrotask(() => {
      const events = pending.splice(0);
      const shared = new URLSearchParams({sid, dl:events.at(-1).get("dl")});
      const body = events.map(event => {
        for (const [key, value] of shared) if (event.get(key) === value) event.delete(key);
        return event.toString();
      }).join("\r\n");
      void fetch(endpoint + shared, {...options, body});
    });
  }
  function run([command, name, params]) {
    if (command === "set") Object.assign(globals, name);
    if (command === "js" && globals.send_page_view !== false) send("page_view", {});
    if (command === "config") Object.assign(config, params);
    if (command === "event") send(name, params);
  }
  dataLayer.forEach(run);
  const push = dataLayer.push.bind(dataLayer);
  dataLayer.push = (...args) => { args.forEach(run); return push(...args); };
})();`;

export async function install(page: Page, region: string | null, collect?: URLSearchParams[]) {
  if (region) await page.setExtraHTTPHeaders({ "CF-IPCountry": region });
  const hits: string[] = [];
  await page.route(google, async route => {
    const url = new URL(route.request().url());
    if (url.pathname === "/g/collect") {
      collect?.push(...ga4CollectEvents(url, route.request().postData()));
      await route.fulfill({ status: 204 });
      return;
    }
    hits.push(url.href);
    await route.fulfill({ contentType: "text/javascript", body: collect ? wireTag : "document.cookie = '_ga_analytics_fixture=1; Path=/';" });
  });
  return hits;
}

export async function commands(page: Page) {
  if (await page.locator(analyticsFrame).count() === 0) return [];
  return await page.frameLocator(analyticsFrame).locator("html").evaluate(
    () => (window as Window & { dataLayer?: IArguments[] }).dataLayer?.map(args => Array.from(args)) ?? [],
  );
}

export async function observeLocalizedAnalytics(page: Page) {
  const wire: URLSearchParams[] = [];
  const hits = await install(page, "DE", wire);
  const frames: string[] = [];
  page.on("request", request => {
    if (new URL(request.url()).pathname === tagPath) frames.push(request.url());
  });
  return { wire, hits, frames };
}

export const noAnalytics = { wire: [], hits: [], frames: [] };
