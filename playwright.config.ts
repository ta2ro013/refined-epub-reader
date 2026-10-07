import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "./src/features/reader",
  testMatch: "**/*.browser.spec.ts",
  outputDir: "/tmp/refined-epub-reader-playwright",
  workers: 1,
  use: { browserName: "chromium", baseURL: "http://127.0.0.1:1420" },
  webServer: {
    command: "bun --no-env-file run dev --host 127.0.0.1",
    url: "http://127.0.0.1:1420",
    reuseExistingServer: false,
  },
});
