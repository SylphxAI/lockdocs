import { expect, type Page } from "@playwright/test";
import { test, code, email } from "./auth-fixture";

function consoleErrors(page: Page) {
  const errors: string[] = [];
  page.on("pageerror", error => errors.push(error.message));
  page.on("console", message => {
    if (message.type() === "error" || /hydrat/i.test(message.text())) errors.push(message.text());
  });
  return errors;
}

const emailField = (page: Page) => page.getByLabel("Email address");
async function enterEmail(page: Page, value = email) {
  await emailField(page).fill(value);
  await page.getByRole("button", { name: "Continue", exact: true }).click();
}

async function addPasskey(page: Page, credential: {
  credentialId: string; isResidentCredential: boolean; rpId: string; privateKey: string; userHandle: string; signCount: number;
}) {
  const cdp = await page.context().newCDPSession(page);
  await cdp.send("WebAuthn.enable");
  const { authenticatorId } = await cdp.send("WebAuthn.addVirtualAuthenticator", { options: {
    protocol: "ctap2", transport: "internal", hasResidentKey: true, hasUserVerification: true,
    isUserVerified: true, automaticPresenceSimulation: true,
  } });
  await cdp.send("WebAuthn.addCredential", { authenticatorId, credential });
  return { cdp, authenticatorId };
}

async function observeMediation(page: Page) {
  await page.addInitScript(() => {
    const get = CredentialsContainer.prototype.get;
    CredentialsContainer.prototype.get = function (options) {
      const node = document.documentElement;
      node.dataset.mediation = (node.dataset.mediation ?? "") + options?.mediation + ",";
      sessionStorage.setItem("last-mediation", options?.mediation ?? "");
      return get.call(this, options);
    };
  });
}

test("Google and last-used work when it is the only enabled passwordless method", async ({ page, auth }, info) => {
  const errors = consoleErrors(page);
  await page.goto(auth.origin + "/pricing?currency=GBP");
  await expect(page.locator(".plan-pro")).toContainText("£99/yr upfront · Tax included");
  expect(auth.calls).toEqual([]);
  await page.getByRole("link", { name: "Sign in", exact: true }).click();
  await expect(page).toHaveURL(auth.origin + "/sign-in?next=%2Fpricing%3Fcurrency%3DGBP");
  await expect(page.getByRole("heading", { name: "Sign in to Lockdocs" })).toBeVisible();
  const google = page.getByRole("link", { name: /Continue with Google/ });
  await expect(page.locator(".auth-providers a").first()).toContainText("Continue with Google");
  await expect(google).toHaveCSS("background-color", "rgb(18, 25, 35)");
  await expect(google).toHaveCSS("border-top-color", "rgb(90, 106, 127)");
  await expect(emailField(page)).toHaveCount(0);
  await expect(page.locator("input[type=password], input[autocomplete=new-password]")).toHaveCount(0);
  await expect(page.locator("main")).not.toContainText(/Create an account|passkey|email code/i);
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(page.viewportSize()!.width);
  await page.screenshot({ path: info.outputPath("sign-in-email.png") });
  await google.click();
  await expect(page).toHaveURL(auth.origin + "/pricing?currency=GBP");
  const googleSession = await page.request.get(auth.origin + "/api/auth/session");
  expect((await googleSession.json()).user.id).toBe("usr_reader");
  await page.goto(auth.origin + "/dashboard");
  await page.getByRole("button", { name: "Sign out", exact: true }).click();
  await expect(page.getByRole("link", { name: "Sign in", exact: true })).toBeVisible();
  await page.goto(auth.origin + "/sign-in");
  await expect(google).toContainText("Last used");
  expect(auth.calls.filter(path => /\/continue|\/passkey/.test(path))).toEqual([]);
  expect(auth.rejections).toEqual([]);
  expect(errors).toEqual([]);
});

