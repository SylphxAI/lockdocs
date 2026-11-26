import { expect, test } from "@playwright/test";
import { observeLocalizedAnalytics } from "./analytics-fixture";

const pythonFixture = ["Pydantic==1.10.18", ...Array.from({ length: 44 }, (_, i) =>
  "workbench-sentinel-" + String(i).padStart(2, "0") + "==2.0." + i)].join("\n");

for (const zh of [false, true]) {
  test("local lockfile workbench in " + (zh ? "zh-TW" : "en"), async ({ page, context }, info) => {
    const { wire } = await observeLocalizedAnalytics(page);
    const leaked: string[] = [];
    page.on("request", request => {
      if (/workbench-sentinel|workbench-private/.test(request.url() + (request.postData() ?? ""))) leaked.push(request.url());
    });
    await page.addInitScript(() => {
      const violations: string[] = [];
      Object.assign(window, { cspViolations: violations });
      document.addEventListener("securitypolicyviolation", event => violations.push(event.violatedDirective));
    });
    await context.grantPermissions(["clipboard-read", "clipboard-write"]);
    const response = await page.goto(zh ? "/zh-TW/playground" : "/playground");
    const policy = response?.headers()["content-security-policy"] ?? "";
    expect(policy).toMatch(/script-src [^;]*'nonce-[^']+'/);
    expect(policy).not.toMatch(/'unsafe-inline'|'unsafe-eval'/);
    const banner = page.getByRole("region", { name: zh ? "Cookie 設定" : "Cookie settings" });
    await expect(banner).toBeVisible();
    await banner.getByRole("button", { name: zh ? "僅限分析" : "Analytics only", exact: true }).click();
    const panel = page.locator(".studio-input");
    const editor = page.locator("#lockfile");
    const format = page.getByLabel(zh ? "lockfile 格式" : "Lockfile format");
    const search = page.getByRole("searchbox", { name: zh ? "篩選套件" : "Filter packages" });
    const count = page.locator(".panel-count");
    const list = page.locator(".package-list").getByRole("listitem");
    const pydantic = page.getByRole("button", { name: "Pydantic 1.10 → 2.9" });
    const next = page.getByRole("button", { name: "Next.js 14 → 15" });
    const open = page.getByRole("button", { name: zh ? "開啟檔案" : "Open file" });
    const picker = panel.locator('input[type="file"]');
    const load = panel.getByRole("button", { name: zh ? "載入範例" : "Load sample" });
    const parseError = page.getByRole("status").filter({ hasText: zh ? "無法讀取這份 lockfile" : "Couldn’t parse this input" });
    const expectNoInputError = async () => {
      await expect(editor).not.toHaveAttribute("aria-invalid");
      await expect(editor).not.toHaveAttribute("aria-describedby");
    };
    const resolve = page.getByRole("button", { name: zh ? "查出套件版本" : "Resolve versions" });
    const showMore = page.getByRole("button", { name: zh ? "顯示更多套件" : "Show more packages" });
    const noOverflow = () => expect(page.evaluate(() => document.documentElement.scrollWidth)).resolves.toBeLessThanOrEqual(page.viewportSize()!.width);
    await expect(search).toHaveCount(0);
    await expect(next).toHaveAttribute("aria-pressed", "true");
    const chosen = page.waitForEvent("filechooser");
    await open.click();
    await (await chosen).setFiles({ name: "REQUIREMENTS.TXT", mimeType: "text/plain", buffer: Buffer.from(pythonFixture) });
    await expect(editor).toHaveValue(pythonFixture);
    await expect(format).toHaveValue("requirements.txt");
    await expect(pydantic).toHaveAttribute("aria-pressed", "true");
    await expect(count).toHaveText(zh ? "已鎖定 45 個套件" : "45 pinned");
    await expect.poll(() => wire.some(params => params.get("en") === "lockfile_resolved" &&
      params.get("ep.source_kind") === "file" && params.get("ep.lockfile_format") === "requirements.txt" &&
      params.get("epn.package_count") === "45")).toBe(true);
    await noOverflow();
    if (process.env.LOCKDOCS_LOCAL_CAPTURES) await page.screenshot({ path: info.outputPath("loaded.png"), fullPage: true });

    await expect(list).toHaveCount(35);
    await showMore.click();
    await expect(list).toHaveCount(45);
    await search.fill("  PYDANTIC  ");
    await expect(count).toHaveText(zh ? "符合 1 / 45 個套件" : "1 of 45 pinned");
    await expect(list).toHaveCount(1);
    await expect(showMore).toHaveCount(0);
    await search.fill("workbench-private-no-match");
    await expect(page.getByText(zh ? "找不到符合的套件" : "No matching packages")).toBeVisible();
    await expect(count).toHaveText(zh ? "符合 0 / 45 個套件" : "0 of 45 pinned");
    await expectNoInputError();
    await search.clear();
    await expect(list).toHaveCount(35);

    const transfer = await page.evaluateHandle(() => {
      const data = new DataTransfer();
      data.items.add(new File(["pytest==8.3.3"], "workbench-private-requirements.txt"));
      return data;
    });
    await editor.dispatchEvent("dragover", { dataTransfer: transfer });
    await expect(panel).toHaveClass(/studio-dragging/);
    await editor.dispatchEvent("dragleave", { dataTransfer: transfer });
    await expect(panel).not.toHaveClass(/studio-dragging/);
    await editor.dispatchEvent("dragover", { dataTransfer: transfer });
    await editor.dispatchEvent("drop", { dataTransfer: transfer });
    await transfer.dispose();
    await expect(panel).not.toHaveClass(/studio-dragging/);
    await expect(format).toHaveValue("requirements.txt");
    await expect(editor).toHaveValue("pytest==8.3.3");
    await expect(count).toHaveText(zh ? "已鎖定 1 個套件" : "1 pinned");
    await expect(search).toHaveCount(0);

    await picker.setInputFiles({ name: "workbench-private-big.txt", mimeType: "text/plain", buffer: Buffer.from("é".repeat(200_001)) });
    await expect(panel.getByRole("status").filter({ hasText: "400,000" })).toContainText(zh ? "400,000 位元組" : "400,000 bytes");
    await expectNoInputError();
    await expect(editor).toHaveValue("pytest==8.3.3");
    await expect(count).toHaveText(zh ? "已鎖定 1 個套件" : "1 pinned");
    await noOverflow();
    if (process.env.LOCKDOCS_LOCAL_CAPTURES) await page.screenshot({ path: info.outputPath("oversize.png"), fullPage: true });
    await panel.getByRole("button", { name: zh ? "複製離線 CLI 指令" : "Copy offline CLI command" }).click();
    await expect.poll(() => page.evaluate(() => navigator.clipboard.readText())).toBe("npx -y @sylphx/lockdocs --offline");

    await format.selectOption("package-lock.json");
    for (const empty of ["", "{}"]) {
      await editor.fill(empty);
      await resolve.click();
      await expect(page.getByRole("status").filter({ hasText: zh ? "找不到指定確切版本的套件" : "No exact package versions found" })).toBeVisible();
      await expectNoInputError();
    }

    const malformed = '{"packages":{"node_modules/workbench-private-invalid":';
    const submitInvalid = async () => {
      await editor.fill(malformed);
      await expectNoInputError();
      await resolve.click();
      await expect(parseError).toBeVisible();
      await expect(count).toHaveText(zh ? "已鎖定 0 個套件" : "0 pinned");
      await expect(editor).toHaveAttribute("aria-invalid", "true");
      const errorId = await editor.getAttribute("aria-describedby");
      expect(errorId).toBeTruthy();
      await expect(parseError).toHaveAttribute("id", errorId!);
      await expect(editor).toHaveAccessibleDescription(await parseError.innerText());
    };
    await submitInvalid();
    await noOverflow();
    if (process.env.LOCKDOCS_LOCAL_CAPTURES) await page.screenshot({ path: info.outputPath("parse-error.png"), fullPage: true });
    await load.click();
    await expect(count).toHaveText(zh ? "已鎖定 3 個套件" : "3 pinned");
    await expectNoInputError();
    await expect(parseError).toHaveCount(0);

    await submitInvalid();
    await format.selectOption("requirements.txt");
    await expect(editor).toHaveValue(malformed);
    await expectNoInputError();
    await expect(parseError).toHaveCount(0);
    await format.selectOption("package-lock.json");
    await resolve.click();
    await expect(editor).toHaveAttribute("aria-invalid", "true");
    await editor.fill('{"packages":{"node_modules/workbench-private-recovery":{"version":"1.2.3"},"node_modules/next":{"version":"14.2.3"}}}');
    await expectNoInputError();
    await expect(parseError).toHaveCount(0);
    await resolve.click();
    await expect(page.getByRole("heading", { name: "next@14.2.3", exact: true })).toBeVisible();
    await expect(count).toHaveText(zh ? "已鎖定 2 個套件" : "2 pinned");
    await expectNoInputError();
    await expect(parseError).toHaveCount(0);
    await expect(next).toHaveAttribute("aria-pressed", "true");
    await expect(panel.getByText(/400,000/)).toHaveCount(0);
    await expect.poll(() => wire.filter(params => params.get("en") === "lockfile_resolved" &&
      params.get("ep.lockfile_format") === "package-lock.json")
      .map(params => [params.get("ep.source_kind"), params.get("epn.package_count")]))
      .toEqual([["pasted", "0"], ["sample", "3"], ["pasted", "2"]]);
    expect(wire.some(params => params.get("en") === "upgrade_preview_switched")).toBe(false);
    await pydantic.click();
    await expect.poll(() => wire.filter(params => params.get("en") === "upgrade_preview_switched")
      .map(params => params.get("ep.example_id"))).toEqual(["pydantic"]);
    expect(leaked).toEqual([]);
    expect(await page.evaluate(() => Reflect.get(window, "cspViolations"))).toEqual([]);
  });
}
