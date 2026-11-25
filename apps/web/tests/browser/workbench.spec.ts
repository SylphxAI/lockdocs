import { expect, test } from "@playwright/test";

const manifestPath = "/manifest.webmanifest";
const icons = [
  { src: "/icon-192.png", sizes: "192x192", type: "image/png", purpose: "any" },
  { src: "/icon-maskable-192.png", sizes: "192x192", type: "image/png", purpose: "maskable" },
  { src: "/icon-512.png", sizes: "512x512", type: "image/png", purpose: "any" },
  { src: "/icon-maskable-512.png", sizes: "512x512", type: "image/png", purpose: "maskable" },
];

test.beforeEach(async ({ page }) => {
  await page.route("**/api/analytics/config", route => route.fulfill({ json: { measurementId: null } }));
});

test("the server paints a usable sample before hydration", async ({ browser, baseURL }) => {
  const page = await browser.newPage({ baseURL, javaScriptEnabled: false });
  try {
    await page.goto("/playground");
    await expect(page.locator("#lockfile")).toHaveValue(/"lockfileVersion": 3[\s\S]*"node_modules\/next"/);
    await expect(page.locator("#lockfile")).toBeVisible();
    await expect(page.locator(".panel-count")).toHaveText("3 pinned");
  } finally { await page.close(); }
});

test("the hydrated editor changes samples but never overwrites visitor edits on format change", async ({ page }) => {
  await page.goto("/playground");
  const editor = page.locator("#lockfile");
  const format = page.getByLabel("Lockfile format");
  const load = page.getByRole("button", { name: "Load sample" });
  await expect(editor).toHaveValue(/"node_modules\/next"/);
  await expect(load).toHaveCSS("background-color", "rgb(18, 25, 35)");
  await expect(load).toHaveCSS("border-top-color", "rgb(56, 73, 96)");
  await expect(load).toHaveCSS("border-top-width", "1px");
  if ((await page.evaluate(() => matchMedia("(hover: hover)").matches))) {
    await load.hover();
    await expect(load).toHaveCSS("border-top-color", "rgb(124, 156, 255)");
  }
  await format.focus();
  await page.keyboard.press("Tab");
  await expect(load).toBeFocused();
  await expect(load).toHaveCSS("outline-width", "2px");
  await expect(load).toHaveCSS("outline-color", "rgb(124, 156, 255)");
  await format.selectOption("requirements.txt");
  await expect(editor).toHaveValue(/pydantic==1\.10\.18/);
  await expect(page.locator(".panel-count")).toHaveText("2 pinned");
  await editor.fill("Django==5.1.3");
  await format.selectOption("package-lock.json");
  await expect(editor).toHaveValue("Django==5.1.3");
  await format.selectOption("requirements.txt");
  await page.getByRole("button", { name: "Resolve versions" }).click();
  await expect(page.getByRole("heading", { name: "django@5.1.3" })).toBeVisible();
  await load.click();
  await expect(editor).toHaveValue(/pydantic==1\.10\.18/);
  await expect(page.locator(".panel-count")).toHaveText("2 pinned");
  await page.goto("/pricing");
  await expect(page.getByRole("link", { name: "Install Lockdocs" })).toHaveCSS("border-top-color", "rgb(56, 73, 96)");
});