test("a runtime flag flip enables OTP, inline six-digit paste and auto-submit, and a 30-second resend cooldown", async ({ page, auth }, info) => {
  const errors = consoleErrors(page);
  await page.clock.install();
  await page.goto(auth.origin + "/sign-in");
  await expect(page.getByRole("link", { name: /Continue with Google/ })).toBeVisible();
  await expect(emailField(page)).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Continue with a passkey" })).toHaveCount(0);
  Object.assign(auth.methods, { email_otp: true, passkey: true, password: false });
  await page.reload();
  await expect(emailField(page)).toHaveAttribute("autocomplete", "username webauthn");
  await expect(page.getByRole("button", { name: "Continue with a passkey" })).toBeVisible();
  const firstResponse = page.waitForResponse(response => response.url().endsWith("/api/auth/sign-in"));
  await enterEmail(page, "new-reader@example.test");
  const first = await firstResponse;
  expect(first.status()).toBe(200);
  expect(await first.json()).toEqual({ step: "email_otp", expiresIn: 600 });
  await expect(page.getByText("n•••@example.test")).toBeVisible();
  await expect(page).toHaveURL(auth.origin + "/sign-in");
  await expect(page.getByRole("button", { name: "Use password instead" })).toHaveCount(0);
  const input = page.getByLabel("Six-digit code");
  await expect(input).toHaveAttribute("autocomplete", "one-time-code");
  await expect(input).toBeFocused();
  const resend = page.getByRole("button", { name: /Resend/ });
  await expect(resend).toHaveText("Resend in 30s");
  await expect(resend).toBeDisabled();
  await page.clock.fastForward(28_500);
  await expect(resend).toBeDisabled();
  expect(auth.shipments).toEqual(["new-reader@example.test"]);
  await page.clock.fastForward(2_000);
  await expect(resend).toBeEnabled();
  await resend.click();
  await expect(resend).toBeDisabled();
  await expect(input).toBeFocused();
  await expect.poll(() => auth.shipments).toEqual(["new-reader@example.test", "new-reader@example.test"]);
  await page.screenshot({ path: info.outputPath("sign-in-code.png") });
  await input.pressSequentially(code.slice(0, 5));
  expect(auth.calls).not.toContain("/v1/client/continue/verify");
  // Paste the entire code over the partial input, including the leading zero.
  await page.context().grantPermissions(["clipboard-read", "clipboard-write"], { origin: auth.origin });
  await page.evaluate(value => navigator.clipboard.writeText(value), code);
  await input.press("Control+V");
  await expect(page).toHaveURL(auth.origin + "/dashboard");
  expect((await (await page.request.get(auth.origin + "/api/auth/session")).json()).user.id).toBe("usr_new_reader");
  const cookie = (await page.context().cookies()).find(item => item.name === "__Host-lockdocs-session");
  expect(cookie).toMatchObject({ secure: true, httpOnly: true, sameSite: "Lax", path: "/" });
  expect(auth.calls).toContain("/v1/client/continue/verify");
  expect(auth.rejections).toEqual([]);
  expect(errors).toEqual([]);
});

test("conditional autofill and the Step-2 picker use a real Chromium virtual authenticator", async ({ page, auth }) => {
  Object.assign(auth.methods, { passkey: true, email_otp: true });
  const errors = consoleErrors(page);
  await observeMediation(page);
  // No credential at first: conditional mediation must not block email or trigger a send.
  const { cdp, authenticatorId } = await addPasskey(page, auth.passkey);
  await cdp.send("WebAuthn.clearCredentials", { authenticatorId });
  await page.goto(auth.origin + "/sign-in");
  await expect(page.locator("html")).toHaveAttribute("data-mediation", "conditional,");
  expect(auth.shipments).toEqual([]);
  await enterEmail(page);
  await expect(page.getByLabel("Six-digit code")).toBeVisible();
  await cdp.send("WebAuthn.addCredential", { authenticatorId, credential: auth.passkey });
  await page.getByRole("button", { name: "Continue with a passkey" }).click();
  await expect(page).toHaveURL(auth.origin + "/dashboard");
  expect((await (await page.request.get(auth.origin + "/api/auth/session")).json()).user.id).toBe("usr_reader");
  expect(await page.evaluate(() => sessionStorage.getItem("last-mediation"))).toBe("required");
  expect(auth.calls).toContain("/v1/client/sign-in/passkey/verify");
  expect(auth.rejections).toEqual([]);
  expect(errors).toEqual([]);
});

test("all five aliases redirect privately and retain only validated return paths", async ({ request, auth }) => {
  for (const alias of ["login", "signup", "sign-up", "register", "forgot-password"]) {
    const response = await request.get(auth.origin + "/" + alias + "?next=%2Fpricing", { maxRedirects: 0 });
    expect(response.status(), alias).toBe(308);
    expect(response.headers()["cache-control"], alias).toBe("private, no-store");
    expect(response.headers().location, alias).toBe("/sign-in?next=%2Fpricing");
  }
  const unsafe = await request.get(auth.origin + "/register?next=%2F%2Fexample.test", { maxRedirects: 0 });
  expect(unsafe.headers().location).toBe("/sign-in?next=%2Fdashboard");
  const checkoutReturn = await request.get(auth.origin + "/api/auth/sign-in?next=%2Fpricing", { maxRedirects: 0 });
  expect(checkoutReturn.headers().location).toBe(auth.origin + "/sign-in?next=%2Fpricing");
  expect(auth.calls).toEqual([]);
});

test("a resident passkey can finish through conditional autofill without first entering an email", async ({ page, auth }) => {
  auth.methods.passkey = true;
  const errors = consoleErrors(page);
  await observeMediation(page);
  await addPasskey(page, auth.passkey);
  await page.goto(auth.origin + "/sign-in");
  await expect(page).toHaveURL(auth.origin + "/dashboard");
  expect((await (await page.request.get(auth.origin + "/api/auth/session")).json()).user.id).toBe("usr_reader");
  expect(await page.evaluate(() => sessionStorage.getItem("last-mediation"))).toBe("conditional");
  expect(auth.calls.filter(path => path.endsWith("/passkey/begin"))).toHaveLength(1);
  expect(auth.shipments).toEqual([]);
  expect(auth.rejections).toEqual([]);
  expect(errors).toEqual([]);
});
