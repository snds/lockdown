import { useState, type FormEvent, type ReactNode } from "react"

import {
  Button,
  Checkbox,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  Field,
  FieldDescription,
  FieldError,
  FieldGroup,
  FieldLabel,
  Input,
  Label,
  Spinner,
} from "@unicron/ui"

import { useSettings } from "@/app/settings"
import { describeError, useClient } from "@/core/client"
import { decodeAndWipe } from "@/core/secret-bytes"
import type { Status } from "@/core/types"
import { OptOutRegister } from "@/features/depth/join-device"
import { OncePage, type ShownPage } from "@/features/status/once-page"

/**
 * Creating from nothing (D94: three screens at most, one sentence each,
 * detail behind "Learn more"; D95: the proofs once, at account creation).
 *
 * 1. What this is, with the tagline (shown once in the app).
 * 2. The Secret Key and the kit: an account on a server makes both, and
 *    the kit page is shown once. "This device only" skips the account and
 *    makes the kit alone.
 * 3. Recovery picks: what to do with the kit, and the drill reminder.
 *
 * When an account was made, the proof moment follows as one panel of
 * three lines, then the vault. Nothing explanatory beyond one sentence
 * per screen lives in the flow itself.
 */
export type FirstRunStep = "intro" | "key" | "recovery" | "proof"

interface FirstRunProps {
  /** The intro is drawn by the unlock screen (it holds the join door too); this starts at step 2. */
  onOpened: (status: Status) => void
  onCancel: () => void
}

export function FirstRunCreate({ onOpened, onCancel }: FirstRunProps) {
  const client = useClient()
  const { settings } = useSettings()
  const [step, setStep] = useState<Exclude<FirstRunStep, "intro">>("key")
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [status, setStatus] = useState<Status | null>(null)
  const [withAccount, setWithAccount] = useState(false)
  const [kit, setKit] = useState<ShownPage | null>(null)
  const [kitOpen, setKitOpen] = useState(false)
  const [printed, setPrinted] = useState(false)
  const [remind, setRemind] = useState(true)
  const [optOut, setOptOut] = useState(false)

  const createIdentity = async () => (status ?? (await client.openVault(true)))

  const makeAccount = async (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    const form = e.currentTarget
    const data = new FormData(form)
    const text = (n: string) => String(data.get(n) ?? "").trim()
    setBusy(true)
    setError(null)
    try {
      const opened = await createIdentity()
      setStatus(opened)
      const page = await client.accountRegister({ server: text("server"), login: text("login"), password: String(data.get("password") ?? "") })
      form.reset()
      setWithAccount(true)
      setKit({ title: "Your recovery kit", lines: page.lines, text: page.page.length ? decodeAndWipe(page.page) : null })
      setKitOpen(true)
      setStep("recovery")
    } catch (err) {
      setError(describeError(err))
    } finally {
      setBusy(false)
    }
  }

  const deviceOnly = async () => {
    setBusy(true)
    setError(null)
    try {
      const opened = await createIdentity()
      setStatus(opened)
      const page = await client.kitCreate({ replace: false })
      setKit({ title: "Your recovery kit", lines: page.lines, text: page.page.length ? decodeAndWipe(page.page) : null })
      setKitOpen(true)
      setStep("recovery")
    } catch (err) {
      setError(describeError(err))
    } finally {
      setBusy(false)
    }
  }

  const finish = async () => {
    // The page's string lives only while this flow does.
    setKitOpen(false)
    setKit(null)
    const final = await client.status().catch(() => status)
    if (final) onOpened(final)
  }

  if (step === "key") {
    return (
      <Step
        title="Your Secret Key and recovery kit"
        sentence="An account on a server syncs your vault between devices and makes the one page that gets you back in: the recovery kit."
        learnMore={
          <>
            <p>The account password never leaves this device; the server keeps a record it cannot turn back into the password (OPAQUE).</p>
            <p>The recovery kit is a Secret Key and a recovery code, made here and shown once. The Secret Key is mixed into the key that opens your account, so the server alone can never open it. The whole kit opens the account when every device is gone.</p>
            <p>"This device only" makes the kit without an account. You can add an account later from Status.</p>
          </>
        }
      >
        <form onSubmit={makeAccount} className="flex flex-col gap-4">
          <FieldGroup>
            <Field>
              <FieldLabel htmlFor="fr-server">Server</FieldLabel>
              <Input id="fr-server" name="server" type="url" placeholder="https://" defaultValue={settings.server ?? ""} required autoComplete="off" />
              <FieldDescription>Yours or ours.</FieldDescription>
            </Field>
            <Field>
              <FieldLabel htmlFor="fr-login">Login name</FieldLabel>
              <Input id="fr-login" name="login" required autoComplete="username" />
            </Field>
            <Field>
              <FieldLabel htmlFor="fr-password">Account password</FieldLabel>
              <Input id="fr-password" name="password" type="password" required autoComplete="new-password" />
            </Field>
          </FieldGroup>
          {error && <FieldError role="alert">{error}</FieldError>}
          <div className="flex flex-col gap-2">
            <Button type="submit" disabled={busy}>
              {busy ? <Spinner /> : null}
              Create account
            </Button>
            <Button type="button" variant="ghost-muted" onClick={deviceOnly} disabled={busy}>
              This device only
            </Button>
            <Button
              type="button"
              variant="link"
              size="sm"
              onClick={() =>
                void createIdentity().then(
                  (opened) => {
                    setStatus(opened)
                    setOptOut(true)
                  },
                  (e) => setError(describeError(e)),
                )
              }
              disabled={busy}
            >
              An account without a Secret Key
            </Button>
            <Button type="button" variant="link" size="sm" onClick={onCancel} disabled={busy}>
              Back
            </Button>
          </div>
        </form>
        <OptOutRegister
          open={optOut}
          server={settings.server ?? ""}
          onClose={() => setOptOut(false)}
          onRegistered={(page) => {
            setOptOut(false)
            setWithAccount(true)
            setKit({ title: "Your recovery kit", lines: page.lines, text: page.page.length ? decodeAndWipe(page.page) : null })
            setKitOpen(true)
            setStep("recovery")
          }}
        />
      </Step>
    )
  }

  if (step === "recovery") {
    return (
      <>
        <Step
          title="Keep the kit somewhere safe"
          sentence="Print the page, save it as a file or write it down, away from your devices; the drill checks it works before you need it."
          learnMore={
            <>
              <p>Lockdown keeps no copy of the kit. A printed page in a drawer, or a line in a safe place you already trust, is what most people lose least.</p>
              <p>A saved file holds everything needed to get into your account with your password, so keep it off this computer: on a USB drive, as a printed copy, or in a safe.</p>
              <p>The recovery drill asks you to type the kit back in. Most people learn their recovery fails at the worst moment; the drill finds out now, and the Recovery screen reminds you until it has passed.</p>
            </>
          }
        >
          <div className="flex flex-col gap-3">
            <div className="flex items-center gap-2">
              <Checkbox id="fr-printed" checked={printed} onCheckedChange={(c) => setPrinted(c === true)} />
              <Label htmlFor="fr-printed">I saved, printed or wrote down the kit</Label>
            </div>
            <div className="flex items-center gap-2">
              <Checkbox id="fr-remind" checked={remind} onCheckedChange={(c) => setRemind(c === true)} />
              <Label htmlFor="fr-remind">Remind me to run the recovery drill</Label>
            </div>
            <Button type="button" variant="outline" onClick={() => setKitOpen(true)} disabled={kit === null}>
              Show the kit again
            </Button>
            <Button type="button" onClick={() => (withAccount ? setStep("proof") : void finish())} disabled={!printed}>
              Continue
            </Button>
          </div>
        </Step>
        <OncePage page={kitOpen ? kit : null} onClose={() => setKitOpen(false)} />
      </>
    )
  }

  return (
    <Step title="Locked, even from us" sentence="Three things are true from now on; the Status screen can re-check them any time.">
      <ul className="flex flex-col gap-2 text-sm">
        <li>Your password never leaves this device.</li>
        <li>The Secret Key on your kit is one we have never seen.</li>
        <li>Your vault history is signed by your devices, and the server's log is checked on every sync.</li>
      </ul>
      <Button type="button" onClick={() => void finish()}>
        Open the vault
      </Button>
    </Step>
  )
}

