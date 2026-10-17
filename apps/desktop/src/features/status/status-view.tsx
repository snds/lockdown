import { useCallback, useEffect, useState, type FormEvent, type ReactNode } from "react"

import {
  Badge,
  Button,
  Dialog,
  DialogAction,
  DialogActions,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  Field,
  FieldDescription,
  FieldError,
  FieldGroup,
  FieldLabel,
  Icon,
  Input,
  Item,
  ItemActions,
  ItemContent,
  ItemDescription,
  ItemGroup,
  ItemTitle,
  NativeSelect,
  NativeSelectOption,
  Spinner,
  Switch,
} from "@unicron/ui"

import { describeError, useClient } from "@/core/client"
import { decodeAndWipe } from "@/core/secret-bytes"
import type { DeviceInfo, LockState, OncePage as OncePageData, Status } from "@/core/types"
import { dateLocal, shortId, whenLocal } from "@/lib/format"

import { OncePage, type ShownPage } from "./once-page"

interface StatusViewProps {
  status: Status
  lock: LockState | null
  onStatus: (status: Status) => void
  onLock: (state: LockState) => void
  onLockNow: () => void
  onNotice: (text: string) => void
}

const IDLE_CHOICES: Array<[string, number | null]> = [
  ["1 minute", 60],
  ["5 minutes", 300],
  ["10 minutes", 600],
  ["30 minutes", 1800],
  ["1 hour", 3600],
  ["4 hours", 14_400],
  ["Never", null],
]

/**
 * Identity, device, persona, vault head, the account and sync, the kit,
 * the devices, and the lock timers. Every proof of "locked, even from us"
 * will live here (design brief §1); today it lists the facts the core has.
 */
