import { RELOADED_KEY } from "./build-watch"
import { holdReload, pageAtRest, pageBuild, reloadHolds, resetBuildWatchForTests, startBuildWatch } from "./build-watch-dom"

function page(html: string) {
  document.body.innerHTML = html
}

afterEach(() => {
  resetBuildWatchForTests()
  document.head.querySelectorAll('meta[name="lockdown-build"]').forEach((m) => m.remove())
  document.body.innerHTML = ""
  vi.restoreAllMocks()
  vi.unstubAllGlobals()
  vi.useRealTimers()
  window.sessionStorage.clear()
})

const SIGN_IN = '<form aria-label="Sign in"><input name="login" /><input name="password" type="password" /><input name="secretKey" type="password" /><button>Sign in</button></form>'

describe("the page at rest", () => {
  it("is an empty form and nothing else", () => {
    page(SIGN_IN)
    expect(pageAtRest()).toBe(true)
  })

  it("is not a form with anything typed, in any field", () => {
    for (const name of ["login", "password", "secretKey"]) {
      page(SIGN_IN)
      document.querySelector<HTMLInputElement>(`input[name="${name}"]`)!.value = "x"
      expect(pageAtRest(), name).toBe(false)
    }
    // A field React controls keeps its attribute in step with what is typed; it still counts.
    page('<input value="typed" />')
    expect(pageAtRest()).toBe(false)
    page("<textarea>draft</textarea>")
    expect(pageAtRest()).toBe(false)
    page('<input /><div contenteditable="true">draft</div>')
    expect(pageAtRest()).toBe(false)
  })

  it("is not a screen without a field: a sign-in waiting for its second step, a spinner, the open vault's list", () => {
    page("<p>Your password is right. Your account asks a new device for a security key too: use it now.</p><button>Use my security key</button>")
    expect(pageAtRest()).toBe(false)
    page('<p>We emailed you a link.</p><span role="status" aria-label="Loading"></span><button>Cancel</button>')
    expect(pageAtRest()).toBe(false)
    page('<ul><li>An item</li></ul><input type="checkbox" /><button>Lock</button>')
    expect(pageAtRest()).toBe(false)
    page("")
    expect(pageAtRest()).toBe(false)
  })

  it("is not a page with something open over it, or busy", () => {
    for (const extra of ['<div role="dialog"></div>', '<div role="alertdialog"></div>', '<div aria-modal="true"></div>', "<dialog open></dialog>", '<form aria-busy="true"></form>', '<div role="progressbar"></div>']) {
      page(SIGN_IN + extra)
      expect(pageAtRest(), extra).toBe(false)
    }
  })

  it("is not a page a screen holds", () => {
    page(SIGN_IN)
    const release = holdReload("a sign-in is waiting for its emailed link")
    const other = holdReload("a file is being read")
    expect(pageAtRest()).toBe(false)
    expect(reloadHolds()).toEqual(["a sign-in is waiting for its emailed link", "a file is being read"])
    release()
    release()
    expect(pageAtRest()).toBe(false)
    other()
    expect(pageAtRest()).toBe(true)
  })
})

describe("the watch in the page", () => {
  function stamp(id: string) {
    const meta = document.createElement("meta")
    meta.name = "lockdown-build"
    meta.content = id
    document.head.append(meta)
  }

  it("reads the page's own id from its tag", () => {
    expect(pageBuild()).toBeNull()
    stamp("  3f9a0c1d2e4b5a6978f0 ")
    expect(pageBuild()).toBe("3f9a0c1d2e4b5a6978f0")
  })

  it("does not run in a page with no build id (vite dev, the tests)", () => {
    const fetched = vi.fn()
    vi.stubGlobal("fetch", fetched)
    expect(startBuildWatch({ state: () => Promise.reject(new Error("never asked")) })).toBe(false)
    expect(fetched).not.toHaveBeenCalled()
  })

  it("asks its own origin for build.json, past the cache, with no cookie and no referrer", async () => {
    stamp("a")
    const fetched = vi.fn(async () => new Response('{"build":"a"}'))
    vi.stubGlobal("fetch", fetched)
    expect(startBuildWatch({ state: async () => ({ signed_in: true }) })).toBe(true)
    await vi.waitFor(() => expect(fetched).toHaveBeenCalledTimes(1))
    const [url, init] = fetched.mock.calls[0] as unknown as [string, RequestInit]
    expect(url).toBe(`${window.location.origin}${import.meta.env.BASE_URL}build.json`)
    expect(new URL(url).origin).toBe(window.location.origin)
    expect(init).toMatchObject({ cache: "no-store", credentials: "omit", redirect: "error", referrerPolicy: "no-referrer" })
    // A second start is the same watch, not a second one.
    expect(startBuildWatch({ state: async () => ({ signed_in: true }) })).toBe(true)
    expect(fetched).toHaveBeenCalledTimes(1)
  })

  it("signed out and at rest, it reloads once the tab has been idle; signed in, it never does", async () => {
    for (const signedIn of [true, false]) {
      resetBuildWatchForTests()
      window.sessionStorage.clear()
      vi.useFakeTimers()
      stamp("a")
      page(SIGN_IN)
      vi.stubGlobal("fetch", vi.fn(async () => new Response('{"build":"b"}')))
      const reload = vi.fn()
      vi.stubGlobal("location", { ...window.location, origin: "https://vault.example", reload })
      const state = vi.fn(async () => ({ signed_in: signedIn }))
      expect(startBuildWatch({ state })).toBe(true)
      await vi.advanceTimersByTimeAsync(6 * 60 * 1000)
      expect(reload, `signed in: ${signedIn}`).toHaveBeenCalledTimes(signedIn ? 0 : 1)
      expect(window.sessionStorage.getItem(RELOADED_KEY)).toBe(signedIn ? null : "b")
      vi.useRealTimers()
      vi.unstubAllGlobals()
      document.head.querySelectorAll('meta[name="lockdown-build"]').forEach((m) => m.remove())
    }
  })
})
