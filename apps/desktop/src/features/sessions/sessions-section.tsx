import { useCallback, useEffect, useState } from "react"

import {
  Badge,
  Button,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  Item,
  ItemActions,
  ItemContent,
  ItemDescription,
  ItemGroup,
  ItemTitle,
  Spinner,
} from "@unicron/ui"

import { LabelledAction, LabelledActions } from "@/components/labelled-actions"
import { describeError } from "@/core/client"
import { shortId, whenLocal } from "@/lib/format"

import { clientWords } from "./client-words"
import { useLiveClient, type LiveClient, type SessionView } from "./live-client"
import { useLive } from "./use-live"

interface SessionsSectionProps {
  onNotice: (text: string) => void
}

type Ending = { kind: "one"; session: SessionView } | { kind: "others" }

function deviceLabel(s: SessionView): string {
  return s.device_name ?? `Device ${shortId(s.device_id)}`
}

/**
 * Where the account is signed in (`account-alerts`): every live session on
 * the server with its device, address, client and times, this device's
 * marked. **End** stops that session's token at once; that device has to
 * log in again with the password. Ending a session doesn't revoke the
 * device: revoking is above, behind a fresh check.
 */
export function SessionsSection({ onNotice }: SessionsSectionProps) {
  const live = useLiveClient()
  return live ? <Sessions live={live} onNotice={onNotice} /> : null
}

function Sessions({ live, onNotice }: SessionsSectionProps & { live: LiveClient }) {
  const [sessions, setSessions] = useState<SessionView[] | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [ending, setEnding] = useState<Ending | null>(null)
  const [busy, setBusy] = useState(false)

  const load = useCallback(
    () =>
      live
        .sessionList()
        .then((s) => {
          setSessions(s)
          setError(null)
        })
        .catch((e) => setError(describeError(e))),
    [live],
  )
  useEffect(() => {
    void load()
  }, [load])
  useLive((n) => {
    if (n.kind === "sessions") void load()
  })

  const end = async (what: Ending) => {
    setBusy(true)
    setError(null)
    try {
      if (what.kind === "one") {
        await live.sessionEnd(what.session.id)
        onNotice(`Ended the session on ${deviceLabel(what.session)}. It has to log in again.`)
      } else {
        const n = await live.sessionEndOthers()
        onNotice(n === 1 ? "Ended 1 other session." : `Ended ${n} other sessions.`)
      }
      await load()
    } catch (e) {
      setError(describeError(e))
    } finally {
      setBusy(false)
    }
  }

  const others = sessions?.filter((s) => !s.current) ?? []
  return (
    <section aria-labelledby="sessions-title" className="flex flex-col gap-3">
      <div className="flex items-center justify-between gap-2">
        <h2 id="sessions-title" className="text-sm font-medium">
          Signed in
        </h2>
        {others.length > 0 && (
          <Button size="sm" variant="ghost-destructive" onClick={() => setEnding({ kind: "others" })} disabled={busy}>
            End all other sessions
          </Button>
        )}
      </div>
      {error && (
        <p role="alert" className="text-sm text-destructive">
          {error}
        </p>
      )}
      {!sessions && !error && <Spinner />}
      <ItemGroup role="list" aria-label="Sessions">
        {sessions?.map((s) => (
          <Item key={s.id} role="listitem" size="sm" variant="outline">
            <ItemContent>
              <ItemTitle>
                {deviceLabel(s)}
                {s.current && <Badge variant="secondary">this device</Badge>}
              </ItemTitle>
              <ItemDescription>
                {[clientWords(s.client), s.address ? `from ${s.address}` : null, `signed in ${whenLocal(s.created_at)}`, `last used ${whenLocal(s.last_seen)}`]
                  .filter(Boolean)
                  .join(" · ")}
              </ItemDescription>
            </ItemContent>
            {!s.current && (
              <ItemActions>
                <Button size="sm" variant="ghost-destructive" onClick={() => setEnding({ kind: "one", session: s })} disabled={busy} aria-label={`End the session on ${deviceLabel(s)}`}>
                  End
                </Button>
              </ItemActions>
            )}
          </Item>
        ))}
      </ItemGroup>
      <Dialog open={ending !== null} onOpenChange={(o) => !o && setEnding(null)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{ending?.kind === "one" ? `End the session on ${deviceLabel(ending.session)}?` : "End every other session?"}</DialogTitle>
            <DialogDescription>
              The server stops taking that session at once. The device stays enrolled and keeps what it has; it needs the account password to sign in
              again. If you don't know the device, revoke it too.
            </DialogDescription>
          </DialogHeader>
          <LabelledActions>
            <LabelledAction kind="cancel" onClick={() => setEnding(null)}>
              Cancel
            </LabelledAction>
            <LabelledAction
              kind="destructive"
              onClick={() => {
                const what = ending
                setEnding(null)
                if (what) void end(what)
              }}
            >
              End
            </LabelledAction>
          </LabelledActions>
        </DialogContent>
      </Dialog>
    </section>
  )
}
