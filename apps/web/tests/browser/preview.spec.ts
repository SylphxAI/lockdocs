import { expect, test } from "@playwright/test";

const pages = ["/", "/playground", "/pricing", "/dashboard", "/docs", "/guide", "/reference", "/benchmarks", "/compare"];
const allDocs = ["/docs", "/guide", "/guide/setup", "/guide/how-it-works", "/guide/ecosystems", "/guide/fetch", "/reference", "/reference/tools", "/benchmarks", "/compare", "/capabilities", "/vision", "/pro"];

test("routes render without errors, overflow, broken images or repeated metadata", async ({ page, baseURL }, info) => {
  const errors: string[] = [];
  const titles: string[] = [];
  const descriptions: string[] = [];
  page.on("console", (message) => { if (message.type() === "error") errors.push(message.text()); });
  page.on("pageerror", (error) => errors.push(error.message));
  page.on("response", (response) => { if (response.url().startsWith(baseURL + "/") && response.status() >= 400) errors.push(response.status() + " " + response.url()); });
  for (const route of pages) {
    const status = await page.goto(route, { waitUntil: "networkidle" });
    expect(status?.status(), route).toBe(200);
    await page.evaluate(() => document.fonts.ready);
    await expect(page.locator("h1"), route).toHaveCount(1);
    const geometry = await page.evaluate(() => ({
      viewport: document.documentElement.clientWidth, scroll: document.documentElement.scrollWidth,
      images: [...document.images].filter((img) => img.naturalWidth === 0).map((img) => img.src),
    }));
    expect(geometry.images, route).toEqual([]);
    expect(geometry.scroll, route).toBeLessThanOrEqual(geometry.viewport);
    await expect(page.locator("body")).not.toContainText(/offline licence key|Sylphx Cloud/i);
    const title = await page.title();
    const description = await page.locator('meta[name="description"]').getAttribute("content");
    expect(title.trim().length).toBeGreaterThan(0);
    expect(description?.trim()).toBeTruthy();
    titles.push(title); descriptions.push(description!);
    if (pages.indexOf(route) <= 4) await page.screenshot({ path: info.outputPath((route.slice(1) || "home") + ".png"), animations: "disabled" });
  }
  expect(new Set(titles).size).toBe(pages.length);
  expect(new Set(descriptions).size).toBe(pages.length);
  expect(errors).toEqual([]);
});

test("workbench resolves pasted text and switches cited migrations", async ({ page }) => {
  await page.goto("/playground");
  await expect(page.getByText("3 pinned")).toBeVisible();
  await expect(page.getByText("next@15.0.0 dist/server/request/cookies.d.ts:24").first()).toBeVisible();
  await page.getByLabel("Lockfile format").selectOption("requirements.txt");
  await page.getByRole("button", { name: "Load sample" }).click();
  await expect(page.getByRole("heading", { name: "pydantic@1.10.18" })).toBeVisible();
  await page.getByLabel("Lockfile input").fill("unknown>=1\nDjango==5.1.3\n");
  await page.getByRole("button", { name: "Resolve versions" }).click();
  await expect(page.getByRole("heading", { name: "django@5.1.3" })).toBeVisible();
  await expect(page.getByText("1 pinned")).toBeVisible();
  await page.getByRole("button", { name: "MCP agent setup" }).click();
  await expect(page.locator(".snippet-box pre")).toContainText("@sylphx/lockdocs");
  await page.getByRole("button", { name: "Pydantic 1.10 → 2.9" }).click();
  await expect(page.getByText("pydantic@2.9.2 pydantic/main.py:352")).toBeVisible();
  await expect(page.getByRole("link", { name: "Pydantic v2 migration", exact: false })).toHaveAttribute("href", /https:\/\/docs\.pydantic\.dev/);
  await page.getByRole("link", { name: "See Pro plans" }).click();
  await expect(page).toHaveURL(/\/pricing$/);
  await expect(page.locator(".plan-pro")).toContainText("$10/mo billed yearly");
  await expect(page.locator(".plan-pro")).toContainText("US$120/yr upfront · Tax included");
  await page.getByRole("link", { name: "GBP" }).click();
  await expect(page.locator(".plan-pro")).toContainText("£8.25/mo billed yearly");
  await expect(page.locator(".plan-pro")).toContainText("£99/yr upfront · Tax included");
  await page.getByRole("link", { name: "EUR" }).click();
  await expect(page.locator(".plan-pro")).toContainText("€9.08/mo billed yearly");
  await expect(page.locator(".plan-pro")).toContainText("€109/yr upfront · Tax included");
  await page.getByRole("link", { name: "HKD" }).click();
  await expect(page.locator(".plan-pro")).toContainText("HK$78.17/mo billed yearly");
  await expect(page.locator(".plan-pro")).toContainText("HK$938/yr upfront · Tax included");
  await expect(page.getByText("generally have 14 days", { exact: false })).toBeVisible();
});

test("all technical docs load with working internal links and anchors", async ({ page, baseURL }) => {
  const allLinks = new Set<string>();
  for (const route of allDocs) {
    expect((await page.goto(route))?.status(), route).toBe(200);
    const links = await page.locator(".doc-body a[href]").evaluateAll((anchors) => anchors.map((a) => (a as HTMLAnchorElement).href));
    for (const link of links) if (link.startsWith(baseURL + "/")) allLinks.add(link);
  }
  for (const link of allLinks) {
    const url = new URL(link);
    expect((await page.goto(url.pathname))?.status(), link).toBe(200);
    if (url.hash) expect(await page.evaluate((id) => Boolean(document.getElementById(id)), decodeURIComponent(url.hash.slice(1))), link).toBeTruthy();
  }
});

test("preview host is noindexed and missing bindings fail closed", async ({ request, page }) => {
  const ordinary = await request.get("/api/health");
  expect(ordinary.headers()["x-robots-tag"]).toBeUndefined();
  const preview = await request.get("/api/health", { headers: { Host: "preview.lockdocs.sylphx.app" } });
  expect(preview.headers()["x-robots-tag"]).toBe("noindex, nofollow");
  expect(await preview.json()).toEqual({ status: "ok" });
  const lookalike = await request.get("/api/health", { headers: { Host: "lockdocs.sylphx.app.example.org" } });
  expect(lookalike.headers()["x-robots-tag"]).toBeUndefined();
  await page.goto("/dashboard");
  await expect(page.getByText("Sign-in isn’t available", { exact: false })).toBeVisible();
  expect((await request.post("/api/billing/checkout")).status()).toBe(503);
});
