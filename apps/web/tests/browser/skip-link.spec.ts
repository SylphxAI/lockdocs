import { expect, test } from "@playwright/test";

for (const path of ["/pricing", "/context7-alternative"]) {
  test("skip link is clipped until keyboard focus on " + path, async ({ page }, info) => {
    await page.goto(path);
    const skip = page.getByRole("link", { name: "Skip to content" });
    const clipped = async () => {
      await expect(skip).not.toBeFocused();
      await expect(skip).toHaveCSS("clip-path", "inset(50%)");
      await expect(skip).toHaveCSS("width", "1px");
      await expect(skip).toHaveCSS("height", "1px");
    };
    await clipped();
    await page.evaluate(() => window.scrollTo(0, 700));
    await expect.poll(() => page.evaluate(() => window.scrollY)).toBeGreaterThan(0);
    await clipped();
    await page.screenshot({ fullPage: true, animations: "disabled",
      path: process.env.LOCKDOCS_LOCAL_CAPTURES ? info.outputPath("skip-clipped.png") : undefined });
    await clipped();
    await page.keyboard.press("Tab");
    await expect(skip).toBeFocused();
    await expect(skip).toHaveCSS("clip-path", "none");
    await expect(skip).toBeInViewport({ ratio: 1 });
    const box = (await skip.boundingBox())!;
    expect(box.width).toBeGreaterThan(80);
    expect(box.height).toBeGreaterThan(40);
    await skip.press("Enter");
    await expect(page).toHaveURL(/#main$/);
  });
}
