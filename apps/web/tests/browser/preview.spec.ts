import { expect, test } from "@playwright/test";

const pages = ["/", "/playground", "/pricing", "/dashboard", "/docs", "/guide", "/reference", "/benchmarks", "/compare", "/pro", "/context7-alternative", "/guides/up-to-date-docs-for-cursor-and-claude-code"];
// These smoke tests exercise the unconfigured preview; analytics.spec.ts covers consent separately.
test.beforeEach(async ({ page }) => {
  await page.route("**/api/analytics/config", route => route.fulfill({ json: { measurementId: null } }));
});

const allDocs = ["/docs", "/guide", "/guide/setup", "/guide/how-it-works", "/guide/ecosystems", "/guide/fetch", "/reference", "/reference/tools", "/benchmarks", "/compare", "/capabilities", "/vision", "/pro", "/context7-alternative", "/guides/up-to-date-docs-for-cursor-and-claude-code"];

test("public HTML protects package versions without changing selectable commands", async ({ page, request }) => {
  const sitemap = await request.get("/sitemap.xml");
  expect(sitemap.ok()).toBeTruthy();
  expect(sitemap.headers()["cache-control"]).toBe("public, max-age=3600");
  const urls = [...(await sitemap.text()).matchAll(/<loc>(.*?)<\/loc>/g)].map(([, url]) => new URL(url));
  for (const url of urls) expect(url.origin).toBe("https://lockdocs.sylphx.com");
  const routes = urls.map((url) => url.pathname);
  expect(routes).toEqual(["/", "/pricing", "/playground", "/legal/privacy", "/legal/terms", ...allDocs, "/zh-TW", "/zh-TW/docs", "/zh-TW/playground", "/zh-TW/pricing"]);
  await Promise.all(routes.map(async (route) => {
    const response = await request.get(route + "?ref=canonical-test");
    expect(response.status(), route).toBe(200);
    const html = await response.text();
    expect([...html.matchAll(/<link[^>]* rel="canonical" href="([^"]+)"[^>]*>/g)].map(([, url]) => url), route)
      .toEqual([`https://lockdocs.sylphx.com${route}`]);
    expect(html, route).not.toMatch(/[\w.-]@[vV]?\d|__cf_email__/);
  }));
  await page.goto("/guide/fetch");
  await expect(page.locator("pre code").filter({ hasText: "github.com/vercel/next.js@v15.1.0" })).toBeVisible();
  for (const route of ["/guide/setup", "/compare", "/context7-alternative", "/guides/up-to-date-docs-for-cursor-and-claude-code"]) {
    await page.goto(route);
    await expect(page.locator("main")).not.toContainText(/@sylphx\/lockdocs@0\.7\.[05]\b/);
  }
  await expect(page.getByLabel("Terminal cross-check", { exact: true })).toContainText("npx -y @sylphx/lockdocs api z.strictObject");
  await page.goto("/guide/setup");
  for (const command of ["npx -y @sylphx/lockdocs setup", "claude mcp add --scope user lockdocs -- npx -y @sylphx/lockdocs mcp"]) {
    const code = page.locator("pre code").filter({ hasText: command });
    await expect(code).toBeVisible();
    expect(await code.evaluate((element) => {
      const range = document.createRange();
      range.selectNodeContents(element);
      return range.toString().trim();
    })).toBe(command);
  }
});

test("legacy guide links permanently redirect only with safe campaign parameters", async ({ request }) => {
  for (const path of ["/quickstart", "/download"]) {
    const bare = await request.head(path, { maxRedirects: 0 });
    expect(bare.status()).toBe(301);
    expect(bare.headers().location).toBe("/guide");
    const alias = await request.get(path + "/?utm_source=release+notes&utm_medium=email&ref=oct&ref=dec&gclid=test-id&next=//example.org&url=https://example.org&utm_term=%0D%0Ax&token=secret", { maxRedirects: 0 });
    expect(alias.status()).toBe(301);
    expect(alias.headers().location).toBe("/guide?gclid=test-id&utm_source=release+notes&utm_medium=email");
    expect((await request.get(path)).status()).toBe(200);
    expect((await request.post(path, { maxRedirects: 0 })).status()).not.toBe(301);
  }
});

test("Pro reference replaces the placeholder and links directly to pricing", async ({ page }) => {
  await page.goto("/pro");
  await expect(page.locator("main")).not.toContainText("<this page>");
  await expect(page.getByText("12 call sites", { exact: false })).toBeVisible();
  const cta = page.getByRole("link", { name: "See Lockdocs Pro pricing" });
  await expect(cta).toHaveAttribute("href", "/pricing?entry_surface=article_pro");
  await expect(cta).toHaveCSS("background-color", "rgb(124, 156, 255)");
  await cta.click();
  await expect(page.getByRole("heading", { name: "Lockdocs Pro", exact: true })).toBeVisible();
});

test("home and pricing show matching offers and FAQs; sign-in is not indexed", async ({ page }) => {
  for (const [path, currency, annual] of [["/", "USD", 0], ["/pricing", "USD", 120], ["/pricing?currency=GBP", "GBP", 99]] as const) {
    const response = await page.goto(path);
    const html = await response!.text();
    const scripts = [...html.matchAll(/<script\b[^>]*\btype="application\/ld\+json"[^>]*>(.*?)<\/script>/gs)].map(([, data]) => JSON.parse(data));
    expect(scripts.map(entry => entry["@type"])).toEqual(["SoftwareApplication", "FAQPage"]);
    expect(scripts[0].offers[0]).toMatchObject({ "@type": "Offer", price: 0, priceCurrency: currency });
    await expect(page.locator('meta[name="twitter:card"]')).toHaveAttribute("content", "summary_large_image");
    if (annual) {
      expect(scripts[0].offers[1]).toMatchObject({ "@type": "Offer", price: annual, priceCurrency: currency, priceSpecification: { billingDuration: "P1Y" } });
      const pro = page.getByRole("link", { name: "Get Lockdocs Pro — " + (currency === "USD" ? "US$" : "£") + annual + "/yr", exact: true });
      await expect(pro).toHaveCSS("background-color", "rgb(124, 156, 255)");
      await expect(pro).toHaveAttribute("href", "/sign-in?next=" + encodeURIComponent(path));
    }
    for (const question of scripts[1].mainEntity) {
      await page.getByText(question.name, { exact: true }).and(page.locator("summary")).click();
      await expect(page.getByText(question.acceptedAnswer.text, { exact: true })).toBeVisible();
    }
  }
  await page.goto("/sign-in?next=%2Fpricing");
  await expect(page.locator('meta[name="robots"]')).toHaveAttribute("content", "noindex");
});

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
  const preview = page.getByRole("region", { name: "Pro Upgrade Preview" });
  await expect(preview).toContainText("Free resolves the exact versions in your lockfile.");
  await expect(preview).toContainText("which call sites an upgrade affects, with citations");
  await expect(preview.getByRole("link", { name: "See Pro plans" })).toHaveAttribute("href", "/pricing?entry_surface=playground_upgrade_cta");
  await page.getByRole("button", { name: "MCP agent setup" }).click();
  await expect(page.getByLabel(".cursor/mcp.json", { exact: true })).toContainText("@sylphx/lockdocs");
  await page.getByRole("button", { name: "Pydantic 1.10 → 2.9" }).click();
  await expect(page.getByText("pydantic@2.9.2 pydantic/main.py:352")).toBeVisible();
  await expect(page.getByRole("link", { name: "Pydantic v2 migration", exact: false })).toHaveAttribute("href", /https:\/\/docs\.pydantic\.dev/);
  await page.getByRole("link", { name: "See Pro plans" }).click();
  await expect(page).toHaveURL(/\/pricing\?entry_surface=playground_upgrade_cta$/);
  await expect(page.locator(".plan-pro .plan-price")).toHaveText("US$120/yr");
  await page.getByRole("link", { name: "GBP" }).click();
  await expect(page.locator(".plan-pro .plan-price")).toHaveText("£99/yr");
  await page.getByRole("link", { name: "EUR" }).click();
  await expect(page.locator(".plan-pro .plan-price")).toHaveText("€109/yr");
  await page.getByRole("link", { name: "HKD" }).click();
  await expect(page.locator(".plan-pro .plan-price")).toHaveText("HK$938/yr");
  await expect(page.locator(".terms")).toContainText("generally have 14 days");
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
  expect(ordinary.headers()["x-robots-tag"]).toBe("noindex, nofollow");
  const preview = await request.get("/api/health", { headers: { Host: "preview.lockdocs.sylphx.app" } });
  expect(preview.headers()["x-robots-tag"]).toBe("noindex, nofollow");
  expect(preview.status()).toBe(200);
  expect(preview.headers()["cache-control"]).toBe("no-store");
  expect(await preview.json()).toEqual({ status: "ok", money: "unconfigured" });
  const lookalike = await request.get("/api/health", { headers: { Host: "lockdocs.sylphx.app.example.org" } });
  expect(lookalike.headers()["x-robots-tag"]).toBe("noindex, nofollow");
  const liveHeaders = { Host: "web.fixture.svc.cluster.local", "X-Forwarded-Host": "lockdocs.sylphx.com" };
  expect((await request.get("/api/health", { headers: liveHeaders })).headers()["x-robots-tag"]).toBeUndefined();
  const home = await request.get("/", { headers: liveHeaders, maxRedirects: 0 });
  expect(home.status()).toBe(200);
  expect(home.headers()["x-robots-tag"]).toBeUndefined();
  expect(await home.text()).toContain('rel="canonical" href="https://lockdocs.sylphx.com/"');
  const html = await home.text();
  const ogImage = "https://lockdocs.sylphx.com/og-image.png";
  expect(html).toContain('property="og:image" content="' + ogImage + '"');
  expect(html).toContain('name="twitter:image" content="' + ogImage + '"');
  const image = await request.get("/og-image.png");
  expect(image.status()).toBe(200);
  expect(image.headers()["content-type"]).toContain("image/png");
  const png = await image.body();
  expect([png.readUInt32BE(16), png.readUInt32BE(20)]).toEqual([1200, 630]);
  const conflict = { ...liveHeaders, "X-Forwarded-Host": "preview.lockdocs.sylphx.app:443" };
  expect((await request.get("/api/health", { headers: conflict })).headers()["x-robots-tag"]).toBe("noindex, nofollow");
  for (const [headers, rule] of [[liveHeaders, "Allow: /\nSitemap: https://lockdocs.sylphx.com/sitemap.xml"], [conflict, "Disallow: /"]] as const) {
    const robots = await request.get("/robots.txt", { headers });
    expect(robots.status()).toBe(200);
    expect(await robots.text()).toBe(`User-agent: *
${rule}
`);
  }
  await page.goto("/dashboard");
  await expect(page.locator('link[rel="canonical"]')).toHaveCount(0);
  await expect(page.getByRole("link", { name: "Sign in", exact: true })).toHaveAttribute("href", "/sign-in?next=%2Fdashboard");
  const signIn = await request.get("/sign-in");
  expect(signIn.headers()["cache-control"]).toBe("private, no-store");
  // Missing bindings render the same fail-closed retry state on the server and in the browser: no method is offered.
  const signInHtml = await signIn.text();
  expect(signInHtml).toContain("Couldn’t load sign-in options. Try again.");
  expect(signInHtml).not.toMatch(/href="\/api\/auth\/(?:google|github)|id="signin-email"/);
  await page.goto("/sign-in");
  await expect(page.getByRole("button", { name: "Try again" })).toBeVisible();
  await expect(page.getByRole("link", { name: /Continue with/ })).toHaveCount(0);
  expect((await request.post("/api/billing/checkout")).status()).toBe(503);
});
