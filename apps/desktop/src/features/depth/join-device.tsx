import { useEffect, useRef, useState, type FormEvent } from "react"

import {
  Button,
  Checkbox,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  Field,
  FieldDescription,
  FieldError,
  FieldGroup,
  FieldLabel,
  Input,
  Spinner,
} from "@unicron/ui"

import { LabelledAction, LabelledActions } from "@/components/labelled-actions"
import { describeError, isAppError } from "@/core/client"
import type { OncePage, Status } from "@/core/types"

import { POLL_MS, isLinkQr, linkCode, useDepthClient, type JoinView } from "./depth-client"

/**
 * "Join from another device" on a device with nothing on it
 * (docs/plan/22-security-depth.md §2): the code (or the QR payload, pasted
 * or read by a camera where the host has one) joins the link the other
 * device opened; both show six digits; once it approves there, the
 * password alone finishes here. No kit is typed; the Secret Key comes with
 * the approval when the other device keeps it.
 */
export function JoinWithDevice({ server, onOpened, scan }: { server?: string; onOpened: (s: Status) => void; scan?: () => Promise<string | null> }) {
  const depth = useDepthClient()
  const [join, setJoin] = useState<JoinView | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const timer = useRef<ReturnType<typeof setInterval> | null>(null)

  useEffect(() => {
    const stop = () => {
      if (timer.current) clearInterval(timer.current)
      timer.current = null
    }
    if (!depth || !join || (join.state !== "waiting" && join.state !== "compare")) return stop
    timer.current = setInterval(() => {
      depth
        .joinPoll()
        .then(setJoin)
        .catch((e) => {
          stop()
          setJoin(null)
          setError(describeError(e))
        })
    }, POLL_MS)
    return stop
  }, [depth, join])

  if (!depth) return null

  const begin = async (invite: string, at: string) => {
    setBusy(true)
    setError(null)
    try {
      setJoin(await depth.joinStart(at, invite))
    } catch (e) {
      setError(describeError(e))
    } finally {
      setBusy(false)
    }
  }

  const start = async (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    const data = new FormData(e.currentTarget)
    const typed = String(data.get("invite") ?? "").trim()
    const invite = isLinkQr(typed) ? typed : linkCode(typed)
    if (!invite) {
      setError("The code is ten letters and digits, like 7KQ2M-9XH4P.")
      return
    }
    await begin(invite, String(data.get("server") ?? server ?? "").trim())
  }

  const scanned = async () => {
    if (!scan) return
    setError(null)
    try {
      const text = await scan()
      if (text && isLinkQr(text)) await begin(text, server ?? "")
      else if (text) setError("That QR code isn't a Lockdown device link.")
    } catch (e) {
      setError(describeError(e))
    }
  }

  const finish = async (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    const form = e.currentTarget
    const data = new FormData(form)
    setBusy(true)
    setError(null)
    try {
      const status = await depth.joinFinish(String(data.get("login") ?? "").trim(), String(data.get("password") ?? ""))
      form.reset()
      onOpened(status)
    } catch (err) {
      setError(describeError(err))
    } finally {
      setBusy(false)
    }
  }

  if (join?.state === "approved") {
    return (
      <form key="finish" onSubmit={(e) => void finish(e)} className="flex flex-col gap-4">
        <p className="text-sm text-muted-foreground">{join.words}</p>
        <FieldGroup>
          <Field>
            <FieldLabel htmlFor="joined-login">Login name</FieldLabel>
            <Input id="joined-login" name="login" required autoComplete="username" />
          </Field>
          <Field>
            <FieldLabel htmlFor="joined-password">Master password</FieldLabel>
            <Input id="joined-password" name="password" type="password" required autoComplete="current-password" />
          </Field>
        </FieldGroup>
        {error && <FieldError role="alert">{error}</FieldError>}
        <Button type="submit" disabled={busy}>
          {busy ? <Spinner /> : null}
          Finish
        </Button>
      </form>
    )
  }

  if (join) {
    return (
      <div className="flex flex-col items-center gap-3" aria-live="polite">
        <p className="text-sm text-muted-foreground">{join.words}</p>
        {join.sas ? (
          <p className="font-mono text-4xl tracking-[0.3em]" aria-label="Digits to compare">
            {join.sas}
          </p>
        ) : (
          <Spinner />
        )}
        <Button
          variant="ghost-muted"
          onClick={() => {
            void depth.joinCancel()
            setJoin(null)
          }}
        >
          Cancel
        </Button>
      </div>
    )
  }

  return (
    <form key="start" onSubmit={(e) => void start(e)} className="flex flex-col gap-4">
      <p className="text-sm text-muted-foreground">
        On a device that already has Lockdown, open Devices and choose “Add a device”, then type the code it shows. Nothing from your kit is needed.
      </p>
      <FieldGroup>
        {server === undefined && (
          <Field>
            <FieldLabel htmlFor="link-server">Server</FieldLabel>
            <Input id="link-server" name="server" type="url" placeholder="https://" autoComplete="off" />
            <FieldDescription>Not needed when you paste the QR code's text.</FieldDescription>
          </Field>
        )}
        <Field>
          <FieldLabel htmlFor="link-code">Code from the other device</FieldLabel>
          <Input id="link-code" name="invite" required autoComplete="off" spellCheck={false} />
        </Field>
      </FieldGroup>
      {error && <FieldError role="alert">{error}</FieldError>}
      <Button type="submit" disabled={busy}>
        {busy ? <Spinner /> : null}
        Join
      </Button>
      {scan && (
        <Button type="button" variant="outline" onClick={() => void scanned()} disabled={busy}>
          Scan the QR code
        </Button>
      )}
    </form>
  )
}

