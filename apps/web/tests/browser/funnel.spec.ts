import { createServer } from "node:http";
import { once } from "node:events";
import type { AddressInfo } from "node:net";
import { expect, test, type Page } from "@playwright/test";
import { test as authTest } from "./auth-fixture";
import { commands, install, settings, observeLocalizedAnalytics, noAnalytics } from "./analytics-fixture";

authTest.use({ billing: true });

const funnelNames = new Set(["lockfile_resolved", "command_copied", "paywall_view", "pro_cta_clicked", "upgrade_preview_switched", "cli_authorized"]);
function funnelWire(wire: URLSearchParams[]) {
  return wire.filter(event => funnelNames.has(event.get("en") ?? "")).map(event => [event.get("en"), Object.fromEntries(
    [...event].filter(([key]) => /^epn?./.test(key)).map(([key, value]) => [key.split(".")[1], key.startsWith("epn.") ? Number(value) : value]),
  )] as const);
}

const privatePackage = "internal-payroll-fixture";
async function copyFrom(page: Page, name: string) {
  const button = page.getByRole("button", { name, exact: true });
  await button.click();
  await expect(button.locator("+ [role=status]")).toHaveText("Copied");
}

for (const mode of ["unanswered", "denied", "gpc", "granted"] as const) {
  test("behavioral funnel: " + mode, async ({ page, context }) => {
    const { wire, hits, frames } = await observeLocalizedAnalytics(page);
    await context.grantPermissions(["clipboard-read", "clipboard-write"]);
    if (mode === "gpc") await page.addInitScript(() => {
      Object.defineProperty(navigator, "globalPrivacyControl", { value: true });
      document.cookie = "__Host-lockdocs-signal-choice=granted; Path=/; Secure; SameSite=Lax";
    });
    await page.goto("/playground?private=fixture-only#fixture-hash");
    const consent = settings(page);
    if (mode === "granted" || mode === "denied") {
      await consent.getByRole("button", { name: mode === "granted" ? "Analytics only" : "Reject optional", exact: true }).click();
    } else {
      await expect(page.getByRole("button", { name: "Cookie settings", exact: true })).toBeEnabled();
    }
    const seen = (event: string, params: object) => expect.poll(() => funnelWire(wire)).toContainEqual([event, params]);
    const ready = () => expect.poll(() => commands(page)).toContainEqual(expect.arrayContaining(["event", "page_view"]));
    const granted = mode === "granted";
    if (granted) await ready();
    const format = page.getByLabel("Lockfile format");
    await format.selectOption("requirements.txt");
    if (granted) await seen("lockfile_resolved", { lockfile_format: "requirements.txt", source_kind: "sample", package_count: 2 });
    await page.locator("#lockfile").fill(privatePackage + "==1.2.3");
    await page.getByRole("button", { name: "Resolve versions" }).click();
    await expect(page.getByRole("heading", { name: privatePackage + "@1.2.3" })).toBeVisible();
    await copyFrom(page, "Copy CLI command");
    await expect.poll(() => page.evaluate(() => navigator.clipboard.readText())).toBe("lockdocs " + privatePackage + " --offline");
    await copyFrom(page, "Copy upgrade command");
    await expect.poll(() => page.evaluate(() => navigator.clipboard.readText())).toBe("lockdocs upgrade next 15.0.0");
    await page.getByRole("button", { name: "MCP agent setup" }).click();
    await copyFrom(page, "Copy");
    expect(await page.evaluate(() => navigator.clipboard.readText())).toContain('"mcpServers"');
    const example = page.getByRole("button", { name: "Pydantic 1.10 → 2.9" });
    await example.click();
    await example.click();
    await copyFrom(page, "Copy upgrade command");
    await expect.poll(() => page.evaluate(() => navigator.clipboard.readText())).toBe("lockdocs upgrade pydantic 2.9.2");
    const report = page.getByRole("region", { name: "See exactly what breaks." });
    await report.getByRole("heading", { level: 2 }).scrollIntoViewIfNeeded();
    if (granted) await seen("paywall_view", { entry_surface: "playground_report_preview" });
    await report.getByRole("link", { name: "Get full reports with Pro" }).click();
    await expect(page).toHaveURL(url => url.pathname === "/pricing" && url.searchParams.get("entry_surface") === "playground_report_preview");
    await page.getByRole("banner").getByRole("link", { name: "Playground" }).click();
    const previewCTA = page.getByRole("link", { name: "See Pro plans" });
    await previewCTA.scrollIntoViewIfNeeded();
    if (granted) await seen("paywall_view", { entry_surface: "playground_upgrade_cta" });
    await previewCTA.click();
    await expect(page).toHaveURL(url => url.pathname === "/pricing" && url.searchParams.get("entry_surface") === "playground_upgrade_cta");
    await page.goto("/guide/setup");
    if (granted) await ready();
    await page.getByRole("button", { name: "Copy code", exact: true }).first().click();
    await expect(page.getByRole("status").filter({ hasText: /^Copied$/ })).toBeAttached();
    if (granted) await seen("command_copied", { surface: "docs_code", command_kind: "code" });
    await page.goto("/");
    if (granted) await ready();
    for (const command_kind of ["npx", "uvx", "pip"]) {
      await page.locator(".install-options label").filter({ hasText: command_kind }).click();
      await copyFrom(page, "Copy install command");
      if (granted) await seen("command_copied", { surface: "home_install", command_kind });
    }
    const homeCTA = page.getByRole("link", { name: "Explore Pro" });
    await homeCTA.scrollIntoViewIfNeeded();
    if (granted) await seen("paywall_view", { entry_surface: "home_pro_band" });
    await homeCTA.click();
    await expect(page).toHaveURL(url => url.pathname === "/pricing" && url.searchParams.get("entry_surface") === "home_pro_band");
    if (granted) {
      await seen("pro_cta_clicked", { entry_surface: "home_pro_band" });
      expect(funnelWire(wire)).toEqual(expect.arrayContaining([
        ["lockfile_resolved", { lockfile_format: "requirements.txt", source_kind: "pasted", package_count: 1 }],
        ["command_copied", { surface: "playground_mcp", command_kind: "mcp_config" }],
        ["pro_cta_clicked", { entry_surface: "playground_report_preview" }],
        ["pro_cta_clicked", { entry_surface: "playground_upgrade_cta" }],
      ]));
      expect(funnelWire(wire).filter(([name, params]) => name === "command_copied" && /^playground_(cli|upgrade)$/.test(String(params.surface)))).toEqual([
        ["command_copied", { surface: "playground_cli", command_kind: "cli" }],
        ["command_copied", { surface: "playground_upgrade", command_kind: "cli" }],
        ["command_copied", { surface: "playground_upgrade", command_kind: "cli" }],
      ]);
      expect(funnelWire(wire).filter(([name]) => name === "upgrade_preview_switched")).toEqual([["upgrade_preview_switched", { example_id: "pydantic" }]]);
      expect(wire.map(event => event.toString()).join(" ")).not.toMatch(/internal-payroll|fixture-only|fixture-hash|mcpServers/);
    } else {
      expect({ wire, hits, frames }).toEqual(noAnalytics);
      expect((await context.cookies()).filter(cookie => /^(?:_ga|_gcl|_fb)/.test(cookie.name))).toEqual([]);
    }
  });
}