test("Traditional Chinese covers the initial, expanded, empty, and invalid workbench states", async ({ page }) => {
  await page.goto("/zh-TW/playground");
  const main = page.getByRole("main");
  await expect(main.getByRole("heading", { name: "lockfile 試用工具" })).toBeVisible();
  await expect(main.getByText(/你貼上的內容只會留在這個瀏覽器裡/)).toBeVisible();
  await expect(main.getByText("已鎖定 3 個套件")).toBeVisible();
  await expect(main.getByText("版本鎖定於 package-lock.json。")).toBeVisible();
  await expect(main.getByRole("heading", { name: "Pro 升級預覽" })).toBeVisible();
  await expect(main.getByRole("heading", { name: "升級後，哪些程式碼需要改？" })).toBeVisible();
  await expect(main.getByRole("heading", { name: "cookies() 改為非同步函式" })).toBeVisible();
  await main.getByRole("button", { name: "MCP 代理工具設定" }).click();
  await expect(main.getByText(/請從專案根目錄啟動代理工具/)).toBeVisible();
  await expect(main.getByRole("button", { name: "複製", exact: true })).toBeVisible();
  await main.getByRole("button", { name: "Pydantic 1.10 → 2.9" }).click();
  await expect(main.getByRole("heading", { name: "Pydantic v2 改用 model_dump()" })).toBeVisible();
  await expect(main.getByText(/這個方法已棄用/)).toBeVisible();
  await expect(main.getByRole("link", { name: "Pydantic v2 升級指南：BaseModel 的變更" })).toBeVisible();
  const editor = main.locator("#lockfile");
  const resolve = main.getByRole("button", { name: "查出套件版本" });
  await editor.fill("{}");
  await resolve.click();
  await expect(main.getByText(/找不到指定確切版本的套件/)).toBeVisible();
  await editor.fill("{");
  await resolve.click();
  await expect(main.getByText(/無法讀取這份 lockfile/)).toBeVisible();
  await expect(main).not.toContainText(/Resolve the exact|\d pinned|Pinned in|Pro Upgrade Preview|See exactly what breaks|REPORT PREVIEW|Example call site|Run your agent|migration|Couldn’t parse|try a pinned sample/i);
});

test("Chromium can load the manifest and official icons under enforcing CSP", async ({ page, context, request, baseURL }) => {
  await page.addInitScript(() => {
    document.addEventListener("securitypolicyviolation", event => {
      (window as Window & { violations?: string[] }).violations?.push(event.violatedDirective);
    });
    Object.assign(window, { violations: [] });
  });
  const doc = await page.goto("/playground");
  const policy = doc?.headers()["content-security-policy"]?.split("; ");
  expect(policy).toContain("default-src 'none'");
  expect(policy).toContain("manifest-src 'self'");
  expect(policy?.find(value => value.startsWith("form-action "))).toContain("http://127.0.0.1:*");
  await expect(page.locator('link[rel="manifest"]')).toHaveAttribute("href", manifestPath);
  await expect(page.locator('meta[name="theme-color"]')).toHaveAttribute("content", "#06080C");
  await expect(page.locator('link[rel="apple-touch-icon"]')).toHaveAttribute("href", "/apple-touch-icon-180.png");
  const cdp = await context.newCDPSession(page);
  const loaded = await cdp.send("Page.getAppManifest");
  expect(loaded.url).toBe(baseURL + manifestPath);
  expect(loaded.errors).toEqual([]);
  expect(loaded.data).toBeTruthy();
  const response = await request.get(manifestPath);
  expect(response.status()).toBe(200);
  expect(response.headers()["content-type"]).toMatch(/application\/(manifest\+json|json)/);
  const manifest = await response.json();
  expect(JSON.parse(loaded.data!)).toEqual(manifest);
  expect(manifest).toMatchObject({ name: "Lockdocs", short_name: "Lockdocs", start_url: "/playground", scope: "/",
    display: "standalone", background_color: "#06080C", theme_color: "#06080C", icons });
  for (const { src, sizes } of [...icons, { src: "/apple-touch-icon-180.png", sizes: "180x180" }]) {
    const image = await request.get(src);
    expect(image.status(), src).toBe(200);
    expect(image.headers()["content-type"]).toBe("image/png");
    const png = await image.body();
    expect([png.readUInt32BE(16), png.readUInt32BE(20)]).toEqual(sizes.split("x").map(Number));
  }
  expect(await page.evaluate(() => (window as Window & { violations?: string[] }).violations)).toEqual([]);
  expect(await page.evaluate(async () => (await navigator.serviceWorker.getRegistrations()).length)).toBe(0);
});


