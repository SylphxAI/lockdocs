import { expect, test as base, type Page } from "@playwright/test";
import { test as authTest } from "./auth-fixture";
import { analyticsFrame, install, settings } from "./analytics-fixture";

const endpoint = "/v1/analytics/events:batch";
const campaign = {
  utm_id: "spring-id", utm_source: "newsletter", utm_medium: "email", utm_campaign: "Docs launch",
  utm_term: "lockfile", utm_content: "hero", utm_source_platform: "web", utm_creative_format: "text", utm_marketing_tactic: "acquisition",
};
const clicks = { gclid: "google-click", gbraid: "google-braid", wbraid: "web-braid", fbclid: "meta-click" };
const query = new URLSearchParams({ ...campaign, ...clicks, token: "private-query-fixture" });
type Row = { event_id: string; event_name: string; distinct_id: string; session_id: string; properties: Record<string, unknown> };

async function intercept(page: Page, origin: string) {
  const requests: string[] = [];
  const batches: { consent: string; events: Row[]; headers: Record<string, string> }[] = [];
  await page.route(origin + endpoint, async route => {
    requests.push(route.request().method());
    if (route.request().method() === "POST") {
      batches.push({ ...route.request().postDataJSON(), headers: await route.request().allHeaders() });
    }
    await route.fulfill({ status: 202, json: { accepted_events: 1 }, headers: {
      "Access-Control-Allow-Origin": "*", "Access-Control-Allow-Headers": "content-type,x-sylphx-key",
    } });
  });
  return { batches, requests, rows: (event: string) => batches.flatMap(batch => batch.events).filter(row => row.event_name === event) };
}

async function signalOnly(page: Page) {
  await page.setExtraHTTPHeaders({ "CF-IPCountry": "DE" });
  await page.route("**/api/analytics/config", async route => {
    const response = await route.fetch();
    await route.fulfill({ response, json: { ...await response.json(), measurementId: null,
      signalKey: "sylphx_pk_signal_fixture", signalApiOrigin: "https://api.sylphx.com" } });
  });
  return intercept(page, "https://api.sylphx.com");
}

base("Signal-only grant keeps nine UTMs, strips click IDs and emits 1/1/1 SPA views", async ({ page, context }) => {
  await context.grantPermissions(["clipboard-read", "clipboard-write"]);
  const { batches, rows } = await signalOnly(page);
  let documents = 0;
  page.on("request", request => {
    if (request.isNavigationRequest() && request.frame() === page.mainFrame()) documents++;
  });
  await page.goto("/?" + query + "#private-fragment");
  await expect(settings(page)).toBeVisible();
  expect(batches).toEqual([]);
  expect(await page.evaluate(() => localStorage.getItem("lockdocs.signal.v1.touch"))).toBeNull();
  await settings(page).getByRole("button", { name: "Analytics only", exact: true }).click();
  await expect.poll(() => rows("page_view").map(row => row.properties)).toEqual([{ path: "/", ...campaign }]);
  await expect(page.locator(analyticsFrame)).toHaveCount(0);
  await page.getByRole("button", { name: "Copy install command" }).click();
  await expect.poll(() => rows("command_copied").map(row => row.properties)).toEqual([
    { ...campaign, surface: "home_install", command_kind: "npx" },
  ]);

  await page.getByRole("navigation", { name: "Main navigation" }).getByRole("link", { name: "Pricing", exact: true }).click();
  await expect(page).toHaveURL(url => url.pathname === "/pricing");
  await page.getByRole("navigation", { name: "Billing currency" }).getByRole("link", { name: "EUR", exact: true }).click();
  await expect(page).toHaveURL(url => url.searchParams.get("currency") === "EUR");
  await page.locator("header .wordmark").click();
  await expect.poll(() => rows("page_view").map(row => row.properties)).toEqual([
    { path: "/", ...campaign }, { path: "/pricing", ...campaign }, { path: "/", ...campaign },
  ]);
  const pricing = () => rows("pricing_view");
  expect(pricing().map(row => row.properties)).toEqual([{ path: "/pricing", entry_surface: "pricing", ...campaign }]);
  expect(documents).toBe(1);
  expect(new Set(rows("page_view").map(row => row.session_id)).size).toBe(1);
  expect(new Set(rows("page_view").map(row => row.event_id)).size).toBe(3);
  expect(JSON.stringify(batches.map(batch => batch.events))).not.toMatch(/private-query|private-fragment|gclid|gbraid|wbraid|fbclid/);
  for (const batch of batches) {
    expect(batch.consent).toBe("granted");
    expect(batch.headers).toMatchObject({ "x-sylphx-key": "sylphx_pk_signal_fixture" });
    for (const header of ["authorization", "cookie", "referer"]) expect(batch.headers[header]).toBeUndefined();
  }

  await page.getByRole("navigation", { name: "Main navigation" }).getByRole("link", { name: "Pricing", exact: true }).click();
  await expect.poll(() => pricing().length).toBe(2);
  expect(pricing()[0].event_id).not.toBe(pricing()[1].event_id);
  await page.locator("header .wordmark").click();
  await expect.poll(() => rows("page_view").length).toBe(5);
  await page.getByRole("button", { name: "Cookie settings", exact: true }).click();
  await settings(page).getByRole("button", { name: "Reject optional", exact: true }).click();
  await expect.poll(() => page.evaluate(() => localStorage.getItem("lockdocs.signal.v1.touch"))).toBeNull();
  const count = batches.length;
  await page.getByRole("navigation", { name: "Main navigation" }).getByRole("link", { name: "Pricing", exact: true }).click();
  await expect(page).toHaveURL(url => url.pathname === "/pricing");
  expect(batches).toHaveLength(count);
});

