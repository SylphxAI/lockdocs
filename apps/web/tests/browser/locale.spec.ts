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

test("Traditional Chinese aliases redirect permanently to the canonical pages with the original query", async ({ request }) => {
  const routes = ["/zh-Hant", "/zh-Hant/docs", "/zh-Hant/playground", "/zh-Hant/pricing"];
  const destinations = ["/zh-TW", "/zh-TW/docs", "/zh-TW/playground", "/zh-TW/pricing"];
  const query = "?currency=HKD&ref=a%20b&ref=other";
  for (const [index, route] of routes.entries()) {
    const destination = destinations[index];
    const bare = await request.get(route, { maxRedirects: 0 });
    expect(bare.status(), route).toBe(308);
    expect(bare.headers().location).toBe(destination);
    const withQuery = await request.get(route + query, { maxRedirects: 0 });
    expect(withQuery.status(), route).toBe(308);
    expect(withQuery.headers().location).toBe(destination + query);
  }
  const head = await request.head("/zh-Hant/pricing/" + query, { maxRedirects: 0 });
  expect(head.status()).toBe(308);
  expect(head.headers().location).toBe("/zh-TW/pricing" + query);
  for (const unknown of ["/zh-Hant/nonexistent", "/zh-Hant-extra/pricing"]) {
    expect((await request.get(unknown, { maxRedirects: 0 })).status()).toBe(404);
  }
});

test("Traditional Chinese pricing shows all four purchase amounts and wraps without an orphan", async ({ page }, testInfo) => {
  await page.goto("/zh-TW/pricing");
  const headingText = "升級前，先找出受影響的程式碼";
  const heading = page.getByRole("heading", { name: headingText, exact: true, level: 1 });
  await expect(heading).toBeVisible();
  for (const width of testInfo.project.name === "desktop" ? [1440, 1280, 1024] : [390]) {
    await page.setViewportSize({ width, height: page.viewportSize()!.height });
    const dimensions = async (locator: typeof heading) => locator.evaluate(async el => {
      await document.fonts.ready;
      const text = document.createTreeWalker(el, NodeFilter.SHOW_TEXT);
      const range = document.createRange();
      const lines = new Map<number, number>();
      while (text.nextNode()) {
        const node = text.currentNode as Text;
        for (let i = 0; i < node.length; i++) {
          range.setStart(node, i);
          range.setEnd(node, i + 1);
          const rect = range.getBoundingClientRect();
          if (rect.width > 0) lines.set(rect.top, (lines.get(rect.top) ?? 0) + (/\S/u.test(node.data[i]) ? 1 : 0));
        }
      }
      return [...lines.values()];
    });
    const lineLengths = await dimensions(heading);
    expect(lineLengths.reduce((total, length) => total + length, 0), `viewport ${width}px`).toBe(headingText.length);
    expect(lineLengths, `viewport ${width}px`).not.toContain(1);
    const subtitle = await dimensions(page.locator(".pricing-intro p"));
    expect(subtitle.at(-1), `subtitle at ${width}px`).toBeGreaterThan(3);
  }
  const currencies = page.getByRole("navigation", { name: "付款幣別" });
  for (const [code, label] of [["USD", "US$120"], ["GBP", "£99"], ["EUR", "€109"], ["HKD", "HK$938"]]) {
    const option = currencies.getByRole("link", { name: code, exact: true });
    if (code !== "USD") await option.click();
    await expect(option).toHaveAttribute("aria-current", "page");
    await expect(page.getByRole("link", { name: `購買 Lockdocs Pro — ${label}／年`, exact: true })).toBeVisible();
  }
});

