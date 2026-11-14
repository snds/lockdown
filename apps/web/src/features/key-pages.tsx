import { useEffect, useState, type FormEvent } from "react"

import { Button, EmptyState, Skeleton, TextField } from "@unicron/ui"

import { describeError } from "@/core/client"
import { ClosedHeading } from "@/features/unlock/closed-frame"

import type { Link } from "../core/link"
import { answerWithKey, registerKey, type CreateRequest, type GetRequest } from "../core/webauthn"
import { Frame } from "./sign-in-screen"

interface KeyPageProps {
  link: Link
  id: string
  credentials: CredentialsContainer
}

/** The request this page answers is gone or can't be read: the server's words, and Try again (brief W2). */
function RequestGone({ overline, words, onTryAgain }: { overline: string; words: string; onTryAgain: () => void }) {
  return (
    <Frame>
      <EmptyState tone="critical" role="alert" overline={overline} title={<h1>This request isn't waiting.</h1>} body={words} actionLabel="Try again" onAction={onTryAgain} />
    </Frame>
  )
}

/**
 * `#/second-factor/<id>`: the page the desktop opens in the system browser
 * when a new device's sign-in waits for a security key
 * (docs/plan/22-security-depth.md §4; brief W2). It names the device asking,
 * uses the key with this browser's WebAuthn, and hands the answer to the
 * server, which checks it. No account is signed in here, and there is no
 * way into a vault from it.
 */
export function SecondFactorPage({ link, id, credentials }: KeyPageProps) {
  const [request, setRequest] = useState<GetRequest | null>(null)
  const [lookup, setLookup] = useState<string | null>(null)
  const [attempt, setAttempt] = useState(0)
  const [state, setState] = useState<"ready" | "busy" | "done">("ready")
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    let live = true
    link
      .call<GetRequest>("page_sign_in_request", { pendingId: id })
      .then((r) => live && setRequest(r))
      .catch((e) => live && setLookup(describeError(e)))
    return () => {
      live = false
    }
  }, [link, id, attempt])

  const use = async () => {
    if (!request) return
    setState("busy")
    setError(null)
    try {
      const answer = await answerWithKey(credentials, request)
      if (!answer) {
        setState("ready")
        return
      }
      await link.call("page_sign_in_answer", { pendingId: id, answer })
      setState("done")
    } catch (e) {
      setState("ready")
      setError(describeError(e))
    }
  }

  if (lookup)
    return (
      <RequestGone
        overline="Security key // sign-in"
        words={lookup}
        onTryAgain={() => {
          setLookup(null)
          setAttempt((n) => n + 1)
        }}
      />
    )
  if (state === "done")
    return (
      <Frame>
        <ClosedHeading overline="Security key // sign-in" title="Done." focusTitle />
        <p role="status" className="type-body text-text-secondary">
          Go back to the Lockdown app; it carries on by itself. You can close this tab.
        </p>
      </Frame>
    )
  return (
    <Frame>
      <ClosedHeading overline="Security key // sign-in" title="Confirm a new sign-in.">
        {request ? `${request.device_label} is signing in to your account and asks for your security key. Use it only if that's you, right now.` : null}
      </ClosedHeading>
      {request === null ? (
        <Skeleton variant="row" aria-hidden />
      ) : (
        <div className="flex flex-col gap-3">
          <div>
            <Button onClick={() => void use()} loading={state === "busy"}>
              Use my security key
            </Button>
          </div>
          {error && (
            <p role="alert" className="type-caption text-critical-text">
              {error}
            </p>
          )}
        </div>
      )}
    </Frame>
  )
}

/**
 * `#/security-key/<id>`: adding a security key or passkey to the account,
 * from the page the app opened (brief W2). The browser makes the
 * credential; the server checks it and keeps its public key.
 */
export function SecurityKeyPage({ link, id, credentials }: KeyPageProps) {
  const [request, setRequest] = useState<CreateRequest | null>(null)
  const [lookup, setLookup] = useState<string | null>(null)
  const [attempt, setAttempt] = useState(0)
  const [state, setState] = useState<"ready" | "busy" | "done">("ready")
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    let live = true
    link
      .call<CreateRequest>("page_key_request", { pendingId: id })
      .then((r) => live && setRequest(r))
      .catch((e) => live && setLookup(describeError(e)))
    return () => {
      live = false
    }
  }, [link, id, attempt])

  const add = async (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    if (!request) return
    const name = String(new FormData(e.currentTarget).get("name") ?? "").trim() || "Security key"
    setState("busy")
    setError(null)
    try {
      const answer = await registerKey(credentials, request, name)
      if (!answer) {
        setState("ready")
        return
      }
      await link.call("page_key_finish", { pendingId: id, answer })
      setState("done")
    } catch (err) {
      setState("ready")
      setError(describeError(err))
    }
  }

  if (lookup)
    return (
      <RequestGone
        overline="Security key // add"
        words={lookup}
        onTryAgain={() => {
          setLookup(null)
          setAttempt((n) => n + 1)
        }}
      />
    )
  if (state === "done")
    return (
      <Frame>
        <ClosedHeading overline="Security key // add" title="Added." focusTitle />
        <p role="status" className="type-body text-text-secondary">
          A new device now asks for this key after your password. You can close this tab.
        </p>
      </Frame>
    )
  return (
    <Frame>
      <ClosedHeading overline="Security key // add" title="Add a security key.">
        {request ? `For ${request.user_name}. Keep a second key somewhere safe: with every key lost, only the server's operator can clear them.` : null}
      </ClosedHeading>
      {request === null ? (
        <Skeleton variant="row" aria-hidden />
      ) : (
        <form onSubmit={(e) => void add(e)} className="flex flex-col gap-6" aria-label="Add a security key">
          <TextField label="Name" name="name" maxLength={64} placeholder="Blue key" autoComplete="off" helper="So you can tell your keys apart later." />
          {error && (
            <p role="alert" className="type-caption text-critical-text">
              {error}
            </p>
          )}
          <div>
            <Button type="submit" loading={state === "busy"}>
              Add key
            </Button>
          </div>
        </form>
      )}
    </Frame>
  )
}
