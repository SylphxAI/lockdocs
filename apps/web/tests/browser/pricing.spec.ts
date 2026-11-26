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

const proofCampaign = { utm_source: "newsletter", utm_medium: "email", utm_campaign: "Next + npm" };
const proofQuery = new URLSearchParams({ ...proofCampaign, currency: "EUR", unrelated: "do-not-carry" });
const proofLocales = [
  { prefix: "", heading: "What Pro shows you", FAQReport: "What does a Pro Upgrade Report show?", price: "€109 per year", privateHeading: "Private sources" },
  { prefix: "/zh-TW", heading: "Pro 功能實例", FAQReport: "Pro 升級報告會列出哪些資訊？", price: "年費為 €109", privateHeading: "Private sources" },
];

for (const { prefix, heading, FAQReport, price, privateHeading } of proofLocales) {
  const pricing = prefix + "/pricing";
  const preview = prefix + "/playground";
  test(pricing + " serves the two proof samples and six matching FAQ entries", async ({ page, request }) => {
    const response = await request.get(pricing + "?" + proofQuery);
    expect(response.status()).toBe(200);
    const html = await response.text();
    const schema = [...html.matchAll(/<script\b[^>]*type="application\/ld\+json"[^>]*>([\s\S]*?)<\/script>/g)]
      .map(([, json]) => JSON.parse(json))
      .filter(data => data["@type"] === "FAQPage");
    expect(schema).toHaveLength(1);
    expect(schema[0]["@id"]).toBe("https://lockdocs.sylphx.com" + pricing + "#faq");
    expect(schema[0].mainEntity).toHaveLength(6);
    expect(schema[0].mainEntity[0].acceptedAnswer.text).toContain(price);

    await page.goto(pricing + "?" + proofQuery);
    const proof = page.getByRole("region", { name: heading });
    await expect(proof.getByRole("heading", { level: 3 })).toHaveCount(2);
    await expect(proof).toContainText("app/account/page.tsx:8");
    await expect(proof).toContainText('const token = cookies().get("session")');
    await expect(proof).toContainText('const token = (await cookies()).get("session")');
    await expect(proof).toContainText("next@14.2.3 dist/client/components/headers.d.ts:13");
    await expect(proof).toContainText("next@15.0.0 dist/server/request/cookies.d.ts:24");
    await expect(proof).toContainText("lockdocs fetch @acme/session");
    await expect(proof).toContainText('lockdocs docs "session renewal" --pkg @acme/session --offline');

    const questions = page.locator("#faq details");
    await expect(questions).toHaveCount(6);
    await expect(questions.locator("summary")).toHaveText(schema[0].mainEntity.map((entry: { name: string }) => entry.name));
    await expect(questions.locator("p")).toHaveText(schema[0].mainEntity.map((entry: { acceptedAnswer: { text: string } }) => entry.acceptedAnswer.text));
    await page.getByText(FAQReport, { exact: true }).click();
    await expect(questions.nth(4)).toHaveAttribute("open", "");
    const overflow = await page.evaluate(() => {
      const root = document.documentElement;
      const region = document.querySelector<HTMLElement>(".pricing-proof")!;
      return [root.scrollWidth - root.clientWidth, region.scrollWidth - region.clientWidth];
    });
    expect(overflow).toEqual([0, 0]);
  });

  test(pricing + " carries the selected example and only approved campaign fields", async ({ page }) => {
    await page.setExtraHTTPHeaders({ "CF-IPCountry": "DE" });
    await page.goto(pricing + "?" + proofQuery);
    const proof = page.getByRole("region", { name: heading });
    const reportFaq = page.locator("#faq details").filter({ hasText: FAQReport });
    await reportFaq.locator("summary").click();
    const links = [
      { link: proof.getByRole("link").nth(0), path: preview, hash: "#upgrade", example: "next" },
      { link: proof.getByRole("link").nth(1), path: "/pro", hash: "#private-sources" },
      { link: reportFaq.getByRole("link").nth(0), path: preview, hash: "#upgrade" },
      { link: reportFaq.getByRole("link").nth(1), path: "/pro", hash: "#upgrade-report" },
    ];
    for (const { link, path, hash, example } of links) {
      const target = new URL((await link.getAttribute("href"))!, page.url());
      expect(target.pathname).toBe(path);
      expect(target.hash).toBe(hash);
      expect(Object.fromEntries(target.searchParams)).toEqual(example ? { ...proofCampaign, example } : proofCampaign);
    }
    expect(await page.context().cookies()).toEqual([]);
    expect(await page.evaluate(() => [localStorage.length, sessionStorage.length])).toEqual([0, 0]);

    await links[0].link.click();
    await expect(page).toHaveURL(url => url.pathname === preview && url.searchParams.get("example") === "next" && url.hash === "#upgrade");
    await expect(page.locator("#upgrade .sample-diff")).toContainText("next@15.0.0 dist/server/request/cookies.d.ts:24");
    await page.goto(pricing + "?" + proofQuery);
    await links[1].link.click();
    await expect(page).toHaveURL(url => url.pathname === "/pro" && url.hash === "#private-sources");
    await expect(page.getByRole("heading", { name: privateHeading, exact: true })).toBeVisible();
  });

  test(pricing + " does not carry campaign data under Global Privacy Control", async ({ page }) => {
    await page.setExtraHTTPHeaders({ "Sec-GPC": "1" });
    await page.goto(pricing + "?" + proofQuery);
    const proof = page.getByRole("region", { name: heading });
    await expect(proof.getByRole("link").nth(0)).toHaveAttribute("href", preview + "?example=next#upgrade");
    await expect(proof.getByRole("link").nth(1)).toHaveAttribute("href", "/pro#private-sources");
  });
}
