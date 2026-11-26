import { expect, test, type Locator } from "@playwright/test";

const cursorConfig = {
  mcpServers: { lockdocs: { command: "npx", args: ["-y", "@sylphx/lockdocs", "mcp"] } },
};
const claudeCommands = "claude mcp add --scope local --transport stdio lockdocs -- npx -y @sylphx/lockdocs mcp\nclaude mcp list";
const campaign = "utm_source=newsletter&utm_campaign=Editor+setup";
const guidePath = "/guides/up-to-date-docs-for-cursor-and-claude-code";

type Batch = { consent: string; events: { event_name: string; properties: Record<string, unknown> }[] };

async function assertPanel(drawer: Locator) {
  const geometry = await drawer.evaluate(element => {
    const header = element.querySelector("figcaption")!;
    const copy = header.querySelector("button")!;
    const label = header.querySelector("span")!;
    const code = element.querySelector("pre")!;
    const box = copy.getBoundingClientRect();
    return {
      overflow: [document.documentElement, element, ...element.querySelectorAll(".segmented, figure, figcaption, pre")]
        .filter(node => node.scrollWidth > node.clientWidth).map(node => node.className || node.tagName),
      buttonWidth: box.width,
      buttonHeight: box.height,
      buttonPosition: getComputedStyle(copy).position,
      codeHeight: code.getBoundingClientRect().height,
      separateRows: box.bottom <= header.getBoundingClientRect().bottom && header.getBoundingClientRect().bottom <= code.getBoundingClientRect().top,
      separateLabel: label.getBoundingClientRect().right <= box.left,
      targets: [...element.querySelectorAll(".segmented button")].map(button => button.getBoundingClientRect().height),
    };
  });
  expect(geometry.overflow).toEqual([]);
  expect(geometry.buttonPosition).not.toMatch(/absolute|fixed/);
  expect(geometry.buttonWidth).toBeGreaterThanOrEqual(44);
  expect(geometry.buttonHeight).toBeGreaterThanOrEqual(44);
  expect(geometry.separateRows && geometry.separateLabel).toBe(true);
  expect(geometry.targets).toHaveLength(2);
  for (const height of geometry.targets) expect(height).toBeGreaterThanOrEqual(44);
  return geometry.codeHeight;
}

