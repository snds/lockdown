import { useEffect, useState, type FormEvent } from "react"

import { Button, Field, FieldDescription, FieldError, FieldLabel, Input, Spinner } from "@unicron/ui"

import { describeError } from "@/core/client"

import type { Link } from "../core/link"
import { answerWithKey, registerKey, type CreateRequest, type GetRequest } from "../core/webauthn"
import { Frame } from "./sign-in-screen"

interface KeyPageProps {
  link: Link
  id: string
  credentials: CredentialsContainer
}

/**
 * `#/second-factor/<id>`: the page the desktop opens in the system browser
 * when a new device's sign-in waits for a security key
 * (docs/plan/22-security-depth.md §4). It names the device asking, uses
 * the key with this browser's WebAuthn, and hands the answer to the
 * server, which checks it. No account is signed in here.
 */
export function SecondFactorPage({ link, id, credentials }: KeyPageProps) {
  const [request, setRequest] = useState<GetRequest | null>(null)
  const [state, setState] = useState<"ready" | "busy" | "done">("ready")
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    link
      .call<GetRequest>("page_sign_in_request", { pendingId: id })
      .then(setRequest)
      .catch((e) => setError(describeError(e)))
  }, [link, id])

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

  return (
    <Frame>
      <h1 className="mb-1 text-lg font-semibold tracking-tight">Confirm a new sign-in</h1>
      {state === "done" ? (
        <p role="status" className="text-sm">
          Done. Go back to the Lockdown app; it carries on by itself. You can close this tab.
        </p>
      ) : (
        <div className="flex flex-col gap-4">
          <p className="text-sm text-muted-foreground">
            {request
              ? `${request.device_label} is signing in to your account and asks for your security key. Use it only if that's you, right now.`
              : "Looking the sign-in up…"}
          </p>
          {error && <FieldError role="alert">{error}</FieldError>}
          <Button onClick={() => void use()} disabled={!request || state === "busy"}>
            {state === "busy" ? <Spinner /> : null}
            Use my security key
          </Button>
        </div>
      )}
    </Frame>
  )
}

/**
 * `#/security-key/<id>`: adding a security key or passkey to the account,
 * from the page the app opened. The browser makes the credential; the
 * server checks it and keeps its public key.
 */
export function SecurityKeyPage({ link, id, credentials }: KeyPageProps) {
  const [request, setRequest] = useState<CreateRequest | null>(null)
  const [state, setState] = useState<"ready" | "busy" | "done">("ready")
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    link
      .call<CreateRequest>("page_key_request", { pendingId: id })
      .then(setRequest)
      .catch((e) => setError(describeError(e)))
  }, [link, id])

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

  return (
    <Frame>
      <h1 className="mb-1 text-lg font-semibold tracking-tight">Add a security key</h1>
      {state === "done" ? (
        <p role="status" className="text-sm">
          Added. A new device now asks for this key after your password. You can close this tab.
        </p>
      ) : (
        <form onSubmit={(e) => void add(e)} className="flex flex-col gap-4">
          <p className="text-sm text-muted-foreground">
            {request ? `For ${request.user_name}. Keep a second key somewhere safe: with every key lost, only the server's operator can clear them.` : "Looking the request up…"}
          </p>
          <Field>
            <FieldLabel htmlFor="key-name">Name</FieldLabel>
            <Input id="key-name" name="name" maxLength={64} placeholder="Blue key" autoComplete="off" />
            <FieldDescription>So you can tell your keys apart later.</FieldDescription>
          </Field>
          {error && <FieldError role="alert">{error}</FieldError>}
          <Button type="submit" disabled={!request || state === "busy"}>
            {state === "busy" ? <Spinner /> : null}
            Add the key
          </Button>
        </form>
      )}
    </Frame>
  )
}
