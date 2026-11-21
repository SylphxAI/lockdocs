import { defineConfig } from "@playwright/test";

const port = 3100;
export default defineConfig({
  testDir: "tests/browser",
  fullyParallel: true,
  workers: 2,
  reporter: [["list"], ["html", { open: "never" }]],
  use: {
    baseURL: "http://127.0.0.1:" + port,
    launchOptions: { executablePath: process.env.LOCKDOCS_CHROMIUM },
    trace: "retain-on-failure",
  },
  projects: [
    { name: "desktop", use: { viewport: { width: 1440, height: 757 } } },
    { name: "mobile", use: { viewport: { width: 390, height: 844 }, isMobile: true, hasTouch: true } },
  ],
  webServer: {
    command: "npm run start", env: { HOST: "127.0.0.1", PORT: String(port) },
    url: "http://127.0.0.1:" + port + "/api/health",
    reuseExistingServer: !process.env.CI,
  },
});
