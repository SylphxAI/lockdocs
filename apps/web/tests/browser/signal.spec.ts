import { expect, test as base, type Page } from "@playwright/test";
import { test as authTest } from "./auth-fixture";
import { analyticsFrame, install, settings } from "./analytics-fixture";

const endpoint = "/v1/analytics/events:batch";
const campaign = {
  utm_id: "spring-id", utm_source: "newsletter", utm_medium: "email", utm_campaign: "Docs launch",
  utm_term: "lockfile", utm_content: "hero", utm_source_platform: "web", utm_creative_format: "text", utm_marketing_tactic: "acquisition",
};
const clicks = { gclid: "google-click", gbraid: "google-braid", wbraid: "web-braid", fbclid: "meta-click" };
const query = new URLSearchParams({ ...campaign, ...clicks, token: "private-query-fixture" });
type Row = { event_id: string; event_name: string; distinct_id: string; session_id: string; properties: Record<string, unknown> };

async function intercept(page: Page, origin: string) {
  const batches: { consent: string; events: Row[]; headers: Record<string, string> }[] = [];
  await page.route(origin + endpoint, async route => {
    if (route.request().method() === "POST") {
      batches.push({ ...route.request().postDataJSON(), headers: await route.request().allHeaders() });
    }
    await route.fulfill({ status: 202, json: { accepted_events: 1 }, headers: {
      "Access-Control-Allow-Origin": "*", "Access-Control-Allow-Headers": "content-type,x-sylphx-key",
    } });
  });
  return { batches, rows: (event: string) => batches.flatMap(batch => batch.events).filter(row => row.event_name === event) };
}

async function signalOnly(page: Page) {
  await page.setExtraHTTPHeaders({ "CF-IPCountry": "DE" });
  await page.route("**/api/analytics/config", async route => {
    const response = await route.fetch();
    await route.fulfill({ response, json: { ...await response.json(), measurementId: null,
      signalKey: "sylphx_pk_signal_fixture", signalApiOrigin: "https://api.sylphx.com" } });
  });
  return intercept(page, "https://api.sylphx.com");
}

base("Signal-only grant keeps nine UTMs, strips click IDs and emits 1/1/1 SPA views", async ({ page, context }) => {
  await context.grantPermissions(["clipboard-read", "clipboard-write"]);
  const { batches, rows } = await signalOnly(page);
  let documents = 0;
  page.on("request", request => {
    if (request.isNavigationRequest() && request.frame() === page.mainFrame()) documents++;
  });
  await page.goto("/?" + query + "#private-fragment");
  await expect(settings(page)).toBeVisible();
  expect(batches).toEqual([]);
  expect(await page.evaluate(() => localStorage.getItem("lockdocs.signal.v1.touch"))).toBeNull();
  await settings(page).getByRole("button", { name: "Analytics only", exact: true }).click();
  await expect.poll(() => rows("page_view").map(row => row.properties)).toEqual([{ path: "/", ...campaign }]);
  await expect(page.locator(analyticsFrame)).toHaveCount(0);
  await page.getByRole("button", { name: "Copy install command" }).click();
  await expect.poll(() => rows("command_copied").map(row => row.properties)).toEqual([
    { ...campaign, surface: "home_install", command_kind: "npx" },
  ]);

  await page.getByRole("navigation", { name: "Main navigation" }).getByRole("link", { name: "Pricing", exact: true }).click();
  await expect(page).toHaveURL(url => url.pathname === "/pricing");
  await page.getByRole("navigation", { name: "Billing currency" }).getByRole("link", { name: "EUR", exact: true }).click();
  await expect(page).toHaveURL(url => url.searchParams.get("currency") === "EUR");
  await page.locator("header .wordmark").click();
  await expect.poll(() => rows("page_view").map(row => row.properties)).toEqual([
    { path: "/", ...campaign }, { path: "/pricing", ...campaign }, { path: "/", ...campaign },
  ]);
  const pricing = () => rows("pricing_view");
  expect(pricing().map(row => row.properties)).toEqual([{ path: "/pricing", entry_surface: "pricing", ...campaign }]);
  expect(documents).toBe(1);
  expect(new Set(rows("page_view").map(row => row.session_id)).size).toBe(1);
  expect(new Set(rows("page_view").map(row => row.event_id)).size).toBe(3);
  expect(JSON.stringify(batches.map(batch => batch.events))).not.toMatch(/private-query|private-fragment|gclid|gbraid|wbraid|fbclid/);
  for (const batch of batches) {
    expect(batch.consent).toBe("granted");
    expect(batch.headers).toMatchObject({ "x-sylphx-key": "sylphx_pk_signal_fixture" });
    for (const header of ["authorization", "cookie", "referer"]) expect(batch.headers[header]).toBeUndefined();
  }

  await page.getByRole("navigation", { name: "Main navigation" }).getByRole("link", { name: "Pricing", exact: true }).click();
  await expect.poll(() => pricing().length).toBe(2);
  expect(pricing()[0].event_id).not.toBe(pricing()[1].event_id);
  await page.locator("header .wordmark").click();
  await expect.poll(() => rows("page_view").length).toBe(5);
  await page.getByRole("button", { name: "Cookie settings", exact: true }).click();
  await settings(page).getByRole("button", { name: "Reject optional", exact: true }).click();
  await expect.poll(() => page.evaluate(() => localStorage.getItem("lockdocs.signal.v1.touch"))).toBeNull();
  const count = batches.length;
  await page.getByRole("navigation", { name: "Main navigation" }).getByRole("link", { name: "Pricing", exact: true }).click();
  await expect(page).toHaveURL(url => url.pathname === "/pricing");
  expect(batches).toHaveLength(count);
});