for (const [path, entry_surface] of [
  ["/compare", "article_compare"], ["/benchmarks", "article_benchmarks"],
  ["/guide", "article_guide"], ["/pro", "article_pro"],
  ["/guide/setup", "article_guide_setup"], ["/guide/ecosystems", "article_guide_ecosystems"],
  ["/guide/fetch", "article_guide_fetch"], ["/guide/how-it-works", "article_guide_how_it_works"],
  ["/reference", "article_reference"], ["/reference/tools", "article_reference_tools"],
  ["/capabilities", "article_capabilities"], ["/docs", "article_docs"], ["/zh-TW/docs", "article_docs_zh_tw"],
] as const) {
  base("article Pro click emits once with analytics consent: " + path, async ({ page }) => {
    const { rows } = await signalOnly(page);
    await page.goto(path + "?" + query);
    const zh = path === "/zh-TW/docs";
    const panel = zh ? page.getByRole("region", { name: "Cookie 設定" }) : settings(page);
    await panel.getByRole("button", { name: zh ? "僅限分析" : "Analytics only", exact: true }).click();
    await expect.poll(() => rows("page_view")).toHaveLength(1);
    const card = page.getByRole("region", { name: zh ? "用 lockfile 試試看" : "Try it with a lockfile" });
    await card.getByRole("link", { name: zh ? /查看 Lockdocs Pro/ : /See Lockdocs Pro/ }).click();
    await expect(page).toHaveURL(url => url.pathname === (zh ? "/zh-TW/pricing" : "/pricing"));
    expect(Object.fromEntries(new URL(page.url()).searchParams)).toEqual({ ...campaign,
      gclid: clicks.gclid, gbraid: clicks.gbraid, wbraid: clicks.wbraid, entry_surface,
    });
    await expect.poll(() => rows("pro_cta_clicked").map(row => row.properties)).toEqual([{ ...campaign, entry_surface }]);
  });
}

