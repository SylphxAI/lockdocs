import { expect, test, type Page } from "@playwright/test";

const latinFace = /\/jetbrains-mono-latin-wght-normal-[^/]+\.woff2$/;
const mixedLockfile = "# café Český Tiếng Việt Ελληνικά Интерфейс Ѧ 繁體中文 🔒\npydantic==1.10.18";

async function monoState(page: Page) {
  return page.evaluate(async () => {
    await document.fonts.ready;
    const family = "JetBrains Mono Variable";
    const mono = [...document.querySelectorAll<HTMLElement>("body *")].filter(el =>
      el.getClientRects().length && getComputedStyle(el).fontFamily.includes(family) &&
      (el instanceof HTMLTextAreaElement ? el.value : el.textContent)?.trim());
    return {
      count: mono.length,
      light: mono.filter(el => Number(getComputedStyle(el).fontWeight) < 400)
        .map(el => el.tagName + (el.id ? "#" + el.id : "")),
      overflow: document.documentElement.scrollWidth - document.documentElement.clientWidth,
      loaded: [...document.fonts].filter(face => face.family.replaceAll('"', "") === family && face.status === "loaded").length,
    };
  });
}

for (const prefix of ["", "/zh-TW"]) {
  test("mono budget and multilingual routing on " + (prefix || "/"), async ({ page }) => {
    const fonts: string[] = [];
    page.on("request", request => {
      if (request.resourceType() === "font") fonts.push(new URL(request.url()).pathname);
    });
    const verifyMono = async () => {
      const state = await monoState(page);
      expect(state.count).toBeGreaterThan(1);
      expect(state.light, "Latin face only has axis data for wght 400–800").toEqual([]);
      expect(state.overflow).toBe(0);
      return state;
    };

    await page.goto(prefix || "/");
    await expect(page.locator("header .wordmark")).toHaveCSS("font-weight", "700");
    expect((await verifyMono()).loaded).toBe(1);
    const faces = await page.evaluate(() => performance.getEntriesByType("resource")
      .filter((entry): entry is PerformanceResourceTiming => entry.name.includes("jetbrains-mono") && "encodedBodySize" in entry)
      .map(entry => ({ path: new URL(entry.name).pathname, size: entry.encodedBodySize, wire: entry.transferSize })));
    expect(faces).toHaveLength(1);
    expect(faces[0].path).toMatch(latinFace);
    expect(faces[0].size).toBeGreaterThan(0);
    expect(faces[0].wire).toBeGreaterThanOrEqual(faces[0].size);
    expect(faces[0].size).toBeLessThanOrEqual(Math.floor(40_404 * .8)); // Untrimmed Fontsource 5.3.0 Latin.

    for (const [path, selector, weight] of [
      [prefix + "/docs", ".doc-body code", "400"],
      ["/guide/fetch", ".doc-body pre", "400"],
      [prefix + "/pricing", ".plan-pro .plan-price", "680"],
      [prefix + "/playground", "#lockfile", "400"],
    ]) {
      await page.goto(path);
      await expect(page.locator(selector).first()).toHaveCSS("font-weight", weight);
      await verifyMono();
    }

    fonts.length = 0;
    await page.locator(".input-tools select").selectOption("requirements.txt");
    await page.locator("#lockfile").fill(mixedLockfile);
    await page.locator(".studio-input button[type=submit]").click();
    await expect(page.getByRole("heading", { name: "pydantic@1.10.18", exact: true })).toBeVisible();
    await expect.poll(async () => (await monoState(page)).loaded).toBe(6); // Includes the inlined Cyrillic-ext face.
    expect(fonts.filter(path => path.includes("jetbrains-mono")).map(path => path.split("-wght-")[0].split("jetbrains-mono-")[1]).sort())
      .toEqual(["cyrillic", "greek", "latin-ext", "vietnamese"]);
    await verifyMono();
  });
}
