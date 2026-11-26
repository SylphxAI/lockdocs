import { expect } from "@playwright/test";
import { test } from "./auth-fixture";

const entry = "?currency=HKD&entry_surface=article_compare";
const rows = [
  { lang: "en", path: "/pricing", pro: /^Get Lockdocs Pro/, heading: "Sign in to Lockdocs",
    email: "Email address", google: "Continue with Google", button: "Continue", toggle: "繁體中文",
    terms: "Terms", privacy: "Privacy Policy" },
  { lang: "zh-TW", path: "/zh-TW/pricing", pro: /^購買 Lockdocs Pro/, heading: "登入 Lockdocs",
    email: "電子郵件地址", google: "使用 Google 登入", button: "繼續", toggle: "English",
    terms: "服務條款", privacy: "隱私權政策" },
] as const;

for (const [index, buyer] of rows.entries()) {
  test(buyer.lang + " pricing keeps the sign-in language and return target", async ({ page, auth }) => {
    auth.methods.email_otp = true;
    const next = buyer.path + entry;
    const encoded = "next=" + encodeURIComponent(next);
    const destination = auth.origin + "/sign-in?" + encoded;
    const card = page.locator(".sign-in-card");
    await page.goto(auth.origin + next);
    const cta = page.getByRole("link", { name: buyer.pro });
    await expect(cta).toHaveAttribute("href", "/sign-in?" + encoded);
    await cta.click();

    for (const [attempt, copy] of [buyer, rows[1 - index], buyer].entries()) {
      const switcher = page.getByRole("link", { name: copy.toggle, exact: true });
      const targetLanguage = copy.lang === "en" ? "zh-TW" : "en";
      await expect(page).toHaveURL(destination + (attempt === 0 ? "" : "&lang=" + copy.lang));
      await expect(page.locator("html")).toHaveAttribute("lang", copy.lang);
      await expect(card.getByRole("heading", { name: copy.heading })).toBeVisible();
      await expect(card.getByRole("link", { name: copy.google })).toBeVisible();
      await expect(card.getByRole("link", { name: copy.google })).toHaveAttribute("href", "/api/auth/google?" + encoded);
      await expect(card.getByLabel(copy.email)).toBeVisible();
      await expect(card.getByRole("button", { name: copy.button, exact: true })).toBeVisible();
      await expect(card.getByRole("link", { name: copy.terms, exact: true })).toHaveAttribute("href", "/legal/terms");
      await expect(card.getByRole("link", { name: copy.privacy, exact: true })).toHaveAttribute("href", "/legal/privacy");
      await expect(switcher).toHaveAttribute("href", "/sign-in?" + encoded + "&lang=" + targetLanguage);
      expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(page.viewportSize()!.width);
      if (attempt === 0) await card.getByLabel(copy.email).fill("reader@example.test");
      else await expect(card.getByLabel(copy.email)).toHaveValue("reader@example.test");
      if (attempt < 2) await switcher.click();
    }
    expect(auth.rejections).toEqual([]);
  });
}

test("a sign-in options error follows the selected language without starting an auth flow", async ({ page, auth }) => {
  await page.route("**/api/auth/options", route => route.fulfill({ status: 503 }), { times: 1 });
  await page.goto(auth.origin + "/sign-in?next=%2Fzh-TW%2Fpricing");
  const card = page.locator(".sign-in-card");
  await expect(card.getByRole("alert")).toHaveText("無法載入登入選項，請重試。");
  await page.getByRole("link", { name: "English", exact: true }).click();
  await expect(card.getByRole("alert")).toHaveText("Couldn’t load sign-in options. Try again.");
  await card.getByRole("button", { name: "Try again", exact: true }).click();
  await expect(card.getByRole("link", { name: "Continue with Google" })).toHaveAttribute("href", "/api/auth/google?next=%2Fzh-TW%2Fpricing");
});

test("sign-in SSR uses only a supported override or the validated return locale and stays noindex", async ({ request, baseURL }) => {
  for (const [query, lang, showError] of [
    ["", "en", false],
    ["?next=%2Fzh-TW%2Fpricing&lang=unsupported&auth=failed", "zh-TW", true],
    ["?next=%2Fzh-TW%2Fpricing&lang=en", "en", false],
    ["?next=%2Fzh-TW%2Fpricing&lang=en&lang=zh-TW", "zh-TW", false],
    ["?next=%2F%2Fexample.test%2Fzh-TW%2Fpricing", "en", false],
  ] as const) {
    const response = await request.get(baseURL + "/sign-in" + query, { maxRedirects: 0 });
    expect(response.status(), query).toBe(200);
    expect(response.headers()["cache-control"]).toBe("private, no-store");
    const html = await response.text();
    expect(html.match(/<html[^>]*\blang="([^"]+)"/)?.[1], query).toBe(lang);
    expect(html).toMatch(/<meta[^>]+ name="robots" content="noindex"/);
    expect(html).not.toMatch(/<link[^>]* rel="(?:canonical|alternate)"/);
    if (showError) expect(html).toContain("未能完成登入，請重試或改用其他登入方式。");
  }
  const sitemap = await request.get(baseURL + "/sitemap.xml", { headers: { host: "lockdocs.sylphx.com" } });
  expect(await sitemap.text()).not.toContain("/sign-in");
});
