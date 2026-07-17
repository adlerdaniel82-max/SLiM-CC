import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "tests/e2e", timeout: 20_000, fullyParallel: false,
  use: { baseURL: "http://127.0.0.1:1420", screenshot: "only-on-failure", trace: "retain-on-failure" },
  webServer: { command: "npm run dev", url: "http://127.0.0.1:1420", reuseExistingServer: true, timeout: 30_000 },
  projects: [{ name: "chromium", use: { browserName: "chromium" } }]
});
