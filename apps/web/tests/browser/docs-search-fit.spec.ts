import { expect, test, type Page } from "@playwright/test";

const cases = [
  {
    path: "/benchmarks", label: "Find a guide", placeholder: "MCP, upgrade",
    navigation: "Documentation", browse: "Browse documentation", hide: "Hide documentation navigation",
    query: "upgrade", match: "Pro reference", unfiltered: "Pro reference", href: "/pro",
    count: "1 matching guide", empty: "No matching guides.",
  },
  {
    path: "/zh-TW/docs", label: "搜尋指南", placeholder: "MCP、下載",
    navigation: "文件導覽", browse: "展開文件導覽", hide: "收合文件導覽",
    query: "下載", match: "下載文件（英文）", unfiltered: "下載文件", href: "/guide/fetch",
    count: "找到 1 篇指南", empty: "找不到符合的指南。",
  },
];

async function placeholderClearance(page: Page, expected: string) {
  await page.evaluate(() => document.fonts.ready);
  const cdp = await page.context().newCDPSession(page);
  try {
    // Measure the native placeholder box; it already excludes the search field's clear button.
    const { root } = await cdp.send("DOM.getDocument");
    const { nodeId } = await cdp.send("DOM.querySelector", { nodeId: root.nodeId, selector: "#docs-query" });
    const { node } = await cdp.send("DOM.describeNode", { nodeId, depth: 2, pierce: true });
    const placeholder = node.shadowRoots?.[0]?.children?.find(child => child.attributes?.includes("-webkit-input-placeholder"));
    const text = placeholder?.children?.[0];
    if (!placeholder || text?.nodeValue !== expected) throw new Error("Native search placeholder is missing or differs from the expected text");
    const { model: slot } = await cdp.send("DOM.getBoxModel", { backendNodeId: placeholder.backendNodeId });
    const { model: glyphs } = await cdp.send("DOM.getBoxModel", { backendNodeId: text.backendNodeId });
    expect(glyphs.width).toBeGreaterThan(0);
    return slot.content[2] - glyphs.content[2];
  } finally {
    await cdp.detach();
  }
}

for (const fixture of cases) {
  test("Docs search examples fit and filters work on " + fixture.path, async ({ page, isMobile }) => {
    const viewports = isMobile ? [{ width: 390, height: 844 }] : [{ width: 1440, height: 900 }, { width: 1280, height: 800 }];
    for (const viewport of viewports) {
      await test.step(viewport.width + "px viewport", async () => {
        await page.setViewportSize(viewport);
        await page.goto(fixture.path);
        const search = page.getByRole("searchbox", { name: fixture.label, exact: true });
        const nav = page.getByRole("navigation", { name: fixture.navigation, exact: true, includeHidden: true });
        const browse = page.getByRole("button", { name: fixture.browse, exact: true });
        const hide = page.getByRole("button", { name: fixture.hide, exact: true });

        await expect(search).toBeInViewport();
        await expect(search).toBeEmpty();
        await expect(search).toHaveAttribute("placeholder", fixture.placeholder);
        await expect(search).toHaveCSS("font-size", isMobile ? "16px" : "13px");
        expect(await placeholderClearance(page, fixture.placeholder), "placeholder clearance in CSS pixels").toBeGreaterThanOrEqual(6);

        if (isMobile) {
          await expect(nav).toBeHidden();
          await expect(browse).toHaveAttribute("aria-expanded", "false");
          await browse.tap();
          await expect(nav).toBeVisible();
          await hide.tap();
          await expect(nav).toBeHidden();
        } else {
          await expect(nav).toBeVisible();
        }

        await search.fill(fixture.query);
        await expect(nav.getByRole("status")).toHaveText(fixture.count);
        await expect(nav.getByRole("link", { name: fixture.match, exact: true })).toHaveAttribute("href", fixture.href);
        await search.fill("does-not-exist");
        await expect(nav.getByRole("status")).toHaveText(fixture.empty);
        await expect(nav.getByRole("link")).toHaveCount(0);

        if (isMobile) {
          await expect(hide).toHaveAttribute("aria-expanded", "true");
          await hide.tap();
          await expect(nav).toBeHidden();
        } else {
          await search.clear();
          await expect(nav.getByRole("status")).toHaveCount(0);
          await expect(nav.getByRole("link", { name: fixture.unfiltered, exact: true })).toHaveAttribute("href", fixture.href);
        }
        await expect(search).toBeEmpty();
      });
    }
  });
}
