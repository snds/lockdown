import { useState, type FormEvent, type ReactNode } from "react"

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

import { describeError } from "@/core/client"
import type { Status } from "@/core/types"
import { needsSecurityKey } from "@/features/depth/depth-client"
import { JoinWithDevice } from "@/features/depth/join-device"

import { cameraScanner } from "../core/camera-scan"
import type { WebSession } from "../core/web-client"
import { answerWithKey, type GetRequest } from "../core/webauthn"

interface SignInScreenProps {
  session: WebSession
  /** The server this page came from: the only one it may sign in to. */
  server: string
  onSignedIn: (status: Status) => void
  /** The browser's WebAuthn, for an account that asks for a security key. */
  credentials?: CredentialsContainer
}

export function Frame({ children }: { children: ReactNode }) {
  return (
    <main className="flex min-h-full items-center justify-center bg-surface-locked/70 p-6">
      <section className="w-full max-w-sm rounded-xl border bg-card p-6 shadow-sm">{children}</section>
    </main>
  )
}

/**
 * The web vault's first screen: sign in to an account with the password
 * and the Secret Key from the recovery kit, as on any new device. The
 * password is stretched and checked in this tab's worker (OPAQUE); the
 * server never receives anything it could test a guess against.
 */
export function SignInScreen({ session, server, onSignedIn, credentials }: SignInScreenProps) {
  const scan = cameraScanner()
  return (
    <Frame>
      <h1 className="mb-1 text-lg font-semibold tracking-tight">Lockdown</h1>
      <p className="mb-4 text-sm text-muted-foreground">
        The web vault for <span className="font-mono">{new URL(server).host}</span>. This tab becomes one of your devices until you sign out.
      </p>
      <Tabs defaultValue="sign-in">
        <TabsList className="mb-4 w-full">
          <TabsTrigger value="sign-in">Sign in</TabsTrigger>
          <TabsTrigger value="device">With a device</TabsTrigger>
          <TabsTrigger value="new">No account yet?</TabsTrigger>
        </TabsList>
        <TabsContent value="sign-in">
          <SignInForm session={session} server={server} onSignedIn={onSignedIn} credentials={credentials} />
        </TabsContent>
        <TabsContent value="device">
          <JoinWithDevice server={server} onOpened={onSignedIn} scan={scan} />
        </TabsContent>
        <TabsContent value="new">
          <NoAccountYet />
        </TabsContent>
      </Tabs>
    </Frame>
  )
}

function SignInForm({ session, server, onSignedIn, credentials }: Pick<SignInScreenProps, "session" | "server" | "onSignedIn" | "credentials">) {
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [waiting, setWaiting] = useState<GetRequest | null>(null)
  const answerKey = async () => {
    if (!waiting || !credentials) return
    setBusy(true)
    setError(null)
    try {
      const answer = await answerWithKey(credentials, waiting)
      if (answer) onSignedIn(await session.signInAnswer(answer))
    } catch (err) {
      setError(describeError(err))
    } finally {
      setBusy(false)
    }
  }
  const submit = async (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    const form = e.currentTarget
    const data = new FormData(form)
    const login = String(data.get("login") ?? "").trim()
    const password = String(data.get("password") ?? "")
    const secretKey = String(data.get("secretKey") ?? "").trim()
    setBusy(true)
    setError(null)
    try {
      const status = await session.signIn({ server, login, password, secretKey })
      form.reset()
      onSignedIn(status)
    } catch (err) {
      // The password passed; the account asks this new device for a key (D250).
      if (needsSecurityKey(err)) {
        form.reset()
        setWaiting(await session.signInWaiting())
        return
      }
      setError(describeError(err))
      const pw = form.elements.namedItem("password")
      if (pw instanceof HTMLInputElement) pw.value = ""
    } finally {
      setBusy(false)
    }
  }
  if (waiting)
    return (
      <div className="flex flex-col gap-4">
        <p className="text-sm">Your password is right. Your account asks a new device for a security key too: use it now.</p>
        {error && <FieldError role="alert">{error}</FieldError>}
        <Button onClick={() => void answerKey()} disabled={busy || !credentials}>
          {busy ? <Spinner /> : null}
          Use my security key
        </Button>
      </div>
    )
  return (
    <form onSubmit={submit} className="flex flex-col gap-4" aria-label="Sign in">
      <FieldGroup>
        <Field>
          <FieldLabel htmlFor="sign-in-login">Login name</FieldLabel>
          <Input id="sign-in-login" name="login" required autoComplete="username" />
        </Field>
        <Field>
          <FieldLabel htmlFor="sign-in-password">Account password</FieldLabel>
          <Input id="sign-in-password" name="password" type="password" required autoComplete="current-password" />
        </Field>
        <Field>
          <FieldLabel htmlFor="sign-in-secret-key">Secret Key</FieldLabel>
          <Input id="sign-in-secret-key" name="secretKey" type="password" required autoComplete="off" spellCheck={false} placeholder="A1-…" />
          <FieldDescription>From your recovery kit. Every new device needs it once, and this tab is a new device.</FieldDescription>
        </Field>
      </FieldGroup>
      {error && <FieldError role="alert">{error}</FieldError>}
      <Button type="submit" disabled={busy}>
        {busy ? <Spinner /> : null}
        {busy ? "Signing in…" : "Sign in"}
      </Button>
    </form>
  )
}

/** Where an account comes from: the app or the command line, never this page (D176). */
function NoAccountYet() {
  return (
    <div className="flex flex-col gap-3 text-sm text-muted-foreground">
      <p>
        Make your account in the Lockdown app on your computer (on first run, or Status, then Register), or with <code className="font-mono">lockdown account register</code>.
        Keep the recovery kit it shows: its Secret Key signs this browser in beside your password.
      </p>
      <p>
        The web vault opens accounts but never makes one: an identity made inside a browser would be stretched less than your other devices accept, and they
        would rightly refuse it.
      </p>
    </div>
  )
}
