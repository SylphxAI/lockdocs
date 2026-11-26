import { expect, test } from "@playwright/test";
import { noAnalytics, observeLocalizedAnalytics, settings } from "./analytics-fixture";

const path = "/guides/up-to-date-docs-for-cursor-and-claude-code";
const cursor = {
  name: "Cursor", label: "Cursor project configuration",
  code: JSON.stringify({ mcpServers: { lockdocs: { command: "npx", args: ["-y", "@sylphx/lockdocs", "mcp"] } } }, null, 2),
};
const claude = {
  name: "Claude Code", label: "Claude Code project setup",
  code: "claude mcp add --scope local --transport stdio lockdocs -- npx -y @sylphx/lockdocs mcp\nclaude mcp list",
};

test.describe("editor guide without JavaScript", () => {
  test.use({ javaScriptEnabled: false });
  test("serves the Cursor config and contextual Zod link in HTML", async ({ page }) => {
    await page.goto(path);
    const picker = page.getByRole("group", { name: "MCP editor setup" });
    await expect(picker.getByRole("button", { pressed: true })).toHaveText(cursor.name);
    await expect(page.locator("#connect").getByText(".cursor/mcp.json", { exact: true })).toBeVisible();
    await expect(page.getByLabel(cursor.label, { exact: true })).toHaveText(cursor.code);
    await expect(page.getByLabel(claude.label, { exact: true })).toBeHidden();
    await expect(page.locator("#verify-cli").getByRole("link", { name: "Try the Zod example in the playground" }))
      .toHaveAttribute("href", "/playground?example=zod#upgrade");
  });
});

test("copies both editor setups and opens the Zod upgrade report with attribution", async ({ page, context, isMobile }, info) => {
  const analytics = await observeLocalizedAnalytics(page);
  const signals: string[] = [];
  const errors: string[] = [];
  page.on("pageerror", error => errors.push(error.message));
  page.on("console", message => { if (message.type() === "error") errors.push(message.text()); });
  await page.exposeFunction("recordGuideCspViolation", (directive: string) => { errors.push(directive); });
  await page.addInitScript(() => {
    document.addEventListener("securitypolicyviolation", event => {
      void Reflect.get(window, "recordGuideCspViolation")(event.violatedDirective);
    });
  });
  await page.route("**/api/analytics/config", async route => {
    const response = await route.fetch();
    await route.fulfill({ response, json: { ...await response.json(), signalKey: "sylphx_pk_signal_fixture",
      signalApiOrigin: "https://api.sylphx.com" } });
  });
  await page.route("**/v1/analytics/events:batch", route => {
    signals.push(route.request().method());
    return route.fulfill({ status: 204, headers: { "Access-Control-Allow-Origin": "*",
      "Access-Control-Allow-Headers": "content-type,x-sylphx-key" } });
  });
  await context.grantPermissions(["clipboard-read", "clipboard-write"]);

  const campaign = "gclid=guide-test&utm_source=newsletter&utm_campaign=Zod+%2B+MCP";
  const response = await page.goto(path + "?" + campaign + "&token=not-a-campaign-param");
  expect(response?.status()).toBe(200);
  const policy = response?.headers()["content-security-policy"] ?? "";
  expect(policy).toMatch(/(?:^|; )style-src [^;]*'nonce-[^']+'/);
  expect(policy).not.toMatch(/'unsafe-inline'|'unsafe-eval'/);
  await expect(settings(page)).toBeVisible();
  await page.evaluate(() => document.fonts.ready);
  const connect = page.locator("#connect");
  const picker = connect.getByRole("group", { name: "MCP editor setup" });
  const geometry = () => connect.evaluate(el => {
    const step3 = document.querySelector("#verify-citation")!;
    const card = el.querySelector(".editor-mcp-panel:not([inert]) .editorial-code")!;
    return {
      stableBoxes: [el, step3, card.querySelector(".copy-button")!].map(node => {
        const { x, y, width, height } = node.getBoundingClientRect();
        return [x, y + scrollY, width, height];
      }),
      cardHeight: card.getBoundingClientRect().height,
      step3HeadingY: step3.querySelector("h2")!.getBoundingClientRect().y + scrollY,
    };
  });
  const assertNoOverflow = async () => expect(await page.evaluate(() => [document.documentElement,
    ...document.querySelectorAll(".editor-mcp, .editor-mcp-panel, .editor-mcp-panel pre, .upgrade-card, .studio-grid")]
    .filter(el => el.scrollWidth > el.clientWidth).map(el => el.className))).toEqual([]);
  const copy = async (setup: typeof cursor) => {
    await expect(picker.getByRole("button", { pressed: true })).toHaveText(setup.name);
    await expect(connect.getByLabel(setup.label, { exact: true })).toBeVisible();
    await expect(connect.getByLabel(setup.label, { exact: true })).toHaveText(setup.code);
    await expect(connect.getByRole("button", { name: /^Copy / })).toHaveCount(1);
    await connect.getByRole("button", { name: "Copy " + setup.label, exact: true }).click();
    await expect(connect.getByRole("status")).toHaveText("Copied");
    expect(await page.evaluate(() => navigator.clipboard.readText())).toBe(setup.code);
  };

  const initialGeometry = await geometry();
  expect(initialGeometry.cardHeight).toBeGreaterThan(220);
  const assertStableLayout = (current: typeof initialGeometry) => {
    expect(Math.abs(current.step3HeadingY - initialGeometry.step3HeadingY)).toBeLessThanOrEqual(1);
    expect(current.stableBoxes).toEqual(initialGeometry.stableBoxes);
  };
  await assertNoOverflow();
  await expect(picker.getByRole("button")).toHaveCount(2);
  await copy(cursor);
  const claudeButton = picker.getByRole("button", { name: claude.name });
  if (isMobile) await claudeButton.tap();
  else await claudeButton.press("Enter");
  await expect(picker.getByRole("button", { pressed: true })).toHaveText(claude.name);
  const claudeGeometry = await geometry();
  expect(claudeGeometry.cardHeight).toBeLessThan(160);
  assertStableLayout(claudeGeometry);
  await assertNoOverflow();
  await copy(claude);
  if (process.env.LOCKDOCS_LOCAL_CAPTURES) await connect.screenshot({ path: info.outputPath("claude-mcp.png") });
  await picker.getByRole("button", { name: cursor.name }).press("Space");
  await expect(picker.getByRole("button", { pressed: true })).toHaveText(cursor.name);
  const restoredGeometry = await geometry();
  expect(restoredGeometry.cardHeight).toBeGreaterThan(220);
  assertStableLayout(restoredGeometry);
  await expect(connect.locator("[style]")).toHaveCount(0);

  const link = page.locator("#verify-cli").getByRole("link", { name: "Try the Zod example in the playground" });
  const destination = "/playground?example=zod&" + campaign + "#upgrade";
  await expect(link).toHaveAttribute("href", destination);
  await link.click();
  await expect(page).toHaveURL(new URL(destination, response!.url()).href);
  await expect(page.getByLabel("Lockfile format")).toHaveValue("yarn.lock");
  await expect(page.getByRole("heading", { name: "zod@3.23.8", exact: true })).toBeVisible();
  const report = page.locator("#upgrade");
  await expect(report.getByRole("button", { pressed: true })).toHaveText("Zod 3.23 → 4.6");
  await expect(report.getByRole("heading", { name: "Use z.email() in Zod v4" })).toBeVisible();
  await assertNoOverflow();
  expect({ ...analytics, signals, errors }).toEqual({ ...noAnalytics, signals: [], errors: [] });
});
