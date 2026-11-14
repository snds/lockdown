import {
  BUILD_JSON_LIMIT,
  CHECK_EVERY_MS,
  HIDDEN_FOR_MS,
  IDLE_FOR_MS,
  MIN_GAP_MS,
  createBuildWatch,
  quietByTheClock,
  readBuild,
  type BuildWatchEnv,
} from "./build-watch"

/** A page on build `a`, a server that answers what `world.body` says, and a clock the test moves. */
function world(over: Partial<BuildWatchEnv> = {}) {
  const w = {
    now: 1_000_000,
    body: '{"build":"a"}' as string | null,
    hidden: false,
    signedIn: false as boolean | "throws",
    atRest: true,
    asked: 0,
    askedCore: 0,
    reloads: 0,
    note: null as string | null,
    keepsNotes: true,
  }
  const env: BuildWatchEnv = {
    current: "a",
    fetchText: async () => {
      w.asked += 1
      return w.body
    },
    now: () => w.now,
    hidden: () => w.hidden,
    signedIn: async () => {
      w.askedCore += 1
      if (w.signedIn === "throws") throw new Error("the worker is gone")
      return w.signedIn
    },
    canReload: () => w.atRest,
    reload: () => {
      w.reloads += 1
    },
    reloadedFor: () => w.note,
    noteReloaded: (id) => {
      if (w.keepsNotes) w.note = id
    },
    ...over,
  }
  return { w, watch: createBuildWatch(env) }
}

describe("build.json", () => {
  it("reads a build id and nothing else", () => {
    expect(readBuild('{"build":"3f9a0c1d2e4b5a6978f0"}')).toBe("3f9a0c1d2e4b5a6978f0")
    expect(readBuild('{"build":"0.1.0-main.20261107093000","more":1}')).toBe("0.1.0-main.20261107093000")
    for (const body of [null, undefined, "", "not json", "<!doctype html><title>Sign in</title>", "[]", "null", '"a"', "{}", '{"build":3}', '{"build":""}', '{"build":"a b"}', '{"build":"<script>"}', `{"build":"${"a".repeat(65)}"}`])
      expect(readBuild(body), String(body)).toBeNull()
    expect(readBuild(`{"build":"a","pad":"${"x".repeat(BUILD_JSON_LIMIT)}"}`)).toBeNull()
  })

  it("the same id is no news; another id is a new version", async () => {
    const { w, watch } = world()
    expect((await watch.check()).ready).toBe(false)
    expect(watch.state()).toMatchObject({ current: "a", latest: null, ready: false, checkedAt: w.now })
    w.body = '{"build":"b"}'
    expect(await watch.check(true)).toMatchObject({ latest: "b", ready: true })
  })

  it("an answer it can't read never means new, and never takes an offer back", async () => {
    const { w, watch } = world()
    for (const body of ["<html>sign in</html>", null, "{}"]) {
      w.body = body
      expect((await watch.check(true)).ready).toBe(false)
    }
    expect(watch.state().checkedAt).toBeNull()
    w.body = '{"build":"b"}'
    await watch.check(true)
    w.body = "<html>bad gateway</html>"
    expect(await watch.check(true)).toMatchObject({ latest: "b", ready: true })
    const failing = world({ fetchText: () => Promise.reject(new Error("offline")) })
    expect((await failing.watch.check()).ready).toBe(false)
  })

  it("a deploy rolled back takes the offer back", async () => {
    const { w, watch } = world()
    w.body = '{"build":"b"}'
    await watch.check(true)
    w.body = '{"build":"a"}'
    expect(await watch.check(true)).toMatchObject({ latest: null, ready: false, needed: false })
  })

  it("asks at most once per few seconds unless forced, and tells its listeners once per change", async () => {
    const { w, watch } = world()
    let told = 0
    watch.subscribe(() => {
      told += 1
    })
    await watch.check()
    await watch.check()
    expect(w.asked).toBe(1)
    w.now += MIN_GAP_MS
    await watch.check()
    expect(w.asked).toBe(2)
    await watch.check(true)
    expect(w.asked).toBe(3)
    // Told when something changed: the first answer, and the later one (its time). Not for a repeat.
    expect(told).toBe(2)
  })
})

describe("when it asks", () => {
  it("every 15 minutes while visible, never while hidden", async () => {
    const { w, watch } = world()
    await watch.tick()
    expect(w.asked).toBe(1)
    w.now += CHECK_EVERY_MS - 1
    await watch.tick()
    expect(w.asked).toBe(1)
    w.now += 1
    await watch.tick()
    expect(w.asked).toBe(2)
    w.hidden = true
    watch.visibility()
    w.now += CHECK_EVERY_MS * 3
    await watch.tick()
    expect(w.asked).toBe(2)
  })

  it("when the tab is shown again and when the device is back online", async () => {
    const { w, watch } = world()
    w.hidden = true
    watch.visibility()
    expect(w.asked).toBe(0)
    w.hidden = false
    watch.visibility()
    expect(w.asked).toBe(1)
    await watch.check()
    w.now += MIN_GAP_MS
    watch.online()
    expect(w.asked).toBe(2)
  })
})

