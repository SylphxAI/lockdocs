import { expect } from "@playwright/test";
import { test } from "./auth-fixture";

test.use({ billing: true });

test("the pricing form posts one browser intent per checkout attempt", async ({ page, auth }) => {
  const submits: string[][] = [];
  await page.route(auth.origin + "/api/billing/checkout", async route => {
    submits.push(new URLSearchParams(route.request().postData() ?? "").getAll("checkout_intent"));
    await route.fulfill({ status: 204 });
  });
  await page.goto(auth.origin + "/sign-in?next=%2Fpricing%3Fcurrency%3DGBP");
  await page.getByRole("link", { name: /Continue with Google/ }).click();
  await expect(page).toHaveURL(auth.origin + "/pricing?currency=GBP");
  const form = page.locator('form[action="/api/billing/checkout"]');
  // Server-rendered disabled, so a no-JS submit sends no intent and checks out as before.
  await expect(form.locator('input[name="checkout_intent"]')).toBeDisabled();
  const submit = async () => {
    const count = submits.length;
    await form.getByRole("button").click();
    await expect.poll(() => submits.length).toBe(count + 1);
    const intents = submits.at(-1)!;
    expect(intents).toHaveLength(1);
    expect(intents[0]).toMatch(/^[a-f0-9]{64}$/);
    return intents[0];
  };

  const attempt = await submit();
  expect(await submit()).toBe(attempt);
  await page.goto(auth.origin + "/pricing?currency=GBP&billing=unavailable");
  expect(await submit()).toBe(attempt);
  await page.goto(auth.origin + "/pricing?currency=USD&billing=unavailable");
  const usd = await submit();
  expect(usd).not.toBe(attempt);
  await page.goto(auth.origin + "/pricing?currency=USD");
  expect(await submit()).not.toBe(usd);
});
