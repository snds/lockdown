import { defineConfig, devices } from "@playwright/test"

const port = process.env.LOCKDOWN_E2E_PORT ?? "8797"

/**
 * The web vault end to end (docs/plan/17-web-vault.md §8): the real
 * server on a throwaway database serving the built page, driven in
 * Chromium.
 */
export default defineConfig({
  testDir: "./e2e",
  timeout: 180_000,
  expect: { timeout: 30_000 },
  fullyParallel: false,
  workers: 1,
  retries: 0,
  reporter: [["list"]],
  use: {
    baseURL: `http://127.0.0.1:${port}`,
    trace: "retain-on-failure",
  },
  projects: [{ name: "chromium", use: { ...devices["Desktop Chrome"] } }],
  webServer: {
    command: "node e2e/serve.mjs",
    url: `http://127.0.0.1:${port}/healthz`,
    reuseExistingServer: false,
    timeout: 60_000,
    env: { LOCKDOWN_E2E_PORT: port },
  },
})
