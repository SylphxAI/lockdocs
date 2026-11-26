import { expect, test, type Page } from "@playwright/test";
import { upgrades } from "../../src/lib/upgrade-examples";
import { observeLocalizedAnalytics } from "./analytics-fixture";

const cases = {
  next: ["Next.js", "package-lock.json", "next", "14.2.3"],
  pydantic: ["Pydantic", "uv.lock", "pydantic", "1.10.18"],
  zod: ["Zod", "yarn.lock", "zod", "3.23.8"],
  react: ["React", "pnpm-lock.yaml", "react-dom", "18.3.1"],
} as const;
type Id = keyof typeof cases;
const ids = Object.keys(cases) as Id[];
const assertNoOverflow = async (page: Page) => expect(await page.evaluate(() => {
  const root = document.documentElement;
  return [root, ...document.querySelectorAll(".hero-preview, .preview-lock, .preview-diff, .upgrade-card, .studio-grid")]
    .filter(el => el.scrollWidth > el.clientWidth).map(el => el.className);
})).toEqual([]);

for (const zh of [false, true]) {
  const prefix = zh ? "/zh-TW" : "";
  const playground = prefix + "/playground";
  const campaign = "utm_source=newsletter&utm_content=docs%2Bapi";
  test(`version inspector and pricing carry the exact upgrade into the Playground (${zh ? "zh-TW" : "en"})`, async ({ page, isMobile }, info) => {
    const errors: string[] = [];
    const signals: string[] = [];
    const { wire } = await observeLocalizedAnalytics(page);
    page.on("pageerror", error => errors.push(error.message));
    page.on("console", message => { if (message.type() === "error") errors.push(message.text()); });
    await page.exposeFunction("recordInspectorViolation", (directive: string) => { errors.push(directive); });
    await page.addInitScript(() => {
      document.addEventListener("securitypolicyviolation", event => {
        void (window as unknown as { recordInspectorViolation: (directive: string) => Promise<void> }).recordInspectorViolation(event.violatedDirective);
      });
    });
    await page.route("**/api/analytics/config", async route => {
      const response = await route.fetch();
      await route.fulfill({ response, json: { ...await response.json(), signalKey: "sylphx_pk_signal_fixture",
        signalApiOrigin: "https://api.sylphx.com" } });
    });
    await page.route("**/v1/analytics/events:batch", async route => {
      signals.push(route.request().method());
      await route.fulfill({ status: 202, json: { accepted_events: 1 }, headers: {
        "Access-Control-Allow-Origin": "*", "Access-Control-Allow-Headers": "content-type,x-sylphx-key",
      } });
    });
    const response = await page.goto(prefix + "/?" + campaign);
    const csp = response?.headers()["content-security-policy"] ?? "";
    expect(csp).toMatch(/style-src [^;]*'nonce-[^']+'/);
    expect(csp).not.toContain("'unsafe-inline'");
    await page.evaluate(() => document.fonts.ready);
    const hero = page.getByRole("region", { name: zh ? "版本檢視器" : "VERSION INSPECTOR" });
    const link = hero.getByRole("link");
    const geometry = () => hero.evaluate(el => [el, el.querySelector(".preview-link")!, document.querySelector(".bench-rail")!]
      .map(node => { const rect = node.getBoundingClientRect(); return [rect.top + window.scrollY, rect.height]; }));
    const initialGeometry = await geometry();
    await expect(hero.getByRole("button", { pressed: true })).toHaveText("Next.js");
    for (const id of ["pydantic", "zod", "react", "next"] as const) {
      const [name, format, pkg, version] = cases[id];
      const button = hero.getByRole("button", { name, exact: true });
      if (isMobile) await button.tap();
      else await button.press(id === "pydantic" ? "Enter" : "Space");
      await expect(hero.getByRole("button", { pressed: true })).toHaveText(name);
      await expect(hero.locator(".preview-files")).toContainText(format);
      await expect(hero.locator(".preview-files")).toContainText(pkg + "@" + version);
      await expect(hero.locator(".preview-diff code")).toHaveText([upgrades[id].before, upgrades[id].after]);
      await expect(hero.locator(".preview-diff small")).toHaveText([upgrades[id].oldSource, upgrades[id].newSource]);
      const snippet = await hero.locator(".preview-lock").textContent();
      expect(snippet?.split("\n")).toHaveLength(3);
      expect(snippet).toContain(version);
      await expect(link).toHaveAttribute("href", playground + "?example=" + id + "&" + campaign + "#upgrade");
      expect(await geometry()).toEqual(initialGeometry);
      await assertNoOverflow(page);
    }
    await expect(hero.locator("[style]")).toHaveCount(0);
    const report = page.locator("#upgrade");
    const format = page.getByLabel(zh ? "lockfile 格式" : "Lockfile format");
    const assertPinned = async (id: Id) => {
      const [, file, pkg, version] = cases[id];
      await expect(format).toHaveValue(file);
      await expect(page.locator("#lockfile")).toHaveValue(new RegExp(version.replaceAll(".", "\\.")));
      await expect(page.getByRole("heading", { name: pkg + "@" + version, exact: true })).toBeVisible();
      await expect(report.getByRole("button", { pressed: true })).toHaveText(upgrades[id].label);
      await expect(report.locator(".sample-diff pre")).toHaveText(["- " + upgrades[id].before, "+ " + upgrades[id].after]);
      await assertNoOverflow(page);
    };
    const target = zh ? "pydantic" : "zod";
    await hero.getByRole("button", { name: cases[target][0], exact: true }).click();
    await hero.screenshot({ path: info.outputPath("inspector.png") });
    await link.click();
    await expect(page).toHaveURL(url => url.pathname === playground && url.searchParams.get("example") === target && url.hash === "#upgrade");
    await assertPinned(target);
    await page.goto(prefix + "/pricing?" + campaign);
    await page.getByRole("button", { name: upgrades.react.label, exact: true }).click();
    await page.getByRole("link", { name: zh ? "互動預覽升級報告" : "Try the report preview", exact: true }).click();
    await expect(page).toHaveURL(url => url.pathname === playground && url.searchParams.get("example") === "react" && url.hash === "#upgrade");
    await assertPinned("react");
    // Direct SSR/hydration and an invalid query keep separate initial samples.
    await page.goto(playground + "?example=next#upgrade");
    await assertPinned("next");
    await page.goto(playground + "?example=bogus#upgrade");
    await expect(format).toHaveValue("package-lock.json");
    await expect(page.getByRole("heading", { name: "next@15.0.0", exact: true })).toBeVisible();
    await expect(report.getByRole("button", { pressed: true })).toHaveText(upgrades.next.label);
    expect(signals).toEqual([]);
    expect(wire).toEqual([]);

    const panel = page.getByRole("region", { name: zh ? "Cookie 設定" : "Cookie settings" });
    await panel.getByRole("button", { name: zh ? "僅限分析" : "Analytics only", exact: true }).click();
    await expect.poll(() => signals).toContain("POST");
    await page.goto(playground + "?example=" + target + "#upgrade");
    await assertPinned(target);
    const switched = () => wire.filter(event => event.get("en") === "upgrade_preview_switched").map(event => event.get("ep.example_id"));
    await report.getByRole("button", { name: upgrades.react.label, exact: true }).click();
    await expect.poll(switched).toEqual(["react"]);
    expect(errors).toEqual([]);
  });
}