for (const mode of ["granted", "denied", "gpc"] as const) {
  authTest("CLI consent keeps the native loopback redirect with analytics " + mode, async ({ page, auth, context }) => {
    const wire: URLSearchParams[] = [];
    const hits = await install(page, null, wire);
    const loopback = createServer((_request, response) => response.end("CLI callback fixture"));
    loopback.listen(0, "127.0.0.1");
    await once(loopback, "listening");
    try {
      const { port } = loopback.address() as AddressInfo;
      if (mode === "gpc") await page.setExtraHTTPHeaders({ "Sec-GPC": "1" });
      await context.addCookies([{ name: "__Host-lockdocs-signal-choice", value: mode === "denied" ? "denied" : "analytics_only",
        url: auth.origin, secure: true }]);
      auth.pro.entitled = true;
      await page.goto(auth.origin + "/sign-in");
      await page.getByRole("link", { name: "Continue with Google" }).click();
      await expect(page).toHaveURL(auth.origin + "/dashboard");
      const state = "s".repeat(42) + "A";
      const challenge = "c".repeat(42) + "E";
      const query = new URLSearchParams({ state, challenge, port: String(port) });
      await page.goto(auth.origin + "/cli/authorize?" + query);
      if (mode === "granted") await expect.poll(() => wire.some(event => event.get("dl") === auth.origin + "/cli/authorize")).toBe(true);
      else await expect(page.getByRole("button", { name: "Cookie settings", exact: true })).toBeEnabled();
      await page.getByRole("button", { name: "Authorize CLI" }).click();
      await expect(page).toHaveURL(url => url.origin === "http://127.0.0.1:" + port && url.pathname === "/callback");
      expect(new URL(page.url()).searchParams.get("state")).toBe(state);
      const callbackCode = new URL(page.url()).searchParams.get("code");
      expect(callbackCode?.length).toBeGreaterThan(10);
      await expect(page.getByText("CLI callback fixture")).toBeVisible();
      if (mode === "granted") {
        await expect.poll(() => funnelWire(wire)).toEqual([["cli_authorized", { entry_surface: "cli_authorize" }]]);
        const all = wire.map(event => event.toString()).join(" ");
        for (const privateValue of [state, challenge, "reader%40example.test", callbackCode!]) expect(all).not.toContain(privateValue);
        expect(wire.find(event => event.get("en") === "cli_authorized")?.get("dl")).toBe(auth.origin + "/cli/authorize");
      } else {
        expect(hits).toEqual([]);
        expect(wire).toEqual([]);
      }
      expect(auth.rejections).toEqual([]);
    } finally { loopback.close(); loopback.closeAllConnections(); }
  });
}
