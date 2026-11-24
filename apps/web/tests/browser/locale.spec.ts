import { expect, test } from "@playwright/test";

const origin = "https://lockdocs.sylphx.com";
const pairs = new Map([["/", "/zh-TW"], ["/docs", "/zh-TW/docs"], ["/playground", "/zh-TW/playground"], ["/pricing", "/zh-TW/pricing"]]);
const alternateLinks = (head: string) => [...head.matchAll(/<link[^>]* rel="alternate" hreflang="([^"]+)" href="([^"]+)"[^>]*>/g)].map(([, lang, url]) => [lang, url]);
const canonicalLinks = (head: string) => [...head.matchAll(/<link[^>]* rel="canonical" href="([^"]+)"[^>]*>/g)].map(([, url]) => url);

test("SSR head contains reciprocal alternatives, a self-canonical and the default fallback", async ({ request }) => {
  for (const [english, chinese] of pairs) {
    for (const [route, locale] of [[english, "en"], [chinese, "zh-TW"]]) {
      const response = await request.get(route + "?currency=GBP&ref=seo");
      expect(response.status(), route).toBe(200);
      const html = await response.text();
      expect(html).toContain('<html lang="' + locale + '">');
      const head = html.slice(0, html.indexOf("</head>"));
      expect(canonicalLinks(head), route).toEqual([origin + route]);
      expect(alternateLinks(head), route).toEqual([["en", origin + english], ["zh-TW", origin + chinese], ["x-default", origin + english]]);
    }
  }
  const englishOnly = await (await request.get("/guide/setup")).text();
  expect(alternateLinks(englishOnly)).toEqual([["en", origin + "/guide/setup"], ["x-default", origin + "/guide/setup"]]);
  for (const [route, status] of [["/sign-in", 200], ["/zh-TW/nonexistent", 404]] as const) {
    const response = await request.get(route);
    expect(response.status(), route).toBe(status);
    const html = await response.text();
    expect(canonicalLinks(html), route).toEqual([]);
    expect(alternateLinks(html), route).toEqual([]);
  }
});

test("localized home, docs search, and pricing stay usable on each viewport", async ({ page }, testInfo) => {
  await page.goto("/");
  await expect(page.locator(".bench-rail")).toContainText("Version-specific questions answered correctly");
  await page.getByRole("link", { name: "繁體中文" }).click();
  await expect(page).toHaveURL(/\/zh-TW$/);
  await expect(page.locator("html")).toHaveAttribute("lang", "zh-TW");
  await expect(page.locator("head link[rel=canonical]")).toHaveAttribute("href", origin + "/zh-TW");
  await expect(page.getByRole("heading", { level: 1 })).toContainText("版本的文件");
  await expect(page.locator(".bench-rail")).toContainText("答對的版本相關問題數");
  const checkWidth = async () => {
    expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(testInfo.project.name === "mobile" ? 390 : 1440);
  };
  await checkWidth();
  await page.getByRole("link", { name: "文件", exact: true }).click();
  await expect(page.getByRole("heading", { name: "文件總覽" })).toBeVisible();
  const search = page.getByRole("searchbox", { name: "搜尋指南" });
  await search.fill("下載文件");
  await expect(page.getByRole("link", { name: "下載文件（英文）" })).toHaveAttribute("href", "/guide/fetch");
  await search.fill("does-not-exist");
  await expect(page.getByText("找不到符合的指南。")).toBeVisible();
  await checkWidth();
  await page.getByRole("link", { name: "價格", exact: true }).click();
  await page.getByRole("link", { name: "HKD", exact: true }).click();
  await expect(page).toHaveURL(/\/zh-TW\/pricing\?currency=HKD$/);
  await expect(page.locator(".plan-total")).toHaveText("HK$938／年，一次付清，已含稅；月均金額已四捨五入");
  await checkWidth();
  await page.getByRole("link", { name: "English" }).click();
  await expect(page).toHaveURL(/\/pricing\?currency=HKD$/);
  await expect(page.locator(".plan-total")).toContainText("HK$938/yr upfront · Tax included");
  await checkWidth();
});

test("Docs exposes mobile navigation first and copies command blocks verbatim", async ({ context, page }, testInfo) => {
  await context.grantPermissions(["clipboard-read", "clipboard-write"]);
  await page.goto("/guide/setup");
  const search = page.getByRole("searchbox", { name: "Find a guide" });
  await expect(search).toBeInViewport();
  if (testInfo.project.name === "mobile") {
    const toggle = page.getByRole("button", { name: "Browse documentation" });
    await expect(toggle).toBeInViewport();
    await expect(page.locator("#docs-navigation")).toBeHidden();
    await toggle.click();
    await expect(page.locator("#docs-navigation")).toBeVisible();
    await page.getByRole("button", { name: "Hide documentation navigation" }).click();
    await expect(page.locator("#docs-navigation")).toBeHidden();
  }
  const command = "claude mcp add --scope user lockdocs -- npx -y @sylphx/lockdocs mcp";
  const block = page.locator(".doc-code").filter({ hasText: command });
  await block.getByRole("button", { name: "Copy code" }).click();
  await expect.poll(() => page.evaluate(() => navigator.clipboard.readText())).toBe(command);
  expect(await block.locator("pre").evaluate(el => [getComputedStyle(el).overflowX, el.scrollWidth >= el.clientWidth])).toEqual(["auto", true]);
});

test("default samples show exact React and Zod source snippets", async ({ page }) => {
  await page.goto("/playground");
  await page.getByRole("button", { name: /react 18\.3\.1 npm/i }).click();
  await expect(page.locator(".sample-source")).toContainText("return dispatcher.useState(initialState);");
  await expect(page.getByRole("link", { name: /ReactHooks.js/ })).toHaveAttribute("href", /v18\.3\.1.*#L80-L85$/);
  await page.getByRole("button", { name: /zod 4\.6\.7 npm/i }).click();
  await expect(page.locator(".sample-source")).toContainText("return core._email(ZodEmail, params);");
  await expect(page.getByRole("link", { name: /schemas.ts/ })).toHaveAttribute("href", /v4\.6\.7.*#L710-L712$/);
});
