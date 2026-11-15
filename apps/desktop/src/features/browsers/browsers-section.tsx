import { useCallback, useEffect, useState } from "react"

import { Badge, Banner, Button, ConfirmDialog, FieldRow, FieldRowAction, SectionHeader } from "@unicron/ui"

import { describeError } from "@/core/client"
import { fullDate } from "@/lib/format"
import { SaveError, useGroupLevel, useRowSize } from "@/features/settings/setting-rows"

import { useBrowserClient, type EndpointStatus, type PairedBrowser } from "./browser-client"

/** EXTENSION's value and line (brief T6): whether the extension can reach this app, and how the caller is checked. */
export function extensionWords(status: EndpointStatus | null): { value: string; line: string | null } {
  if (status === null) return { value: "Checking…", line: null }
  if (!status.listening) return { value: "Can't reach this app", line: status.problem ?? "Lockdown isn't listening for the browser." }
  return { value: "Can reach this app", line: status.peer_check && status.peer_check.by !== "refused" ? status.peer_check.detail : null }
}

/**
 * Settings → Browsers (brief T6, doc 80): whether the extension can reach
 * this app, the browsers on this computer (CONNECT writes their
 * native-messaging manifest) and the paired browsers, each with REMOVE
 * behind a Question. When the core refused the browser host, that is a
 * critical Banner at the top. The passkey offer lives on This Mac (T1).
 */
export function BrowsersSection({ onNotice }: { onNotice: (text: string) => void }) {
  const client = useBrowserClient()
  const level = useGroupLevel()
  const size = useRowSize()
  const [status, setStatus] = useState<EndpointStatus | null>(null)
  const [paired, setPaired] = useState<PairedBrowser[] | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const [removing, setRemoving] = useState<PairedBrowser | null>(null)
  const [removeError, setRemoveError] = useState<string | null>(null)

  const load = useCallback(() => {
    client.status().then(setStatus).catch((e) => setError(describeError(e)))
    client.paired().then(setPaired).catch(() => setPaired([]))
  }, [client])
  useEffect(() => {
    load()
  }, [load])

  const connect = async (browsers: Parameters<typeof client.install>[0], words: (n: number) => string) => {
    setBusy(true)
    setError(null)
    try {
      onNotice(words((await client.install(browsers)).length))
      load()
    } catch (e) {
      setError(describeError(e))
    } finally {
      setBusy(false)
    }
  }

  const remove = async (p: PairedBrowser) => {
    setRemoveError(null)
    try {
      await client.unpair(p.client_id)
      onNotice(`${p.name} removed`)
      load()
    } catch (e) {
      setRemoveError(describeError(e))
      throw e
    }
  }

  const refused = status?.peer_check?.by === "refused"
  const extension = extensionWords(status)
  const browsers = status?.browsers ?? []
  const unconnected = browsers.filter((b) => !b.connected)

  return (
    <div className="flex flex-col gap-8">
      {refused && status?.peer_check && (
        <Banner tone="critical" title="Lockdown refused the browser host" body={[status.problem, status.peer_check.detail].filter(Boolean).join(" ")} />
      )}
      <div className="flex flex-col">
        <FieldRow size={size} label="Extension" description={refused ? undefined : (extension.line ?? undefined)}>
          {extension.value}
        </FieldRow>
        <SaveError text={error} />
      </div>
      {browsers.length > 0 && (
        <div className="flex flex-col">
          <SectionHeader
            level={level}
            title="Browsers on this computer"
            count={browsers.length}
            action={
              unconnected.length > 1 ? (
                <Button variant="ghost" size="sm" disabled={busy} onClick={() => void connect([], (n) => `${n} browsers connected`)}>
                  Connect all
                </Button>
              ) : undefined
            }
          />
          {browsers.map((b) => (
            <FieldRow
              key={b.browser}
              size={size}
              label={b.name}
              badge={b.connected ? <Badge tone="neutral">Connected</Badge> : undefined}
              action={
                b.connected ? undefined : (
                  <FieldRowAction disabled={busy} onClick={() => void connect([b.browser], () => `${b.name} connected`)}>
                    Connect
                  </FieldRowAction>
                )
              }
            />
          ))}
        </div>
      )}
      <div className="flex flex-col">
        <SectionHeader level={level} title="Paired" count={paired ? paired.length : undefined} />
        {paired?.length === 0 && <p className="type-caption text-text-secondary py-3">None yet. A browser pairs once, with a code you compare here.</p>}
        {paired?.map((p) => (
          <FieldRow
            key={p.client_id}
            size={size}
            label={p.name}
            action={
              <FieldRowAction
                onClick={() => {
                  setRemoveError(null)
                  setRemoving(p)
                }}
              >
                Remove
              </FieldRowAction>
            }
          >
            {`Paired ${fullDate(p.paired_at)}`}
          </FieldRow>
        ))}
      </div>
      <ConfirmDialog
        open={removing !== null}
        onOpenChange={(o) => !o && setRemoving(null)}
        overline={removing?.name}
        title={`Remove ${removing?.name ?? "this browser"}?`}
        description="It can't fill or save logins until it pairs again, with a new code."
        confirmLabel="Remove"
        tone="destructive"
        error={removeError}
        onConfirm={() => (removing ? remove(removing) : undefined)}
      />
    </div>
  )
}
