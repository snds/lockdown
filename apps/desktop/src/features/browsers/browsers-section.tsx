import { useCallback, useEffect, useState } from "react"

import { Badge, Button, Field, FieldDescription, FieldLabel, Item, ItemActions, ItemContent, ItemDescription, ItemGroup, ItemTitle, Switch } from "@unicron/ui"

import { useSettings } from "@/app/settings"
import { describeError } from "@/core/client"
import { dateLocal } from "@/lib/format"

import { useBrowserClient, type EndpointStatus, type PairedBrowser } from "./browser-client"

/**
 * Settings → Browsers: whether the app can hear the extension, connecting
 * the browsers on this computer (writing their native-messaging manifest),
 * and the paired browsers with Remove.
 */
export function BrowsersSection({ onNotice }: { onNotice: (text: string) => void }) {
  const client = useBrowserClient()
  const { settings, update } = useSettings()
  const [status, setStatus] = useState<EndpointStatus | null>(null)
  const [paired, setPaired] = useState<PairedBrowser[]>([])
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  const load = useCallback(() => {
    client.status().then(setStatus).catch((e) => setError(describeError(e)))
    client.paired().then(setPaired).catch(() => setPaired([]))
  }, [client])
  useEffect(() => {
    load()
  }, [load])

  const run = async (what: () => Promise<string>) => {
    setBusy(true)
    setError(null)
    try {
      onNotice(await what())
      load()
    } catch (e) {
      setError(describeError(e))
    } finally {
      setBusy(false)
    }
  }

  const unconnected = status?.browsers.filter((b) => !b.connected) ?? []
  return (
    <section aria-labelledby="browsers-title" className="flex flex-col gap-3 rounded-xl border bg-card p-4">
      <h2 id="browsers-title" className="font-medium">
        Browsers
      </h2>
      <p className="text-sm text-muted-foreground">
        {status === null
          ? "Checking the browser connection…"
          : status.listening
            ? "The Lockdown extension can reach this app while it is open. A browser pairs once, with a code you compare here."
            : `The browser extension can't reach this app: ${status.problem ?? "not running"}.`}
      </p>
      {error && (
        <p role="alert" className="text-sm text-destructive">
          {error}
        </p>
      )}
      <Field orientation="horizontal">
        <FieldLabel htmlFor="browser-passkeys">Offer Lockdown's passkeys in browsers</FieldLabel>
        <Switch
          id="browser-passkeys"
          checked={settings.browser_passkeys ?? true}
          disabled={busy}
          onCheckedChange={(on) =>
            void run(async () => (await update({ browser_passkeys: on }), on ? "Browsers offer Lockdown's passkeys." : "Browsers use their own passkeys; Lockdown steps aside."))
          }
        />
      </Field>
      <FieldDescription>
        When a site asks for a passkey, the paired browser offers the ones in Lockdown and can save new ones here, with a choice to use another device
        or security key instead. Off, every passkey request goes to the browser's own.
      </FieldDescription>
      {status && status.browsers.length > 0 && (
        <ItemGroup role="list" aria-label="Browsers on this computer">
          {status.browsers.map((b) => (
            <Item key={b.browser} role="listitem" size="sm" variant="outline">
              <ItemContent>
                <ItemTitle>
                  {b.name}
                  {b.connected && <Badge variant="secondary">connected</Badge>}
                </ItemTitle>
              </ItemContent>
              {!b.connected && (
                <ItemActions>
                  <Button size="sm" variant="outline" disabled={busy} onClick={() => void run(async () => (await client.install([b.browser]), `${b.name} can now reach Lockdown.`))}>
                    Connect
                  </Button>
                </ItemActions>
              )}
            </Item>
          ))}
        </ItemGroup>
      )}
      {unconnected.length > 1 && (
        <div>
          <Button size="sm" variant="outline" disabled={busy} onClick={() => void run(async () => `${(await client.install([])).length} browsers can now reach Lockdown.`)}>
            Connect all
          </Button>
        </div>
      )}
      <ItemGroup role="list" aria-label="Paired browsers">
        {paired.map((p) => (
          <Item key={p.client_id} role="listitem" size="sm" variant="outline">
            <ItemContent>
              <ItemTitle>{p.name}</ItemTitle>
              <ItemDescription>Paired {dateLocal(p.paired_at)}</ItemDescription>
            </ItemContent>
            <ItemActions>
              <Button size="sm" variant="ghost-destructive" disabled={busy} onClick={() => void run(async () => (await client.unpair(p.client_id), `${p.name} is no longer paired.`))}>
                Remove
              </Button>
            </ItemActions>
          </Item>
        ))}
      </ItemGroup>
    </section>
  )
}
