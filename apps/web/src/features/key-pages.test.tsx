import { render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { describe, expect, it, vi } from "vitest"

import type { Status } from "@/core/types"

import type { Link } from "../core/link"
import type { WebSession } from "../core/web-client"
import { SecondFactorPage, SecurityKeyPage } from "./key-pages"
import { SignInScreen } from "./sign-in-screen"

const ab = (bytes: number[]) => new Uint8Array(bytes).buffer
const assertion = { rawId: ab([9]), response: { clientDataJSON: ab([1]), authenticatorData: ab([2]), signature: ab([3]) } } as unknown as PublicKeyCredential
const getReq = { pending_id: "p", challenge: [1], rp_id: "vault.example", allow: [[9]], timeout_ms: 1000, expires_at: 1, device_label: "Lockdown on Sam's laptop" }

function fakeLink(answers: Record<string, unknown>): Link & { call: ReturnType<typeof vi.fn> } {
  const call = vi.fn(async (cmd: string) => {
    if (cmd in answers) return answers[cmd]
    throw { code: "refused", message: `no ${cmd}` }
  })
  return { call, callBytes: vi.fn() } as unknown as Link & { call: ReturnType<typeof vi.fn> }
}

describe("the key pages the desktop opens", () => {
  it("confirms a waiting sign-in with the key, naming the device", async () => {
    const link = fakeLink({ page_sign_in_request: getReq, page_sign_in_answer: null })
    const credentials = { get: vi.fn(async () => assertion) } as unknown as CredentialsContainer
    render(<SecondFactorPage link={link} id="p" credentials={credentials} />)
    expect(await screen.findByText(/Sam's laptop is signing in/)).toBeInTheDocument()
    await userEvent.click(screen.getByRole("button", { name: "Use my security key" }))
    expect(await screen.findByRole("heading", { level: 1, name: "Done." })).toBeInTheDocument()
    expect(screen.getByText(/Go back to the Lockdown app/)).toHaveAttribute("role", "status")
    expect(link.call).toHaveBeenCalledWith("page_sign_in_answer", {
      pendingId: "p",
      answer: { credentialId: [9], clientDataJson: [1], authenticatorData: [2], signature: [3] },
    })
  })

  it("adds a key under the name given", async () => {
    const link = fakeLink({
      page_key_request: { pending_id: "k", challenge: [4], rp_id: "vault.example", rp_name: "Lockdown", user_id: [1], user_name: "me@example.com", exclude: [], timeout_ms: 1000 },
      page_key_finish: { id: "x", name: "Blue key" },
    })
    const made = { rawId: ab([5]), response: { clientDataJSON: ab([6]), attestationObject: ab([7]) } } as unknown as PublicKeyCredential
    const credentials = { create: vi.fn(async () => made) } as unknown as CredentialsContainer
    render(<SecurityKeyPage link={link} id="k" credentials={credentials} />)
    await screen.findByText(/For me@example.com/)
    await userEvent.type(screen.getByLabelText("Name"), "Blue key")
    await userEvent.click(screen.getByRole("button", { name: "Add key" }))
    expect(await screen.findByRole("heading", { level: 1, name: "Added." })).toBeInTheDocument()
    expect(screen.getByText(/You can close this tab/)).toHaveAttribute("role", "status")
    expect(link.call).toHaveBeenCalledWith("page_key_finish", { pendingId: "k", answer: { name: "Blue key", clientDataJson: [6], attestationObject: [7] } })
  })

  it("says so when the sign-in is no longer waiting, and offers no key to use", async () => {
    const link = fakeLink({})
    render(<SecondFactorPage link={link} id="p" credentials={{} as CredentialsContainer} />)
    // Brief W2: a request that is gone is a critical Empty State in the server's words, with Try again.
    expect(await screen.findByRole("alert")).toHaveTextContent("no page_sign_in_request")
    expect(screen.getByRole("heading", { level: 1, name: "This request isn't waiting." })).toBeInTheDocument()
    expect(screen.queryByRole("button", { name: "Use my security key" })).toBeNull()
    await userEvent.click(screen.getByRole("button", { name: "Try again" }))
    await waitFor(() => expect(link.call).toHaveBeenCalledTimes(2))
  })

  it("keeps the key button's label while the key is in use (cross-app decisions §9)", async () => {
    const link = fakeLink({ page_sign_in_request: getReq })
    const credentials = { get: vi.fn(() => new Promise(() => {})) } as unknown as CredentialsContainer
    render(<SecondFactorPage link={link} id="p" credentials={credentials} />)
    await userEvent.click(await screen.findByRole("button", { name: "Use my security key" }))
    const busy = screen.getByRole("button", { name: /Use my security key$/ })
    expect(busy).toHaveAttribute("aria-busy", "true")
    expect(busy).toHaveTextContent("Use my security key")
  })
})

describe("a web sign-in that asks for a security key", () => {
  it("uses the key in this tab after the password passed", async () => {
    const s: WebSession = {
      state: vi.fn(),
      signIn: vi.fn(async () => Promise.reject({ code: "approval_needed", message: "use your key", detail: "security_key" })),
      unlock: vi.fn(),
      signOut: vi.fn(),
      signInWaiting: vi.fn(async () => getReq),
      signInAnswer: vi.fn(async () => ({}) as Status),
      signInContinue: vi.fn(async () => null),
    }
    const onSignedIn = vi.fn()
    const credentials = { get: vi.fn(async () => assertion) } as unknown as CredentialsContainer
    render(<SignInScreen session={s} server="https://vault.example" onSignedIn={onSignedIn} credentials={credentials} />)
    await userEvent.type(screen.getByLabelText("Login name"), "me@example.com")
    await userEvent.type(screen.getByLabelText("Master password"), "correct horse")
    await userEvent.type(screen.getByLabelText("Secret Key"), "A1-ABC")
    await userEvent.click(screen.getByRole("button", { name: "Sign in" }))
    await userEvent.click(await screen.findByRole("button", { name: "Use my security key" }))
    await waitFor(() => expect(onSignedIn).toHaveBeenCalled())
    expect(s.signInAnswer).toHaveBeenCalledWith({ credentialId: [9], clientDataJson: [1], authenticatorData: [2], signature: [3] })
  })
})
