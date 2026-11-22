import { expect, test } from "@playwright/test";

const origin = "https://lockdocs.sylphx.com";
const comparison = "/context7-alternative";
const guide = "/guides/up-to-date-docs-for-cursor-and-claude-code";

for (const { path, from, heading, type } of [
  { path: comparison, from: "/compare", heading: "Context7 alternative for exact-version docs", type: "SoftwareApplication" },
  { path: guide, from: "/docs", heading: "Up-to-date docs for Cursor and Claude Code", type: "HowTo" },
]) {
  test(heading + " is discoverable and serves structured data without loading Google tags", async ({ page, request }, info) => {
    const googleRequests: string[] = [];
    await page.route(/^https:\/\/([^/]+\.)?(googletagmanager\.com|google-analytics\.com|doubleclick\.net|google\.com|googleadservices\.com)\//, route => {
      googleRequests.push(route.request().url());
      return route.abort();
    });
    await page.route("**/cdn-cgi/trace", route => route.fulfill({ contentType: "text/plain", body: "loc=DE\n" }));

    const source = await request.get(from);
    expect(await source.text()).toContain('href="' + path + '"');
    await page.goto(from);
    await page.locator('#main a[href="' + path + '"]').click();
    await expect(page.getByRole("heading", { level: 1, name: heading })).toBeVisible();
    await expect(page.getByRole("region", { name: "Cookie settings" })).toBeVisible();
    await expect(page).toHaveTitle(heading + " · Lockdocs");

    const response = await page.goto(path + "?ref=editorial-check");
    expect(response?.status()).toBe(200);
    const canonical = origin + path;
    await expect(page.locator('link[rel="canonical"]')).toHaveAttribute("href", canonical);
    await expect(page.locator('meta[property="og:url"]')).toHaveAttribute("content", canonical);
    await expect(page.locator('meta[property="og:title"]')).toHaveAttribute("content", heading + " · Lockdocs");
    const description = await page.locator('meta[name="description"]').getAttribute("content");
    expect(description).toBeTruthy();
    await expect(page.locator('meta[property="og:description"]')).toHaveAttribute("content", description!);
    await expect(page.locator('meta[property="og:image"]')).toHaveAttribute("content", origin + "/og-image.png");
    const ogImage = await request.get("/og-image.png");
    expect(ogImage.ok()).toBeTruthy();
    expect(ogImage.headers()["content-type"]).toContain("image/png");

    const html = await response!.text();
    const schema = [...html.matchAll(/<script\b[^>]*\btype="application\/ld\+json"[^>]*>(.*?)<\/script>/gs)].map(([, json]) => JSON.parse(json));
    expect(schema.map(entry => entry["@type"])).toEqual([type, "FAQPage"]);
    expect(schema.every(entry => entry["@context"] === "https://schema.org")).toBeTruthy();
    await expect(page.locator('script[type="application/ld+json"]')).toHaveCount(2);
    for (const item of schema[1].mainEntity) {
      const summary = page.getByText(item.name, { exact: true }).and(page.locator("summary"));
      await expect(summary).toBeVisible();
      await summary.click();
      await expect(page.getByText(item.acceptedAnswer.text, { exact: true })).toBeVisible();
      await summary.click();
    }
    if (type === "HowTo") {
      expect(schema[0].step).toHaveLength(3);
      for (const step of schema[0].step) {
        const anchor = new URL(step.url).hash;
        await expect(page.locator(anchor).getByRole("heading", { name: step.name })).toBeVisible();
        await expect(page.locator(anchor).getByText(step.text, { exact: true })).toBeVisible();
      }
      const config = await page.getByLabel("Cursor project configuration", { exact: true }).textContent();
      expect(JSON.parse(config!)).toEqual({ mcpServers: { lockdocs: { command: "npx", args: ["-y", "@sylphx/lockdocs@0.7.5", "mcp"] } } });
    } else {
      expect(schema[0]).toMatchObject({ name: "Lockdocs", url: origin, offers: { price: 0, priceCurrency: "USD" } });
      await expect(page.locator('main a[href="/playground"]')).toBeVisible();
    }

    await expect(page.locator('iframe[src="/api/analytics/tag"]')).toHaveCount(0);
    expect(googleRequests).toEqual([]);
    if (process.env.LOCKDOCS_LOCAL_CAPTURES) {
      await page.getByRole("button", { name: "Reject optional" }).click();
      await page.evaluate(() => document.fonts.ready);
      await page.locator("h1").evaluate(element => window.scrollTo(0, element.closest("header")!.offsetTop - 67));
      await page.screenshot({ path: info.outputPath("editorial.png"), fullPage: true, animations: "disabled" });
    }
  });
}

test("sitemap is XML, cached, and lists both canonical routes once", async ({ page }) => {
  const response = await page.goto("/sitemap.xml");
  expect(response?.status()).toBe(200);
  expect(response?.headers()["content-type"]).toBe("application/xml; charset=utf-8");
  expect(response?.headers()["cache-control"]).toBe("public, max-age=3600");
  const urls = await page.evaluate(async () => {
    const xml = new DOMParser().parseFromString(await (await fetch("/sitemap.xml")).text(), "application/xml");
    return [...xml.getElementsByTagNameNS("http://www.sitemaps.org/schemas/sitemap/0.9", "loc")].map(loc => loc.textContent);
  });
  for (const path of [comparison, guide]) expect(urls.filter(url => url === origin + path)).toHaveLength(1);
});