test("Traditional Chinese cookie settings and source link are localized on every translated page", async ({ page }, testInfo) => {
  await page.setExtraHTTPHeaders({ "CF-IPCountry": "DE" });
  // Keep this copy-only check off the real collector, even on a consent regression.
  await page.route(/^https:\/\/(www\.googletagmanager\.com|[^/]+\.google-analytics\.com)\//, route => route.abort());
  await page.goto("/zh-TW/pricing");
  const banner = page.getByRole("region", { name: "Cookie 設定" });
  for (const [navLabel, path] of [[null, "/zh-TW/pricing"], ["lockdocs.", "/zh-TW"], ["文件", "/zh-TW/docs"], ["試用工具", "/zh-TW/playground"]] as const) {
    if (navLabel) await page.getByRole("banner").getByRole("link", { name: navLabel, exact: true }).click();
    await expect(page).toHaveURL(url => url.pathname === path);
    const footer = page.getByRole("contentinfo");
    const settings = footer.getByRole("button", { name: "Cookie 設定", exact: true });
    await expect(settings).toBeEnabled();
    if (navLabel) await settings.click();
    await expect(banner.getByText("選用 Cookie 用於分析與個人化廣告。", { exact: false })).toBeVisible();
    await expect(banner.getByRole("link", { name: "隱私權政策" })).toHaveAttribute("href", "/legal/privacy");
    const reject = banner.getByRole("button", { name: "拒絕選用項目" });
    await expect(reject).toBeEnabled();
    await expect(banner.getByRole("button", { name: "僅限分析" })).toBeEnabled();
    await expect(banner.getByRole("button", { name: "允許分析與廣告" })).toBeEnabled();
    await expect(footer.getByRole("link", { name: "原始碼", exact: true })).toHaveAttribute("href", "https://github.com/SylphxAI/lockdocs");
    if (!navLabel && process.env.LOCKDOCS_LOCAL_CAPTURES) await page.screenshot({ path: testInfo.outputPath("zh-tw-pricing.png") });
    await reject.click();
    await expect(banner).toHaveCount(0);
  }
});

test("localized home, docs search, and pricing stay usable on each viewport", async ({ page }, testInfo) => {
  await page.goto("/");
  await expect(page.locator(".bench-rail")).toContainText("With Lockdocs");
  await page.getByRole("link", { name: "繁體中文" }).click();
  await expect(page).toHaveURL(/\/zh-TW$/);
  await expect(page.locator("html")).toHaveAttribute("lang", "zh-TW");
  await expect(page.locator("head link[rel=canonical]")).toHaveAttribute("href", origin + "/zh-TW");
  await expect(page.getByRole("heading", { level: 1 })).toContainText("版本的文件");
  await expect(page.locator(".bench-rail")).toContainText("答對題數 · 預先下載文件後");
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
  await expect(page.locator(".plan-pro .plan-price")).toHaveText("HK$938／年");
  await expect(page.locator(".plan-total")).toHaveText("年費一次付清，已含稅");
  await checkWidth();
  await page.getByRole("link", { name: "English" }).click();
  await expect(page).toHaveURL(/\/pricing\?currency=HKD$/);
  await expect(page.locator(".plan-pro .plan-price")).toHaveText("HK$938/yr");
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

test("Traditional Chinese Playground copies both commands without clipping", async ({ context, page }, info) => {
  await context.grantPermissions(["clipboard-read", "clipboard-write"]);
  await page.setExtraHTTPHeaders({ "CF-IPCountry": "DE" });
  await page.goto("/zh-TW/playground");
  await page.getByRole("button", { name: "拒絕選用項目" }).click();
  await page.getByLabel("lockfile 格式").selectOption("requirements.txt");
  const name = "example-" + "a".repeat(160);
  await page.locator("#lockfile").fill(name + "==1.2.3");
  await page.getByRole("button", { name: "查出套件版本" }).click();
  for (const [label, command] of [
    ["複製 CLI 指令", `npx -y @sylphx/lockdocs pypi:${name}@1.2.3 --offline`],
    ["複製升級指令", "lockdocs upgrade next 15.0.0"],
  ]) {
    const button = page.getByRole("button", { name: label, exact: true });
    const row = button.locator("..");
    await expect(row.locator("code")).toHaveText(command);
    if (info.project.name === "mobile") await button.tap();
    else await button.press("Enter");
    await expect(button.locator("+ [role=status]")).toHaveText("已複製");
    await expect.poll(() => page.evaluate(() => navigator.clipboard.readText())).toBe(command);
    expect(await row.evaluate(el => el.scrollWidth)).toBe(await row.evaluate(el => el.clientWidth));
  }
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(page.viewportSize()!.width);
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
