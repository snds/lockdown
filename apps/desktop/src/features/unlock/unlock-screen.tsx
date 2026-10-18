import { useEffect, useState, type FormEvent } from "react"

import { Button, Field, FieldDescription, FieldError, FieldGroup, FieldLabel, Input, Spinner, Tabs, TabsContent, TabsList, TabsTrigger } from "@unicron/ui"

import { useSettings } from "@/app/settings"
import { describeError, useClient } from "@/core/client"
import type { LockReason, Probe, Status } from "@/core/types"
import { needsEmailApproval, needsSecurityKey } from "@/features/depth/depth-client"
import { JoinWithDevice, SecondFactorWait } from "@/features/depth/join-device"

import { FirstRunCreate, Step } from "./first-run"

interface UnlockScreenProps {
  /** Why the vault is shown locked, when a timer did it. */
  reason?: LockReason | null
  onOpened: (status: Status) => void
}

const REASONS: Record<LockReason, string> = {
  requested: "Locked.",
  idle: "Locked after a while without you.",
  hidden: "Locked while the window was out of view.",
  sleep: "Locked while this computer slept.",
}

/**
 * The first screen and the lock screen. What it offers depends on what is
 * on this device (the core's `probe`):
 *
 * - an identity this device is enrolled in → **Unlock** (the device
 *   keystore is the key, D11; no password exists for a local open);
 * - nothing at all → the first run (D94): one screen with the tagline and
 *   the three doors, **Create** (then the Secret Key and recovery screens),
 *   **Join** an account from another device, or **Recover** with the kit;
 * - an identity this device is not enrolled in → a plain explanation.
 *
 * Every error comes from the core in plain words and is shown as is.
 */
export function UnlockScreen({ reason, onOpened }: UnlockScreenProps) {
  const client = useClient()
  const [probe, setProbe] = useState<Probe | null>(null)
  const [probeError, setProbeError] = useState<string | null>(null)

  useEffect(() => {
    let live = true
    client
      .probe()
      .then((p) => live && setProbe(p))
      .catch((e) => live && setProbeError(describeError(e)))
    return () => {
      live = false
    }
  }, [client])

  if (probeError) {
    return (
      <Frame>
        <h1 className="mb-4 text-lg font-semibold tracking-tight">Lockdown can't start</h1>
        <p role="alert" className="text-sm text-destructive">
          {probeError}
        </p>
      </Frame>
    )
  }
  if (!probe) {
    return (
      <Frame>
        <Spinner />
      </Frame>
    )
  }
  const enrolled = probe.identity_present && probe.device_key_present
  if (enrolled) {
    return <AttachForm probe={probe} reason={reason} onOpened={onOpened} />
  }
  if (probe.identity_present) {
    return (
      <Frame>
        <h1 className="mb-4 text-lg font-semibold tracking-tight">This device isn't enrolled yet</h1>
        <p className="text-sm text-muted-foreground">
          An ecosystem identity already exists at <code className="font-mono">{probe.root}</code>, but it doesn't list this device. Enrol it from another
          ecosystem app on this computer, or recover with the kit on a device with nothing on it. A second identity is never created next to the first.
        </p>
      </Frame>
    )
  }
  return <FirstRun onOpened={onOpened} />
}

function Frame({ children }: { children: React.ReactNode }) {
  return (
    <main className="flex min-h-full items-center justify-center bg-surface-locked/70 p-6">
      <section className="w-full max-w-sm rounded-xl border bg-card p-6 shadow-sm">{children}</section>
    </main>
  )
}

function AttachForm({ probe, reason, onOpened }: { probe: Probe; reason?: LockReason | null; onOpened: (s: Status) => void }) {
  const client = useClient()
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const unlock = async (e: FormEvent) => {
    e.preventDefault()
    setBusy(true)
    setError(null)
    try {
      onOpened(await client.unlock())
    } catch (err) {
      setError(describeError(err))
    } finally {
      setBusy(false)
    }
  }
  return (
    <Frame>
      <h1 className="mb-4 text-lg font-semibold tracking-tight">Lockdown</h1>
      <form onSubmit={unlock} className="flex flex-col gap-4">
        <p className="text-sm text-muted-foreground">
          {reason ? REASONS[reason] : "The vault is locked."} This device ({probe.device_name}) opens it with its own key.
        </p>
        {error && <FieldError role="alert">{error}</FieldError>}
        <Button type="submit" disabled={busy} autoFocus>
          {busy ? <Spinner /> : null}
          Unlock
        </Button>
      </form>
    </Frame>
  )
}

