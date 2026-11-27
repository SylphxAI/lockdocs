import { expect, test } from "@playwright/test";
import { upgrades } from "../../src/lib/upgrade-examples";
import { privateProofExamples } from "../../src/lib/private-proof-examples";

for (const zh of [false, true]) {
  const prefix = zh ? "/zh-TW" : "";
  const path = prefix + "/pricing";
  test(path + " puts a report example beside the signed-out purchase decision and keeps the full proofs usable", async ({ page }, info) => {
    const errors: string[] = [];
    page.on("pageerror", error => errors.push(error.message));
    page.on("console", message => { if (message.type() === "error") errors.push(message.text()); });
    await page.exposeFunction("recordPricingCspViolation", (directive: string) => errors.push(directive));
    await page.addInitScript(() => {
      document.addEventListener("securitypolicyviolation", event => {
        void (window as unknown as { recordPricingCspViolation: (directive: string) => Promise<unknown> })
          .recordPricingCspViolation(event.violatedDirective);
      });
    });
    await page.setExtraHTTPHeaders({ "CF-IPCountry": "DE" });
    const signalRequests: string[] = [];
    await page.route("**/v1/analytics/events:batch", async route => {
      signalRequests.push(route.request().method());
      await route.fulfill({ status: 202, json: { accepted_events: 1 } });
    });
    await page.route("**/api/analytics/config", async route => {
      const response = await route.fetch();
      await route.fulfill({ response, json: { ...await response.json(), measurementId: null,
        signalKey: "sylphx_pk_signal_fixture", signalApiOrigin: "https://api.sylphx.com" } });
    });
    const config = page.waitForResponse(response => response.url().endsWith("/api/analytics/config"));
    const surface = "context7_alternative";
    const params = `currency=EUR&entry_surface=${surface}&utm_source=newsletter`;
    const response = await page.goto(path + "?" + params);
    const csp = response?.headers()["content-security-policy"] ?? "";
    expect(csp).toMatch(/style-src [^;]*'nonce-[^']+'/);
    expect(csp).not.toContain("'unsafe-inline'");
    expect((await (await config).json()).signalKey).toBe("sylphx_pk_signal_fixture");

    const locale = zh ? 1 : 0;
    const plan = page.getByRole("article").filter({ has: page.getByRole("heading", { name: "Lockdocs Pro", exact: true }) });
    const decisionProof = plan.locator(".pricing-decision-proof");
    const example = upgrades.next;
    await expect(decisionProof).toBeVisible();
    await expect(decisionProof).toContainText(example.label);
    await expect(decisionProof).toContainText(example.change[locale]);
    await expect(decisionProof.locator(".decision-site code")).toHaveText(example.path);
    const buy = plan.getByRole("link", { name: zh ? "購買 Lockdocs Pro — €109／年" : "Get Lockdocs Pro — €109/yr", exact: true });
    await expect(buy).toHaveAccessibleDescription(zh ? "請先登入，回到此頁後即可繼續結帳。"
      : "Sign in first, then return here to continue to checkout.");
    await expect(buy).toBeVisible();
    expect(await buy.evaluate(link => {
      const next = link.nextElementSibling!;
      const proof = next.nextElementSibling!;
      const [linkBox, nextBox, proofBox] = [link, next, proof].map(el => el.getBoundingClientRect());
      return proof.scrollWidth <= proof.clientWidth && linkBox.height >= 44 &&
        linkBox.bottom <= nextBox.top && nextBox.bottom <= proofBox.top && proofBox.top - linkBox.bottom < 60;
    })).toBe(true);
    if (process.env.LOCKDOCS_LOCAL_CAPTURES) await plan.screenshot({ path: info.outputPath("pricing-decision.png"), animations: "disabled" });
    const proof = page.getByRole("region", { name: zh ? "Pro 功能實例" : "What Pro shows you" });
    const upgrade = proof.getByRole("article").filter({ has: page.getByRole("group", { name: zh ? /升級報告/ : /UPGRADE REPORT/ }) });
    const registry = proof.getByRole("article").filter({ has: page.getByRole("group", { name: zh ? /私有套件文件/ : /PRIVATE PACKAGE DOCS/ }) });
    const widthFits = async () => expect(await proof.evaluate(region => {
      const root = document.documentElement;
      const blocks = [root, region, ...region.querySelectorAll('article, [role="group"], pre')];
      return blocks.filter(el => el.scrollWidth > el.clientWidth).map(el => el.tagName);
    })).toEqual([]);

    await expect(upgrade.getByRole("button", { pressed: true })).toHaveText(upgrades.next.label);
    await expect(registry.getByRole("button", { pressed: true })).toHaveText("npm");
    await expect(upgrade.getByRole("group").getByRole("button")).toHaveCount(4);
    await expect(registry.getByRole("group").getByRole("button")).toHaveCount(4);
    for (const sample of Object.values(upgrades)) {
      const button = upgrade.getByRole("button", { name: sample.label, exact: true });
      if (sample === upgrades.pydantic) await button.press("Enter");
      else await button.click();
      await expect(upgrade.getByRole("button", { pressed: true })).toHaveText(sample.label);
      await expect(upgrade).toContainText(sample.change[locale]);
      await expect(upgrade.locator(".pricing-proof-path")).toHaveText(sample.path);
      await expect(upgrade.locator("pre")).toHaveText(["- " + sample.before, "+ " + sample.after]);
      await expect(upgrade.locator(".sample-diff p")).toHaveText([sample.oldSource, sample.newSource]);
      await widthFits();
    }
    for (const sample of Object.values(privateProofExamples)) {
      await registry.getByRole("button", { name: sample.label, exact: true }).click();
      await expect(registry.getByRole("button", { pressed: true })).toHaveText(sample.label);
      await expect(registry.locator("pre")).toHaveText([sample.fetch, sample.search]);
      await expect(registry).toContainText(sample.setup[locale]);
      await expect(registry).toContainText(zh ? "憑證只會傳到設定的來源，專案程式碼仍留在本機。"
        : "Credentials go only to their configured origin; your project code stays local.");
      await widthFits();
    }
    await expect(upgrade.getByRole("button", { pressed: true })).toHaveText(upgrades.react.label);
    await expect(upgrade.getByRole("link")).toHaveAttribute("href", prefix + "/playground?example=react&utm_source=newsletter#upgrade");
    await expect(registry.getByRole("link")).toHaveAttribute("href", "/pro?utm_source=newsletter#private-sources");
    await expect(proof.locator("[style]")).toHaveCount(0);

    const picker = page.getByRole("navigation", { name: zh ? "付款幣別" : "Billing currency" });
    await expect(picker.locator('[aria-current="page"]')).toHaveText("EUR");
    const checkout = page.getByRole("link", { name: zh ? /^購買 Lockdocs Pro/ : /^Get Lockdocs Pro/ });
    for (const [currency, amount] of [["EUR", "€109"], ["HKD", "HK$938"]]) {
      if (currency === "HKD") await picker.getByRole("link", { name: currency }).click();
      await expect(picker.locator('[aria-current="page"]')).toHaveText(currency);
      await expect(checkout).toContainText(amount);
      const next = path + `?currency=${currency}&entry_surface=${surface}`;
      const url = new URL((await checkout.getAttribute("href"))!, page.url());
      expect(url.pathname).toBe("/sign-in");
      expect(Object.fromEntries(url.searchParams)).toEqual({ next, utm_source: "newsletter" });
    }
    expect(signalRequests).toEqual([]);
    expect(errors).toEqual([]);
  });
}
