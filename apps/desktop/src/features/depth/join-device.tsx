import { useEffect, useRef, useState, type FormEvent } from "react"

import {
  Banner,
  Button,
  Checkbox,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  Field,
  FieldError,
  FieldGroup,
  FieldLabel,
  Input,
  Spinner,
  TextField,
} from "@unicron/ui"
import { SecretField } from "@unicron/ui/security/secret-field"

import { LabelledAction, LabelledActions } from "@/components/labelled-actions"
import { describeError, isAppError } from "@/core/client"
import type { OncePage, Status } from "@/core/types"
import { WelcomeScreen } from "@/features/unlock/welcome"

import { POLL_MS, isLinkQr, linkCode, useDepthClient, type JoinView } from "./depth-client"

/** Where a join from another device stands, for the frame's folio (brief F3). */
export type JoinStage = "code" | "compare" | "finish"

/** Brings a refusal that appeared below the fold into view, so it is seen as well as announced. */
export function useShownWhen(on: unknown) {
  const ref = useRef<HTMLDivElement>(null)
  useEffect(() => {
    if (on) ref.current?.scrollIntoView?.({ block: "nearest" })
  }, [on])
  return ref
}

/** The core's words, as a sentence. */
const sentence = (words: string) => `${words.charAt(0).toUpperCase()}${words.slice(1)}${/[.!?]$/.test(words) ? "" : "."}`

/**
 * "Join from another device" on a device with nothing on it
 * (docs/plan/22-security-depth.md §2; brief F3): the code (or the QR
 * payload, pasted or read by a camera where the host has one) joins the
 * link the other device opened; both show six digits; once it approves
 * there, the password alone finishes here. No kit is typed; the Secret Key
 * comes with the approval when the other device keeps it.
 *
 * It draws inside its host's focus frame (the desktop's screen J, the web
 * sign-in's With a device tab): Text Fields, one primary (Join, then
 * Finish), and the other device's refusal as a compact critical Banner.
 * `onStage` tells the host where it stands, for a folio.
 */