/** Screen 1 of the first run (D94): the tagline, once, and the three doors. */
function FirstRun({ onOpened }: { onOpened: (s: Status) => void }) {
  const [creating, setCreating] = useState(false)
  if (creating) {
    return (
      <Frame>
        <FirstRunCreate onOpened={onOpened} onCancel={() => setCreating(false)} />
      </Frame>
    )
  }
  return (
    <Frame>
      <Step
        title="Every way you sign in. Locked down, even from us."
        sentence="One vault for passwords, passkeys and codes, opened by this device's own key."
        learnMore={
          <>
            <p>Lockdown keeps every way you sign in to a site together: the password, the passkeys and the one-time codes of one account are one record.</p>
            <p>The vault is encrypted on this device and opened by a key only this device holds. A server, when you add one, syncs encrypted history it cannot read and keeps a public log this app checks on every sync.</p>
            <p>Export is one click and never gated behind a plan; your own server is a first-class choice.</p>
          </>
        }
      >
        <Tabs defaultValue="create">
          <TabsList className="mb-4 w-full">
            <TabsTrigger value="create">Create</TabsTrigger>
            <TabsTrigger value="join">Join</TabsTrigger>
            <TabsTrigger value="recover">Recover</TabsTrigger>
          </TabsList>
          <TabsContent value="create">
            <div className="flex flex-col gap-4">
              <p className="text-sm text-muted-foreground">Start a new identity on this computer.</p>
              <Button onClick={() => setCreating(true)}>Create</Button>
            </div>
          </TabsContent>
          <TabsContent value="join">
            <JoinForm mode="join" onOpened={onOpened} />
            <JoinWithDeviceToggle onOpened={onOpened} />
          </TabsContent>
          <TabsContent value="recover">
            <JoinForm mode="recover" onOpened={onOpened} />
          </TabsContent>
        </Tabs>
      </Step>
    </Frame>
  )
}

/** Join from an account (password + Secret Key) or recover with the whole kit. */
function JoinForm({ mode, onOpened }: { mode: "join" | "recover"; onOpened: (s: Status) => void }) {
  const client = useClient()
  const { settings } = useSettings()
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [notes, setNotes] = useState<string[]>([])
  const [waiting, setWaiting] = useState<string | null>(null)
  const [emailed, setEmailed] = useState<string | null>(null)
  const submit = async (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    const form = e.currentTarget
    const data = new FormData(form)
    const field = (name: string) => String(data.get(name) ?? "").trim()
    setBusy(true)
    setError(null)
    try {
      const common = { server: field("server"), login: field("login"), password: String(data.get("password") ?? "") }
      const lines =
        mode === "join"
          ? await client.accountLogin({ ...common, secretKey: field("secretKey") || undefined })
          : await client.recover({ ...common, secretKey: field("secretKey"), recoveryCode: field("recoveryCode") })
      setNotes(lines)
      // The typed secrets are cleared as soon as they have been sent.
      form.reset()
      onOpened(await client.openVault(true))
    } catch (err) {
      // A new device on an account with security keys waits for one (D250).
      if (needsSecurityKey(err)) {
        form.reset()
        setEmailed(null)
        setWaiting(field("server"))
      } else if (needsEmailApproval(err)) {
        // The server emailed the account a link to approve this device.
        form.reset()
        setEmailed(describeError(err))
        setWaiting(field("server"))
      } else setError(describeError(err))
    } finally {
      setBusy(false)
    }
  }
  if (waiting !== null)
    return (
      <SecondFactorWait
        server={waiting}
        email={emailed}
        onCancel={() => setWaiting(null)}
        onDone={(lines) => {
          setWaiting(null)
          setNotes(lines)
          void client.openVault(true).then(onOpened, (e) => setError(describeError(e)))
        }}
      />
    )
  return (
    <form onSubmit={submit} className="flex flex-col gap-4">
      <p className="text-sm text-muted-foreground">
        {mode === "join" ? "Enrol this device in an account you already have; another device of yours stays as it is." : "Bring the identity back on a device with nothing on it, with the whole kit."}
      </p>
      <FieldGroup>
        <Field>
          <FieldLabel htmlFor={`${mode}-server`}>Server</FieldLabel>
          <Input id={`${mode}-server`} name="server" type="url" placeholder="https://" defaultValue={settings.server ?? ""} required autoComplete="off" />
        </Field>
        <Field>
          <FieldLabel htmlFor={`${mode}-login`}>Login name</FieldLabel>
          <Input id={`${mode}-login`} name="login" required autoComplete="username" />
        </Field>
        <Field>
          <FieldLabel htmlFor={`${mode}-password`}>Account password</FieldLabel>
          <Input id={`${mode}-password`} name="password" type="password" required autoComplete="current-password" />
        </Field>
        <Field>
          <FieldLabel htmlFor={`${mode}-secret-key`}>Secret Key</FieldLabel>
          <Input id={`${mode}-secret-key`} name="secretKey" type="password" required={mode === "recover"} autoComplete="off" />
          <FieldDescription>{mode === "join" ? "From your recovery kit. Needed when the account was protected with it." : "From your recovery kit."}</FieldDescription>
        </Field>
        {mode === "recover" && (
          <Field>
            <FieldLabel htmlFor="recover-code">Recovery code</FieldLabel>
            <Input id="recover-code" name="recoveryCode" type="password" required autoComplete="off" />
          </Field>
        )}
      </FieldGroup>
      {error && <FieldError role="alert">{error}</FieldError>}
      {notes.length > 0 && (
        <ul className="text-sm text-muted-foreground">
          {notes.map((n) => (
            <li key={n}>{n}</li>
          ))}
        </ul>
      )}
      <Button type="submit" disabled={busy}>
        {busy ? <Spinner /> : null}
        {mode === "join" ? "Join" : "Recover"}
      </Button>
    </form>
  )
}

/** "With another device instead": the approval path, no kit typed. */
function JoinWithDeviceToggle({ onOpened }: { onOpened: (s: Status) => void }) {
  const [open, setOpen] = useState(false)
  if (!open)
    return (
      <div className="mt-2">
        <Button variant="link" size="sm" onClick={() => setOpen(true)}>
          Join with another device instead
        </Button>
      </div>
    )
  return (
    <div className="mt-4 border-t pt-4">
      <JoinWithDevice onOpened={onOpened} />
    </div>
  )
}
