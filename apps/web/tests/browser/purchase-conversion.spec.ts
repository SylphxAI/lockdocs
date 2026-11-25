import { expect, type Page } from "@playwright/test";
import { createHash } from "node:crypto";
import { test, adsId, adsLabel, paidCheckout, paidSession } from "./auth-fixture";
import { analyticsFrame, commands, install, settings } from "./analytics-fixture";
import { claimedPurchasesKey, purchaseConversionPath } from "../../src/lib/google-ads";

test.use({ billing: true });
const returnPath = "/dashboard?billing=return";
const checkoutReturn = returnPath + "&session_id=" + paidSession;
const adsCommands = async (page: Page) => (await commands(page)).filter(([command, target]) =>
  (command === "config" && typeof target === "string" && target.startsWith("AW-")) || (command === "event" && target === "conversion"));
const cookieButton = (page: Page) => page.getByRole("button", { name: "Cookie settings", exact: true });

async function signIn(page: Page, origin: string) {
  await page.goto(origin + "/sign-in?next=%2Fdashboard");
  await page.getByRole("link", { name: "Continue with Google" }).click();
  await expect(page).toHaveURL(origin + "/dashboard");
  await expect(page.getByRole("heading", { name: "Fixture reader" })).toBeVisible();
}

test("claims a Money-verified Ads conversion once and strips the return URL", async ({ page, auth }) => {
  const hits = await install(page, "DE");
  await signIn(page, auth.origin);
  const landing = await page.goto(auth.origin + "/pro?gclid=TEST123&private=sentinel");
  expect(landing?.ok()).toBe(true);
  await settings(page).getByRole("button", { name: "Allow analytics & ads" }).click();
  await expect.poll(() => adsCommands(page)).toEqual([["config", adsId, { send_page_view: false }]]);
  await expect.poll(() => page.frameLocator(analyticsFrame).locator("html").evaluate(() => location.search)).toBe("?gclid=TEST123");

  const claims: unknown[] = [];
  const referrers: string[] = [];
  page.on("request", request => {
    referrers.push(request.headers().referer ?? "");
    if (new URL(request.url()).pathname === purchaseConversionPath) claims.push(request.postDataJSON());
  });
  const response = await page.goto(auth.origin + checkoutReturn);
  expect(response?.headers()["referrer-policy"]).toBe("no-referrer");
  await expect(page).toHaveURL(auth.origin + returnPath);
  const id = createHash("sha256").update("lockdocs-ads-purchase:" + paidCheckout).digest("hex").slice(0, 32);
  await expect.poll(() => adsCommands(page)).toEqual([
    ["config", adsId, { send_page_view: false }],
    ["event", "conversion", { send_to: adsId + "/" + adsLabel, value: 82.5, currency: "GBP", transaction_id: id }],
  ]);
  expect(claims).toEqual([{ session_id: paidSession, ads_consent: true }]);
  expect(referrers.every(value => !value.includes(paidSession))).toBe(true);
  expect(JSON.stringify(await commands(page))).not.toContain(paidSession);
  expect(hits.length).toBeGreaterThan(0);
  const cookie = (await page.context().cookies(auth.origin + purchaseConversionPath)).find(entry => entry.name === "__Secure-lockdocs-ads-claimed");
  expect(cookie).toMatchObject({ secure: true, httpOnly: true, sameSite: "Lax", path: purchaseConversionPath });
  expect(await page.evaluate(key => localStorage.getItem(key), claimedPurchasesKey)).toBe(id);
  expect(await page.evaluate(() => (window as Window & { gtag?: unknown }).gtag)).toBeUndefined();

  await page.reload();
  await expect.poll(() => adsCommands(page)).toEqual([["config", adsId, { send_page_view: false }]]);
  expect(claims).toHaveLength(1);
  const [replay] = await Promise.all([
    page.waitForResponse(item => new URL(item.url()).pathname === purchaseConversionPath),
    page.goto(auth.origin + checkoutReturn),
  ]);
  expect(replay.status()).toBe(204);
  await expect(page).toHaveURL(auth.origin + returnPath);
  expect(await adsCommands(page)).toEqual([["config", adsId, { send_page_view: false }]]);
  expect(auth.rejections).toEqual([]);

  // Simulate a removed server cookie. The verified 200 still must not queue a duplicate locally.
  await page.context().clearCookies({ name: "__Secure-lockdocs-ads-claimed" });
  const [recovered] = await Promise.all([
    page.waitForResponse(item => new URL(item.url()).pathname === purchaseConversionPath),
    page.goto(auth.origin + checkoutReturn),
  ]);
  expect(recovered.status()).toBe(200);
  expect(await adsCommands(page)).toEqual([["config", adsId, { send_page_view: false }]]);
});

for (const [choice, gpc] of [
  ["Reject optional", "off"], ["Analytics only", "off"], ["Allow analytics & ads", "header"], ["Allow analytics & ads", "navigator"],
] as const) {
  test("does not configure or claim Ads with " + choice + " and GPC " + gpc, async ({ page, auth }) => {
    const hits = await install(page, "DE");
    await signIn(page, auth.origin);
    await settings(page).getByRole("button", { name: choice, exact: true }).click();
    if (gpc !== "off") await expect.poll(() => hits.length).toBe(1);
    if (gpc === "header") await page.setExtraHTTPHeaders({ "CF-IPCountry": "DE", "Sec-GPC": "1" });
    if (gpc === "navigator") await page.addInitScript(() => Object.defineProperty(navigator, "globalPrivacyControl", { value: true }));
    hits.length = 0;
    const claims: string[] = [];
    page.on("request", request => { if (new URL(request.url()).pathname === purchaseConversionPath) claims.push(request.url()); });
    await page.goto(auth.origin + checkoutReturn);
    await expect(page).toHaveURL(auth.origin + returnPath);
    await expect(cookieButton(page)).toBeEnabled();
    if (choice === "Analytics only") {
      await expect.poll(() => commands(page)).toContainEqual(["config", "G-TEST123456", expect.objectContaining({ send_page_view: false })]);
    } else {
      await expect(page.locator(analyticsFrame)).toHaveCount(0);
      expect(hits).toEqual([]);
    }
    expect(await adsCommands(page)).toEqual([]);
    expect(claims).toEqual([]);
    expect(auth.calls).not.toContain("/v1/money/checkout_sessions:redeem");
  });
}