base("localized pricing emits a sanitized pricing view only on grant", async ({ page }) => {
  const { batches, rows } = await signalOnly(page);
  await page.goto("/zh-TW/pricing?currency=HKD&token=fixture-private");
  const panel = page.getByRole("region", { name: "Cookie 設定" });
  await expect(panel).toBeVisible();
  expect(batches).toEqual([]);
  await panel.getByRole("button", { name: "僅限分析", exact: true }).click();
  await expect.poll(() => rows("pricing_view").map(row => row.properties)).toEqual([
    { path: "/zh-TW/pricing", entry_surface: "pricing" },
  ]);
  expect(JSON.stringify(batches)).not.toContain("fixture-private");
});

base("Sec-GPC denies Signal even with an existing grant", async ({ page }) => {
  const { batches } = await signalOnly(page);
  await page.setExtraHTTPHeaders({ "CF-IPCountry": "US", "Sec-GPC": "1" });
  await page.addInitScript(() => {
    document.cookie = "__Host-lockdocs-signal-choice=granted; Path=/; Secure; SameSite=Lax";
  });
  await page.goto("/?" + query);
  await page.getByRole("button", { name: "Cookie settings", exact: true }).click();
  await expect(settings(page).getByRole("status")).toContainText("Global Privacy Control");
  await expect(settings(page).getByRole("button", { name: "Analytics only" })).toBeDisabled();
  await page.getByRole("navigation", { name: "Main navigation" }).getByRole("link", { name: "Pricing", exact: true }).click();
  await expect(page).toHaveURL(url => url.pathname === "/pricing");
  expect(batches).toEqual([]);
  expect(await page.evaluate(() => localStorage.getItem("lockdocs.signal.v1.touch"))).toBeNull();
});

authTest.use({ billing: true });
authTest("full grant binds all UTMs through sign-in and emits begin_checkout and checkout_started", async ({ page, auth }) => {
  await install(page, "DE");
  auth.ticketSignal.is_new_user = true;
  const { batches, rows } = await intercept(page, auth.origin);
  await page.goto(auth.origin + "/pricing?" + query);
  await expect(settings(page)).toBeVisible();
  expect(batches).toEqual([]);
  await settings(page).getByRole("button", { name: "Allow analytics & ads", exact: true }).click();
  await expect.poll(() => rows("page_view").map(row => row.properties)).toEqual([{ path: "/pricing", ...campaign, ...clicks }]);
  await page.getByRole("link", { name: /Get Lockdocs Pro/ }).click();
  await page.getByRole("link", { name: "Continue with Google" }).click();
  await expect(page).toHaveURL(auth.origin + "/pricing");
  await expect.poll(() => rows("sign_up").map(row => row.properties)).toEqual([
    { ...campaign, ...clicks, method: "google", entry_surface: "pricing" },
  ]);
  // Cancel native submission twice to exercise the same logical begin_checkout
  // twice, then submit for real and observe the separate server-side milestone.
  await page.evaluate(() => {
    let remaining = 2;
    document.querySelector('form[action="/api/billing/checkout"]')!.addEventListener("submit", event => {
      if (remaining-- > 0) event.preventDefault();
    });
  });
  const button = page.getByRole("button", { name: "Continue to secure checkout" });
  await button.click();
  await button.click();
  await expect.poll(() => rows("begin_checkout").map(row => row.properties)).toEqual([
    { ...campaign, ...clicks, entry_surface: "pricing", plan_sku: "lockdocs_pro_seat_year", currency: "USD", value: 120 },
  ]);
  await page.route("https://checkout.stripe.com/fixture", route => route.fulfill({ status: 200, body: "Fixture checkout" }));
  await button.click();
  await expect(page).toHaveURL("https://checkout.stripe.com/fixture");
  expect(rows("begin_checkout")).toHaveLength(1);
  expect(JSON.stringify(batches.map(batch => batch.events))).not.toMatch(/private-query|reader@|usr_reader|auth_ticket|checkout_fixture/);
  await expect.poll(() => auth.signalBatches.flatMap(batch => batch.events)).toMatchObject([
    { event_name: "checkout_started", numeric_value: 120,
      properties: { entry_surface: "pricing", plan_sku: "lockdocs_pro_seat_year", currency: "USD" } },
  ]);
  await expect.poll(() => auth.bindings).toHaveLength(1);
  const binding = auth.bindings[0] as { gclid: string; gbraid: string; wbraid: string; fbc: string; page_location: string };
  expect(binding).toMatchObject({ gclid: clicks.gclid, gbraid: clicks.gbraid, wbraid: clicks.wbraid });
  expect(binding.fbc).toMatch(/\.meta-click$/);
  expect(Object.fromEntries(new URL(binding.page_location).searchParams)).toEqual(campaign);
  expect(auth.rejections).toEqual([]);
});
