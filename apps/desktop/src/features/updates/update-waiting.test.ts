import type { UpdateStatus } from "./update-client"
import { UPDATE_WAIT_DAYS, readySince, updateWaitingNews, waitedTooLong } from "./update-waiting"

const DAY = 86_400
const base: UpdateStatus = {
  version: "0.1.0-main.20261101000000",
  channel: "main",
  enabled: true,
  off: null,
  auto_check: true,
  state: "ready",
  percent: null,
  offered: "0.1.0-main.20261108000000",
  error: null,
  link: null,
  link_why: null,
  asked: false,
}

describe("a ready update that waits (Lockdown 7)", () => {
  beforeEach(() => window.localStorage.clear())

  it("notes the day it became ready, keeps it across checks, and shows as news only after seven days", () => {
    const t0 = 1_800_000_000
    expect(readySince(base, t0)).toBe(t0)
    // A later look at the same ready version keeps the first day; so does a check on the way.
    expect(readySince(base, t0 + 3 * DAY)).toBe(t0)
    expect(readySince({ ...base, state: "checking" }, t0 + 4 * DAY)).toBeNull()
    expect(readySince(base, t0 + 5 * DAY)).toBe(t0)
    expect(waitedTooLong(t0, t0 + (UPDATE_WAIT_DAYS - 1) * DAY)).toBe(false)
    expect(waitedTooLong(t0, t0 + UPDATE_WAIT_DAYS * DAY)).toBe(true)
    expect(waitedTooLong(null, t0 + 30 * DAY)).toBe(false)
  })

  it("starts again for a newer version, and forgets once installed", () => {
    const t0 = 1_800_000_000
    readySince(base, t0)
    expect(readySince({ ...base, offered: "0.1.0-main.20261110000000" }, t0 + 2 * DAY)).toBe(t0 + 2 * DAY)
    expect(readySince({ ...base, state: "up_to_date", offered: null }, t0 + 3 * DAY)).toBeNull()
    expect(readySince(base, t0 + 4 * DAY)).toBe(t0 + 4 * DAY)
  })

  it("is one piece of news with RESTART TO UPDATE, which restarts", () => {
    const restart = vi.fn()
    const news = updateWaitingNews(base, restart)
    expect(news.id).toBe("update:0.1.0-main.20261108000000")
    expect(news.title).toBe("Lockdown 0.1.0-main.20261108000000 has waited a week")
    expect(news.action.label).toBe("Restart to update")
    news.action.onClick()
    expect(restart).toHaveBeenCalledTimes(1)
  })
})
