import { expect, type Page } from "@playwright/test";
import { test } from "./auth-fixture";

const summary = (page: Page) => page.getByRole("region", { name: /^Lockdocs Pro · / });

for (const [locale, pricing, buy, back] of [
  ["en", "/pricing", /^Get Lockdocs Pro — £99\/yr/, "Back to pricing"],
  ["zh-TW", "/zh-TW/pricing", /^購買 Lockdocs Pro — £99／年/, "返回價格頁面"],
] as const) {
  test(locale + " keeps the selected plan visible through sign-in and returns to one checkout action", async ({ page, auth }) => {
    const next = pricing + "?currency=GBP";
    const checkouts: string[] = [];
    page.on("request", request => { if (new URL(request.url()).pathname === "/api/billing/checkout") checkouts.push(request.method()); });
    await page.goto(auth.origin + next);
    await page.getByRole("link", { name: buy }).click();
    await expect(page).toHaveURL(auth.origin + "/sign-in?" + new URLSearchParams({ next }));
    await expect(summary(page)).toHaveAccessibleName(locale === "en" ? "Lockdocs Pro · £99/yr" : "Lockdocs Pro · £99／年");
    await expect(summary(page).getByRole("link", { name: back })).toHaveAttribute("href", next);
    const html = await (await page.request.get(auth.origin + "/sign-in?" + new URLSearchParams({ next }))).text();
    expect(html).toMatch(/Lockdocs Pro · (?:<!--\$-->)?£99/);

    await page.getByRole("link", { name: locale === "en" ? "Continue with Google" : "使用 Google 登入", exact: true }).click();
    await expect(page).toHaveURL(auth.origin + next);
    await expect(page.getByRole("navigation", { name: locale === "en" ? "Billing currency" : "付款幣別" }).getByRole("link", { name: "GBP", exact: true })).toHaveAttribute("aria-current", "page");
    // Returning signed in never starts checkout by itself.
    await page.reload();
    expect(checkouts).toEqual([]);
    expect(auth.checkouts).toEqual([]);
    expect(auth.rejections).toEqual([]);
  });
}

for (const [locale, pricing] of [["en", "/pricing"], ["zh-TW", "/zh-TW/pricing"]] as const) {
  test(locale + " summary repeats the pricing card's price and billing terms character for character", async ({ page, auth }) => {
    for (const currency of ["USD", "GBP", "EUR", "HKD"]) {
      const next = currency === "USD" ? pricing : pricing + "?currency=" + currency;
      await page.goto(auth.origin + next);
      const card = page.locator(".plan-pro");
      const price = (await card.locator(".plan-price").textContent())!;
      const terms = (await card.locator(".plan-total").textContent())!;
      await page.goto(auth.origin + "/sign-in?" + new URLSearchParams({ next }));
      await expect(summary(page)).toHaveAccessibleName("Lockdocs Pro · " + price);
      await expect(summary(page).getByText(terms, { exact: true })).toBeVisible();
    }
  });
}

test("back from a failed sign-in returns to the same pricing selection", async ({ page, auth }) => {
  await page.goto(auth.origin + "/sign-in?auth=failed&next=" + encodeURIComponent("/pricing?currency=EUR"));
  await expect(page.getByRole("status").filter({ hasText: "Sign-in didn’t finish" })).toBeVisible();
  await expect(summary(page)).toHaveAccessibleName("Lockdocs Pro · €109/yr");
  await summary(page).getByRole("link", { name: "Back to pricing" }).click();
  await expect(page).toHaveURL(auth.origin + "/pricing?currency=EUR");
});

for (const next of ["//example.invalid/pricing", "/pricing?currency=ZZZ", "/pricing?amount=1", "/pricing?currency=USD&currency=GBP", "/dashboard"]) {
  test("shows no purchase for return target " + next, async ({ page, auth }) => {
    await page.goto(auth.origin + "/sign-in?" + new URLSearchParams({ next }));
    await expect(page.locator(".sign-in-card").getByRole("link", { name: "Continue with Google" })).toBeVisible();
    await expect(summary(page)).toHaveCount(0);
  });
}
