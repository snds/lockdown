import { defineConfig } from "@playwright/test";

// The harness is small and deterministic: one browser, one worker, no
// retries, so a failure is a real failure. Chromium only for now; the
// matrix widens with the extension (06-roadmap workstream 6).
export default defineConfig({
  testDir: "./test",
  timeout: 30_000,
  fullyParallel: false,
  workers: 1,
  retries: 0,
  reporter: process.env.CI ? [["list"], ["github"]] : [["list"]],
  use: {
    browserName: "chromium",
    headless: true,
    viewport: { width: 900, height: 700 },
    trace: "retain-on-failure",
  },
});
