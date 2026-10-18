import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

import { WebClipboard, clearAfter, type ClipboardLike } from "./clipboard"

function fakeClipboard(readable = true) {
  let text = ""
  const board: ClipboardLike & { text: () => string; set: (t: string) => void } = {
    writeText: async (t) => {
      text = t
    },
    readText: async () => {
      if (!readable) throw new Error("not allowed")
      return text
    },
    text: () => text,
    set: (t) => {
      text = t
    },
  }
  return board
}

const bytes = (t: string) => new TextEncoder().encode(t)

describe("copy and clear in the browser", () => {
  beforeEach(() => vi.useFakeTimers())
  afterEach(() => vi.useRealTimers())

  it("keeps the timer in 10–90 seconds, 30 by default", () => {
    expect(clearAfter(undefined)).toBe(30)
    expect(clearAfter(1)).toBe(10)
    expect(clearAfter(500)).toBe(90)
    expect(clearAfter(45)).toBe(45)
  })

  it("writes, wipes the bytes it was given, and clears after the timer", async () => {
    const board = fakeClipboard()
    const clip = new WebClipboard(board)
    const secret = bytes("hunter2")
    const copied = await clip.copy(secret, 10)
    expect(copied.clears_in_secs).toBe(10)
    expect(secret.every((b) => b === 0)).toBe(true)
    expect(board.text()).toBe("hunter2")
    expect(clip.holds(copied.copy)).toBe(true)
    await vi.advanceTimersByTimeAsync(10_000)
    // The clear compares digests (Web Crypto), which settles off the timers.
    await vi.waitFor(() => expect(board.text()).toBe(""))
    expect(clip.holds(copied.copy)).toBe(false)
  })

  it("leaves the person's own later copy alone", async () => {
    const board = fakeClipboard()
    const clip = new WebClipboard(board)
    const copied = await clip.copy(bytes("hunter2"))
    board.set("their own text")
    expect(await clip.clear(copied.copy)).toBe(false)
    expect(board.text()).toBe("their own text")
  })

  it("clears only for the last copy, and clears anyway when the page can't look", async () => {
    const board = fakeClipboard(false)
    const clip = new WebClipboard(board)
    const first = await clip.copy(bytes("one"))
    const second = await clip.copy(bytes("two"))
    expect(await clip.clear(first.copy)).toBe(false)
    expect(board.text()).toBe("two")
    expect(await clip.clear(second.copy)).toBe(true)
    expect(board.text()).toBe("")
  })
})
