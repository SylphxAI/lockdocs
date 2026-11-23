import { expect, test } from "@playwright/test";

const commands = ["npx -y @sylphx/lockdocs setup", "uvx lockdocs setup", "pip install lockdocs"];
const pypi = "https://pypi.org/project/lockdocs/";

test("the homepage serves every install command in HTML and the docs explain the Python paths", async ({ request }) => {
  const home = await request.get("/");
  expect(home.status()).toBe(200);
  const html = await home.text();
  for (const command of commands) expect(html).toContain(command + "</code>");

  for (const route of ["/docs", "/guide", "/guide/setup", "/guides/up-to-date-docs-for-cursor-and-claude-code", "/llms.txt"]) {
    const response = await request.get(route);
    expect(response.status(), route).toBe(200);
    const body = await response.text();
    for (const text of ["uvx lockdocs", "pip install lockdocs", pypi]) expect(body, route).toContain(text);
    if (route === "/guide/setup") expect(body).toContain('id="without-nodejs"');
  }
});

test("choose and copy each install command without moving the hero", async ({ page, context }) => {
  await page.route("**/api/analytics/config", route => route.fulfill({ json: { measurementId: null } }));
  await context.grantPermissions(["clipboard-read", "clipboard-write"]);
  const errors: string[] = [];
  page.on("pageerror", error => errors.push(error.message));
  page.on("console", message => { if (message.type() === "error") errors.push(message.text()); });
  await page.goto("/");
  await page.evaluate(() => document.fonts.ready);

  const card = page.getByRole("region", { name: "Install Lockdocs" });
  const copy = card.getByRole("button", { name: "Copy install command" });
  const preview = page.locator(".hero-preview");
  const npx = card.getByRole("radio", { name: "npx" });
  const uvx = card.getByRole("radio", { name: "uvx" });
  const pip = card.getByRole("radio", { name: "pip" });
  const layout = async () => [await card.boundingBox(), await copy.boundingBox(), await preview.boundingBox()];
  expect(await card.getByRole("link", { name: "PyPI" }).getAttribute("href")).toBe(pypi);
  const base = await layout();
  expect(base.every(box => box !== null)).toBeTruthy();

  const check = async (command: string) => {
    const active = card.locator("code:visible");
    await expect(active).toHaveText(command);
    await expect(card.getByRole("status")).toBeEmpty();
    expect(await active.evaluate(element => {
      const range = document.createRange();
      range.selectNodeContents(element);
      return range.getClientRects().length;
    })).toBe(1);
    await copy.click();
    await expect(card.getByRole("status")).toHaveText("Copied");
    expect(await page.evaluate(() => navigator.clipboard.readText())).toBe(command);
    expect(await layout()).toEqual(base);
  };

  await expect(npx).toBeChecked();
  await check(commands[0]);
  await npx.focus();
  await page.keyboard.press("ArrowRight");
  await expect(uvx).toBeChecked();
  await check(commands[1]);
  if (page.viewportSize()!.width < 600) await card.getByText("pip", { exact: true }).tap();
  else await card.getByText("pip", { exact: true }).click();
  await expect(pip).toBeChecked();
  await check(commands[2]);
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(page.viewportSize()!.width);
  expect(errors).toEqual([]);
});
