import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react"

import { isAppError, useClient } from "@/core/client"

import type { PendingApproval } from "./approval-sheet"

/*
 * What the three kinds of request share (brief A1, X3 rows 1 to 3): a poll
 * of what waits, each request kept from the moment it was first seen with
 * the time it runs out, a needs-you row per request, and a review that opens
 * only from that row's REVIEW. A review never opens by itself, and closing
 * it decides nothing (Q3; changes D156 and D208).
 */

/** How often the app looks for a request waiting for the person. */
export const POLL_MS = 1500

/**
 * A request the core stops listing this close to its end has run out: it
 * stays in the needs-you until its own time is up, so the shell says
 * "… ran out" once. One that leaves earlier was answered somewhere else.
 */
const RAN_OUT_WITHIN_MS = 5000

/** One request, as first seen. */
export interface Waiting<T> {
  id: string
  request: T
  /** When it runs out (ms since the epoch), from the time left the core gave when it was first seen. */
  expiresAt: number
}

/**
 * Polls `pending` every 1.5 s while mounted (the app is unlocked). Each
 * request keeps the time it runs out from its first sighting, so the
 * countdown does not jump with each poll. `forget` drops one that was
 * answered here, so a poll already on its way does not bring it back.
 */
export function usePolledRequests<T>(pending: () => Promise<T[]>, idOf: (r: T) => string, secondsLeft: (r: T) => number) {
  const seen = useRef(new Map<string, Waiting<T>>())
  const answered = useRef(new Set<string>())
  const [waiting, setWaiting] = useState<Waiting<T>[]>([])
  const read = useRef({ pending, idOf, secondsLeft })
  useLayoutEffect(() => {
    read.current = { pending, idOf, secondsLeft }
  })

  const look = useCallback(() => {
    const { pending: get, idOf: id, secondsLeft: left } = read.current
    get()
      .then((list) => {
        const now = Date.now()
        const listed = new Set<string>()
        for (const r of list) {
          const key = id(r)
          listed.add(key)
          if (answered.current.has(key)) continue
          const had = seen.current.get(key)
          seen.current.set(key, { id: key, request: r, expiresAt: had?.expiresAt ?? now + left(r) * 1000 })
        }
        for (const [key, w] of seen.current) {
          if (listed.has(key)) continue
          const ranOut = w.expiresAt - now <= RAN_OUT_WITHIN_MS
          if (!ranOut || now > w.expiresAt + POLL_MS) seen.current.delete(key)
        }
        setWaiting([...seen.current.values()])
      })
      .catch(() => {
        seen.current.clear()
        setWaiting([])
      })
  }, [])

  useEffect(() => {
    look()
    const t = setInterval(look, POLL_MS)
    return () => clearInterval(t)
  }, [look])

  const forget = useCallback((key: string) => {
    answered.current.add(key)
    seen.current.delete(key)
    setWaiting([...seen.current.values()])
  }, [])

  return { waiting, look, forget }
}

/**
 * How long a closed review keeps its request off the needs-you: until the
 * sheet has left the page, so the compact line (rows 1 to 3 over an open
 * layer) never flashes on the review that is closing.
 */
const SETTLE_MS = 300

/**
 * The one review open at a time, opened only by REVIEW. `hidden(id)` says
 * whether that request's needs-you row steps aside meanwhile: the review is
 * the request, so it never shows as a second yellow shape on top of itself.
 * `show` again (after a failed answer) mounts it anew, armed again.
 */
export function useReview<T>() {
  const [open, setOpen] = useState<{ w: Waiting<T>; attempt: number } | null>(null)
  const [held, setHeld] = useState<string | null>(null)
  const attempts = useRef(0)
  const current = useRef(open)
  useLayoutEffect(() => {
    current.current = open
  })
  const show = useCallback((w: Waiting<T>) => {
    attempts.current += 1
    setOpen({ w, attempt: attempts.current })
  }, [])
  const close = useCallback(() => {
    const was = current.current
    if (!was) return
    setOpen(null)
    setHeld(was.w.id)
    window.setTimeout(() => setHeld((h) => (h === was.w.id ? null : h)), SETTLE_MS)
  }, [])
  /** Closes the review if it is the one on this request (it was answered). */
  const closeIf = useCallback(
    (id: string) => {
      if (current.current?.w.id === id) close()
    },
    [close],
  )
  const hidden = (id: string) => id === open?.w.id || id === held
  return { open, show, close, closeIf, hidden }
}

/** The gate was backed out of: nothing was approved and nothing failed, so the review just stays. */
export function isCancelled(e: unknown): boolean {
  return isAppError(e) && e.code === "refused" && /^Cancelled/.test(e.message)
}

/** The yes of a review the gate answers: named by the gate's route (brief A1; app spec, "Approve with Touch ID"). */
export interface ApproveWords {
  label: string
  icon?: "fingerprint-recognition"
}

/**
 * "Approve with Touch ID" only when this computer's `approve` route is the
 * platform's own check; otherwise "Approve", and the gate then asks for the
 * master password (or the window's confirm) after it is pressed.
 */
export function useApproveWords(): ApproveWords {
  const client = useClient()
  const [words, setWords] = useState<ApproveWords>({ label: "Approve" })
  useEffect(() => {
    let live = true
    Promise.resolve()
      .then(() => client.approvalRoute("approve"))
      .then((r) => live && setWords(r.route === "platform" ? { label: `Approve with ${r.method}`, icon: "fingerprint-recognition" } : { label: "Approve" }))
      .catch(() => live && setWords({ label: "Approve" }))
    return () => {
      live = false
    }
  }, [client])
  return words
}


/** The one sentence a review shows while its answer is on its way, or once it ran out (brief A1). */
export const WAITING_WORDS = "Waiting for the answer"
export const RAN_OUT_WORDS = "This request ran out."

/** A request's line on the needs-you: its own words, or its what and who from the facts. */
export function requestLine(r: PendingApproval): string {
  if (r.title) return r.title
  const pick = (label: string) => r.context.find((c) => c.label.toLowerCase() === label)?.value
  const said = [pick("what"), pick("who") ?? pick("device")].filter(Boolean).join(" // ")
  return said || "A device asks to be approved."
}

/** The sheet arms its actions after `--c-approval-arm-delay` (0.5 s); the request tests wait a little longer. */
export const ARMED_MS = 600
