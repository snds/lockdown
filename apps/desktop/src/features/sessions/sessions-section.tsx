import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react"

import { Badge, Banner, Button, ConfirmDialog, FieldRow, FieldRowAction, SectionHeader, Spinner, useViewport } from "@unicron/ui"

import { describeError } from "@/core/client"
import { fullDate, shortId } from "@/lib/format"
import { ClockTime } from "@/components/clock-time"

import { clientWords } from "./client-words"
import { useLiveClient, type SessionView } from "./live-client"
import { useLive } from "./use-live"

/** Where the account is signed in, read once and again whenever the live channel says it changed. */
export interface SessionList {
  sessions: SessionView[] | null
  error: string | null
  reload: () => Promise<void>
}

/**
 * The account's live sessions, for the Signed in section and for the device
 * rows above it (a device's last use is its newest session's, brief T2).
 * `null` where the host has no live client (no account server to ask).
 * `enabled` false reads nothing: a section handed its list by its screen
 * does not ask twice.
 */
export function useSessionList(enabled = true): SessionList | null {
  const live = useLiveClient()
  const [sessions, setSessions] = useState<SessionView[] | null>(null)
  const [error, setError] = useState<string | null>(null)
  const on = useRef(enabled && live !== null)
  useLayoutEffect(() => {
    on.current = enabled && live !== null
  })

  const reload = useCallback(async () => {
    if (!live || !on.current) return
    try {
      setSessions(await live.sessionList())
      setError(null)
    } catch (e) {
      setError(describeError(e))
    }
  }, [live])
  useEffect(() => {
    void reload()
  }, [reload, enabled])
  useLive((n) => {
    if (n.kind === "sessions" && on.current) void reload()
  })
  return live ? { sessions, error, reload } : null
}

/** A session's device, by its name in the identity's records or by the first part of its id. */
export function sessionDevice(s: SessionView): string {
  return s.device_name ?? `Device ${shortId(s.device_id)}`
}

interface SessionsSectionProps {
  onNotice: (text: string) => void
  /** The list the screen already reads (Devices); left out, the section reads its own. */
  list?: SessionList | null
}

type Ending = { kind: "one"; session: SessionView } | { kind: "others" }

/**
 * Signed in (`account-alerts`; brief T2): every live session on the server,
 * a Field Row each, with its device, when it was last used, its app,
 * address and sign-in date; this device's marked. **End** stops that
 * session's token at once, and that device has to sign in again with the
 * password; **End all others** does it to every session but this one. Each
 * asks first and takes no gate: ending a session takes access away (D221,
 * brief X5). Ending a session doesn't revoke the device: revoking is above.
 */
export function SessionsSection({ onNotice, list }: SessionsSectionProps) {
  const live = useLiveClient()
  const own = useSessionList(list === undefined)
  const shown = list === undefined ? own : list
  if (!live || !shown) return null
  return <Sessions list={shown} onNotice={onNotice} end={live} />
}

function Sessions({ list, onNotice, end: live }: { list: SessionList; onNotice: (text: string) => void; end: NonNullable<ReturnType<typeof useLiveClient>> }) {
  const { sessions, reload } = list
  const [ending, setEnding] = useState<Ending | null>(null)
  // A phone puts the label over the value, so a date or a name is never cut (FieldRow `stacked`).
  const rowSize = useViewport().widthClass === "compact" ? "stacked" : "default"
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  const end = async (what: Ending) => {
    setBusy(true)
    setError(null)
    try {
      if (what.kind === "one") {
        await live.sessionEnd(what.session.id)
        onNotice(`Ended the session on ${sessionDevice(what.session)}`)
      } else {
        const n = await live.sessionEndOthers()
        onNotice(n === 1 ? "Ended 1 other session" : `Ended ${n} other sessions`)
      }
      setEnding(null)
      await reload()
    } catch (e) {
      setEnding(null)
      setError(describeError(e))
    } finally {
      setBusy(false)
    }
  }

  const others = sessions?.filter((s) => !s.current) ?? []
  const shownError = error ?? list.error
  // Nothing signed in on the server (a fresh account between syncs): the section says nothing at all.
  if (sessions && sessions.length === 0 && !shownError) return null
  return (
    <section aria-label="Signed in" className="flex flex-col">
      <SectionHeader
        title="Signed in"
        count={sessions?.length}
        action={
          others.length > 0 ? (
            <Button size="sm" variant="ghost-destructive" onClick={() => setEnding({ kind: "others" })} disabled={busy}>
              End all others
            </Button>
          ) : undefined
        }
      />
      {shownError && <Banner size="compact" title={shownError} />}
      {!sessions && !shownError && <Spinner />}
      <div role="list" aria-label="Sessions">
        {sessions?.map((s) => (
          <div role="listitem" key={s.id}>
            <FieldRow
              size={rowSize}
              label={sessionDevice(s)}
              badge={s.current ? <Badge>This device</Badge> : undefined}
              description={[clientWords(s.client), s.address ? `from ${s.address}` : null, `signed in ${fullDate(s.created_at)}`].filter(Boolean).join(" // ")}
              action={
                s.current ? undefined : (
                  <FieldRowAction
                    variant="ghost-destructive"
                    onClick={() => setEnding({ kind: "one", session: s })}
                    disabled={busy}
                    aria-label={`End the session on ${sessionDevice(s)}`}
                  >
                    End
                  </FieldRowAction>
                )
              }
            >
              <ClockTime secs={s.last_seen} prefix="Last used" />
            </FieldRow>
          </div>
        ))}
      </div>
      <ConfirmDialog
        open={ending !== null}
        onOpenChange={(o) => !o && !busy && setEnding(null)}
        overline={ending?.kind === "one" ? sessionDevice(ending.session) : "Signed in"}
        title={ending?.kind === "one" ? `End the session on ${sessionDevice(ending.session)}?` : "End every other session?"}
        description="It signs out at once and needs the master password to sign in again. The device stays one of yours; revoke it too if you don't know it."
        confirmLabel={ending?.kind === "others" ? "End all others" : "End"}
        tone="destructive"
        onConfirm={() => (ending ? end(ending) : undefined)}
      />
    </section>
  )
}
