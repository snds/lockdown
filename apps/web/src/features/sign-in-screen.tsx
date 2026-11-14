import { useEffect, useState, type FormEvent, type ReactNode } from "react"

import { Banner, Button, EmptyState, Skeleton, Spinner, Tabs, TabsContent, TabsList, TabsTrigger, TextField } from "@unicron/ui"
import { SecretField } from "@unicron/ui/security/secret-field"

import { describeError } from "@/core/client"
import type { Status } from "@/core/types"
import { needsEmailApproval, needsSecurityKey } from "@/features/depth/depth-client"
import { JoinWithDevice } from "@/features/depth/join-device"
import { ClosedFrame, ClosedHeading } from "@/features/unlock/closed-frame"

import { cameraScanner } from "../core/camera-scan"
import type { WebSession } from "../core/web-client"
import { answerWithKey, type GetRequest } from "../core/webauthn"

/** How often a sign-in approved by email is asked after. */
const EMAIL_POLL_MS = 2_000

interface SignInScreenProps {
  session: WebSession
  /** The server this page came from: the only one it may sign in to. */
  server: string
  onSignedIn: (status: Status) => void
  /** The browser's WebAuthn, for an account that asks for a security key. */
  credentials?: CredentialsContainer
}

/**
 * Every page of the web vault before the vault, and the pages without one
 * (brief W1, W2): the focus frame, one 432 column with the lockup at its
 * top. `end` is the screen's primary as a phone float button.
 */
export function Frame({ children, end }: { children: ReactNode; end?: ReactNode }) {
  return <ClosedFrame end={end}>{children}</ClosedFrame>
}

/** The overline of the web vault's own pages: `WEB VAULT // VAULT.EXAMPLE`. The host is its own text, as written. */
export function webVaultLine(server: string): ReactNode {
  return (
    <>
      Web vault // <span>{new URL(server).host}</span>
    </>
  )
}

/** The page's worker hasn't answered yet: the lockup and "Starting the vault…" (brief W1). For `web-app.tsx` to mount. */
export function WebStarting() {
  return (
    <Frame>
      <h1 className="sr-only">Lockdown web vault</h1>
      <div className="flex flex-col gap-4" role="status">
        <p className="type-body text-text-secondary">Starting the vault…</p>
        <Skeleton variant="row" aria-hidden />
      </div>
    </Frame>
  )
}