export function JoinWithDevice({
  server,
  onOpened,
  scan,
  onStage,
}: {
  server?: string
  onOpened: (s: Status) => void
  scan?: () => Promise<string | null>
  onStage?: (stage: JoinStage) => void
}) {
  const depth = useDepthClient()
  const [join, setJoin] = useState<JoinView | null>(null)
  // A code or QR that isn't one: under the code field. Anything the core refused: a Banner.
  const [codeError, setCodeError] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const [password, setPassword] = useState("")
  const timer = useRef<ReturnType<typeof setInterval> | null>(null)
  const codeField = useRef<HTMLInputElement>(null)
  const refusal = useShownWhen(error)

  const stage: JoinStage = join?.state === "approved" ? "finish" : join ? "compare" : "code"
  useEffect(() => {
    onStage?.(stage)
  }, [onStage, stage])

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
    setCodeError(null)
    if (!invite) {
      setCodeError("The code is ten letters and digits, like 7KQ2M-9XH4P.")
      return
    }
    await begin(invite, String(data.get("server") ?? server ?? "").trim())
  }

  const scanned = async () => {
    if (!scan) return
    setError(null)
    setCodeError(null)
    try {
      const text = await scan()
      if (text && isLinkQr(text)) await begin(text, server ?? "")
      else if (text) setCodeError("That QR code isn't a Lockdown device link.")
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
      const status = await depth.joinFinish(String(data.get("login") ?? "").trim(), password)
      form.reset()
      setPassword("")
      onOpened(status)
    } catch (err) {
      setError(describeError(err))
    } finally {
      setBusy(false)
    }
  }

  if (join?.state === "approved") {
    return (
      <form key="finish" onSubmit={(e) => void finish(e)} aria-label="Finish joining" className="flex flex-col gap-6">
        <p className="type-body text-text-body" aria-live="polite">
          {sentence(join.words)}
        </p>
        <TextField id="joined-login" name="login" label="Login name" required autoComplete="username" readOnly={busy} />
        <SecretField
          mode="edit"
          id="joined-password"
          label="Master password"
          name="password"
          mono={false}
          value={password}
          onChange={setPassword}
          required
          disabled={busy}
          autoComplete="current-password"
          description="The one you use on your other devices."
        />
        {error ? <Banner tone="critical" size="compact" title={error} /> : null}
        <div className="flex flex-col gap-3">
          <Button size="lg" block type="submit" loading={busy}>
            Finish
          </Button>
        </div>
      </form>
    )
  }

  if (join) {
    return (
      <div className="flex flex-col gap-6" aria-live="polite">
        <p className="type-body text-text-body">{sentence(join.words)}</p>
        {join.sas ? (
          // The digits stay plain text the web's device-link test reads ("418 902"); DigitGroups
          // splits them into spans with no space, so the swap waits for the integrator (doc 78 §4).
          <p className="type-code-display text-text-body tabular-nums" aria-label="Digits to compare">
            {join.sas}
          </p>
        ) : (
          <div>
            <Spinner />
          </div>
        )}
        {/* A text button hangs its word on the column's edge, as the title does. */}
        <div className="-ms-4">
          <Button
            variant="ghost"
            onClick={() => {
              void depth.joinCancel()
              setJoin(null)
            }}
          >
            Cancel
          </Button>
        </div>
      </div>
    )
  }

  return (
    <form key="start" onSubmit={(e) => void start(e)} aria-label="Join with a code" className="flex flex-col gap-6" noValidate>
      <p className="type-caption text-text-secondary">
        On a device that already has Lockdown, open Devices and choose Add device, then type the code it shows. Nothing from your kit is needed.
      </p>
      {server === undefined && (
        <TextField
          id="link-server"
          name="server"
          type="url"
          label="Server"
          placeholder="https://"
          autoComplete="off"
          inputMode="url"
          helper="Not needed when you paste the QR code's text."
        />
      )}
      <TextField
        id="link-code"
        name="invite"
        label="Code from the other device"
        ref={codeField}
        required
        autoComplete="off"
        spellCheck={false}
        helper="Ten letters and digits, like 7KQ2M-9XH4P."
        error={codeError ?? undefined}
      />
      {/* The other device said no, or the code ran out: the core's words, and a fresh start. */}
      {error ? (
        <div ref={refusal}>
          <Banner
            tone="critical"
            size="compact"
            title={sentence(error)}
            action={
              <Button
                type="button"
                variant="secondary"
                size="sm"
                onClick={() => {
                  setError(null)
                  const field = codeField.current
                  if (field) {
                    field.value = ""
                    field.focus()
                  }
                }}
              >
                Try again
              </Button>
            }
          />
        </div>
      ) : null}
      <div className="flex flex-col gap-3">
        <Button size="lg" block type="submit" loading={busy}>
          Join
        </Button>
        {scan && (
          <Button size="lg" block type="button" variant="ghost" onClick={() => void scanned()} disabled={busy}>
            Scan QR code
          </Button>
        )}
      </div>
    </form>
  )
}

/**
 * A sign-in waiting for a second factor (§4; brief F3), in the same frame:
 * "Use your security key." while the key is used on the server's page,
 * which opens in the system browser. With `email` set the server mailed the
 * account a link to approve this device instead (docs/plan/25-self-host-2.md
 * §5): "Check your email.", the core's words with the masked address, no
 * page. A live line while it waits; Open page again and Cancel as text
 * buttons. A cancelled wait opens nothing.
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
    <>
      <WelcomeScreen
        title={email ? "Check your email." : "Use your security key."}
        sentence={email ? sentence(email) : "Your account asks for a security key on a new device. Use it on the page that opened in your browser."}
      />
      {error ? (
        <Banner tone="critical" size="compact" title={sentence(error)} />
      ) : (
        <p role="status" className="type-caption text-text-secondary flex items-center gap-3">
          <Spinner aria-hidden />
          {email ? "Waiting for the approval." : "Waiting for your security key."}
        </p>
      )}
      {/* Text buttons hang their words on the column's edge, as the title does. */}
      <div className="-ms-4 flex flex-wrap gap-2">
        {depth && !email && (
          <Button variant="ghost" onClick={() => void depth.secondFactorOpen(server).catch((e) => setError(describeError(e)))}>
            Open page again
          </Button>
        )}
        <Button variant="ghost" onClick={onCancel}>
          Cancel
        </Button>
      </div>
    </>
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
