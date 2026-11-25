import { expect, test } from "@playwright/test";

const campaign = {
  gclid: "article-click", gbraid: "article-braid", wbraid: "article-web-braid",
  utm_id: "123456", utm_source: "google", utm_medium: "cpc", utm_campaign: "Docs + MCP & Pro",
  utm_term: "lockfile", utm_content: "upgrade report",
};
const query = new URLSearchParams({ ...campaign, token: "private-sentinel" });

test("keeps campaign parameters on the Pro Markdown CTA before a consent choice", async ({ page }) => {
  await page.setExtraHTTPHeaders({ "CF-IPCountry": "DE" });
  await page.goto("/pro?" + query);
  const cookieSettings = page.getByRole("region", { name: "Cookie settings" });
  await expect(cookieSettings).toBeVisible();

  const cta = page.locator("article").getByRole("link", { name: "See Lockdocs Pro pricing", exact: true });
  await expect(cta).toContainClass("button primary");
  await expect(cta.locator("svg")).toBeVisible();
  const destination = new URL((await cta.getAttribute("href"))!, page.url());
  expect(destination.pathname).toBe("/pricing");
  expect(Object.fromEntries(destination.searchParams)).toEqual(campaign);

  await cta.click();
  await expect(page).toHaveURL(url => url.pathname === "/pricing");
  expect(Object.fromEntries(new URL(page.url()).searchParams)).toEqual(campaign);
  await expect(cookieSettings).toBeVisible();
  expect(await page.context().cookies()).toEqual([]);
  expect(await page.evaluate(() => [localStorage.length, sessionStorage.length])).toEqual([0, 0]);
});

test("leaves non-pricing Markdown links unchanged on campaign landings", async ({ page }) => {
  await page.goto("/docs?" + query);
  const article = page.locator("article");
  await expect(article.locator("p").getByRole("link", { name: "Quickstart", exact: true })).toHaveAttribute("href", "/guide#install-lockdocs");
  await expect(article.getByRole("link", { name: "PyPI", exact: true })).toHaveAttribute("href", "https://pypi.org/project/lockdocs/");
});