for (const [path, zh] of [["/benchmarks", false], ["/compare", false], ["/zh-TW/docs", true]] as const) {
  base("article example links preserve campaign without sending Signal: " + path, async ({ page }, info) => {
    const { requests } = await signalOnly(page);
    const errors: string[] = [];
    page.on("pageerror", error => errors.push(error.message));
    page.on("console", message => { if (message.type() === "error") errors.push(message.text()); });
    await page.addInitScript(() => {
      const violations: string[] = [];
      Object.assign(window, { articleCspViolations: violations });
      document.addEventListener("securitypolicyviolation", event => violations.push(event.violatedDirective));
    });

    const response = await page.goto(path + "?" + query);
    expect(response?.status()).toBe(200);
    const policy = response?.headers()["content-security-policy"] ?? "";
    expect(policy).toMatch(/(?:^|; )style-src [^;]*'nonce-[^']+'/);
    expect(policy).not.toMatch(/'unsafe-inline'|'unsafe-eval'/);
    await expect(zh ? page.getByRole("region", { name: "Cookie 設定" }) : settings(page)).toBeVisible();
    const bridge = page.getByRole("region", { name: zh ? "用 lockfile 試試看" : "Try it with a lockfile" });
    const examples = bridge.getByRole("navigation", { name: zh ? "升級範例" : "Upgrade examples" });
    const links = examples.getByRole("link");
    await expect(links).toHaveText(["Next.js", "Pydantic", "Zod", "React"]);
    const carried = new URLSearchParams({ gclid: clicks.gclid, gbraid: clicks.gbraid, wbraid: clicks.wbraid, ...campaign });
    const playground = zh ? "/zh-TW/playground" : "/playground";
    const destination = (example: string) => playground + "?example=" + example + "&" + carried + "#upgrade";
    for (const [index, example] of ["next", "pydantic", "zod", "react"].entries()) {
      await expect(links.nth(index)).toHaveAttribute("href", destination(example));
    }
    const expectNoOverflow = async () => expect(await page.evaluate(() => [document.documentElement,
      ...document.querySelectorAll(".article-bridge, .article-upgrade-links, .article-upgrade-row, #upgrade")]
      .filter(node => node.scrollWidth > node.clientWidth).map(node => node.className))).toEqual([]);
    await expectNoOverflow();
    await expect(bridge.locator("[style]")).toHaveCount(0);
    if (process.env.LOCKDOCS_LOCAL_CAPTURES) await bridge.screenshot({ path: info.outputPath("article-bridge.png") });

    await examples.getByRole("link", { name: "Pydantic", exact: true }).click();
    await expect(page).toHaveURL(new URL(destination("pydantic"), response!.url()).href);
    await expect(page.getByLabel(zh ? "lockfile 格式" : "Lockfile format")).toHaveValue("uv.lock");
    const report = page.locator("#upgrade");
    await expect(report.getByRole("button", { pressed: true })).toHaveText("Pydantic 1.10 → 2.9");
    await expect(report.getByRole("heading", { name: zh ? "Pydantic v2 改用 model_dump()" : "Use model_dump() in Pydantic v2" })).toBeVisible();
    await expectNoOverflow();
    expect({ requests, errors, csp: await page.evaluate(() => Reflect.get(window, "articleCspViolations")) })
      .toEqual({ requests: [], errors: [], csp: [] });
  });
}

for (const mode of ["pre-choice", "denied", "GPC"]) {
  base("article Pro navigation respects " + mode, async ({ page }) => {
    const { batches } = await signalOnly(page);
    if (mode === "GPC") await page.setExtraHTTPHeaders({ "CF-IPCountry": "US", "Sec-GPC": "1" });
    await page.goto("/pro?" + query);
    if (mode === "denied") await settings(page).getByRole("button", { name: "Reject optional", exact: true }).click();
    if (mode === "pre-choice") await expect(settings(page)).toBeVisible();
    const card = page.getByRole("region", { name: "Try it with a lockfile" });
    const pro = card.getByRole("link", { name: /See Lockdocs Pro/ });
    if (mode !== "pre-choice") {
      await expect(pro).toHaveAttribute("href", "/pricing?entry_surface=article_pro");
      await expect(card.getByRole("link", { name: "Try the playground" })).toHaveAttribute("href", "/playground");
    }
    await pro.click();
    await expect(page).toHaveURL(url => url.pathname === "/pricing" && (mode === "pre-choice" ? url.searchParams.has("utm_source") : url.search === "?entry_surface=article_pro"));
    await expect(page.getByRole("button", { name: "Cookie settings", exact: true })).toBeEnabled();
    expect(batches).toEqual([]);
    expect(await page.evaluate(() => localStorage.getItem("lockdocs.signal.v1.touch"))).toBeNull();
  });
}

