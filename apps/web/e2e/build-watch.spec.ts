import { expect, test } from "@playwright/test"

import { signInForm } from "./steps/unlock.ts"

/**
 * The build watch against the real server (docs/plan/66-shared-feed.md §4):
 * how an open web vault learns that the server has a newer page. No
 * account is needed: a reload by itself only ever happens when nobody is
 * signed in, so that is what there is to see in a browser. The rules for a
 * signed-in tab (it only offers) are in src/core/build-watch.test.ts.
 */

const BUILD_JSON = "**/vault/build.json"

test("the server hands out the page's own build id, never from a cache", async ({ page, request }) => {
  const res = await request.get("/vault/build.json")
  expect(res.status()).toBe(200)
  expect(res.headers()["content-type"]).toBe("application/json")
  expect(res.headers()["cache-control"]).toBe("no-store")
  const body = (await res.json()) as { build: string }
  expect(body.build).toMatch(/^[0-9a-f]{20}$/)
  await page.goto("/vault/")
  await expect(page.locator('meta[name="lockdown-build"]')).toHaveAttribute("content", body.build)
})

test("an idle sign-in page reloads once for a new build, and never with something typed", async ({ page }) => {
  const refused: string[] = []
  page.on("console", (m) => {
    if (m.type() === "error" && /Content Security Policy/i.test(m.text())) refused.push(m.text())
  })
  let asked = 0
  let loads = 0
  page.on("load", () => {
    loads += 1
  })
  await page.clock.install()
  // The server now has another build: every ask says so.
  await page.route(BUILD_JSON, async (route) => {
    asked += 1
    expect(route.request().headers()["cookie"], "the ask carries no cookie").toBeUndefined()
    await route.fulfill({ contentType: "application/json", headers: { "cache-control": "no-store" }, body: '{"build":"a-newer-build"}' })
  })
  await page.goto("/vault/")
  const form = signInForm(page)
  await expect(form).toBeVisible()
  await expect.poll(() => asked, "the page asks at load").toBeGreaterThan(0)

  // Something typed: ten idle minutes change nothing, and the words are still there.
  await form.getByLabel("Login name").fill("someone@example.com")
  await page.clock.fastForward("10:00")
  await page.waitForTimeout(1_500)
  expect(loads).toBe(1)
  await expect(form.getByLabel("Login name")).toHaveValue("someone@example.com")

  // The form empty again: the next quiet look reloads the page.
  await form.getByLabel("Login name").fill("")
  await page.clock.fastForward("06:00")
  await expect.poll(() => loads, "the idle, empty sign-in page reloads").toBe(2)
  await expect(signInForm(page)).toBeVisible()
  expect(await page.evaluate(() => window.sessionStorage.getItem("lockdown.build.reloadedFor"))).toBe("a-newer-build")

  // The server still names that build (a deploy gone wrong): no second reload, however long.
  await expect.poll(() => asked, "the reloaded page asks again").toBeGreaterThan(1)
  await page.clock.fastForward("30:00")
  await page.waitForTimeout(1_500)
  await page.clock.fastForward("30:00")
  await page.waitForTimeout(1_500)
  expect(loads).toBe(2)
  expect(refused, "no request was refused by the page's policy").toEqual([])
})

test("an answer that isn't a build id is never taken for a new build", async ({ page }) => {
  let asked = 0
  let loads = 0
  page.on("load", () => {
    loads += 1
  })
  await page.clock.install()
  // A proxy's error page where build.json should be.
  await page.route(BUILD_JSON, async (route) => {
    asked += 1
    await route.fulfill({ contentType: "text/html", body: "<!doctype html><title>Bad gateway</title>" })
  })
  await page.goto("/vault/")
  await expect(signInForm(page)).toBeVisible()
  await expect.poll(() => asked).toBeGreaterThan(0)
  await page.clock.fastForward("20:00")
  await page.waitForTimeout(1_500)
  expect(loads).toBe(1)
})
