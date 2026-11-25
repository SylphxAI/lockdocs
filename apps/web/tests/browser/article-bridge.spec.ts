import { expect, test } from "@playwright/test";
import { settings } from "./analytics-fixture";

const campaign = { utm_source: "newsletter", utm_medium: "email", utm_campaign: "Docs + Pro", gclid: "sample-click" };
const search = new URLSearchParams({ ...campaign, token: "private-sentinel" });
const bridgeTitle = "Try it with a lockfile";
const paths = ["/compare", "/benchmarks", "/guide", "/pro"];

for (const path of paths) {
  test("shows the reachable campaign links at the end of " + path, async ({ page }, info) => {
    await page.setExtraHTTPHeaders({ "CF-IPCountry": "DE" });
    const response = await page.goto(path + "?" + search);
    expect(response?.status()).toBe(200);
    const card = page.getByRole("region", { name: bridgeTitle });
    await expect(card).toHaveCount(1);
    await expect(card.locator("xpath=..")).toHaveClass("doc-body");
    expect(await card.evaluate(element => element.nextElementSibling)).toBeNull();
    await expect(settings(page)).toBeVisible();
    await page.evaluate(() => document.fonts.ready);
    await card.scrollIntoViewIfNeeded();
    const viewport = page.viewportSize()!.width;
    expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(viewport);
    for (const [name, path] of [["Try the playground", "/playground"], ["See Lockdocs Pro — US$120/yr", "/pricing"]]) {
      const link = card.getByRole("link", { name, exact: true });
      await expect(link).toBeInViewport({ ratio: 1 });
      const href = new URL((await link.getAttribute("href"))!, page.url());
      expect(href.pathname).toBe(path);
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
    const playground = card.getByRole("link", { name: "Try the playground" });
    if (info.project.name === "desktop") {
      await playground.focus();
      await expect(playground).toBeFocused();
      await playground.press("Enter");
    } else {
      await playground.tap();
    }
    await expect(page).toHaveURL(url => url.pathname === "/playground");
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

test("keeps other documentation free of the conversion bridge", async ({ page }) => {
  await page.goto("/guide/how-it-works");
  await expect(page.getByRole("region", { name: bridgeTitle })).toHaveCount(0);
});
