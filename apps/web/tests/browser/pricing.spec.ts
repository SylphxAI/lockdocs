import { expect, test } from "@playwright/test";

const plans = [
  { currency: "USD", free: "US$0", annual: "US$120" },
  { currency: "GBP", free: "£0", annual: "£99" },
  { currency: "EUR", free: "€0", annual: "€109" },
  { currency: "HKD", free: "HK$0", annual: "HK$938" },
];

// Verify prices and metadata without relying on a client render.
for (const plan of plans) {
  test("renders " + plan.currency + " prices and one set of metadata in SSR", async ({ request }) => {
    const response = await request.get("/pricing?currency=" + plan.currency);
    expect(response.status()).toBe(200);
    const html = (await response.text()).replace(/<!--[\s\S]*?-->/g, "");
    const head = html.match(/<head>[\s\S]*?<\/head>/)?.[0] ?? "";
    for (const tag of [/<head>/g, /<title\b/g, /<meta\b[^>]*name="description"/g, /<link\b[^>]*rel="canonical"/g, /<meta\b[^>]*property="og:image"/g]) {
      expect(head.match(tag)).toHaveLength(1);
      expect(html.match(tag)).toHaveLength(1);
    }
    expect(head).toContain('rel="canonical" href="https://lockdocs.sylphx.com/pricing"');
    expect(html.match(/class="plan-price">([^<]*)/g)).toEqual([
      'class="plan-price">' + plan.annual + "/yr", 'class="plan-price">' + plan.free,
    ]);
    const picker = html.match(/<nav[^>]*aria-label="Billing currency"[^>]*>[\s\S]*?<\/nav>/)?.[0] ?? "";
    expect(picker.match(/aria-current="page"/g)).toHaveLength(1);
    expect(picker.match(/<a[^>]*aria-current="page"[^>]*>([^<]+)<\/a>/)?.[1]).toBe(plan.currency);
    expect(html).toContain("Billed annually upfront · Tax included");
    expect(html).toContain('"price":0,"priceCurrency":"' + plan.currency + '"');
  });
}

test.describe("pricing in a browser using German number conventions", () => {
  test.use({ locale: "de-DE", timezoneId: "Asia/Hong_Kong" });

  test("preserves the catalogue and sign-in return path when switching currencies", async ({ page }) => {
    const errors: string[] = [];
    page.on("pageerror", error => errors.push(error.message));
    page.on("console", message => { if (/hydrat/i.test(message.text())) errors.push(message.text()); });
    await page.goto("/pricing?currency=EUR");
    const picker = page.getByRole("navigation", { name: "Billing currency" });
    const free = page.locator(".plan:not(.plan-pro) .plan-price");
    const pro = page.locator(".plan-pro");
    await expect(pro.locator(".plan-price")).toHaveText("€109/yr");
    for (const plan of plans) {
      await picker.getByRole("link", { name: plan.currency, exact: true }).click();
      await expect(picker.locator('[aria-current="page"]')).toHaveText(plan.currency);
      await expect(free).toHaveText(plan.free);
      await expect(pro.locator(".plan-price")).toHaveText(plan.annual + "/yr");
      await expect(pro.locator(".plan-total")).toContainText("Billed annually upfront · Tax included");
    }
    await expect(pro.getByRole("link", { name: "Get Lockdocs Pro — HK$938/yr" }))
      .toHaveAttribute("href", "/sign-in?next=%2Fpricing%3Fcurrency%3DHKD");
    await expect(page.locator('head link[rel="canonical"]')).toHaveCount(1);
    expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(page.viewportSize()!.width);
    await page.getByRole("link", { name: "Privacy", exact: true }).click();
    await expect(page.locator("time")).toHaveText("23. November 2026");
    await expect(page.locator("time")).toHaveAttribute("datetime", "2026-11-23");
    expect(errors).toEqual([]);
  });
});

test("account and privacy distinguish subscription cancellation from privacy rights", async ({ page }) => {
  await page.goto("/dashboard");
  await page.getByRole("link", { name: "subscription terms" }).click();
  await expect(page).toHaveURL(/\/legal\/terms$/);
  await expect(page.getByRole("link", { name: "hi@sylphx.com" })).toHaveAttribute("href", "mailto:hi@sylphx.com");
  await expect(page.getByRole("link", { name: "Account", exact: true }).last()).toHaveAttribute("href", "/dashboard");
  await page.getByRole("link", { name: "Privacy", exact: true }).click();
  await expect(page.getByRole("link", { name: "Lockdocs subscription terms" })).toHaveAttribute("href", "/legal/terms");
  await expect(page.getByRole("link", { name: "Sylphx Privacy Policy" })).toHaveAttribute("href", "https://sylphx.com/legal/privacy");
});