base("localized pricing emits a sanitized pricing view only on grant", async ({ page }) => {
  const { batches, rows } = await signalOnly(page);
  await page.goto("/zh-TW/pricing?currency=HKD&token=fixture-private");
  const panel = page.getByRole("region", { name: "Cookie 設定" });
  await expect(panel).toBeVisible();
  expect(batches).toEqual([]);
  await panel.getByRole("button", { name: "僅限分析", exact: true }).click();
  await expect.poll(() => rows("pricing_view").map(row => row.properties)).toEqual([
    { path: "/zh-TW/pricing", entry_surface: "pricing" },
  ]);
  expect(JSON.stringify(batches)).not.toContain("fixture-private");
});

for (const [path, heading, entry_surface] of [
  ["/guides/up-to-date-docs-for-cursor-and-claude-code", "Look up your next API", "guide_cursor_claude_code"],
  ["/context7-alternative", "Try a version-sensitive lookup", "context7_alternative"],
] as const) {
  for (const mode of ["pre-consent", "Sec-GPC", "regional default", "analytics only"] as const) {
    base(heading + " Pro link: " + mode, async ({ page }, info) => {
      const { requests, rows } = await signalOnly(page);
      const gpc = mode === "Sec-GPC";
      const regionalDefault = mode === "regional default";
      if (regionalDefault || gpc) await page.setExtraHTTPHeaders({ "CF-IPCountry": "US", ...(gpc ? { "Sec-GPC": "1" } : {}) });
      if (gpc) await page.addInitScript(() => {
        document.cookie = "__Host-lockdocs-signal-choice=analytics_only; Path=/; Secure; SameSite=Lax";
      });
      const response = await page.goto(path + "?" + query);
      expect(response?.status()).toBe(200);
      await expect(page.getByRole("button", { name: "Cookie settings", exact: true })).toBeEnabled();
      if (mode === "pre-consent" || mode === "analytics only") {
        await expect(settings(page)).toBeVisible();
        expect(requests).toEqual([]);
      }
      if (mode === "analytics only") {
        await settings(page).getByRole("button", { name: "Analytics only", exact: true }).click();
      }
      if (mode === "analytics only" || regionalDefault) {
        await expect.poll(() => rows("page_view").map(row => row.properties.path)).toEqual([path]);
      }

      const band = page.getByRole("region", { name: heading, exact: true });
      const link = band.getByRole("link", { name: "See Lockdocs Pro", exact: true });
      await expect(link).toBeVisible();
      await page.evaluate(() => document.fonts.ready);
      expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(page.viewportSize()!.width);
      const expectedQuery = gpc ? { entry_surface } : { ...campaign, entry_surface, gclid: clicks.gclid, gbraid: clicks.gbraid, wbraid: clicks.wbraid };
      const href = new URL((await link.getAttribute("href"))!, page.url());
      expect(href.pathname).toBe("/pricing");
      expect(Object.fromEntries(href.searchParams)).toEqual(expectedQuery);
      if (process.env.LOCKDOCS_LOCAL_CAPTURES && mode === "analytics only") {
        await band.screenshot({ path: info.outputPath("closing-pro-link.png"), animations: "disabled" });
      }
      await link.click();
      await expect(page).toHaveURL(url => url.pathname === "/pricing");
      expect(Object.fromEntries(new URL(page.url()).searchParams)).toEqual(expectedQuery);
      if (mode === "analytics only" || regionalDefault) {
        await expect.poll(() => rows("pricing_view").map(row => row.properties.entry_surface)).toEqual([entry_surface]);
        await expect.poll(() => rows("pro_cta_clicked").map(row => row.properties)).toEqual(
          mode === "analytics only" ? [{ ...campaign, entry_surface }] : [],
        );
      } else {
        expect(requests).toEqual([]);
        await expect(page.locator(analyticsFrame)).toHaveCount(0);
        expect(await page.evaluate(() => localStorage.getItem("lockdocs.signal.v1.touch"))).toBeNull();
      }
    });
  }
}

