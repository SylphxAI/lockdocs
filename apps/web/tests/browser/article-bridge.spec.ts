import { expect, test } from "@playwright/test";
import { settings } from "./analytics-fixture";

const campaign = {
  utm_id: "docs-id", utm_source: "newsletter", utm_medium: "email", utm_campaign: "Docs + Pro", utm_term: "lockfile",
  utm_content: "footer CTA", utm_source_platform: "web", utm_creative_format: "text", utm_marketing_tactic: "acquisition",
  gclid: "sample-click",
};
const search = new URLSearchParams({ ...campaign, token: "private-sentinel" });
const bridgeTitle = "Try it with a lockfile";
const articlePaths = [
  "/guide", "/guide/setup", "/guide/ecosystems", "/guide/fetch", "/guide/how-it-works",
  "/reference", "/reference/tools", "/capabilities",
];
const hubPaths = ["/docs", "/zh-TW/docs"];
const paths = ["/compare", "/benchmarks", "/pro", ...articlePaths, ...hubPaths];

for (const path of paths) {
  test("shows the reachable campaign links at the end of " + path, async ({ page }, info) => {
    await page.setExtraHTTPHeaders({ "CF-IPCountry": "DE" });
    const response = await page.goto(path + "?" + search);
    expect(response?.status()).toBe(200);
    const zh = path === "/zh-TW/docs";
    const card = page.getByRole("region", { name: zh ? "用 lockfile 試試看" : bridgeTitle });
    const playgroundLabel = zh ? "開啟試用工具" : "Try the playground";
    const proLabel = zh ? "查看 Lockdocs Pro — US$120／年" : "See Lockdocs Pro — US$120/yr";
    const prefix = zh ? "/zh-TW" : "";
    await expect(card).toHaveCount(1);
    await expect(card.locator("xpath=..")).toHaveClass("doc-body");
    expect(await card.evaluate(element => element.nextElementSibling)).toBeNull();
    await expect(zh ? page.getByRole("region", { name: "Cookie 設定" }) : settings(page)).toBeVisible();
    await page.evaluate(() => document.fonts.ready);
    await card.scrollIntoViewIfNeeded();
    const viewport = page.viewportSize()!.width;
    expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(viewport);
    for (const [name, destination] of [[playgroundLabel, "/playground"], [proLabel, "/pricing"]]) {
      const link = card.getByRole("link", { name, exact: true });
      await expect(link).toBeInViewport({ ratio: 1 });
      const href = new URL((await link.getAttribute("href"))!, page.url());
      expect(href.pathname).toBe(prefix + destination);
      expect(Object.fromEntries(href.searchParams)).toEqual(campaign);
      const box = (await link.boundingBox())!;
      expect(box.height).toBeGreaterThanOrEqual(44);
      expect(box.x).toBeGreaterThanOrEqual(0);
      expect(box.x + box.width).toBeLessThanOrEqual(viewport);
      expect(await link.evaluate(element => {
        const box = element.getBoundingClientRect();
        return element.contains(document.elementFromPoint(box.x + box.width / 2, box.y + box.height / 2));
      })).toBe(true);
    }
    if (path === "/compare") await expect(page.locator("article a[href='/context7-alternative']")).toBeVisible();
    if (process.env.LOCKDOCS_LOCAL_CAPTURES) await card.screenshot({ path: info.outputPath("article-bridge.png") });
    const playground = card.getByRole("link", { name: playgroundLabel });
    if (info.project.name === "desktop") {
      await playground.focus();
      await expect(playground).toBeFocused();
      await playground.press("Enter");
    } else {
      await playground.tap();
    }
    await expect(page).toHaveURL(url => url.pathname === prefix + "/playground");
    expect(Object.fromEntries(new URL(page.url()).searchParams)).toEqual(campaign);
  });
}

test("SSR publishes a single annual Pro offer without campaign data", async ({ request }) => {
  const response = await request.get("/pro?" + search);
  expect(response.ok()).toBe(true);
  const html = await response.text();
  const scripts = [...html.matchAll(/<script\b[^>]*type="application\/ld\+json"[^>]*>(.*?)<\/script>/gs)].map(match => JSON.parse(match[1]));
  const origin = "https://lockdocs.sylphx.com";
  expect(scripts).toEqual([{
    "@context": "https://schema.org", "@type": "SoftwareApplication", "@id": origin + "/#lockdocs",
    name: "Lockdocs", url: origin + "/pro", applicationCategory: "DeveloperApplication", operatingSystem: "Linux, macOS, Windows",
    offers: [{ "@type": "Offer", name: "Lockdocs Pro", description: expect.stringMatching(/lockdocs upgrade.*private package registries/),
      url: origin + "/pricing", price: 120, priceCurrency: "USD",
      priceSpecification: { "@type": "UnitPriceSpecification", price: 120, priceCurrency: "USD", billingDuration: "P1Y", valueAddedTaxIncluded: true },
    }],
  }]);
  expect(html).toContain('rel="canonical" href="' + origin + '/pro"');
});

test("leaves vision and legal pages free of the conversion bridge", async ({ page }) => {
  for (const path of ["/vision", "/legal/privacy", "/legal/terms"]) {
    await page.goto(path);
    await expect(page.getByRole("region", { name: bridgeTitle })).toHaveCount(0);
  }
});