for (const zh of [false, true]) {
  const path = zh ? "/zh-TW/playground" : "/playground";
  test(path + " copies right-sized MCP setups and sends Signal only after consent", async ({ page, context, isMobile }, info) => {
    const requests: string[] = [];
    const batches: Batch[] = [];
    const errors: string[] = [];
    page.on("pageerror", error => errors.push(error.message));
    page.on("console", message => { if (message.type() === "error") errors.push(message.text()); });
    await page.exposeFunction("recordMcpCspViolation", (directive: string) => { errors.push(directive); });
    await page.addInitScript(() => {
      document.addEventListener("securitypolicyviolation", event => {
        void Reflect.get(window, "recordMcpCspViolation")(event.violatedDirective);
      });
    });
    await page.setExtraHTTPHeaders({ "CF-IPCountry": "DE" });
    await page.route("**/api/analytics/config", async route => {
      const response = await route.fetch();
      await route.fulfill({ response, json: { ...await response.json(), measurementId: null,
        signalKey: "sylphx_pk_signal_fixture", signalApiOrigin: "https://api.sylphx.com" } });
    });
    await page.route("**/v1/analytics/events:batch", async route => {
      requests.push(route.request().method());
      if (route.request().method() === "POST") batches.push(route.request().postDataJSON());
      await route.fulfill({ status: 202, json: { accepted_events: 1 }, headers: {
        "Access-Control-Allow-Origin": "*", "Access-Control-Allow-Headers": "content-type,x-sylphx-key",
      } });
    });
    await context.grantPermissions(["clipboard-read", "clipboard-write"]);
    const config = page.waitForResponse(response => response.url().endsWith("/api/analytics/config"));
    const response = await page.goto(path + "?" + campaign + "&private=not-carried");
    expect(response?.status()).toBe(200);
    const policy = response?.headers()["content-security-policy"] ?? "";
    expect(policy).toMatch(/(?:^|; )style-src [^;]*'nonce-[^']+'/);
    expect(policy).not.toMatch(/'unsafe-inline'|'unsafe-eval'/);
    expect((await config).ok()).toBe(true);
    await page.evaluate(() => document.fonts.ready);

    const settings = page.getByRole("region", { name: zh ? "Cookie 設定" : "Cookie settings" });
    await expect(settings).toBeVisible();
    const drawer = page.locator(".playground-mcp");
    const toggle = drawer.getByRole("button", { name: zh ? "MCP 代理工具設定" : "MCP agent setup" });
    const picker = drawer.getByRole("group", { name: zh ? "MCP 編輯器設定" : "MCP editor setup" });
    await expect(toggle).toHaveAttribute("aria-expanded", "false");
    await expect(picker).toBeHidden();
    await toggle.click();
    await expect(toggle).toHaveAttribute("aria-expanded", "true");
    const link = drawer.getByRole("link", { name: zh ? "Cursor 與 Claude Code 設定指南（英文）" : "Cursor & Claude Code setup guide" });
    await expect(link).toHaveAttribute("href", guidePath + "?" + campaign);
    const choose = async (editor: string) => {
      const button = picker.getByRole("button", { name: editor, exact: true });
      if (isMobile) await button.tap();
      else await button.press("Enter");
      await expect(picker.getByRole("button", { pressed: true })).toHaveText(editor);
    };
    const copy = async (label: string, expected: string) => {
      const panel = drawer.getByLabel(label, { exact: true });
      expect(await panel.textContent()).toBe(expected);
      await drawer.getByRole("button", { name: (zh ? "複製 " : "Copy ") + label, exact: true }).click();
      await expect(drawer.getByRole("status")).toHaveText(zh ? "已複製" : "Copied");
      expect(await page.evaluate(() => navigator.clipboard.readText())).toBe(expected);
    };
    const cursor = JSON.stringify(cursorConfig, null, 2);
    await expect(picker.getByRole("button", { pressed: true })).toHaveText("Cursor");
    const cursorHeight = await assertPanel(drawer);
    await copy(".cursor/mcp.json", cursor);
    await choose("Claude Code");
    expect(await assertPanel(drawer)).toBeLessThan(cursorHeight);
    await copy("Claude Code CLI", claudeCommands);
    if (process.env.LOCKDOCS_LOCAL_CAPTURES) await drawer.screenshot({ path: info.outputPath("claude-mcp.png") });
    await choose("Cursor");
    expect(await assertPanel(drawer)).toBe(cursorHeight);
    if (process.env.LOCKDOCS_LOCAL_CAPTURES) await drawer.screenshot({ path: info.outputPath("cursor-mcp.png") });
    await expect(drawer.locator("[style]")).toHaveCount(0);
    expect(requests).toEqual([]);

    await settings.getByRole("button", { name: zh ? "僅限分析" : "Analytics only", exact: true }).click();
    const events = (name: string) => batches.flatMap(batch => batch.events).filter(event => event.event_name === name).map(event => event.properties);
    await expect.poll(() => events("page_view").length).toBe(1);
    expect(events("command_copied")).toEqual([]);
    await copy(".cursor/mcp.json", cursor);
    await choose("Claude Code");
    await copy("Claude Code CLI", claudeCommands);
    const attribution = { utm_source: "newsletter", utm_campaign: "Editor setup", surface: "playground_mcp" };
    await expect.poll(() => events("command_copied")).toEqual([
      { ...attribution, command_kind: "mcp_config" }, { ...attribution, command_kind: "cli" },
    ]);
    expect(batches.every(batch => batch.consent === "granted")).toBe(true);
    await link.click();
    await expect(page).toHaveURL(url => url.pathname + url.search === guidePath + "?" + campaign);
    expect(errors).toEqual([]);
  });
}
