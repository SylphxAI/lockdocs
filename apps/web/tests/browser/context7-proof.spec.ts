import { expect, test } from "@playwright/test";
import { upgrades } from "../../src/lib/upgrade-examples";
import { noAnalytics, observeLocalizedAnalytics, settings } from "./analytics-fixture";

const path = "/context7-alternative";
const title = "VERSION-SENSITIVE PROOF";
const campaign = "utm_source=newsletter&utm_campaign=Docs+%2B+MCP";
const deepLink = (id: string) => "/playground?example=" + id + "&" + campaign + "#upgrade";

test.describe("comparison without JavaScript", () => {
  test.use({ javaScriptEnabled: false });
  test("Next.js proof and playground link render on the server", async ({ page }) => {
    await page.goto(path);
    const proof = page.getByRole("complementary", { name: title });
    await expect(proof.getByRole("button", { pressed: true })).toHaveText("Next.js");
    await expect(proof.locator("dl:visible dd code")).toHaveText([upgrades.next.before, upgrades.next.after]);
    await expect(proof.locator("dl:visible dd span.version-proof-citation")).toHaveText([upgrades.next.oldSource, upgrades.next.newSource]);
    await expect(proof.getByRole("link")).toHaveAttribute("href", "/playground?example=next#upgrade");
  });
});

test("comparison proof stays in place and carries the selected upgrade into the Playground", async ({ page, isMobile }, info) => {
  const analytics = await observeLocalizedAnalytics(page);
  const signals: string[] = [];
  const errors: string[] = [];
  page.on("pageerror", error => errors.push(error.message));
  page.on("console", message => { if (message.type() === "error") errors.push(message.text()); });
  await page.exposeFunction("recordProofViolation", (directive: string) => { errors.push(directive); });
  await page.addInitScript(() => {
    document.addEventListener("securitypolicyviolation", event => {
      void (window as unknown as { recordProofViolation: (directive: string) => Promise<void> }).recordProofViolation(event.violatedDirective);
    });
  });
  await page.route("**/api/analytics/config", async route => {
    const response = await route.fetch();
    await route.fulfill({ response, json: { ...await response.json(), signalKey: "sylphx_pk_signal_fixture",
      signalApiOrigin: "https://api.sylphx.com" } });
  });
  await page.route("**/v1/analytics/events:batch", route => {
    signals.push(route.request().method());
    return route.fulfill({ status: 204, headers: { "Access-Control-Allow-Origin": "*",
      "Access-Control-Allow-Headers": "content-type,x-sylphx-key" } });
  });

  const response = await page.goto(path + "?" + campaign);
  const policy = response?.headers()["content-security-policy"] ?? "";
  expect(policy).toMatch(/style-src [^;]*'nonce-[^']+'/);
  expect(policy).not.toMatch(/'unsafe-inline'|'unsafe-eval'/);
  await expect(settings(page)).toBeVisible();
  await page.evaluate(() => document.fonts.ready);
  const proof = page.getByRole("complementary", { name: title });
  const picker = proof.getByRole("group", { name: "Upgrade ecosystem" });
  const link = proof.getByRole("link");
  const geometry = () => proof.evaluate(el => [el, el.querySelector("a")!, document.querySelector("#lookup-title")!]
    .map(node => { const rect = node.getBoundingClientRect(); return [rect.top + scrollY, rect.height]; }));
  const assertNoOverflow = async () => expect(await page.evaluate(() => [document.documentElement,
    ...document.querySelectorAll(".version-proof, .version-proof-switcher, .version-proof-call, .upgrade-card, .studio-grid")]
    .filter(el => el.scrollWidth > el.clientWidth).map(el => el.className))).toEqual([]);
  const initialGeometry = await geometry();
  await expect(picker.getByRole("button")).toHaveCount(4);
  await expect(picker.getByRole("button", { pressed: true })).toHaveText("Next.js");
  for (const id of ["pydantic", "react", "next", "zod"] as const) {
    const sample = upgrades[id];
    const button = picker.getByRole("button", { name: sample.name, exact: true });
    if (isMobile) await button.tap();
    else await button.press(id === "pydantic" ? "Enter" : "Space");
    await expect(picker.getByRole("button", { pressed: true })).toHaveText(sample.name);
    await expect(proof.locator("dl:visible dd code")).toHaveText([sample.before, sample.after]);
    await expect(proof.locator("dl:visible .version-proof-citation")).toHaveText([sample.oldSource, sample.newSource]);
    await expect(link).toHaveText("Inspect " + sample.name + " in playground");
    await expect(link).toHaveAttribute("href", deepLink(id));
    expect(await geometry()).toEqual(initialGeometry);
    await assertNoOverflow();
  }
  await expect(proof.locator("[style]")).toHaveCount(0);
  if (process.env.LOCKDOCS_LOCAL_CAPTURES) await proof.screenshot({ path: info.outputPath("zod-proof.png") });
  await link.click();
  await expect(page).toHaveURL(new URL(deepLink("zod"), response!.url()).href);
  await expect(page.getByLabel("Lockfile format")).toHaveValue("yarn.lock");
  await expect(page.getByRole("heading", { name: "zod@3.23.8", exact: true })).toBeVisible();
  const report = page.locator("#upgrade");
  await expect(report.getByRole("button", { pressed: true })).toHaveText(upgrades.zod.label);
  await expect(report.getByRole("heading", { name: upgrades.zod.change[0] })).toBeVisible();
  await assertNoOverflow();
  expect({ ...analytics, signals, errors }).toEqual({ ...noAnalytics, signals: [], errors: [] });
});