test("the three new formats resolve locally, and format changes preserve pasted text", async ({ page }) => {
  await page.goto("/playground");
  const format = page.getByLabel("Lockfile format");
  const editor = page.locator("#lockfile");
  const resolve = page.getByRole("button", { name: "Resolve versions" });
  await expect(format.locator("option")).toHaveCount(8);
  const outgoing: string[] = [];
  page.on("request", request => outgoing.push((request.postData() ?? "") + request.url()));
  for (const row of [
    { format: "pnpm-lock.yaml", sample: "react@18.3.1", input: "packages: {'preview-only@1.2.3(react@19.0.0)': {}}", output: "preview-only@1.2.3" },
    { format: "yarn.lock", sample: "@babel/core@7.24.1", input: '"preview-only@npm:^2.0.0":\n  version: 2.1.0', output: "preview-only@2.1.0" },
    { format: "go.mod", sample: "github.com/spf13/cobra@v1.8.0", input: "require example.com/preview-only v1.4.0 // indirect", output: "example.com/preview-only@v1.4.0" },
  ]) {
    await format.selectOption(row.format);
    if (row.format !== "pnpm-lock.yaml") await expect(editor).toHaveValue(/preview-only/);
    await page.getByRole("button", { name: "Load sample" }).click();
    await expect(page.getByRole("heading", { name: row.sample, exact: true })).toBeVisible();
    await editor.fill(row.input);
    await resolve.click();
    await expect(page.getByRole("heading", { name: row.output, exact: true })).toBeVisible();
  }
  await format.selectOption("pnpm-lock.yaml");
  await editor.fill("packages: [");
  await resolve.click();
  await expect(page.getByRole("status").filter({ hasText: "Couldn’t parse" })).toBeVisible();
  await expect(page.locator(".panel-count")).toHaveText("0 pinned");
  expect(outgoing.filter(request => request.includes("preview-only"))).toEqual([]);
});

test("Traditional Chinese resolves Go pins using the same Playground", async ({ page }) => {
  await page.goto("/zh-TW/playground");
  await page.getByLabel("lockfile 格式").selectOption("go.mod");
  await expect(page.getByText("已鎖定 2 個套件")).toBeVisible();
  await expect(page.getByRole("heading", { name: "github.com/spf13/cobra@v1.8.0" })).toBeVisible();
});

test("route-specific JSON-LD is in the initial HTML and drops attribution parameters", async ({ request }) => {
  for (const [path, type, name, language] of [
    ["/playground", "WebApplication", "Lockfile Playground", "en"],
    ["/zh-TW/playground", "WebApplication", "lockfile 試用工具", "zh-TW"],
    ["/compare", "TechArticle", "Lockdocs vs Context7", "en"],
    ["/benchmarks", "TechArticle", "Benchmarks", "en"],
    ["/guide", "TechArticle", "Quickstart", "en"],
    ["/guide/setup", "TechArticle", "Agent setup", "en"],
    ["/guide/ecosystems", "TechArticle", "Supported ecosystems", "en"],
    ["/guide/fetch", "TechArticle", "Fetching docs", "en"],
    ["/guide/how-it-works", "TechArticle", "How it works", "en"],
    ["/reference", "TechArticle", "CLI reference", "en"],
    ["/reference/tools", "TechArticle", "MCP tools", "en"],
    ["/capabilities", "TechArticle", "Capabilities", "en"],
    ["/vision", "TechArticle", "Vision", "en"],
    ["/docs", "CollectionPage", "Documentation", "en"],
    ["/zh-TW/docs", "CollectionPage", "文件總覽", "zh-TW"],
  ]) {
    const response = await request.get(path + "?utm_source=github&utm_medium=readme&utm_content=nav");
    expect(response.ok()).toBe(true);
    const html = await response.text();
    const url = "https://lockdocs.sylphx.com" + path;
    expect(html).toContain('rel="canonical" href="' + url + '"');
    const scripts = [...html.matchAll(/<script[^>]*type="application\/ld\+json"[^>]*>(.*?)<\/script>/gs)];
    expect(scripts, path).toHaveLength(1);
    const data = JSON.parse(scripts[0][1]);
    expect(JSON.stringify(data), path).not.toMatch(/utm_/);
    expect(data).toMatchObject({ "@context": "https://schema.org", "@type": type, url, inLanguage: language });
    if (type === "WebApplication") {
      expect(data).toMatchObject({ name, isAccessibleForFree: true, offers: { "@type": "Offer", price: 0, priceCurrency: "USD", url } });
    } else if (type === "TechArticle") {
      expect(data.headline).toBe(name);
    } else {
      expect(data.name).toBe(name);
    }
  }
});
