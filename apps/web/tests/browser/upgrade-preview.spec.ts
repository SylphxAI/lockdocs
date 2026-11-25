import { expect, test } from "@playwright/test";
import { observeLocalizedAnalytics } from "./analytics-fixture";

const yarn = "# yarn lockfile v1\nzod@^3.23.0:\n  version \"3.23.8\"\n";
const pnpm = "lockfileVersion: '9.0'\npackages:\n  react@18.3.1: {}\n  react-dom@18.3.1: {}\n";
const examples = [
  ["Next.js 14 → 15", "cookies() is now asynchronous", "cookies() 改為非同步函式", "next@15.0.0 dist/server/request/cookies.d.ts:24"],
  ["Pydantic 1.10 → 2.9", "Use model_dump() in Pydantic v2", "Pydantic v2 改用 model_dump()", "pydantic@2.9.2 pydantic/main.py:352"],
  ["Zod 3.23 → 4.6", "Use z.email() in Zod v4", "Zod v4 改用 z.email()", "zod@4.6.7 src/v4/classic/schemas.ts:710–712"],
  ["React 18 → 19", "React 19 removes ReactDOM.render()", "React 19 移除了 ReactDOM.render()", "react-dom@19.0.0 cjs/react-dom-client.development.js:24452, 24602"],
] as const;

for (const zh of [false, true]) {
  test("upgrade previews and lockfile matches in " + (zh ? "zh-TW" : "en"), async ({ page }) => {
    const { wire, frames, hits } = await observeLocalizedAnalytics(page);
    const signalRequests: string[] = [];
    await page.route("**/api/analytics/config", async route => {
      const response = await route.fetch();
      await route.fulfill({ response, json: { ...await response.json(), signalKey: "sylphx_pk_signal_fixture",
        signalApiOrigin: "https://api.sylphx.com" } });
    });
    await page.route("**/v1/analytics/events:batch", async route => {
      signalRequests.push(route.request().method());
      await route.fulfill({ status: 202, json: { accepted_events: 1 }, headers: {
        "Access-Control-Allow-Origin": "*", "Access-Control-Allow-Headers": "content-type,x-sylphx-key",
      } });
    });
    await page.addInitScript(() => {
      const violations: string[] = [];
      Object.assign(window, { cspViolations: violations });
      document.addEventListener("securitypolicyviolation", event => violations.push(event.violatedDirective));
    });
    const response = await page.goto(zh ? "/zh-TW/playground" : "/playground");
    const policy = response?.headers()["content-security-policy"] ?? "";
    expect(policy).toMatch(/script-src [^;]*'nonce-[^']+'/);
    expect(policy).not.toMatch(/'unsafe-inline'|'unsafe-eval'/);
    const report = page.locator("#upgrade");
    const picker = report.getByRole("group", { name: zh ? "升級範例" : "Upgrade example" });
    const example = (index: number) => picker.getByRole("button", { name: examples[index][0], exact: true });
    const banner = page.getByRole("region", { name: zh ? "Cookie 設定" : "Cookie settings" });
    await expect(banner).toBeVisible();
    await expect(picker.getByRole("button")).toHaveCount(4);
    await expect(example(0)).toHaveAttribute("aria-pressed", "true");
    await expect(report.getByRole("link", { name: zh ? "用 Pro 取得完整升級報告" : "Get full reports with Pro" }))
      .toHaveAttribute("href", (zh ? "/zh-TW" : "") + "/pricing?entry_surface=playground_report_preview");
    await expect(page.locator(".upgrade-cta a")).toHaveAttribute("href", (zh ? "/zh-TW" : "") + "/pricing?entry_surface=playground_upgrade_cta");

    for (const index of [1, 2, 3, 0]) {
      const [, enTitle, zhTitle, citation] = examples[index];
      await example(index).click();
      await expect(example(index)).toHaveAttribute("aria-pressed", "true");
      await expect(picker.locator('[aria-pressed="true"]')).toHaveCount(1);
      await expect(report.getByRole("heading", { name: zh ? zhTitle : enTitle })).toBeVisible();
      await expect(report.locator(".sample-diff")).toContainText(citation);
      expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(page.viewportSize()!.width);
    }
    const buttonRows = await picker.getByRole("button").evaluateAll(buttons => buttons.map(button => button.getBoundingClientRect().top));
    expect(new Set(buttonRows).size).toBe(page.viewportSize()!.width === 390 ? 2 : 1);
    expect(await picker.evaluate(element => element.scrollWidth <= element.clientWidth)).toBe(true);
    await expect(report.locator("[style]")).toHaveCount(0);
    expect({ signalRequests, wire, frames, hits }).toEqual({ signalRequests: [], wire: [], frames: [], hits: [] });

    // Prove the analytics fixture works, then distinguish user clicks from contextual matches.
    await banner.getByRole("button", { name: zh ? "僅限分析" : "Analytics only", exact: true }).click();
    await expect.poll(() => signalRequests).toContain("POST");
    const switched = () => wire.filter(params => params.get("en") === "upgrade_preview_switched")
      .map(params => params.get("ep.example_id"));
    await example(1).click();
    await expect.poll(switched).toEqual(["pydantic"]);
    const editor = page.locator("#lockfile");
    await page.getByLabel(zh ? "lockfile 格式" : "Lockfile format").selectOption("yarn.lock");
    await expect(example(1)).toHaveAttribute("aria-pressed", "true"); // Loading a bundled sample does not select its preview.
    await editor.fill(yarn);
    await page.getByRole("button", { name: zh ? "查出套件版本" : "Resolve versions" }).click();
    await expect(example(2)).toHaveAttribute("aria-pressed", "true");
    const transfer = await page.evaluateHandle(input => {
      const data = new DataTransfer();
      data.items.add(new File([input], "pnpm-lock.yaml"));
      return data;
    }, pnpm);
    await editor.dispatchEvent("drop", { dataTransfer: transfer });
    await transfer.dispose();
    await expect(example(3)).toHaveAttribute("aria-pressed", "true");
    await expect.poll(() => wire.filter(params => params.get("en") === "lockfile_resolved")
      .map(params => [params.get("ep.lockfile_format"), params.get("ep.source_kind")])).toEqual([
      ["yarn.lock", "sample"], ["yarn.lock", "pasted"], ["pnpm-lock.yaml", "file"],
    ]);
    await example(2).click();
    await example(3).click();
    await expect.poll(switched).toEqual(["pydantic", "zod", "react"]);
    expect(await page.evaluate(() => Reflect.get(window, "cspViolations"))).toEqual([]);
  });
}