for (const [path, heading, panelName, grant, labels, reportPath] of [
  ["/pricing", "What Pro shows you", "Cookie settings", "Analytics only",
    ["Try the report preview", "Read the private registry setup"], "/playground"],
  ["/zh-TW/pricing", "Pro 功能實例", "Cookie 設定", "僅限分析",
    ["互動預覽升級報告", "查看私有套件庫設定（英文）"], "/zh-TW/playground"],
] as const) {
  for (const mode of ["pre-consent", "Sec-GPC", "regional default", "analytics only"] as const) {
    base(path + " proof clicks: " + mode, async ({ page }) => {
      const { batches, requests, rows } = await signalOnly(page);
      const gpc = mode === "Sec-GPC";
      const regionalDefault = mode === "regional default";
      if (regionalDefault || gpc) await page.setExtraHTTPHeaders({ "CF-IPCountry": "US", ...(gpc ? { "Sec-GPC": "1" } : {}) });
      if (gpc) {
        await page.addInitScript(() => {
          document.cookie = "__Host-lockdocs-signal-choice=analytics_only; Path=/; Secure; SameSite=Lax";
        });
      }
      const panel = page.getByRole("region", { name: panelName });
      const settingsButton = page.getByRole("button", { name: panelName, exact: true });
      const proof = page.getByRole("region", { name: heading });
      const targets = [
        { pathname: reportPath, hash: "#upgrade", entry_surface: "pricing_proof_upgrade_report" },
        { pathname: "/pro", hash: "#private-sources", entry_surface: "pricing_proof_private_sources" },
      ];
      const expected = [];
      for (const [i, { pathname, hash, entry_surface }] of targets.entries()) {
        await page.goto(path + "?" + query);
        await expect(settingsButton).toBeEnabled();
        if (mode === "analytics only" && i === 0) {
          await expect(panel).toBeVisible();
          expect(batches).toEqual([]);
          await panel.getByRole("button", { name: grant, exact: true }).click();
        }
        if (mode === "analytics only" || regionalDefault) {
          await expect.poll(() => rows("pricing_view")).toHaveLength(i + 1);
        }
        if (gpc) {
          expect(await page.evaluate(() => document.cookie)).toContain("__Host-lockdocs-signal-choice=analytics_only");
          await settingsButton.click();
          await expect(panel.getByRole("button", { name: grant, exact: true })).toBeDisabled();
          await page.keyboard.press("Escape");
        }
        const landed = gpc ? {} : { ...campaign, gclid: clicks.gclid, gbraid: clicks.gbraid, wbraid: clicks.wbraid };
        const link = proof.getByRole("link", { name: labels[i], exact: true });
        await link.click();
        await expect(page).toHaveURL(url => url.pathname === pathname && url.hash === hash);
        expect(Object.fromEntries(new URL(page.url()).searchParams)).toEqual(i === 0 ? { ...landed, example: "next" } : landed);
        if (mode === "analytics only") {
          expected.push({ ...campaign, entry_surface });
          await expect.poll(() => rows("pro_cta_clicked").map(row => row.properties)).toEqual(expected);
          await expect.poll(() => rows("page_view").filter(row => row.properties.path === pathname)).toHaveLength(1);
        } else if (regionalDefault) {
          // Existing regional-default page views are unchanged, but a proof click requires a choice.
          await expect.poll(() => rows("page_view").filter(row => row.properties.path === pathname)).toHaveLength(1);
          expect(rows("pro_cta_clicked")).toEqual([]);
        } else {
          expect(requests).toEqual([]);
          expect(await page.evaluate(() => localStorage.getItem("lockdocs.signal.v1.touch"))).toBeNull();
        }
      }
      if (mode === "analytics only") {
        const clickBatches = batches.filter(batch => batch.events.some(row => row.event_name === "pro_cta_clicked"));
        expect(clickBatches.map(batch => [batch.consent, batch.events.length])).toEqual([["granted", 1], ["granted", 1]]);
        expect(new Set(rows("pro_cta_clicked").map(row => row.event_id)).size).toBe(2);
      }
    });
  }
}

