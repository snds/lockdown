import { useEffect, useState, type FormEvent } from "react"

import {
  Button,
  Field,
  FieldDescription,
  FieldError,
  FieldGroup,
  FieldLabel,
  Input,
  Spinner,
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
} from "@unicron/ui"

import { describeError, useClient } from "@/core/client"
import type { LockReason, Probe, Status } from "@/core/types"

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
 * - nothing at all → **Create** a new identity here, or **Join** an
 *   account from another device (login name, password, Secret Key), or
 *   **Recover** with the whole kit when no other device is left;
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
      <Frame title="Lockdown can't start">
        <p role="alert" className="text-sm text-destructive">
          {probeError}
        </p>
      </Frame>
    )
  }
  if (!probe) {
    return (
      <Frame title="Lockdown">
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
      <Frame title="This device isn't enrolled yet">
        <p className="text-sm text-muted-foreground">
          An ecosystem identity already exists at <code className="font-mono">{probe.root}</code>, but it doesn't list
          this device. Enrol it from another ecosystem app on this computer, or recover with the kit on a device with
          nothing on it. A second identity is never created next to the first.
        </p>
      </Frame>
    )
  }
  return <FirstRun probe={probe} onOpened={onOpened} />
}

function Frame({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <main className="flex min-h-full items-center justify-center bg-surface-locked p-6">
      <section className="w-full max-w-sm rounded-xl border bg-card p-6 shadow-sm">
        <h1 className="mb-4 text-lg font-semibold tracking-tight">{title}</h1>
        {children}
      </section>
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
    <Frame title="Lockdown">
      <form onSubmit={unlock} className="flex flex-col gap-4">
        <p className="text-sm text-muted-foreground">
          {reason ? REASONS[reason] : "The vault is locked."} This device ({probe.device_name}) opens it with its own
          key; nothing leaves this computer to do so.
        </p>
        {error && (
          <FieldError role="alert" className="text-sm">
            {error}
          </FieldError>
        )}
        <Button type="submit" disabled={busy} autoFocus>
          {busy ? <Spinner /> : null}
          Unlock
        </Button>
      </form>
    </Frame>
  )
}

function FirstRun({ probe, onOpened }: { probe: Probe; onOpened: (s: Status) => void }) {
  return (
    <Frame title="Every way you sign in. Locked down, even from us.">
      <p className="mb-4 text-sm text-muted-foreground">
        Nothing is set up on this computer yet ({probe.root}). Start a new identity here, or join one you already have
        on another device.
      </p>
      <Tabs defaultValue="create">
        <TabsList className="mb-4 w-full">
          <TabsTrigger value="create">Create</TabsTrigger>
          <TabsTrigger value="join">Join</TabsTrigger>
          <TabsTrigger value="recover">Recover</TabsTrigger>
        </TabsList>
        <TabsContent value="create">
          <CreateForm onOpened={onOpened} />
        </TabsContent>
        <TabsContent value="join">
          <JoinForm mode="join" onOpened={onOpened} />
        </TabsContent>
        <TabsContent value="recover">
          <JoinForm mode="recover" onOpened={onOpened} />
        </TabsContent>
      </Tabs>
    </Frame>
  )
}

function CreateForm({ onOpened }: { onOpened: (s: Status) => void }) {
  const client = useClient()
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const create = async (e: FormEvent) => {
    e.preventDefault()
    setBusy(true)
    setError(null)
    try {
      onOpened(await client.openVault(true))
    } catch (err) {
      setError(describeError(err))
    } finally {
      setBusy(false)
    }
  }
  return (
    <form onSubmit={create} className="flex flex-col gap-4">
      <p className="text-sm text-muted-foreground">
        Makes the shared ecosystem identity and your first vault, keyed to this device. An account on a server and the
        recovery kit come after, from the Status screen.
      </p>
      {error && (
        <FieldError role="alert" className="text-sm">
          {error}
        </FieldError>
      )}
      <Button type="submit" disabled={busy}>
        {busy ? <Spinner /> : null}
        Create
      </Button>
    </form>
  )
}

/** Join from an account (password + Secret Key) or recover with the whole kit. */
function JoinForm({ mode, onOpened }: { mode: "join" | "recover"; onOpened: (s: Status) => void }) {
  const client = useClient()
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [notes, setNotes] = useState<string[]>([])
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
      setError(describeError(err))
    } finally {
      setBusy(false)
    }
  }
  return (
    <form onSubmit={submit} className="flex flex-col gap-4">
      <FieldGroup>
        <Field>
          <FieldLabel htmlFor={`${mode}-server`}>Server</FieldLabel>
          <Input id={`${mode}-server`} name="server" type="url" placeholder="https://" required autoComplete="off" />
          <FieldDescription>Your own server is a first-class choice.</FieldDescription>
        </Field>
        <Field>
          <FieldLabel htmlFor={`${mode}-login`}>Login name</FieldLabel>
          <Input id={`${mode}-login`} name="login" required autoComplete="username" />
        </Field>
        <Field>
          <FieldLabel htmlFor={`${mode}-password`}>Account password</FieldLabel>
          <Input id={`${mode}-password`} name="password" type="password" required autoComplete="current-password" />
          <FieldDescription>Never leaves this device (OPAQUE).</FieldDescription>
        </Field>
        <Field>
          <FieldLabel htmlFor={`${mode}-secret-key`}>Secret Key</FieldLabel>
          <Input id={`${mode}-secret-key`} name="secretKey" type="password" required={mode === "recover"} autoComplete="off" />
          <FieldDescription>
            {mode === "join"
              ? "From your recovery kit. Needed when the account was protected with it."
              : "From your recovery kit."}
          </FieldDescription>
        </Field>
        {mode === "recover" && (
          <Field>
            <FieldLabel htmlFor="recover-code">Recovery code</FieldLabel>
            <Input id="recover-code" name="recoveryCode" type="password" required autoComplete="off" />
          </Field>
        )}
      </FieldGroup>
      {error && (
        <FieldError role="alert" className="text-sm">
          {error}
        </FieldError>
      )}
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
