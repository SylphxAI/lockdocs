import { expect, type Page } from "@playwright/test";
import { test } from "./auth-fixture";

const path = "/sign-in?next=%2Fpricing";

async function recordShifts(page: Page) {
  await page.addInitScript(() => {
    const shifts: number[] = [];
    Object.defineProperty(window, "__layoutShifts", { value: shifts });
    new PerformanceObserver(list => {
      for (const entry of list.getEntries() as (PerformanceEntry & { value: number; hadRecentInput: boolean })[]) {
        if (!entry.hadRecentInput) shifts.push(entry.value);
      }
    }).observe({ type: "layout-shift", buffered: true });
  });
}

const settle = (page: Page) => page.evaluate(() => new Promise(done => requestAnimationFrame(() => requestAnimationFrame(done))));
const shiftTotal = (page: Page) => page.evaluate(() => (window as unknown as { __layoutShifts: number[] }).__layoutShifts.reduce((sum, value) => sum + value, 0));

async function serverLayout(page: Page, url: string) {
  const context = await page.context().browser()!.newContext({ javaScriptEnabled: false, viewport: page.viewportSize(), ignoreHTTPSErrors: true });
  try {
    const plain = await context.newPage();
    await plain.goto(url);
    return await plain.locator(".auth-terms").boundingBox();
  } finally { await context.close(); }
}

for (const passkey of [false, true]) {
  test(`sign-in ships its final methods in the server HTML and hydrates in place (passkey ${passkey ? "on" : "off"})`, async ({ page, auth }) => {
    auth.methods.email_otp = true;
    auth.methods.passkey = passkey;
    const html = await (await page.request.get(auth.origin + path)).text();
    expect(html).toContain('href="/api/auth/google?next=%2Fpricing"');
    expect(html).toContain('href="/api/auth/github?next=%2Fpricing"');
    expect(html).toMatch(/<input[^>]+id="signin-email"/);
    expect(html.includes("Continue with a passkey")).toBe(passkey);
    expect(html).not.toContain("Loading sign-in options");

    const before = await serverLayout(page, auth.origin + path);
    await recordShifts(page);
    const session = page.waitForResponse(response => new URL(response.url()).pathname === "/api/auth/session");
    await page.goto(auth.origin + path);
    expect((await session).status()).toBe(200);
    await expect(page.getByRole("button", { name: "Continue", exact: true })).toBeEnabled();
    if (passkey) await expect(page.getByRole("button", { name: "Continue with a passkey" })).toBeEnabled();
    await settle(page);
    expect(await page.locator(".auth-terms").boundingBox()).toEqual(before);
    expect(await shiftTotal(page)).toBeLessThan(0.01);
    expect(auth.rejections).toEqual([]);
  });
}

for (const mode of ["unavailable", "refused", "missing"] as const) {
  test(`${mode} Auth metadata renders the retry state on the server and recovers on request`, async ({ page, auth }) => {
    auth.outages.mode = mode;
    auth.outages.config = 1;
    const html = await (await page.request.get(auth.origin + path)).text();
    expect(html).toContain("Couldn’t load sign-in options. Try again.");
    expect(html).not.toMatch(/href="\/api\/auth\/(?:google|github)|id="signin-email"/);
    auth.outages.config = 1;
    await recordShifts(page);
    await page.goto(auth.origin + path);
    const card = page.locator(".sign-in-card");
    await expect(card.getByRole("alert")).toHaveText("Couldn’t load sign-in options. Try again.");
    await settle(page);
    expect(await shiftTotal(page)).toBeLessThan(0.01);
    await card.getByRole("button", { name: "Try again", exact: true }).click();
    await expect(card.getByRole("link", { name: "Continue with Google" })).toHaveAttribute("href", "/api/auth/google?next=%2Fpricing");
    await expect(card.getByRole("alert")).toHaveCount(0);
    expect(auth.rejections).toEqual([]);
  });
}

test("browser calls keep the Auth handler's fetch-metadata checks; only the page render reads metadata server-side", async ({ page, auth }) => {
  const options = (site: string) => page.request.get(auth.origin + "/api/auth/options", { headers: { "sec-fetch-site": site, "sec-fetch-mode": "cors", "sec-fetch-dest": "empty" } });
  expect((await options("same-origin")).status()).toBe(200);
  expect((await options("same-site")).status()).toBe(503);
  expect((await options("cross-site")).status()).toBe(503);
  // The handler's config route is not exposed to browsers at all.
  expect((await page.request.get(auth.origin + "/api/auth/config")).status()).toBe(404);
  // A navigation from another site still gets the methods in its HTML.
  const html = await (await page.request.get(auth.origin + path, { headers: { "sec-fetch-site": "cross-site", "sec-fetch-mode": "navigate", "sec-fetch-dest": "document" } })).text();
  expect(html).toContain('href="/api/auth/google?next=%2Fpricing"');
  expect(auth.rejections).toEqual([]);
});

test("pricing Buy opens sign-in with the methods already in place", async ({ page, auth }) => {
  auth.methods.email_otp = true;
  auth.methods.passkey = true;
  await page.goto(auth.origin + "/pricing");
  const session = page.waitForResponse(response => new URL(response.url()).pathname === "/api/auth/session");
  await page.getByRole("link", { name: /^Get Lockdocs Pro/ }).click();
  await expect(page).toHaveURL(auth.origin + path);
  const google = page.locator(".sign-in-card").getByRole("link", { name: "Continue with Google" });
  await expect(google).toBeVisible();
  const first = await page.locator(".auth-terms").boundingBox();
  await session;
  await expect(page.getByRole("button", { name: "Continue", exact: true })).toBeEnabled();
  await settle(page);
  expect(await page.locator(".auth-terms").boundingBox()).toEqual(first);
  expect(auth.rejections).toEqual([]);
});
