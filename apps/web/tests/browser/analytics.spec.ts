import { expect, test, type Page } from "@playwright/test";
import { test as authTest, email, code } from "./auth-fixture";

const settings = (page: Page) => page.getByRole("region", { name: "Cookie settings" });
const google = /https:\/\/([^/]+\.)?(googletagmanager\.com|google-analytics\.com|doubleclick\.net|google\.com|googleadservices\.com)\//;

// Exercise the browser-to-collector boundary without sending data to a real
// property. Model the stream's bootstrap page view before its config command,
// then forward event parameters using the Google tag's collection fields.
const wireTag = `(() => {
  const globals = {};
  const config = {};
  const sid = Math.floor(Date.now() / 1000);
  function send(event, params) {
    const eventParams = {...globals, ...config, ...params};
    const query = new URLSearchParams({en:event, sid, dl:eventParams.page_location ?? location.href,
      dr:eventParams.page_referrer ?? document.referrer, dt:eventParams.page_title ?? document.title});
    if (eventParams.engagement_time_msec !== undefined) query.set("_et", eventParams.engagement_time_msec);
    for (const key of ["method", "entry_surface"]) {
      if (eventParams[key] !== undefined) query.set("ep." + key, eventParams[key]);
    }
    fetch("https://www.google-analytics.com/g/collect?" + query, {mode:"no-cors", keepalive:true});
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

async function install(page: Page, trace: string | null, collect?: URLSearchParams[]) {
  const hits: string[] = [];
  await page.route(google, async route => {
    const url = new URL(route.request().url());
    if (url.pathname === "/g/collect") {
      collect?.push(url.searchParams);
      await route.fulfill({ status: 204 });
      return;
    }
    hits.push(url.href);
    await route.fulfill({ contentType: "text/javascript", body: collect ? wireTag : "document.cookie = '_ga_analytics_fixture=1; Path=/';" });
  });
  await page.route("**/cdn-cgi/trace", route => trace === null
    ? route.abort("failed") : route.fulfill({ contentType: "text/plain", body: trace }));
  return hits;
}

async function commands(page: Page) {
  const frame = page.frames().find(f => f.url().endsWith("/api/analytics/tag"));
  return (await frame?.evaluate(() => (window as Window & { dataLayer?: IArguments[] }).dataLayer?.map(args => Array.from(args)))) ?? [];
}

test("EEA waits for a choice, records only the permitted pricing event, and stops after rejection", async ({ page }, info) => {
  const hits = await install(page, "loc=DE\n");
  await page.goto("/pricing?unexpected=synthetic-query@fixture.test");
  const banner = settings(page);
  await expect(banner).toBeVisible();
  await expect(banner.getByRole("button")).toHaveCount(3);
  for (const label of ["Reject optional", "Analytics only", "Allow analytics & ads"]) {
    const button = banner.getByRole("button", { name: label, exact: true });
    await expect(button).toBeEnabled();
    await expect(button).toHaveCSS("border-radius", "999px");
    await expect(button).toHaveCSS("border-top-width", "1px");
    expect((await button.boundingBox())?.height).toBeGreaterThanOrEqual(44);
  }
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(page.viewportSize()!.width);
  expect((await banner.boundingBox())!.y).toBeGreaterThan(page.viewportSize()!.height / 2);
  if (process.env.LOCKDOCS_LOCAL_CAPTURES) await page.screenshot({ path: info.outputPath("cookie-settings.png") });
  expect(hits).toEqual([]);
  await expect(page.locator("iframe")).toHaveCount(0);
  await banner.getByRole("button", { name: "Analytics only" }).click();
  await expect.poll(() => hits.length).toBe(1);
  await expect.poll(() => commands(page)).toEqual(expect.arrayContaining([
    ["consent", "default", { analytics_storage: "granted", ad_storage: "denied", ad_user_data: "denied", ad_personalization: "denied" }],
    ["event", "pricing_view", { send_to: "G-TEST123456", page_location: new URL("/pricing", page.url()).href, page_referrer: "", page_title: "Lockdocs" }],
  ]));
  expect(JSON.stringify(await commands(page))).not.toContain("synthetic-query");
  const footer = page.getByRole("button", { name: "Cookie settings", exact: true });
  await footer.click();
  await expect(banner).toBeFocused();
  await banner.getByRole("button", { name: "Reject optional" }).click();
  await expect(footer).toBeFocused();
  await expect(page.locator("iframe")).toHaveCount(0);
  expect(await page.evaluate(() => document.cookie)).not.toContain("_ga_analytics_fixture");
  await page.goto("/privacy");
  await expect(page.locator("time")).toHaveAttribute("datetime", "2026-11-22");
  await expect(footer).toBeEnabled();
  await expect(banner).toHaveCount(0);
  expect(hits).toHaveLength(1);
});

test("collector sees one sanitized page view per route and foreground time on the same GA4 session", async ({ page }) => {
  const wire: URLSearchParams[] = [];
  const hits = await install(page, "loc=DE\n", wire);
  await page.clock.install();
  await page.goto("/pricing?token=fixture-secret&email=fixture%40example.test&code=fixture-code#secret", {
    referer: "https://example.test/?email=external-fixture",
  });
  await expect(settings(page)).toBeVisible();
  expect(hits).toEqual([]);
  expect(wire).toEqual([]);
  await settings(page).getByRole("button", { name: "Analytics only" }).click();
  const origin = new URL(page.url()).origin;
  const views = () => wire.filter(params => params.get("en") === "page_view");
  await expect.poll(() => views().map(params => params.get("dl"))).toEqual([origin + "/pricing"]);
  expect(views()[0].get("dt")).toBe("Pricing · Lockdocs");
  expect(views()[0].get("dr")).toBe("");
  const sid = views()[0].get("sid");
  expect(sid).toMatch(/^\d+$/);
  await expect.poll(() => page.evaluate(() => document.hasFocus() && !document.hidden)).toBe(true);
  await page.clock.runFor(10_100);
  const foreground = () => wire.find(params => params.get("en") === "user_engagement");
  await expect.poll(() => Number(foreground()?.get("_et") ?? 0)).toBeGreaterThanOrEqual(10_000);
  expect(foreground()?.get("sid")).toBe(sid);
  expect(foreground()?.get("dl")).toBe(origin + "/pricing");
  await page.getByRole("navigation", { name: "Billing currency" }).getByRole("link", { name: "EUR", exact: true }).click();
  await expect(page).toHaveURL(/currency=EUR$/);
  expect(views()).toHaveLength(1);
  const navigation = page.getByRole("navigation", { name: "Main navigation" });
  await navigation.getByRole("link", { name: "Playground", exact: true }).click();
  await expect.poll(() => views().map(params => params.get("dl"))).toEqual([origin + "/pricing", origin + "/playground"]);
  expect(views()[1].get("dr")).toBe(origin + "/pricing");
  expect(views()[1].get("dt")).toBe("Lockfile Playground · Lockdocs");
  expect(views()[1].get("sid")).toBe(sid);
  await navigation.getByRole("link", { name: "Docs", exact: true }).click();
  await expect.poll(() => views().map(params => params.get("dl"))).toEqual([origin + "/pricing", origin + "/playground", origin + "/docs"]);
  expect(wire.map(params => params.toString()).join(" ")).not.toMatch(/analytics%2Ftag|fixture|secret/);
  await page.getByRole("button", { name: "Cookie settings", exact: true }).click();
  await settings(page).getByRole("button", { name: "Reject optional" }).click();
  await expect(page.locator("iframe")).toHaveCount(0);
  const prior = wire.length;
  await navigation.getByRole("link", { name: "Pricing", exact: true }).click();
  await page.clock.runFor(11_000);
  expect(wire).toHaveLength(prior);
  expect(hits).toHaveLength(1);
});

test("Sec-GPC blocks frame startup even when the navigator signal is unavailable", async ({ page }) => {
  const wire: URLSearchParams[] = [];
  const hits = await install(page, "loc=US\n", wire);
  await page.setExtraHTTPHeaders({ "Sec-GPC": "1" });
  await page.goto("/pricing");
  const refused = page.waitForResponse(response => new URL(response.url()).pathname === "/api/analytics/tag");
  await settings(page).getByRole("button", { name: "Analytics only" }).click();
  expect((await refused).status()).toBe(404);
  expect(hits).toEqual([]);
  expect(wire).toEqual([]);
});

test("the compact consent bar leaves the workbench and footer controls usable", async ({ page }, info) => {
  const hits = await install(page, "loc=DE\n");
  await page.goto("/playground");
  const banner = settings(page);
  await expect(banner).toBeVisible();
  const { width, height } = page.viewportSize()!;
  const box = (await banner.boundingBox())!;
  expect(box.height).toBeLessThanOrEqual(width === 390 ? 110 : 72);
  expect(box.y + box.height).toBeCloseTo(height, 0);
  // Chromium rounds serialized CSS pixel values, unlike boundingBox().
  for (const [selector, property] of [["body", "padding-bottom"], ["html", "scroll-padding-bottom"]]) {
    await expect.poll(() => page.locator(selector).evaluate(
      (el, prop) => Number.parseFloat(getComputedStyle(el).getPropertyValue(prop)), property,
    )).toBeCloseTo(box.height, 2);
  }
  const choices = await banner.getByRole("button").all();
  expect(choices).toHaveLength(3);
  for (const choice of choices) expect((await choice.boundingBox())!.height).toBeGreaterThanOrEqual(44);
  const resolve = page.getByRole("button", { name: "Resolve versions" });
  await resolve.click({ trial: true });
  expect((await resolve.boundingBox())!.y + (await resolve.boundingBox())!.height).toBeLessThan(box.y);
  if (process.env.LOCKDOCS_LOCAL_CAPTURES) await page.screenshot({ path: info.outputPath("playground-consent.png") });
  const footer = page.getByRole("button", { name: "Cookie settings", exact: true });
  await footer.click({ trial: true });
  expect(hits).toEqual([]);
  await expect(page.locator('iframe[src="/api/analytics/tag"]')).toHaveCount(0);
  await banner.getByRole("button", { name: "Reject optional" }).click();
  await expect(page.locator("body")).toHaveCSS("padding-bottom", "0px");
});

for (const [region, trace] of [["UK", "loc=GB"], ["Switzerland", "loc=CH"], ["unknown", "loc=XX"], ["unreachable", null]] as const) {
  test(region + " fails closed and persists a first-visit rejection", async ({ page }) => {
    const hits = await install(page, trace);
    await page.goto("/pricing");
    await settings(page).getByRole("button", { name: "Reject optional" }).click();
    await page.reload();
    await expect(page.getByRole("button", { name: "Cookie settings", exact: true })).toBeEnabled();
    await expect(settings(page)).toHaveCount(0);
    await expect(page.locator("iframe")).toHaveCount(0);
    expect(hits).toEqual([]);
  });
}

test("a fresh visitor has no tag, geo lookup, or storage write before an explicit grant", async ({ page }) => {
  const wire: URLSearchParams[] = [];
  const hits = await install(page, "loc=US\n", wire);
  const lookups: string[] = [];
  page.on("request", request => {
    if (new URL(request.url()).pathname === "/cdn-cgi/trace") lookups.push(request.url());
  });
  await page.addInitScript(() => {
    const record = document.createElement("meta");
    record.name = "storage-writes";
record.content = "";
    const original = Storage.prototype.setItem;
    Storage.prototype.setItem = function (key, value) {
      record.content += key + ",";
      return original.call(this, key, value);
    };
    document.addEventListener("DOMContentLoaded", () => document.head.append(record));
  });
  await page.goto("/pricing?gclid=fixture-click");
  const banner = settings(page);
  await expect(banner).toBeVisible();
  await expect(page.locator("iframe")).toHaveCount(0);
  expect(await page.evaluate(() => !!(window as Window & { gtag?: unknown }).gtag)).toBe(false);
  await expect(page.locator('meta[name="storage-writes"]')).toHaveAttribute("content", "");
  expect(await page.evaluate(() => [document.cookie, localStorage.length, sessionStorage.length])).toEqual(["", 0, 0]);
  expect(lookups).toEqual([]);
  expect(hits).toEqual([]);
  expect(wire).toEqual([]);
  await banner.getByRole("button", { name: "Allow analytics & ads" }).click();
  await expect.poll(() => hits.length).toBe(1);
  await expect.poll(() => wire.filter(params => params.get("en") === "page_view").map(params => params.get("dl"))).toEqual([new URL("/pricing", page.url()).href]);
  expect(await commands(page)).toContainEqual(["consent", "default", {
    analytics_storage: "granted", ad_storage: "granted", ad_user_data: "granted", ad_personalization: "denied",
  }]);
});

authTest.use({ billing: true });

const signupEvents = async (page: Page) => (await commands(page)).filter(([command, event]) => command === "event" && event === "sign_up");
const signupCookie = "__Host-lockdocs-signup";

for (const [method, providerFlag, choice, surface] of [
  ["google", true, "Analytics only", "pricing"],
  ["email_otp", true, "Allow analytics & ads", "dashboard"],
  ["google", false, "Analytics only", "pricing"],
  ["google", undefined, "Analytics only", "pricing"],
  ["google", true, null, "pricing"],
  ["google", true, "Reject optional", "pricing"],
] as const) {
  authTest(
    `${method} signup flag ${providerFlag} with ${choice ?? "no choice"}`,
    async ({ page, auth }) => {
      auth.ticketSignal.is_new_user = providerFlag;
      auth.methods.email_otp = method === "email_otp";
      const wire: URLSearchParams[] = [];
      const hits = await install(page, null, wire);
      const wantsAnalytics = choice === "Analytics only" || choice === "Allow analytics & ads";
      const shouldEmit = wantsAnalytics && providerFlag === true;
      const cookieWrites: string[] = [];
      page.on("response", async response => {
        if (new URL(response.url()).origin !== auth.origin) return;
        cookieWrites.push(...await response.headerValues("set-cookie"));
      });
      const storageWrites: string[] = [];
      await page.exposeFunction("recordStorageWrite", (label: string) => { storageWrites.push(label); });
      await page.addInitScript(() => {
        const record = (label: string) => (window as unknown as { recordStorageWrite(label: string): void }).recordStorageWrite(label);
        const setItem = Storage.prototype.setItem;
        Storage.prototype.setItem = function (key, value) {
          record(key);
          return setItem.call(this, key, value);
        };
        const cookie = Object.getOwnPropertyDescriptor(Document.prototype, "cookie")!;
        Object.defineProperty(document, "cookie", { ...cookie, set(value: string) {
          record(value.split("=", 1)[0]);
          cookie.set!.call(this, value);
        } });
      });
      await page.goto(auth.origin + "/sign-in?next=/" + surface);
      const banner = settings(page);
      await expect(banner).toBeVisible();
      if (choice) await banner.getByRole("button", { name: choice, exact: true }).click();
      if (wantsAnalytics) await expect.poll(() => hits.length).toBe(1);

      if (method === "google") {
        await page.getByRole("link", { name: "Continue with Google" }).click();
      } else {
        await page.getByLabel("Email address").fill(email);
        await page.getByRole("button", { name: "Continue", exact: true }).click();
        await page.getByLabel("Six-digit code").fill(code);
      }
      await expect(page).toHaveURL(auth.origin + "/" + surface);
      expect(auth.calls).toContain("/v1/client/tickets:redeem");
      expect(auth.rejections).toEqual([]);
      const signupWire = () => wire.filter(params => params.get("en") === "sign_up");
      if (wantsAnalytics) await expect.poll(() => hits.length).toBe(2);
      if (shouldEmit) {
        await expect.poll(() => signupEvents(page)).toEqual([
          ["event", "sign_up", { method, entry_surface: surface, send_to: "G-TEST123456" }],
        ]);
        await expect.poll(() => signupWire().map(params => [params.get("ep.method"), params.get("ep.entry_surface")])).toEqual([[method, surface]]);
        expect(signupWire()[0].get("dl")).toBe(auth.origin + "/" + surface);
        expect(signupWire()[0].toString()).not.toMatch(/reader|fixture.test|auth_ticket|token|usr_/);
      } else {
        expect(await signupEvents(page)).toEqual([]);
      }
      await page.reload();
      if (wantsAnalytics) await expect.poll(() => hits.length).toBe(3);
      expect(await signupEvents(page)).toEqual([]);
      expect(signupWire()).toHaveLength(shouldEmit ? 1 : 0);
      expect((await page.context().cookies()).some(cookie => cookie.name === signupCookie)).toBe(false);
      expect(cookieWrites.filter(value => value.startsWith(signupCookie + "="))).toEqual(shouldEmit ? [
        signupCookie + "=" + method + "." + surface + "; Max-Age=60; Path=/; Secure; SameSite=Lax",
      ] : []);
      if (!wantsAnalytics) {
        await expect(page.locator('iframe[title="Analytics"]')).toHaveCount(0);
        expect(await commands(page)).toEqual([]);
        expect(hits).toEqual([]);
        expect(wire).toEqual([]);
        // Saving an explicit rejection is essential; it isn't an optional write.
        expect(storageWrites).toEqual(choice ? ["lockdocs.cookie-settings.v1"] : []);
        expect(cookieWrites.filter(value => !/^__Host-lockdocs-(oauth|session|last-auth-method)=/.test(value))).toEqual([]);
        if (!choice) {
          await settings(page).getByRole("button", { name: "Analytics only", exact: true }).click();
          await expect.poll(() => hits.length).toBe(1);
          expect(await signupEvents(page)).toEqual([]);
        }
      }
    },
  );
}
authTest("authenticated pricing submit emits one consented checkout event and no sign-up on sign-in", async ({ page, auth }) => {
  const hits = await install(page, null);
  const submits: URLSearchParams[] = [];
  await page.route(auth.origin + "/api/billing/checkout", async route => {
    submits.push(new URLSearchParams(route.request().postData() ?? ""));
    await route.fulfill({ status: 204 });
  });
  await page.goto(auth.origin + "/sign-in?next=%2Fpricing%3Fcurrency%3DGBP");
  await page.getByRole("link", { name: /Continue with Google/ }).click();
  await expect(page).toHaveURL(auth.origin + "/pricing?currency=GBP");
  const checkout = page.locator('form[action="/api/billing/checkout"]').getByRole("button");
  await expect(settings(page)).toBeVisible();
  await checkout.click();
  await expect.poll(() => submits.length).toBe(1);
  expect(hits).toEqual([]);
  expect(submits[0].get("checkout_consent")).toBe("reject");
  expect(submits[0].getAll("entry_surface")).toEqual(["pricing"]);

  await settings(page).getByRole("button", { name: "Analytics only" }).click();
  await expect.poll(() => hits.length).toBe(1);
  const event = (name: string) => commands(page).then(entries => entries.filter(([command, key]) => command === "event" && key === name));
  expect(await event("begin_checkout")).toEqual([]);
  await checkout.click();
  await expect.poll(() => submits.length).toBe(2);
  expect(submits[1].get("checkout_consent")).toBe("analytics");
  expect(submits[1].getAll("entry_surface")).toEqual(["pricing"]);
  await expect.poll(() => event("begin_checkout")).toEqual([["event", "begin_checkout", {
    send_to: "G-TEST123456", page_location: auth.origin + "/pricing", page_referrer: "", page_title: "Lockdocs",
    currency: "GBP", value: 99, plan_sku: "lockdocs_pro_seat_year", entry_surface: "pricing",
    items: [{ item_id: "lockdocs_pro_seat_year", item_name: "Lockdocs Pro", quantity: 1, price: 99 }],
  }]]);
  expect(await event("sign_up")).toEqual([]);
  expect(await event("purchase")).toEqual([]);
  expect(auth.rejections).toEqual([]);
});

test("dashboard return works with analytics active but never reports a browser purchase", async ({ page }) => {
  const hits = await install(page, "loc=DE\n");
  await page.clock.install();
  const checks: string[] = [];
  await page.route("**/api/analytics/purchase", route => route.fulfill({
    json: { currency: "GBP", value: 82.5, transaction_id: "a".repeat(40) },
  }));
  page.on("request", request => {
    if (new URL(request.url()).pathname === "/api/analytics/purchase") checks.push(request.method());
  });

  const returnPath = "/dashboard?billing=return";
  const notice = page.getByRole("status").filter({ hasText: "Back from checkout. Pro access updates once payment is confirmed." });
  async function expectNoBrowserPurchase() {
    await expect(page).toHaveURL(returnPath);
    await expect(notice).toBeVisible();
    await expect(page.getByRole("heading", { name: "Billing & recovery" })).toBeVisible();
    await expect.poll(() => commands(page)).toContainEqual(["consent", "default", {
      analytics_storage: "granted", ad_storage: "denied", ad_user_data: "denied", ad_personalization: "denied",
    }]);
    // Let the tag load and advance past the retired poll's full retry window.
    await expect.poll(() => hits.length).toBeGreaterThan(0);
    await page.clock.runFor(7500);
    expect((await commands(page)).filter(([command, event]) => command === "event" && event === "purchase")).toEqual([]);
    expect(checks).toEqual([]);
  }

  await page.goto(returnPath);
  await expect(settings(page)).toBeVisible();
  expect(hits).toEqual([]);
  await settings(page).getByRole("button", { name: "Analytics only" }).click();
  await expectNoBrowserPurchase();
  await page.reload();
  await expectNoBrowserPurchase();
});

test("GPC overrides an existing grant outside the restricted regions", async ({ page }) => {
  const hits = await install(page, "loc=US\n");
  await page.addInitScript(() => {
    Object.defineProperty(navigator, "globalPrivacyControl", { value: true });
    localStorage.setItem("lockdocs.cookie-settings.v1", JSON.stringify({ choice: "all", expires: Date.now() + 60_000 }));
  });
  await page.goto("/pricing");
  await page.getByRole("button", { name: "Cookie settings", exact: true }).click();
  await expect(settings(page).getByRole("status")).toContainText("Global Privacy Control");
  await expect(settings(page).getByRole("button", { name: "Analytics only" })).toBeDisabled();
  await expect(settings(page).getByRole("button", { name: "Allow analytics & ads" })).toBeDisabled();
  await expect(page.locator("iframe")).toHaveCount(0);
  expect(hits).toEqual([]);
});


test("campaign cookies require ad consent, survive navigation, and are removed on downgrade", async ({ page }) => {
  await install(page, "loc=DE\n");
  await page.goto("/pricing?gclid=fixture-click&gbraid=fixture-braid&wbraid=bad%2Fid");
  const campaign = async () => (await page.context().cookies()).find(cookie => cookie.name === "__Host-lockdocs-checkout-campaign");
  await expect(settings(page)).toBeVisible();
  expect(await campaign()).toBeUndefined();
  await settings(page).getByRole("button", { name: "Analytics only" }).click();
  await expect.poll(() => commands(page)).not.toEqual([]);
  expect(await campaign()).toBeUndefined();
  const footer = page.getByRole("button", { name: "Cookie settings", exact: true });
  await footer.click();
  await settings(page).getByRole("button", { name: "Allow analytics & ads" }).click();
  await expect.poll(campaign).toMatchObject({ value: "gclid%3Dfixture-click%26gbraid%3Dfixture-braid", path: "/", secure: true, sameSite: "Lax" });
  await page.goto("/pricing");
  expect(await campaign()).toBeDefined();
  await expect(footer).toBeEnabled();
  await footer.click();
  await settings(page).getByRole("button", { name: "Analytics only" }).click();
  await expect.poll(campaign).toBeUndefined();
});
