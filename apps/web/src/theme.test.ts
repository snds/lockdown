/*
 * The web vault's ground (docs/plan/62-unicron-1-0.md §3): System follows the
 * browser; Stone and Graphite hold, kept in this browser's local storage and
 * read at the start, from the page's own bundle. The module keeps its choice
 * in memory, so each test loads it fresh, as a page load does.
 */

type Theme = typeof import("./theme")

/** What the browser prefers, with a way to change it the way the browser does. */
function browser(prefers: "light" | "dark") {
  let scheme = prefers
  const listeners = new Set<() => void>()
  window.matchMedia = ((query: string) => ({
    get matches() {
      return query.includes("dark") ? scheme === "dark" : scheme === "light"
    },
    media: query,
    addEventListener: (_: string, listener: () => void) => listeners.add(listener),
    removeEventListener: (_: string, listener: () => void) => listeners.delete(listener),
  })) as unknown as typeof window.matchMedia
  return {
    prefer(next: "light" | "dark") {
      scheme = next
      for (const listener of listeners) listener()
    },
  }
}

async function start(): Promise<Theme> {
  vi.resetModules()
  const theme = await import("./theme")
  theme.followColorScheme()
  return theme
}

const root = document.documentElement

beforeEach(() => {
  window.localStorage.clear()
  root.classList.remove("dark")
  root.style.colorScheme = ""
})

afterEach(() => {
  vi.restoreAllMocks()
})

describe("the web vault's theme", () => {
  it("follows the browser while the choice is System, and stores nothing", async () => {
    const b = browser("dark")
    const theme = await start()
    expect(theme.themeChoice()).toBe("system")
    expect(root).toHaveClass("dark")
    expect(root.style.colorScheme).toBe("dark")
    b.prefer("light")
    expect(root).not.toHaveClass("dark")
    expect(root.style.colorScheme).toBe("light")
    expect(window.localStorage.length).toBe(0)
  })

  it("Graphite and Stone hold whatever the browser prefers, change the page at once, and are kept in this browser", async () => {
    const b = browser("light")
    const theme = await start()
    const heard = vi.fn()
    theme.subscribeTheme(heard)

    theme.setThemeChoice("graphite")
    expect(theme.themeChoice()).toBe("graphite")
    expect(root).toHaveClass("dark")
    expect(root.style.colorScheme).toBe("dark")
    expect(window.localStorage.getItem(theme.THEME_KEY)).toBe("graphite")
    expect(heard).toHaveBeenCalledTimes(1)

    theme.setThemeChoice("stone")
    expect(root).not.toHaveClass("dark")
    expect(window.localStorage.getItem(theme.THEME_KEY)).toBe("stone")
    b.prefer("dark")
    expect(root).not.toHaveClass("dark")
  })

  it("a reload reads the stored choice at the start, before anything asks", async () => {
    browser("light")
    ;(await start()).setThemeChoice("graphite")
    root.classList.remove("dark")
    // A new page load on a browser that prefers light.
    const again = await start()
    expect(again.themeChoice()).toBe("graphite")
    expect(root).toHaveClass("dark")
  })

  it("System forgets the stored choice and follows the browser again", async () => {
    const b = browser("dark")
    const theme = await start()
    theme.setThemeChoice("stone")
    expect(root).not.toHaveClass("dark")
    theme.setThemeChoice("system")
    expect(window.localStorage.getItem(theme.THEME_KEY)).toBeNull()
    expect(root).toHaveClass("dark")
    b.prefer("light")
    expect(root).not.toHaveClass("dark")
  })

  it("reads a stored value it does not know as System", async () => {
    browser("dark")
    window.localStorage.setItem("lockdown.theme", "sepia")
    const theme = await start()
    expect(theme.themeChoice()).toBe("system")
    expect(root).toHaveClass("dark")
  })

  it("takes a choice made in another tab", async () => {
    browser("light")
    const theme = await start()
    const heard = vi.fn()
    theme.subscribeTheme(heard)
    window.localStorage.setItem(theme.THEME_KEY, "graphite")
    window.dispatchEvent(new StorageEvent("storage", { key: theme.THEME_KEY, newValue: "graphite" }))
    expect(theme.themeChoice()).toBe("graphite")
    expect(root).toHaveClass("dark")
    expect(heard).toHaveBeenCalledTimes(1)
    // Another key's change is none of its business.
    window.dispatchEvent(new StorageEvent("storage", { key: "lockdown.expiry-reminded", newValue: "2026-11-14" }))
    expect(heard).toHaveBeenCalledTimes(1)
  })

  it("with storage off it follows the browser, and a choice holds until the page reloads", async () => {
    browser("dark")
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new DOMException("storage is off", "SecurityError")
    })
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new DOMException("storage is off", "SecurityError")
    })
    const theme = await start()
    expect(theme.themeChoice()).toBe("system")
    expect(root).toHaveClass("dark")
    theme.setThemeChoice("stone")
    expect(theme.themeChoice()).toBe("stone")
    expect(root).not.toHaveClass("dark")
  })

  it("writes no script and no style element: the page's policy allows neither inline", async () => {
    browser("dark")
    const before = document.querySelectorAll("script, style").length
    const theme = await start()
    theme.setThemeChoice("graphite")
    theme.setThemeChoice("system")
    expect(document.querySelectorAll("script, style").length).toBe(before)
    expect(root.getAttribute("data-theme")).toBeNull()
  })
})
