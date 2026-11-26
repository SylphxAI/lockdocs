import { expect, test, type Page } from "@playwright/test";

const touch = {
  gclid: "sample-gclid", gbraid: "sample-gbraid", wbraid: "sample-wbraid",
  utm_id: "1234567890", utm_source: "google", utm_medium: "cpc", utm_campaign: "Docs & MCP", utm_term: "lockfile", utm_content: "upgrade report",
};
const campaign = new URLSearchParams({ ...touch, token: "private-sentinel" });
const dock = (page: Page) => page.getByRole("region", { name: "Cookie settings" });

for (const consentAt of ["pricing", "sign-in"]) {
  test("preserves all campaign fields without pre-choice storage, then captures them at " + consentAt, async ({ page }) => {
    await page.setExtraHTTPHeaders({ "CF-IPCountry": "DE" });
    const writes: string[] = [];
    await page.exposeFunction("recordStorageWrite", (kind: string) => writes.push(kind));
    await page.addInitScript(() => {
      const record = (window as unknown as { recordStorageWrite: (kind: string) => Promise<void> }).recordStorageWrite;
      const setItem = Storage.prototype.setItem;
      Storage.prototype.setItem = function (key, value) {
        void record("Storage.setItem");
        return setItem.call(this, key, value);
      };
      const cookie = Object.getOwnPropertyDescriptor(Document.prototype, "cookie")!;
      Object.defineProperty(Document.prototype, "cookie", { ...cookie, set(value: string) {
        void record("document.cookie");
        cookie.set!.call(this, value);
      } });
    });
    const tagRequests: string[] = [];
    await page.route("https://www.googletagmanager.com/**", route => {
      tagRequests.push(route.request().url());
      return route.fulfill({ contentType: "text/javascript", body: "" });
    });
    const beforeChoice = async () => {
      await expect(page.locator("#cookie-settings")).toBeVisible();
      expect(await page.context().cookies()).toEqual([]);
      expect(await page.evaluate(() => [localStorage.length, sessionStorage.length])).toEqual([0, 0]);
      expect(tagRequests).toEqual([]);
      expect(writes).toEqual([]);
    };
    const loaded = async (path: string, ownQuery: Record<string, string> = {}) => {
      await expect(page).toHaveURL(url => url.pathname === path);
      expect(Object.fromEntries(new URL(page.url()).searchParams)).toEqual({ ...ownQuery, ...touch });
      await beforeChoice();
    };
    await page.goto("/?" + campaign);
    await beforeChoice();
    await page.getByRole("navigation", { name: "Main navigation" }).getByRole("link", { name: "Pricing" }).click();
    await loaded("/pricing");
    await page.getByRole("link", { name: "EUR", exact: true }).click();
    await loaded("/pricing", { currency: "EUR" });
    await page.getByRole("link", { name: "繁體中文", exact: true }).click();
    await loaded("/zh-TW/pricing", { currency: "EUR" });
    await page.getByRole("link", { name: "English", exact: true }).click();
    await loaded("/pricing", { currency: "EUR" });
    if (consentAt === "sign-in") {
      await page.getByRole("link", { name: "Get Lockdocs Pro — €109/yr" }).click();
      await loaded("/sign-in", { next: "/pricing?currency=EUR" });
      await page.reload();
      await loaded("/sign-in", { next: "/pricing?currency=EUR" });
    }
    await dock(page).getByRole("button", { name: "Allow analytics & ads" }).click();
    await expect.poll(() => tagRequests.length).toBe(1);
    const saved = await page.evaluate(() => localStorage.getItem("lockdocs.signal.v1.touch"));
    expect(JSON.parse(saved!).touch).toEqual(touch);
    const tagDocument = page.frameLocator('iframe[title="Analytics"]').locator("html");
    expect(await tagDocument.evaluate(() => location.search)).toBe("");
  });
}

