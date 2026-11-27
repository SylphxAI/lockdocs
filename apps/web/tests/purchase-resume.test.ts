import assert from "node:assert/strict";
import { test } from "node:test";
import { pricingPath, validateNext } from "../src/lib/auth-entry";
import { annualPrice } from "../src/lib/pro-plan";
import { purchaseResume } from "../src/lib/purchase-resume";

test("describes the pricing return with the catalogue price", () => {
  assert.deepEqual(purchaseResume("/pricing?currency=GBP", "en"), { currency: "GBP", price: "£99", path: "/pricing", query: "currency=GBP" });
  assert.deepEqual(purchaseResume("/zh-TW/pricing?currency=HKD&entry_surface=context7_alternative", "zh-TW"), { currency: "HKD", price: "HK$938", path: "/zh-TW/pricing", query: "currency=HKD&entry_surface=context7_alternative" });
  assert.deepEqual(purchaseResume("/pricing", "en"), { currency: "USD", price: "US$120", path: "/pricing", query: "" });
});

test("the sign-in summary and pricing render the same amount for each currency and locale", () => {
  const expected = { USD: "US$120", GBP: "£99", EUR: "€109", HKD: "HK$938" } as const;
  for (const currency of ["USD", "GBP", "EUR", "HKD"] as const) {
    for (const locale of ["en", "zh-TW"] as const) {
      const next = validateNext(pricingPath(currency, undefined, locale));
      const summary = purchaseResume(next, locale);
      assert.equal(summary?.currency, currency, next);
      assert.equal(summary?.price, expected[currency], next);
      assert.equal(summary?.price, annualPrice(currency, locale), next);
    }
  }
});

test("shows no purchase for other, unsafe or ambiguous return targets", () => {
  for (const next of [
    "/dashboard", "/cli/authorize?port=1&state=a&challenge=b", "/pricing/", "/pricingx", "//example.invalid/pricing",
    "https://example.invalid/pricing", "/pricing?currency=ZZZ", "/pricing?currency=usd", "/pricing?currency=USD&currency=HKD",
    "/pricing?amount=1", "/pricing?currency=GBP&price=1", "/pricing?entry_surface=injected", "/pricing?currency=G%42P",
    "/pricing#plans", "/zh-Hant/pricing", "/sign-in?next=/pricing", "/api/billing/checkout",
  ]) assert.equal(purchaseResume(next, "en"), undefined, next);
});
