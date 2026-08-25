import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "./tests",
  timeout: 30_000,
  use: { baseURL: "http://127.0.0.1:1420", trace: "retain-on-failure" },
  webServer: {
    command:
      "pnpm build && pnpm preview --host 127.0.0.1 --port 1420 --strictPort",
    url: "http://127.0.0.1:1420",
    env: { VITE_E2E: "1" },
    reuseExistingServer: !process.env.CI,
  },
});