test("serves an enforced nonce CSP without blocking document, hydration or collector traffic", async ({ page, request }) => {
  const violations: string[] = [];
  page.on("console", message => { if (/content security policy/i.test(message.text())) violations.push(message.text()); });
  await page.exposeFunction("collectViolation", (message: string) => violations.push(message));
  await page.addInitScript(() => window.addEventListener("securitypolicyviolation", event => {
    void (window as unknown as { collectViolation: (message: string) => Promise<void> }).collectViolation(event.effectiveDirective + " " + event.blockedURI);
  }));
  const googleAds = [
    "https://www.google.com", "https://www.googleadservices.com",
    "https://googleads.g.doubleclick.net", "https://stats.g.doubleclick.net",
  ];
  const hits = new Set<string>();
  await page.route("https://www.googletagmanager.com/**", route => route.fulfill({
    contentType: "text/javascript",
    body: "for (const origin of " + JSON.stringify(googleAds) + ") fetch(origin + '/csp-probe', {mode:'no-cors'});",
  }));
  for (const origin of googleAds) await page.route(origin + "/csp-probe", route => { hits.add(origin); return route.fulfill({ status: 204 }); });
  const response = await page.goto("/");
  expect(response?.status()).toBe(200);
  const csp = response!.headers()["content-security-policy"];
  expect(csp).toContain("default-src 'none'");
  expect(csp).toContain("frame-ancestors 'none'");
  expect(csp).not.toMatch(/unsafe-inline|unsafe-eval/);
  const scriptPolicy = csp.split(";").map(x => x.trim()).find(x => x.startsWith("script-src"));
  const nonce = scriptPolicy?.match(/'nonce-([^']+)'/)?.[1];
  expect(nonce).toBeTruthy();
  expect((await response!.text())).toContain('nonce="' + nonce + '"');
  expect((await request.get("/sign-in")).headers()["content-security-policy"]).not.toContain(nonce!);
  await page.getByRole("navigation", { name: "Main navigation" }).getByRole("link", { name: "Pricing" }).click();
  await expect(page.getByRole("heading", { name: "Lockdocs Pro" })).toBeVisible();
  const framePolicy = (await request.get("/api/analytics/tag")).headers()["content-security-policy"];
  for (const policy of [csp, framePolicy]) {
    const connect = policy.split(";").map(x => x.trim()).find(x => x.startsWith("connect-src"))?.split(" ");
    for (const origin of googleAds) expect(connect).toContain(origin);
  }
  await dock(page).getByRole("button", { name: "Allow analytics & ads" }).click();
  await expect.poll(() => [...hits].sort()).toEqual([...googleAds].sort());
  expect(violations).toEqual([]);
});

test("keeps the two benchmark scores distinct and the Home demo above the cookie dock", async ({ page }) => {
  await page.goto("/");
  await expect(dock(page)).toBeVisible();
  await page.evaluate(() => document.fonts.ready);
  const benchmark = page.getByRole("region", { name: "Benchmark highlights" });
  await expect(benchmark.getByRole("heading", { name: "With Lockdocs", exact: true })).toBeVisible();
  await expect(benchmark.getByRole("heading", { name: "Baseline (without Lockdocs)", exact: true })).toBeVisible();
  await expect(benchmark.locator(".bench-lockdocs")).toContainText("96/105");
  await expect(benchmark.locator(".bench-lockdocs")).toContainText("+19");
  await expect(benchmark.locator(".bench-baseline")).toContainText("77/105");
  const top = (await dock(page).boundingBox())!.y;
  for (const label of ["Try the playground", "Inspect the Next.js upgrade"]) {
    const cta = await page.getByRole("link", { name: label }).boundingBox();
    expect(top - (cta!.y + cta!.height)).toBeGreaterThanOrEqual(20);
  }
  for (const choice of await dock(page).getByRole("button").all()) {
    const box = (await choice.boundingBox())!;
    expect(box.height).toBeGreaterThanOrEqual(44);
    expect(box.width).toBeGreaterThanOrEqual(44);
  }
  expect((await dock(page).boundingBox())!.height).toBeLessThanOrEqual(page.viewportSize()!.width === 390 ? 92 : 66);
});

test("shows Pro and US$120/yr plus the checkout CTA above the dock before scrolling", async ({ page }) => {
  await page.goto("/pricing");
  await expect(dock(page)).toBeVisible();
  await page.evaluate(() => document.fonts.ready);
  const plans = page.locator(".plan-grid article");
  await expect(plans).toHaveCount(2);
  await expect(plans.first().getByRole("heading", { name: "Lockdocs Pro" })).toBeVisible();
  const price = plans.first().locator(".plan-price");
  await expect(price).toHaveText("US$120/yr");
  const glyphs = await price.evaluate(el => {
    const range = document.createRange();
    range.selectNodeContents(el);
    return [...range.getClientRects()].map(rect => ({ top: rect.top, bottom: rect.bottom }));
  });
  expect(glyphs.length).toBeGreaterThan(0);
  expect(new Set(glyphs.map(rect => rect.top)).size).toBe(1);
  const usableBottom = (await dock(page).boundingBox())!.y;
  expect(Math.max(...glyphs.map(rect => rect.bottom))).toBeLessThan(usableBottom);
  const checkout = page.getByRole("link", { name: "Get Lockdocs Pro — US$120/yr" });
  const box = (await checkout.boundingBox())!;
  expect(box.height).toBeGreaterThanOrEqual(44);
  expect(usableBottom - (box.y + box.height)).toBeGreaterThanOrEqual(20);
  expect(await checkout.evaluate(el => {
    const { x, y, width, height } = el.getBoundingClientRect();
    return el.contains(document.elementFromPoint(x + width / 2, y + height / 2));
  })).toBe(true);
  if (page.viewportSize()!.width === 390) expect((await plans.first().boundingBox())!.y).toBeLessThan((await plans.last().boundingBox())!.y);
});