/** The page can't start (the core module failed its check, say): the reason, and Try again, which reloads it (brief W1). For `web-app.tsx` to mount. */
export function WebCantStart({ server, reason, onTryAgain }: { server: string; reason: string; onTryAgain: () => void }) {
  return (
    <Frame>
      <EmptyState tone="critical" role="alert" overline={webVaultLine(server)} title={<h1>The web vault can't start.</h1>} body={reason} actionLabel="Try again" onAction={onTryAgain} />
    </Frame>
  )
}

type Step = { kind: "form" } | { kind: "key"; request: GetRequest } | { kind: "email"; words: string }

/**
 * The web vault's first screen (brief W1): sign in to an account with the
 * password and the Secret Key from the recovery kit, as on any new device.
 * The password is stretched and checked in this tab's worker (OPAQUE); the
 * server never receives anything it could test a guess against. A security
 * key or an emailed approval is a step of its own, in the same frame.
 */
export function SignInScreen({ session, server, onSignedIn, credentials }: SignInScreenProps) {
  const scan = cameraScanner()
  const [step, setStep] = useState<Step>({ kind: "form" })
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const back = () => {
    setError(null)
    setStep({ kind: "form" })
  }

  // The server emailed a link to approve this tab: wait for it here.
  const emailed = step.kind === "email"
  useEffect(() => {
    if (!emailed) return
    const timer = setInterval(() => {
      session
        .signInContinue()
        .then((status) => {
          if (status) {
            clearInterval(timer)
            onSignedIn(status)
          }
        })
        .catch((e) => {
          clearInterval(timer)
          setStep({ kind: "form" })
          setError(describeError(e))
        })
    }, EMAIL_POLL_MS)
    return () => clearInterval(timer)
  }, [emailed, session, onSignedIn])

  if (step.kind === "key") {
    const answerKey = async () => {
      if (!credentials) return
      setBusy(true)
      setError(null)
      try {
        const answer = await answerWithKey(credentials, step.request)
        if (answer) onSignedIn(await session.signInAnswer(answer))
      } catch (err) {
        setError(describeError(err))
      } finally {
        setBusy(false)
      }
    }
    return (
      <Frame>
        <ClosedHeading overline={webVaultLine(server)} title="Use your security key." focusTitle>
          Your password is right. Your account asks a new device for a security key too: use it now.
        </ClosedHeading>
        <div className="flex flex-col gap-3">
          <div className="flex flex-wrap items-start gap-4">
            <Button onClick={() => void answerKey()} loading={busy} disabled={!credentials} disabledReason={credentials ? undefined : "This browser can't use a security key."}>
              Use my security key
            </Button>
            <Button variant="ghost" onClick={back}>
              Cancel
            </Button>
          </div>
          {error && (
            <p role="alert" className="type-caption text-critical-text">
              {error}
            </p>
          )}
        </div>
      </Frame>
    )
  }
  if (step.kind === "email") {
    return (
      <Frame>
        <ClosedHeading overline={webVaultLine(server)} title="Check your email." focusTitle />
        <div className="flex items-start gap-3" aria-live="polite">
          <Spinner aria-hidden />
          <p className="type-body text-text-body">{`${step.words.charAt(0).toUpperCase()}${step.words.slice(1)}.`}</p>
        </div>
        <div className="-ms-4">
          <Button variant="ghost" onClick={back}>
            Cancel
          </Button>
        </div>
      </Frame>
    )
  }
  return (
    <Frame>
      <ClosedHeading overline={webVaultLine(server)} title="Sign in.">
        This tab becomes one of your devices until you sign out.
      </ClosedHeading>
      <Tabs defaultValue="sign-in">
        <TabsList variant="line">
          <TabsTrigger value="sign-in">Sign in</TabsTrigger>
          <TabsTrigger value="device">With a device</TabsTrigger>
          <TabsTrigger value="new">No account yet?</TabsTrigger>
        </TabsList>
        <TabsContent value="sign-in">
          <div className="pt-6">
            <SignInForm
              session={session}
              server={server}
              onSignedIn={onSignedIn}
              busy={busy}
              setBusy={setBusy}
              error={error}
              setError={setError}
              onKey={(request) => setStep({ kind: "key", request })}
              onEmailed={(words) => setStep({ kind: "email", words })}
            />
          </div>
        </TabsContent>
        <TabsContent value="device">
          <div className="pt-6">
            <JoinWithDevice server={server} onOpened={onSignedIn} scan={scan} />
          </div>
        </TabsContent>
        <TabsContent value="new">
          <div className="pt-6">
            <NoAccountYet />
          </div>
        </TabsContent>
      </Tabs>
    </Frame>
  )
}

interface SignInFormProps extends Pick<SignInScreenProps, "session" | "server" | "onSignedIn"> {
  busy: boolean
  setBusy: (busy: boolean) => void
  error: string | null
  setError: (error: string | null) => void
  /** The password passed; the account asks this new device for a key (D250). */
  onKey: (request: GetRequest) => void
  /** The server mailed a link to approve this tab (D300), in its words. */
  onEmailed: (words: string) => void
}

function SignInForm({ session, server, onSignedIn, busy, setBusy, error, setError, onKey, onEmailed }: SignInFormProps) {
  // Typed secrets are held only while the form is up, and cleared as soon as they are sent.
  const [password, setPassword] = useState("")
  const [secretKey, setSecretKey] = useState("")
  const submit = async (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    const form = e.currentTarget
    const login = String(new FormData(form).get("login") ?? "").trim()
    const typed = { password, secretKey: secretKey.trim() }
    setBusy(true)
    setError(null)
    try {
      const status = await session.signIn({ server, login, ...typed })
      form.reset()
      setPassword("")
      setSecretKey("")
      onSignedIn(status)
    } catch (err) {
      if (needsSecurityKey(err)) {
        setPassword("")
        setSecretKey("")
        onKey(await session.signInWaiting())
        return
      }
      if (needsEmailApproval(err)) {
        setPassword("")
        setSecretKey("")
        onEmailed(describeError(err))
        return
      }
      // Too many tries, a wrong password or Secret Key: the server's words, in one compact Banner over the button.
      setError(describeError(err))
      setPassword("")
    } finally {
      setBusy(false)
    }
  }
  return (
    <form onSubmit={submit} className="flex flex-col gap-6" aria-label="Sign in">
      <div className="flex flex-col gap-4">
        <TextField label="Login name" name="login" required autoComplete="username" spellCheck={false} />
        <SecretField mode="edit" label="Master password" name="password" mono={false} required value={password} onChange={setPassword} autoComplete="current-password" />
        <SecretField
          mode="edit"
          label="Secret Key"
          name="secretKey"
          required
          value={secretKey}
          onChange={setSecretKey}
          autoComplete="off"
          placeholder="A1-…"
          description="From your recovery kit. Every new device needs it once, and this tab is a new device."
        />
      </div>
      {error && <Banner tone="critical" size="compact" title={error} />}
      <div>
        <Button type="submit" loading={busy}>
          Sign in
        </Button>
      </div>
    </form>
  )
}

/** Where an account comes from: the app or the command line, never this page (D176). */
function NoAccountYet() {
  return (
    <div className="flex flex-col gap-4 type-body text-text-secondary">
      <p>
        Make your account in the Lockdown app on your computer (on first run, or Status, then Register), or with{" "}
        <code className="type-code-inline text-text-body">lockdown account register</code>. Keep the recovery kit it shows: its Secret Key signs this browser in
        beside your password.
      </p>
      <p>
        The web vault opens accounts but never makes one: an identity made inside a browser would be stretched less than your other devices accept, and they
        would rightly refuse it.
      </p>
    </div>
  )
}