export function StatusView({ status, lock, onStatus, onLock, onLockNow, onNotice }: StatusViewProps) {
  const client = useClient()
  const [busy, setBusy] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [devices, setDevices] = useState<DeviceInfo[] | null>(null)
  const [revoking, setRevoking] = useState<DeviceInfo | null>(null)
  const [dialog, setDialog] = useState<"register" | "login" | "kit-create" | "kit-verify" | null>(null)
  const [once, setOnce] = useState<ShownPage | null>(null)
  const showOnce = (page: OncePageData) =>
    setOnce({ title: "Your recovery kit", lines: page.lines, text: page.page.length ? decodeAndWipe(page.page) : null })

  const refresh = useCallback(async () => {
    onStatus(await client.status())
    setDevices(await client.deviceList())
  }, [client, onStatus])

  useEffect(() => {
    client.deviceList().then(setDevices).catch((e) => setError(describeError(e)))
  }, [client])

  const run = async (name: string, action: () => Promise<string[] | void>) => {
    setBusy(name)
    setError(null)
    try {
      const lines = await action()
      await refresh()
      if (lines && lines.length) onNotice(lines.join(" "))
    } catch (e) {
      setError(describeError(e))
    } finally {
      setBusy(null)
    }
  }

  const setIdle = (value: string) =>
    run("policy", async () => {
      const idle = value === "" ? null : Number(value)
      onLock(await client.setPolicy({ idle_secs: idle, hidden_secs: lock?.hidden_timeout_secs ?? null, sleep_gap_secs: 30 }))
    })

  const setHiddenLock = (on: boolean) =>
    run("policy", async () => {
      onLock(await client.setPolicy({ idle_secs: lock?.idle_timeout_secs ?? null, hidden_secs: on ? 300 : null, sleep_gap_secs: 30 }))
    })

  return (
    <div className="mx-auto flex w-full max-w-3xl flex-col gap-6 p-4 md:p-6">
      <h1 className="text-xl font-semibold tracking-tight">Status</h1>
      {error && (
        <p role="alert" className="text-sm text-destructive">
          {error}
        </p>
      )}

      <Section title="This device">
        <Fact label="Identity root" value={status.root} mono />
        <Fact label="Data" value={status.data} mono />
        <Fact label="Profile" value={`${shortId(status.profile_id)} · managed by ${status.manager ?? "no app"}`} />
        <Fact label="Device" value={`${status.device_name} (${shortId(status.device_id)})`} />
        <Fact label="Persona" value={status.persona_label} />
        <Fact label="Vault" value={`${shortId(status.vault_id)} · ${status.live_items} live items`} />
        <Fact label="Vault head" value={`seq ${status.head_seq} · ${status.head_hash.slice(0, 16)}…`} mono />
        <Fact
          label="Recovery kit"
          value={status.recovery === "none" ? "not made yet" : status.recovery === "drilled" ? "made and drilled" : "made, not drilled"}
          tone={status.recovery === "none" ? "attention" : "secure"}
        />
      </Section>

      <Section
        title="Account and sync"
        actions={
          status.account ? (
            <Button size="sm" onClick={() => run("sync", async () => (await client.sync()).lines)} disabled={busy !== null}>
              {busy === "sync" ? <Spinner /> : <Icon name="sync" data-icon="inline-start" />}
              Sync now
            </Button>
          ) : (
            <>
              <Button size="sm" variant="outline" onClick={() => setDialog("login")}>
                Log in
              </Button>
              <Button size="sm" onClick={() => setDialog("register")}>
                Register
              </Button>
            </>
          )
        }
      >
        {status.account ? (
          <>
            <Fact label="Server" value={status.account.server} mono />
            <Fact label="Login" value={status.account.login} />
            <Fact label="Synced" value={`seq ${status.account.synced_seq} · item revision ${status.account.synced_rev}`} />
            <Fact label="Last sync" value={whenLocal(status.account.last_sync_at)} />
            <Fact label="Session until" value={whenLocal(status.account.session_expires_at)} />
            <Fact label="Identity records" value={`manifest seq ${status.identity_seq}`} />
            <div className="pt-1">
              <Button size="sm" variant="ghost-muted" onClick={() => setDialog("login")}>
                Log in again
              </Button>
            </div>
          </>
        ) : (
          <p className="text-sm text-muted-foreground">
            No account on this device. Register one on a server (yours or ours) to sync, or log in to one you have.
          </p>
        )}
      </Section>

      <Section
        title="Recovery kit"
        actions={
          <>
            {status.recovery !== "none" && (
              <Button size="sm" variant="outline" onClick={() => setDialog("kit-verify")}>
                Verify
              </Button>
            )}
            <Button size="sm" onClick={() => setDialog("kit-create")}>
              {status.recovery === "none" ? "Create" : "Replace"}
            </Button>
          </>
        }
      >
        <p className="text-sm text-muted-foreground">
          One printed page: a Secret Key and a recovery code. The Secret Key protects the account beside the password;
          the server has never seen it. The whole kit gets you back in when every device is gone.
        </p>
      </Section>

      <Section title={`Devices (${status.devices}${status.devices_revoked ? `, ${status.devices_revoked} revoked` : ""})`}>
        {!devices && <Spinner />}
        <ItemGroup className="gap-1">
          {devices?.map((d) => (
            <Item key={d.id} size="sm" variant="muted">
              <ItemContent>
                <ItemTitle>
                  {d.name}
                  {d.this_device && (
                    <Badge variant="secondary" className="ml-2">
                      this device
                    </Badge>
                  )}
                  {d.revoked_at && (
                    <Badge variant="destructive" className="ml-2">
                      revoked {dateLocal(d.revoked_at)}
                    </Badge>
                  )}
                </ItemTitle>
                <ItemDescription>
                  {d.platform} · enrolled {dateLocal(d.enrolled_at)} · {shortId(d.id)}
                </ItemDescription>
              </ItemContent>
              {!d.this_device && !d.revoked_at && (
                <ItemActions>
                  <Button size="sm" variant="ghost-destructive" onClick={() => setRevoking(d)}>
                    Revoke
                  </Button>
                </ItemActions>
              )}
            </Item>
          ))}
        </ItemGroup>
      </Section>

      <Section
        title="Lock"
        actions={
          <Button size="sm" variant="outline" onClick={onLockNow}>
            <Icon name="lock" data-icon="inline-start" />
            Lock now
          </Button>
        }
      >
        <FieldGroup>
          <Field orientation="horizontal">
            <FieldLabel htmlFor="idle-timeout">Lock after idle</FieldLabel>
            <NativeSelect
              id="idle-timeout"
              value={lock?.idle_timeout_secs === null || lock?.idle_timeout_secs === undefined ? "" : String(lock.idle_timeout_secs)}
              onChange={(e) => setIdle(e.target.value)}
              disabled={busy === "policy"}
            >
              {IDLE_CHOICES.map(([label, secs]) => (
                <NativeSelectOption key={label} value={secs === null ? "" : String(secs)}>
                  {label}
                </NativeSelectOption>
              ))}
            </NativeSelect>
          </Field>
          <Field orientation="horizontal">
            <FieldLabel htmlFor="hidden-lock">Lock when the window is out of view for 5 minutes</FieldLabel>
            <Switch id="hidden-lock" checked={!!lock?.hidden_timeout_secs} onCheckedChange={setHiddenLock} disabled={busy === "policy"} />
          </Field>
          <FieldDescription>
            The vault also locks when this computer wakes from sleep. The core enforces every timer; this screen only
            sets them.
          </FieldDescription>
        </FieldGroup>
      </Section>

      <AccountDialog
        kind={dialog === "register" || dialog === "login" ? dialog : null}
        onClose={() => setDialog(null)}
        onDone={(lines, page) => {
          setDialog(null)
          if (page && page.page.length) showOnce(page)
          void run("account", async () => lines)
        }}
      />
      <KitDialog
        kind={dialog === "kit-create" || dialog === "kit-verify" ? dialog : null}
        hasAccount={status.account !== null}
        onClose={() => setDialog(null)}
        onDone={(lines, page) => {
          setDialog(null)
          if (page) showOnce(page)
          void run("kit", async () => lines)
        }}
      />
      <OncePage page={once} onClose={() => setOnce(null)} />

      <Dialog open={revoking !== null} onOpenChange={(o) => !o && setRevoking(null)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Revoke {revoking?.name}?</DialogTitle>
            <DialogDescription>
              Nothing it signs from now on is accepted by the server or by any other device. Its earlier history still
              verifies. A revocation never comes back.
            </DialogDescription>
          </DialogHeader>
          <DialogActions>
            <DialogAction kind="cancel" onClick={() => setRevoking(null)}>
              Cancel
            </DialogAction>
            <DialogAction
              kind="destructive"
              onClick={() => {
                const id = revoking?.id
                setRevoking(null)
                if (id) void run("revoke", () => client.deviceRevoke(id))
              }}
            >
              Revoke
            </DialogAction>
          </DialogActions>
        </DialogContent>
      </Dialog>
    </div>
  )
}

function Section({ title, actions, children }: { title: string; actions?: ReactNode; children: ReactNode }) {
  return (
    <section className="flex flex-col gap-3 rounded-xl border bg-card p-4">
      <div className="flex items-center justify-between gap-2">
        <h2 className="font-medium">{title}</h2>
        <div className="flex items-center gap-1">{actions}</div>
      </div>
      {children}
    </section>
  )
}

function Fact({ label, value, mono, tone }: { label: string; value: string; mono?: boolean; tone?: "secure" | "attention" }) {
  return (
    <div className="flex flex-col gap-0.5 text-sm sm:flex-row sm:gap-3">
      <span className="w-36 shrink-0 text-muted-foreground">{label}</span>
      <span className={[mono ? "font-mono break-all" : "", tone === "secure" ? "text-secure" : "", tone === "attention" ? "text-attention" : ""].join(" ")}>
        {value}
      </span>
    </div>
  )
}

function AccountDialog({
  kind,
  onClose,
  onDone,
}: {
  kind: "register" | "login" | null
  onClose: () => void
  onDone: (lines: string[], page: OncePageData | null) => void
}) {
  const client = useClient()
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const submit = async (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    const form = e.currentTarget
    const data = new FormData(form)
    const text = (n: string) => String(data.get(n) ?? "").trim()
    const input = { server: text("server"), login: text("login"), password: String(data.get("password") ?? ""), secretKey: text("secretKey") || undefined }
    setBusy(true)
    setError(null)
    try {
      if (kind === "register") {
        const page = await client.accountRegister(input)
        form.reset()
        onDone(page.lines, page)
      } else {
        const lines = await client.accountLogin(input)
        form.reset()
        onDone(lines, null)
      }
    } catch (err) {
      setError(describeError(err))
    } finally {
      setBusy(false)
    }
  }
  return (
    <Dialog open={kind !== null} onOpenChange={(o) => !o && onClose()}>
      <DialogContent>
        <form onSubmit={submit} className="flex flex-col gap-4">
          <DialogHeader>
            <DialogTitle>{kind === "register" ? "Register an account" : "Log in"}</DialogTitle>
            <DialogDescription>
              {kind === "register"
                ? "The password never leaves this device (OPAQUE). If no recovery kit exists yet, one is made now and shown once."
                : "A fresh session on this device. Give the Secret Key too to finish protecting an older account."}
            </DialogDescription>
          </DialogHeader>
          <FieldGroup>
            <Field>
              <FieldLabel htmlFor="acct-server">Server</FieldLabel>
              <Input id="acct-server" name="server" type="url" placeholder="https://" required autoComplete="off" />
            </Field>
            <Field>
              <FieldLabel htmlFor="acct-login">Login name</FieldLabel>
              <Input id="acct-login" name="login" required autoComplete="username" />
            </Field>
            <Field>
              <FieldLabel htmlFor="acct-password">Account password</FieldLabel>
              <Input id="acct-password" name="password" type="password" required autoComplete={kind === "register" ? "new-password" : "current-password"} />
            </Field>
            <Field>
              <FieldLabel htmlFor="acct-secret-key">Secret Key</FieldLabel>
              <Input id="acct-secret-key" name="secretKey" type="password" autoComplete="off" />
              <FieldDescription>Only when a recovery kit already exists.</FieldDescription>
            </Field>
          </FieldGroup>
          {error && (
            <FieldError role="alert" className="text-sm">
              {error}
            </FieldError>
          )}
          <DialogActions>
            <DialogAction kind="cancel" type="button" onClick={onClose} disabled={busy}>
              Cancel
            </DialogAction>
            <DialogAction kind="confirm" type="submit" disabled={busy}>
              {kind === "register" ? "Register" : "Log in"}
            </DialogAction>
          </DialogActions>
        </form>
      </DialogContent>
    </Dialog>
  )
}

function KitDialog({
  kind,
  hasAccount,
  onClose,
  onDone,
}: {
  kind: "kit-create" | "kit-verify" | null
  hasAccount: boolean
  onClose: () => void
  onDone: (lines: string[], page: OncePageData | null) => void
}) {
  const client = useClient()
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const submit = async (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    const form = e.currentTarget
    const data = new FormData(form)
    const text = (n: string) => String(data.get(n) ?? "").trim()
    const password = String(data.get("password") ?? "") || undefined
    setBusy(true)
    setError(null)
    try {
      if (kind === "kit-create") {
        const page = await client.kitCreate({ replace: data.get("replace") === "on", password })
        form.reset()
        onDone(page.lines, page)
      } else {
        const lines = await client.kitVerify({ secretKey: text("secretKey"), recoveryCode: text("recoveryCode"), password, drill: data.get("drill") === "on" })
        form.reset()
        onDone(lines, null)
      }
    } catch (err) {
      setError(describeError(err))
    } finally {
      setBusy(false)
    }
  }
  return (
    <Dialog open={kind !== null} onOpenChange={(o) => !o && onClose()}>
      <DialogContent>
        <form onSubmit={submit} className="flex flex-col gap-4">
          <DialogHeader>
            <DialogTitle>{kind === "kit-create" ? "Recovery kit" : "Verify the kit"}</DialogTitle>
            <DialogDescription>
              {kind === "kit-create"
                ? "Makes the page and shows it once. With an account, the account password is needed: its slot takes the new Secret Key."
                : "Types the kit back in and checks it opens this identity. Changes nothing unless you record the drill."}
            </DialogDescription>
          </DialogHeader>
          <FieldGroup>
            {kind === "kit-verify" && (
              <>
                <Field>
                  <FieldLabel htmlFor="kit-secret-key">Secret Key</FieldLabel>
                  <Input id="kit-secret-key" name="secretKey" type="password" required autoComplete="off" />
                </Field>
                <Field>
                  <FieldLabel htmlFor="kit-recovery-code">Recovery code</FieldLabel>
                  <Input id="kit-recovery-code" name="recoveryCode" type="password" required autoComplete="off" />
                </Field>
              </>
            )}
            {hasAccount && (
              <Field>
                <FieldLabel htmlFor="kit-password">Account password</FieldLabel>
                <Input id="kit-password" name="password" type="password" required={kind === "kit-create"} autoComplete="current-password" />
                {kind === "kit-verify" && <FieldDescription>Optional: also checks the Secret Key against the account slot.</FieldDescription>}
              </Field>
            )}
            {kind === "kit-create" ? (
              <Field orientation="horizontal">
                <FieldLabel htmlFor="kit-replace">Replace the kit that exists (the old page stops working)</FieldLabel>
                <Switch id="kit-replace" name="kit-replace-switch" onCheckedChange={(on) => ((document.getElementById("kit-replace-value") as HTMLInputElement).value = on ? "on" : "")} />
                <input type="hidden" id="kit-replace-value" name="replace" defaultValue="" />
              </Field>
            ) : (
              <Field orientation="horizontal">
                <FieldLabel htmlFor="kit-drill">Record a passing check as the recovery drill</FieldLabel>
                <Switch id="kit-drill" onCheckedChange={(on) => ((document.getElementById("kit-drill-value") as HTMLInputElement).value = on ? "on" : "")} />
                <input type="hidden" id="kit-drill-value" name="drill" defaultValue="" />
              </Field>
            )}
          </FieldGroup>
          {error && (
            <FieldError role="alert" className="text-sm">
              {error}
            </FieldError>
          )}
          <DialogActions>
            <DialogAction kind="cancel" type="button" onClick={onClose} disabled={busy}>
              Cancel
            </DialogAction>
            <DialogAction kind="confirm" type="submit" disabled={busy}>
              {kind === "kit-create" ? "Make the kit" : "Verify"}
            </DialogAction>
          </DialogActions>
        </form>
      </DialogContent>
    </Dialog>
  )
}