describe("a reload by itself", () => {
  it("is by the clock: hidden for a minute, or idle for five", () => {
    expect(quietByTheClock(IDLE_FOR_MS, null, 0)).toBe(true)
    expect(quietByTheClock(IDLE_FOR_MS - 1, null, 0)).toBe(false)
    expect(quietByTheClock(HIDDEN_FOR_MS, 0, HIDDEN_FOR_MS)).toBe(true)
    expect(quietByTheClock(HIDDEN_FOR_MS - 1, 0, HIDDEN_FOR_MS - 1)).toBe(false)
  })

  /** A page that knows of build `b`. */
  async function ready(over: Partial<BuildWatchEnv> = {}) {
    const made = world(over)
    made.w.body = '{"build":"b"}'
    await made.watch.check(true)
    return made
  }

  it("happens when nobody is signed in, the page is at rest and the moment is quiet", async () => {
    const { w, watch } = await ready()
    expect(await watch.tick()).toBe(false)
    expect(w.askedCore, "the core isn't asked before the clock says yes").toBe(0)
    w.now += IDLE_FOR_MS
    expect(await watch.tick()).toBe(true)
    expect(w.reloads).toBe(1)
    expect(w.note).toBe("b")
    // Reloading already: nothing more happens.
    expect(await watch.tick()).toBe(true)
    expect(w.reloads).toBe(1)
  })

  it("never happens while someone is signed in, open or locked: it only offers", async () => {
    const { w, watch } = await ready()
    w.signedIn = true
    w.hidden = true
    watch.visibility()
    for (let i = 0; i < 20; i++) {
      w.now += IDLE_FOR_MS * 12
      expect(await watch.tick()).toBe(false)
    }
    expect(w.reloads).toBe(0)
    expect(watch.state()).toMatchObject({ ready: true, latest: "b" })
    // Signed out later (the person, or the server ending the session): now it may.
    w.signedIn = false
    expect(await watch.tick()).toBe(true)
    expect(w.reloads).toBe(1)
  })

  it("never happens when the core can't be asked", async () => {
    const { w, watch } = await ready()
    w.signedIn = "throws"
    w.now += IDLE_FOR_MS
    expect(await watch.tick()).toBe(false)
    expect(w.reloads).toBe(0)
  })

  it("never happens while the page holds anything", async () => {
    const { w, watch } = await ready()
    w.atRest = false
    w.now += IDLE_FOR_MS
    expect(await watch.tick()).toBe(false)
    expect(w.askedCore, "the core isn't asked either").toBe(0)
    const throwing = await ready({
      canReload: () => {
        throw new Error("no document")
      },
    })
    throwing.w.now += IDLE_FOR_MS
    expect(await throwing.watch.tick()).toBe(false)
    expect(throwing.w.reloads).toBe(0)
  })

  it("never happens under someone's eyes: activity, or the tab shown, while the core answers", async () => {
    for (const interrupt of ["a key", "the tab shown", "typing began"] as const) {
      let answer: (signedIn: boolean) => void = () => {}
      const { w, watch } = await ready({ signedIn: () => new Promise<boolean>((resolve) => (answer = resolve)) })
      w.hidden = true
      watch.visibility()
      w.now += HIDDEN_FOR_MS
      const ticking = watch.tick()
      await Promise.resolve()
      if (interrupt === "a key") watch.activity()
      if (interrupt === "the tab shown") {
        w.hidden = false
        watch.visibility()
      }
      if (interrupt === "typing began") w.atRest = false
      answer(false)
      expect(await ticking, interrupt).toBe(false)
      expect(w.reloads, interrupt).toBe(0)
    }
  })

  it("happens once per build, so a bad deploy can't loop", async () => {
    const first = await ready()
    first.w.now += IDLE_FOR_MS
    await first.watch.tick()
    // The same tab after that reload, still on build `a` (the deploy is broken): the note is kept.
    const again = await ready()
    again.w.note = "b"
    again.w.now += IDLE_FOR_MS * 10
    expect(await again.watch.tick()).toBe(false)
    expect(again.w.reloads).toBe(0)
    expect(again.watch.state().ready, "it still offers").toBe(true)
    // A later build is another matter.
    again.w.body = '{"build":"c"}'
    await again.watch.check(true)
    expect(await again.watch.tick()).toBe(true)
  })

  it("never happens when the note can't be kept (no session storage)", async () => {
    const { w, watch } = await ready()
    w.keepsNotes = false
    w.now += IDLE_FOR_MS
    expect(await watch.tick()).toBe(false)
    expect(w.reloads).toBe(0)
  })

  it("never happens without a new build", async () => {
    const { w, watch } = world()
    await watch.check()
    w.now += IDLE_FOR_MS * 10
    expect(await watch.tick()).toBe(false)
    expect(w.reloads + w.askedCore).toBe(0)
  })
})

describe("Reload, pressed", () => {
  it("reloads at once, whoever is signed in, and only once", async () => {
    const { w, watch } = world()
    w.signedIn = true
    w.atRest = false
    watch.reloadNow()
    watch.reloadNow()
    expect(w.reloads).toBe(1)
    expect(w.askedCore).toBe(0)
  })
})

describe("a lazy file that failed to load", () => {
  it("with no new build, changes nothing", async () => {
    const { w, watch } = world()
    expect(await watch.preloadFailed()).toBe(false)
    expect(watch.state().needed).toBe(false)
    expect(w.reloads).toBe(0)
  })

  it("with a new build and someone signed in, says Reload to continue", async () => {
    const { w, watch } = world()
    w.body = '{"build":"b"}'
    w.signedIn = true
    w.now += IDLE_FOR_MS
    expect(await watch.preloadFailed()).toBe(false)
    expect(watch.state()).toMatchObject({ ready: true, needed: true })
    expect(w.reloads).toBe(0)
  })

  it("with a new build and nothing to lose, reloads", async () => {
    const { w, watch } = world()
    w.body = '{"build":"b"}'
    w.now += IDLE_FOR_MS
    expect(await watch.preloadFailed()).toBe(true)
    expect(w.reloads).toBe(1)
  })
})