base("Sec-GPC denies Signal even with an existing grant", async ({ page }) => {
  const { batches } = await signalOnly(page);
  await page.setExtraHTTPHeaders({ "CF-IPCountry": "US", "Sec-GPC": "1" });
  await page.addInitScript(() => {
    document.cookie = "__Host-lockdocs-signal-choice=granted; Path=/; Secure; SameSite=Lax";
  });
  await page.goto("/?" + query);
  await page.getByRole("button", { name: "Cookie settings", exact: true }).click();
  await expect(settings(page).getByRole("status")).toContainText("Global Privacy Control");
  await expect(settings(page).getByRole("button", { name: "Analytics only" })).toBeDisabled();
  await page.getByRole("navigation", { name: "Main navigation" }).getByRole("link", { name: "Pricing", exact: true }).click();
  await expect(page).toHaveURL(url => url.pathname === "/pricing");
  expect(batches).toEqual([]);
  expect(await page.evaluate(() => localStorage.getItem("lockdocs.signal.v1.touch"))).toBeNull();
});

authTest.use({ billing: true });
authTest("organic Pro CTA keeps its source and campaign through sign-in and checkout", async ({ page, auth }) => {
  await install(page, "DE");
  auth.ticketSignal.is_new_user = true;
  const { batches, rows } = await intercept(page, auth.origin);
  const source = "/context7-alternative";
  const entry_surface = "context7_alternative";
  const next = "/pricing?currency=GBP&entry_surface=" + entry_surface;
  await page.goto(auth.origin + source + "?" + query);
  await expect(settings(page)).toBeVisible();
  expect(batches).toEqual([]);
  await settings(page).getByRole("button", { name: "Allow analytics & ads", exact: true }).click();
  await expect.poll(() => rows("page_view").map(row => row.properties)).toEqual([{ path: source, ...campaign, ...clicks }]);
  await page.locator(".editorial-hero").getByRole("link", { name: "See Lockdocs Pro" }).click();
  await page.getByRole("link", { name: "GBP", exact: true }).click();
  await expect(page).toHaveURL(url => url.pathname === "/pricing" && url.searchParams.get("entry_surface") === entry_surface);
  expect(await page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth)).toBe(0);
  await page.getByRole("link", { name: /Get Lockdocs Pro/ }).click();
  await expect(page).toHaveURL(url => url.pathname === "/sign-in" && url.searchParams.get("next") === next &&
    url.searchParams.get("utm_campaign") === campaign.utm_campaign);
  await page.getByRole("link", { name: "Continue with Google" }).click();
  await expect(page).toHaveURL(auth.origin + next);
  const { gclid, gbraid, wbraid } = clicks;
  const carried = { ...campaign, gclid, gbraid, wbraid };
  await expect.poll(() => rows("sign_up").map(row => row.properties)).toEqual([
    { ...carried, method: "google", entry_surface: "pricing" },
  ]);
  // Cancel native submission twice to exercise the same logical begin_checkout
  // twice, then submit for real and observe the separate server-side milestone.
  await page.evaluate(() => {
    let remaining = 2;
    document.querySelector('form[action="/api/billing/checkout"]')!.addEventListener("submit", event => {
      if (remaining-- > 0) event.preventDefault();
    });
  });
  const button = page.getByRole("button", { name: "Continue to secure checkout" });
  await button.click();
  await button.click();
  await expect.poll(() => rows("begin_checkout").map(row => row.properties)).toEqual([
    { ...carried, entry_surface, plan_sku: "lockdocs_pro_seat_year", currency: "GBP", value: 99 },
  ]);
  await page.route("https://checkout.stripe.com/fixture", route => route.fulfill({ status: 200, body: "Fixture checkout" }));
  const posted = page.waitForRequest(request => new URL(request.url()).pathname === "/api/billing/checkout" && request.method() === "POST");
  await button.click();
  expect(new URLSearchParams((await posted).postData()!).getAll("entry_surface")).toEqual([entry_surface]);
  await expect(page).toHaveURL("https://checkout.stripe.com/fixture");
  expect(auth.checkouts).toMatchObject([{ currency_code: "GBP", metadata: { entry_surface }, cancel_url: auth.origin + next }]);
  expect(rows("begin_checkout")).toHaveLength(1);
  expect(JSON.stringify(batches.map(batch => batch.events))).not.toMatch(/private-query|reader@|usr_reader|auth_ticket|checkout_fixture/);
  await expect.poll(() => auth.signalBatches.flatMap(batch => batch.events)).toMatchObject([
    { event_name: "checkout_started", numeric_value: 99,
      properties: { entry_surface, plan_sku: "lockdocs_pro_seat_year", currency: "GBP" } },
  ]);
  await expect.poll(() => auth.bindings).toHaveLength(1);
  const binding = auth.bindings[0] as { gclid: string; gbraid: string; wbraid: string; fbc: string; page_location: string };
  expect(binding).toMatchObject({ gclid: clicks.gclid, gbraid: clicks.gbraid, wbraid: clicks.wbraid });
  expect(binding.fbc).toBeUndefined();
  expect(Object.fromEntries(new URL(binding.page_location).searchParams)).toEqual(campaign);
  expect(auth.rejections).toEqual([]);
});

