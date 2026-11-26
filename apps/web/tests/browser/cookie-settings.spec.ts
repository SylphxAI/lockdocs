import { expect, test, type Page } from "@playwright/test";
import { analyticsFrame, commands, noAnalytics, observeLocalizedAnalytics } from "./analytics-fixture";

const copy = [
  { locale: "en", path: "/pricing", heading: "Cookie settings", privacy: "Privacy policy",
    reject: "Reject optional", analytics: "Analytics only", ads: "Allow analytics & ads",
    gpc: "Global Privacy Control blocks optional cookies." },
  { locale: "zh-TW", path: "/zh-TW/pricing", heading: "Cookie 設定", privacy: "隱私權政策",
    reject: "拒絕選用項目", analytics: "僅限分析", ads: "允許分析與廣告",
    gpc: "全域隱私控制（GPC）已封鎖選用 Cookie。" },
] as const;

const choiceCookie = (page: Page) => page.context().cookies().then(cookies =>
  cookies.find(cookie => cookie.name === "__Host-lockdocs-signal-choice")?.value);

// Keep even newly introduced external destinations off the network. The
// existing Google fixture registers afterward and fulfills collector requests.
async function isolate(page: Page, baseURL: string) {
  const origin = new URL(baseURL).origin;
  await page.route("**/*", route => new URL(route.request().url()).origin === origin ? route.continue() : route.abort());
  return observeLocalizedAnalytics(page);
}

for (const { locale, path, heading, privacy, reject, analytics, ads, gpc } of copy) {
  test.describe(locale + " cookie settings", () => {
    test("rejects optional cookies and remembers the choice", async ({ page, baseURL }) => {
      const observed = await isolate(page, baseURL!);
      await page.goto(path);
      const panel = page.getByRole("region", { name: heading });
      const footer = page.getByRole("button", { name: heading, exact: true });
      await expect(panel).toBeVisible();
      await expect(panel.getByRole("link", { name: privacy })).toHaveAttribute("href", "/legal/privacy");
      expect((await panel.boundingBox())!.width).toBeLessThanOrEqual(page.viewportSize()!.width);
      for (const name of [reject, analytics, ads]) {
        const button = panel.getByRole("button", { name, exact: true });
        await expect(button).toBeInViewport({ ratio: 1 });
        expect((await button.boundingBox())!.height).toBeGreaterThanOrEqual(44);
      }
      expect(observed).toEqual(noAnalytics);
      await panel.getByRole("button", { name: reject, exact: true }).click();
      await expect(panel).toHaveCount(0);
      expect(await choiceCookie(page)).toBe("denied");
      await page.reload();
      await expect(footer).toBeEnabled();
      await expect(panel).toHaveCount(0);
      await expect(page.locator(analyticsFrame)).toHaveCount(0);
      expect(await choiceCookie(page)).toBe("denied");
      expect(observed).toEqual(noAnalytics);
    });

    test("remembers analytics-only consent without granting ads", async ({ page, baseURL }) => {
      const observed = await isolate(page, baseURL!);
      await page.goto(path);
      const panel = page.getByRole("region", { name: heading });
      const footer = page.getByRole("button", { name: heading, exact: true });
      await expect(panel).toBeVisible();
      expect(observed).toEqual(noAnalytics);
      await panel.getByRole("button", { name: analytics, exact: true }).click();
      await expect(panel).toHaveCount(0);
      const analyticsOnly = ["consent", "default", {
        analytics_storage: "granted", ad_storage: "denied", ad_user_data: "denied", ad_personalization: "denied",
      }];
      await expect.poll(() => commands(page)).toContainEqual(analyticsOnly);
      await expect.poll(() => observed.hits.length).toBe(1);
      expect(await choiceCookie(page)).toBe("analytics_only");
      await page.reload();
      await expect(footer).toBeEnabled();
      await expect(panel).toHaveCount(0);
      await expect.poll(() => commands(page)).toContainEqual(analyticsOnly);
      expect(await choiceCookie(page)).toBe("analytics_only");
      await expect.poll(() => observed.hits.length).toBe(2);
      await footer.click();
      await expect(panel).toBeFocused();
      await page.keyboard.press("Escape");
      await expect(panel).toHaveCount(0);
      await expect(footer).toBeFocused();
      expect(await choiceCookie(page)).toBe("analytics_only");
    });

    test("GPC refuses both optional grants even with a stored grant", async ({ page, baseURL }) => {
      const observed = await isolate(page, baseURL!);
      await page.setExtraHTTPHeaders({ "CF-IPCountry": "US", "Sec-GPC": "1" });
      await page.addInitScript(() => {
        document.cookie = "__Host-lockdocs-signal-choice=granted; Path=/; Secure; SameSite=Lax";
      });
      await page.goto(path);
      const panel = page.getByRole("region", { name: heading });
      await page.getByRole("button", { name: heading, exact: true }).click();
      await expect(panel.getByRole("status")).toHaveText(gpc);
      await expect(panel.getByRole("button", { name: reject, exact: true })).toBeEnabled();
      await expect(panel.getByRole("button", { name: analytics, exact: true })).toBeDisabled();
      await expect(panel.getByRole("button", { name: ads, exact: true })).toBeDisabled();
      await expect(page.locator(analyticsFrame)).toHaveCount(0);
      expect(observed).toEqual(noAnalytics);
    });
  });
}
