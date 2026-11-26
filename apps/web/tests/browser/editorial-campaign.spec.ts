import { expect, test, type Page } from "@playwright/test";
import { analyticsFrame, install, settings, tagPath } from "./analytics-fixture";

const comparison = "/context7-alternative";
const guide = "/guides/up-to-date-docs-for-cursor-and-claude-code";
const campaign = { gclid: "TEST999", utm_source: "google", utm_medium: "cpc", utm_campaign: "Docs + MCP & Pro" };
const query = new URLSearchParams({ ...campaign, token: "private-sentinel" });
const hero = (page: Page) => page.locator(".editorial-hero");
const playground = (page: Page) => hero(page).getByRole("link", { name: /^(Try the playground|Try the Zod example)$/ });
const pricing = (page: Page) => hero(page).getByRole("link", { name: "See Lockdocs Pro" });

for (const [path, secondary, destinations] of [
  [comparison, "See Lockdocs Pro", ["/playground", "/pricing", "/playground#upgrade", "/guide/how-it-works", "/compare", "/benchmarks", "/playground", "/docs", guide, "/pricing"]],
  [guide, "Compare with Context7", ["/playground#upgrade", comparison, "/guide/setup#without-nodejs", "/playground#upgrade", "/reference/tools", comparison, "/pricing"]],
] as const) {
  test(`shows campaign CTAs and keeps attribution URL-only on ${path}`, async ({ page }, info) => {
    const google = await install(page, "DE");
    const errors: string[] = [];
    const analytics: string[] = [];
    page.on("pageerror", error => errors.push(error.message));
    page.on("console", message => { if (message.type() === "error") errors.push(message.text()); });
    page.on("request", request => {
      if (new URL(request.url()).pathname === tagPath || request.url().includes("/v1/analytics/events:batch")) analytics.push(request.url());
    });
    const response = await page.goto(path + "?" + query);
    expect(response?.status()).toBe(200);
    expect(response?.headers()["cache-control"]).toBe("private, no-store");
    await expect(settings(page)).toBeVisible();
    await page.evaluate(() => document.fonts.ready);
    const dock = (await settings(page).boundingBox())!;
    const width = page.viewportSize()!.width;
    expect(await page.evaluate(() => ({ scrollX, scrollY, width: document.documentElement.scrollWidth }))).toEqual({ scrollX: 0, scrollY: 0, width });
    for (const cta of [playground(page), hero(page).getByRole("link", { name: secondary })]) {
      await expect(cta).toBeInViewport({ ratio: 1 });
      const { x, y, width: w, height } = (await cta.boundingBox())!;
      expect(x).toBeGreaterThanOrEqual(0);
      expect(x + w).toBeLessThanOrEqual(width);
      expect(height).toBeGreaterThanOrEqual(44);
      expect(y + height + 20).toBeLessThanOrEqual(dock.y);
      expect(await cta.evaluate(el => {
        const box = el.getBoundingClientRect();
        return el.contains(document.elementFromPoint(box.x + box.width / 2, box.y + box.height / 2));
      })).toBe(true);
    }
    const hrefs = await page.locator("main a").evaluateAll(links => links.map(link => link.getAttribute("href")!));
    const bodyLinks = hrefs.filter(href => href.startsWith("/")).map(href => new URL(href, page.url()));
    expect(bodyLinks.map(url => url.pathname + url.hash)).toEqual(destinations);
    for (const url of bodyLinks) expect(Object.fromEntries(url.searchParams)).toEqual(url.pathname === "/pricing"
      ? { ...campaign, entry_surface: path === comparison ? "context7_alternative" : "guide_cursor_claude_code" }
      : url.hash === "#upgrade" ? { ...campaign, example: path === comparison ? "next" : "zod" } : campaign);
    for (const href of hrefs.filter(href => !href.startsWith("/"))) expect(href).not.toMatch(/gclid|utm_|private-sentinel/);
    if (process.env.LOCKDOCS_LOCAL_CAPTURES) {
      await page.screenshot({ path: info.outputPath("hero.png"), animations: "disabled" });
    }
    const next = path === comparison ? "/pricing" : "/playground";
    await (path === comparison ? pricing(page) : playground(page)).click();
    await expect(page).toHaveURL(url => url.pathname === next && url.searchParams.get("gclid") === campaign.gclid);
    expect(Object.fromEntries(new URL(page.url()).searchParams)).toEqual(path === comparison
      ? { ...campaign, entry_surface: "context7_alternative" } : { ...campaign, example: "zod" });
    await expect(settings(page)).toBeVisible();
    await expect(page.locator(analyticsFrame)).toHaveCount(0);
    expect(await page.context().cookies()).toEqual([]);
    expect(await page.evaluate(() => [localStorage.length, sessionStorage.length])).toEqual([0, 0]);
    expect([...google, ...analytics, ...errors]).toEqual([]);
  });
}

for (const choice of ["Sec-GPC", "Reject optional"] as const) {
  test(`${choice} strips campaign data on initial HTML and SPA navigation`, async ({ page, request }) => {
    const google = await install(page, "DE");
    const headers: Record<string, string> = choice === "Sec-GPC" ? { "Sec-GPC": "1" } : { Cookie: "__Host-lockdocs-signal-choice=denied" };
    const response = await request.get(guide + "?" + query, { headers });
    expect(response.ok()).toBe(true);
    expect(response.headers()["cache-control"]).toBe("private, no-store");
    expect(response.headers().vary).toContain("Sec-GPC");
    expect(await response.text()).toContain('href="/playground?example=zod#upgrade"');
    expect(await response.text()).not.toMatch(/href="[^\"]*gclid=/);
    if (choice === "Sec-GPC") await page.setExtraHTTPHeaders(headers);
    await page.goto(guide + "?" + query);
    if (choice === "Reject optional") {
      await expect(playground(page)).toHaveAttribute("href", /gclid=TEST999/);
      await settings(page).getByRole("button", { name: choice }).click();
    }
    await expect(playground(page)).toHaveAttribute("href", "/playground?example=zod#upgrade");
    await hero(page).getByRole("link", { name: "Compare with Context7" }).click();
    await expect(page).toHaveURL(url => url.pathname === comparison && !url.search);
    await page.goto(comparison + "?" + query);
    await expect(pricing(page)).toHaveAttribute("href", "/pricing?entry_surface=context7_alternative");
    await pricing(page).click();
    await expect(page).toHaveURL(url => url.pathname === "/pricing" && url.search === "?entry_surface=context7_alternative");
    await expect(page.locator(analyticsFrame)).toHaveCount(0);
    expect(google).toEqual([]);
    expect(await page.evaluate(() => localStorage.getItem("lockdocs.signal.v1.touch"))).toBeNull();
  });
}

test("counts the Pro CTA after analytics-only consent without exposing campaign IDs or other inputs", async ({ page }) => {
  const wire: URLSearchParams[] = [];
  await install(page, "DE", wire);
  await page.goto(comparison + "?" + query);
  await settings(page).getByRole("button", { name: "Analytics only", exact: true }).click();
  await expect.poll(() => wire.some(event => event.get("en") === "page_view")).toBe(true);
  await pricing(page).click();
  await expect(page).toHaveURL(url => url.pathname === "/pricing");
  await expect.poll(() => wire.filter(event => event.get("en") === "pro_cta_clicked").map(event =>
    [...event].filter(([key]) => /^epn?\./.test(key)),
  )).toEqual([[["ep.entry_surface", "context7_alternative"]]]);
  expect(wire.map(event => event.toString()).join(" ")).not.toMatch(/TEST999|private-sentinel|utm_/);
});