for (const mode of ["pre-consent", "GPC"] as const) {
  authTest("Traditional Chinese organic checkout retains its source with zero analytics under " + mode, async ({ page, auth }) => {
    const hits = await install(page, "DE");
    const { requests } = await intercept(page, auth.origin);
    const gpc = mode === "GPC";
    if (gpc) await page.setExtraHTTPHeaders({ "CF-IPCountry": "DE", "Sec-GPC": "1" });
    const entry_surface = "article_docs_zh_tw";
    const next = "/zh-TW/pricing?currency=HKD&entry_surface=" + entry_surface;
    await page.goto(auth.origin + "/zh-TW/docs?" + query);
    const pro = page.getByRole("link", { name: /查看 Lockdocs Pro/ });
    await expect(pro).toHaveAttribute("href", gpc ? "/zh-TW/pricing?entry_surface=" + entry_surface : /utm_source=newsletter/);
    await pro.click();
    await page.getByRole("link", { name: "HKD", exact: true }).click();
    const destination = new URL(page.url());
    expect(destination.pathname + (gpc ? destination.search : "")).toBe(gpc ? next : "/zh-TW/pricing");
    expect(await page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth)).toBe(0);
    await page.getByRole("link", { name: /^購買 Lockdocs Pro/ }).click();
    await expect(page).toHaveURL(url => url.pathname === "/sign-in" && url.searchParams.get("next") === next &&
      url.searchParams.get("utm_source") === (gpc ? null : campaign.utm_source));
    await page.getByRole("link", { name: "Continue with Google" }).click();
    await expect(page).toHaveURL(auth.origin + next);

    for (const invalid of ["unknown", entry_surface + "&entry_surface=" + entry_surface]) {
      const response = await page.request.post(auth.origin + "/api/billing/checkout", {
        headers: { Origin: auth.origin, "Content-Type": "application/x-www-form-urlencoded" },
        data: "currency=HKD&entry_surface=" + invalid,
      });
      expect(response.status()).toBe(400);
      expect(await response.json()).toEqual({ error: "invalid_entry_surface" });
    }
    expect(auth.checkouts).toEqual([]);
    const form = page.locator('form[action="/api/billing/checkout"]');
    await expect(form.locator('input[name="entry_surface"]')).toHaveValue(entry_surface);
    await expect(page.locator(analyticsFrame)).toHaveCount(0);
    await page.route("https://checkout.stripe.com/fixture", route => route.fulfill({ status: 200, body: "Fixture checkout" }));
    const posted = page.waitForRequest(request => new URL(request.url()).pathname === "/api/billing/checkout" && request.method() === "POST");
    await form.getByRole("button").click();
    const data = new URLSearchParams((await posted).postData()!);
    expect(data.getAll("entry_surface")).toEqual([entry_surface]);
    expect(data.get("signal_blocked")).toBe("1");
    expect(data.has("signal_touch")).toBe(false);
    await expect(page).toHaveURL("https://checkout.stripe.com/fixture");
    expect(auth.checkouts).toMatchObject([{ currency_code: "HKD", metadata: { entry_surface } }]);
    expect([...requests, ...hits, ...auth.signalBatches, ...auth.rejections]).toEqual([]);
  });
}