/**
 * A sign-in waiting for a security key (§4): the key is used on the
 * server's page, which opens in the system browser; this waits for it.
 * With `email` set the server mailed the account a link to approve this
 * device instead (docs/plan/25-self-host-2.md §5): no page opens, and the
 * wait is the same.
 */
export function SecondFactorWait({
  server,
  email,
  onDone,
  onCancel,
}: {
  server: string
  email?: string | null
  onDone: (lines: string[]) => void
  onCancel: () => void
}) {
  const depth = useDepthClient()
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    if (!depth) return
    if (!email) void depth.secondFactorOpen(server).catch((e) => setError(describeError(e)))
    const timer = setInterval(() => {
      depth
        .secondFactorWait()
        .then((lines) => {
          if (lines) {
            clearInterval(timer)
            onDone(lines)
          }
        })
        .catch((e) => {
          clearInterval(timer)
          setError(describeError(e))
        })
    }, POLL_MS)
    return () => clearInterval(timer)
  }, [depth, server, email, onDone])

  return (
    <div className="flex flex-col items-center gap-3" aria-live="polite">
      <p className="text-sm">
        {email
          ? `${email.charAt(0).toUpperCase()}${email.slice(1)}.`
          : "Your account asks for a security key on a new device. Use it on the page that opened in your browser."}
      </p>
      {error ? <FieldError role="alert">{error}</FieldError> : <Spinner />}
      <Button variant="ghost-muted" onClick={onCancel}>
        Cancel
      </Button>
      {depth && !email && (
        <Button variant="link" size="sm" onClick={() => void depth.secondFactorOpen(server).catch((e) => setError(describeError(e)))}>
          Open the page again
        </Button>
      )}
    </div>
  )
}

/**
 * Making the account without a Secret Key (04-security §3's opt-out): the
 * password alone then protects it against a stolen server database, so it
 * is asked for in words and confirmed.
 */
export function OptOutRegister({ open, server, onClose, onRegistered }: { open: boolean; server: string; onClose: () => void; onRegistered: (page: OncePage) => void }) {
  const depth = useDepthClient()
  const [understood, setUnderstood] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  if (!depth) return null

  const submit = async (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    const form = e.currentTarget
    const data = new FormData(form)
    const text = (n: string) => String(data.get(n) ?? "").trim()
    setBusy(true)
    setError(null)
    try {
      const page = await depth.accountRegisterOptedOut({ server: text("server"), login: text("login"), password: String(data.get("password") ?? "") }, understood)
      form.reset()
      onRegistered(page)
    } catch (err) {
      setError(isAppError(err) && err.detail === "confirm_opt_out" ? "Tick the box to confirm first." : describeError(err))
    } finally {
      setBusy(false)
    }
  }

  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent>
        <form onSubmit={(e) => void submit(e)} className="flex flex-col gap-4">
          <DialogHeader>
            <DialogTitle>An account without a Secret Key</DialogTitle>
            <DialogDescription>
              Without it, your password alone protects your account: someone who copies the server's database can try guessing it offline, and
              only its length slows them. Choose this only with a long, unique password.
            </DialogDescription>
          </DialogHeader>
          <FieldGroup>
            <Field>
              <FieldLabel htmlFor="optout-server">Server</FieldLabel>
              <Input id="optout-server" name="server" type="url" defaultValue={server} required autoComplete="off" />
            </Field>
            <Field>
              <FieldLabel htmlFor="optout-login">Login name</FieldLabel>
              <Input id="optout-login" name="login" required autoComplete="username" />
            </Field>
            <Field>
              <FieldLabel htmlFor="optout-password">Master password</FieldLabel>
              <Input id="optout-password" name="password" type="password" required autoComplete="new-password" />
            </Field>
            <Field orientation="horizontal">
              <Checkbox id="optout-understood" checked={understood} onCheckedChange={(v) => setUnderstood(v === true)} />
              <FieldLabel htmlFor="optout-understood">I understand my password alone protects this account.</FieldLabel>
            </Field>
          </FieldGroup>
          {error && <FieldError role="alert">{error}</FieldError>}
          <LabelledActions>
            <LabelledAction kind="cancel" type="button" onClick={onClose} disabled={busy}>
              Keep the Secret Key
            </LabelledAction>
            <LabelledAction kind="destructive" type="submit" disabled={busy}>
              Make it without
            </LabelledAction>
          </LabelledActions>
        </form>
      </DialogContent>
    </Dialog>
  )
}
