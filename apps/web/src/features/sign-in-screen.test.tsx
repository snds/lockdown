import { render, screen } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { describe, expect, it, vi } from "vitest"

import type { Status } from "@/core/types"

import type { WebSession } from "../core/web-client"
import { SignInScreen } from "./sign-in-screen"
import { WebUnlockScreen } from "./web-unlock-screen"

function session(over: Partial<WebSession> = {}): WebSession {
  return {
    state: vi.fn(),
    signIn: vi.fn(async () => ({}) as Status),
    unlock: vi.fn(async () => ({}) as Status),
    signOut: vi.fn(async () => {}),
    signInWaiting: vi.fn(),
    signInAnswer: vi.fn(async () => ({}) as Status),
    ...over,
  }
}

describe("the web vault's sign-in", () => {
  it("signs in with the password and the Secret Key, to the server the page came from", async () => {
    const s = session()
    const onSignedIn = vi.fn()
    render(<SignInScreen session={s} server="https://vault.example" onSignedIn={onSignedIn} />)
    expect(screen.getByText("vault.example")).toBeInTheDocument()
    await userEvent.type(screen.getByLabelText("Login name"), "me@example.com")
    await userEvent.type(screen.getByLabelText("Account password"), "correct horse")
    await userEvent.type(screen.getByLabelText("Secret Key"), "A1-ABC")
    await userEvent.click(screen.getByRole("button", { name: "Sign in" }))
    expect(s.signIn).toHaveBeenCalledWith({ server: "https://vault.example", login: "me@example.com", password: "correct horse", secretKey: "A1-ABC" })
    expect(onSignedIn).toHaveBeenCalled()
    expect(screen.getByLabelText("Account password")).toHaveValue("")
  })

  it("shows the core's words and clears the password when sign-in fails", async () => {
    const s = session({ signIn: vi.fn(async () => Promise.reject({ code: "refused", message: "the login name and password didn't match" })) })
    render(<SignInScreen session={s} server="https://vault.example" onSignedIn={vi.fn()} />)
    await userEvent.type(screen.getByLabelText("Login name"), "me")
    await userEvent.type(screen.getByLabelText("Account password"), "wrong")
    await userEvent.type(screen.getByLabelText("Secret Key"), "A1-ABC")
    await userEvent.click(screen.getByRole("button", { name: "Sign in" }))
    expect(await screen.findByRole("alert")).toHaveTextContent("didn't match")
    expect(screen.getByLabelText("Account password")).toHaveValue("")
  })

  it("explains where an account comes from instead of making one", async () => {
    render(<SignInScreen session={session()} server="https://vault.example" onSignedIn={vi.fn()} />)
    await userEvent.click(screen.getByRole("tab", { name: "No account yet?" }))
    expect(screen.getByText(/never makes one/)).toBeInTheDocument()
  })

  it("unlocks with the password and says why it locked", async () => {
    const s = session()
    const onUnlocked = vi.fn()
    render(<WebUnlockScreen session={s} login="me@example.com" reason="hidden" onUnlocked={onUnlocked} onSignedOut={vi.fn()} />)
    expect(screen.getByText(/out of view/)).toBeInTheDocument()
    await userEvent.type(screen.getByLabelText("Account password"), "correct horse")
    await userEvent.click(screen.getByRole("button", { name: "Unlock" }))
    expect(s.unlock).toHaveBeenCalledWith("correct horse")
    expect(onUnlocked).toHaveBeenCalled()
  })
})
