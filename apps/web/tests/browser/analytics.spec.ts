import { expect, test, type Page } from "@playwright/test";

const settings = (page: Page) => page.getByRole("region", { name: "Cookie settings" });
const google = /https:\/\/([^/]+\.)?(googletagmanager\.com|google-analytics\.com|doubleclick\.net|google\.com|googleadservices\.com)\//;

async function install(page: Page, trace: string | null) {
  const hits: string[] = [];
  await page.route(google, async route => {
    hits.push(route.request().url());
    await route.fulfill({ contentType: "text/javascript", body: "document.cookie = '_ga_analytics_fixture=1; Path=/';" });
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

test("the compact consent bar leaves the workbench and footer controls usable", async ({ page }, info) => {
  const hits = await install(page, "loc=DE\n");
  await page.goto("/playground");
  const banner = settings(page);
  await expect(banner).toBeVisible();
  const { width, height } = page.viewportSize()!;
  const box = (await banner.boundingBox())!;
  expect(box.height).toBeLessThanOrEqual(width === 390 ? 110 : 72);
  expect(box.y + box.height).toBeCloseTo(height, 0);
  await expect(page.locator("body")).toHaveCSS("padding-bottom", box.height + "px");
  await expect(page.locator("html")).toHaveCSS("scroll-padding-bottom", box.height + "px");
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

test("recognized outside region grants measurement without allowing personalized ads", async ({ page }) => {
  const hits = await install(page, "loc=US\n");
  await page.goto("/pricing");
  await expect.poll(() => hits.length).toBe(1);
  await expect(settings(page)).toHaveCount(0);
  expect(await commands(page)).toContainEqual(["consent", "default", {
    analytics_storage: "granted", ad_storage: "granted", ad_user_data: "granted", ad_personalization: "denied",
  }]);
});

test("dashboard return sends only the verified purchase fields and never treats a return URL as proof", async ({ page }) => {
  const hits = await install(page, "loc=DE");
  let status = 404;
  let checks = 0;
  const transaction_id = "95a19ce7e909322bfb8d68b26c5836a25dccac1f";
  await page.route("**/api/analytics/purchase", async route => {
    expect(route.request().method()).toBe("POST");
    checks++;
    await route.fulfill({ status, json: status === 200 ? {
      currency: "GBP", value: 82.5, transaction_id,
      receipt_id: "receipt-fixture", email: "buyer@example.test",
    } : {} });
  });
  await page.goto("/dashboard?billing=return&checkout=checkout-fixture");
  await expect(settings(page)).toBeVisible();
  expect(hits).toEqual([]);
  expect(checks).toBe(0);
  const noPurchase = page.waitForResponse(response => response.url().endsWith("/api/analytics/purchase") && response.status() === 404);
  await settings(page).getByRole("button", { name: "Analytics only" }).click();
  await noPurchase;
  expect((await commands(page)).filter(entry => entry[0] === "event")).toEqual([]);
  status = 200;
  await page.reload();
  await expect.poll(() => commands(page)).toContainEqual(["event", "purchase", {
    currency: "GBP", value: 82.5, transaction_id,
    items: [{ item_id: "lockdocs_pro_seat_year", item_name: "Lockdocs Pro", quantity: 1 }],
    send_to: "G-TEST123456", page_location: new URL("/dashboard", page.url()).href, page_referrer: "", page_title: "Lockdocs",
  }]);
  expect(JSON.stringify(await commands(page))).not.toMatch(/checkout-fixture|receipt-fixture|buyer@example.test/);
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