/** One first-run screen: a title, one sentence, "Learn more", the controls. */
export function Step({ title, sentence, learnMore, children }: { title: string; sentence: string; learnMore?: ReactNode; children: ReactNode }) {
  const [open, setOpen] = useState(false)
  return (
    <section className="flex flex-col gap-4">
      <div className="flex flex-col gap-1">
        <h1 className="text-lg font-semibold tracking-tight">{title}</h1>
        {/* "Learn more" reads as a link at rest (design review R20): unicron's
            link variant underlines only on hover, in the same ink as the
            sentence, so the sentence underlines it; it sits on the
            sentence's baseline in its size. */}
        <p className="text-sm text-muted-foreground [&>[data-slot=button]]:text-foreground [&>[data-slot=button]]:underline [&>[data-slot=button]]:underline-offset-4">
          {sentence}
          {learnMore && (
            <>
              {" "}
              <Button variant="link" size="inline" onClick={() => setOpen(true)}>
                Learn more
              </Button>
            </>
          )}
        </p>
      </div>
      {children}
      {learnMore && (
        <Dialog open={open} onOpenChange={setOpen}>
          <DialogContent>
            <DialogHeader>
              <DialogTitle>{title}</DialogTitle>
              <DialogDescription>{sentence}</DialogDescription>
            </DialogHeader>
            <div className="flex flex-col gap-3 text-sm">{learnMore}</div>
            <DialogFooter>
              <Button onClick={() => setOpen(false)}>Got it</Button>
            </DialogFooter>
          </DialogContent>
        </Dialog>
      )}
    </section>
  )
}
