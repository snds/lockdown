import { render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { describe, expect, it, vi } from "vitest"

import type { Status } from "@/core/types"

import type { WebSession } from "../core/web-client"
import { SignInScreen } from "./sign-in-screen"
import { NotSyncedQuestion, WebUnlockScreen } from "./web-unlock-screen"

function session(over: Partial<WebSession> = {}): WebSession {
  return {
    state: vi.fn(),
    signIn: vi.fn(async () => ({}) as Status),
    unlock: vi.fn(async () => ({}) as Status),
    signOut: vi.fn(async () => {}),
    signInWaiting: vi.fn(),
    signInAnswer: vi.fn(async () => ({}) as Status),
    signInContinue: vi.fn(async () => null),
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
    await userEvent.type(screen.getByLabelText("Master password"), "correct horse")
    await userEvent.type(screen.getByLabelText("Secret Key"), "A1-ABC")
    await userEvent.click(screen.getByRole("button", { name: "Sign in" }))
    expect(s.signIn).toHaveBeenCalledWith({ server: "https://vault.example", login: "me@example.com", password: "correct horse", secretKey: "A1-ABC" })
    expect(onSignedIn).toHaveBeenCalled()
    expect(screen.getByLabelText("Master password")).toHaveValue("")
  })

  it("shows the core's words and clears the password when sign-in fails", async () => {
    const s = session({ signIn: vi.fn(async () => Promise.reject({ code: "refused", message: "the login name and password didn't match" })) })
    render(<SignInScreen session={s} server="https://vault.example" onSignedIn={vi.fn()} />)
    await userEvent.type(screen.getByLabelText("Login name"), "me")
    await userEvent.type(screen.getByLabelText("Master password"), "wrong")
    await userEvent.type(screen.getByLabelText("Secret Key"), "A1-ABC")
    await userEvent.click(screen.getByRole("button", { name: "Sign in" }))
    expect(await screen.findByRole("alert")).toHaveTextContent("didn't match")
    expect(screen.getByLabelText("Master password")).toHaveValue("")
  })

  it("waits for the emailed link to approve a new tab, then opens", async () => {
    const opened = { opened: true } as unknown as Status
    let answered = 0
    const s = session({
      signIn: vi.fn(async () =>
        Promise.reject({ code: "approval_needed", message: "the server emailed a link to s•••@example.com: open it and choose Approve, and this tab carries on by itself", detail: "email" }),
      ),
      signInContinue: vi.fn(async () => (++answered >= 2 ? opened : null)),
    })
    const onSignedIn = vi.fn()
    render(<SignInScreen session={s} server="https://vault.example" onSignedIn={onSignedIn} />)
    await userEvent.type(screen.getByLabelText("Login name"), "sean@example.com")
    await userEvent.type(screen.getByLabelText("Master password"), "correct horse")
    await userEvent.type(screen.getByLabelText("Secret Key"), "A1-ABC")
    await userEvent.click(screen.getByRole("button", { name: "Sign in" }))
    expect(await screen.findByRole("heading", { level: 1, name: "Check your email." })).toBeInTheDocument()
    expect(screen.getByText(/^The server emailed a link to s•••@example\.com/)).toBeInTheDocument()
    expect(s.signInWaiting).not.toHaveBeenCalled()
    await vi.waitFor(() => expect(onSignedIn).toHaveBeenCalledWith(opened), { timeout: 6000 })
  })

  it("puts a refused sign-in in one compact critical Banner, in the server's words", async () => {
    const s = session({ signIn: vi.fn(async () => Promise.reject({ code: "refused", message: "Too many sign-in attempts for this login. Try again in 15 minutes." })) })
    render(<SignInScreen session={s} server="https://vault.example" onSignedIn={vi.fn()} />)
    await userEvent.type(screen.getByLabelText("Login name"), "me")
    await userEvent.type(screen.getByLabelText("Master password"), "x")
    await userEvent.type(screen.getByLabelText("Secret Key"), "A1")
    await userEvent.click(screen.getByRole("button", { name: "Sign in" }))
    const banner = await screen.findByRole("alert")
    expect(banner).toHaveTextContent("Too many sign-in attempts")
    expect(banner).toHaveAttribute("data-slot", "banner")
  })

  it("keeps Sign in's label while it works", async () => {
    const s = session({ signIn: vi.fn(() => new Promise<Status>(() => {})) })
    render(<SignInScreen session={s} server="https://vault.example" onSignedIn={vi.fn()} />)
    await userEvent.type(screen.getByLabelText("Login name"), "me")
    await userEvent.type(screen.getByLabelText("Master password"), "x")
    await userEvent.type(screen.getByLabelText("Secret Key"), "A1")
    await userEvent.click(screen.getByRole("button", { name: "Sign in" }))
    const busy = screen.getByRole("button", { name: /Sign in$/ })
    expect(busy).toHaveAttribute("aria-busy", "true")
    expect(busy).toHaveTextContent("Sign in")
  })

  it("names the web vault and its server in the overline, with the frame's title", () => {
    render(<SignInScreen session={session()} server="https://vault.example" onSignedIn={vi.fn()} />)
    expect(screen.getByRole("heading", { level: 1, name: "Sign in." })).toBeInTheDocument()
    expect(screen.getAllByRole("tab").map((t) => t.textContent)).toEqual(["Sign in", "With a device", "No account yet?"])
  })

  it("says another device ended the session, and signs the tab out", async () => {
    const s = session()
    const onSignedOut = vi.fn()
    render(<WebUnlockScreen session={s} login="me@example.com" reason="requested" sessionEnded onUnlocked={vi.fn()} onSignedOut={onSignedOut} />)
    expect(screen.getByText(/session was ended from another of your devices/)).toBeInTheDocument()
    await userEvent.click(screen.getByRole("button", { name: "Sign out of this tab" }))
    expect(s.signOut).toHaveBeenCalled()
    expect(onSignedOut).toHaveBeenCalled()
  })

  it("asks before signing out with changes not yet synced, focus on Cancel", async () => {
    const onSignOut = vi.fn()
    const onCancel = vi.fn()
    render(<NotSyncedQuestion open reason="The server can't be reached." onSignOut={onSignOut} onCancel={onCancel} />)
    expect(await screen.findByRole("heading", { name: "Not synced, so still signed in." })).toBeInTheDocument()
    expect(screen.getByText("The server can't be reached.")).toBeInTheDocument()
    await waitFor(() => expect(screen.getByRole("button", { name: "Cancel" })).toHaveFocus())
    await userEvent.click(screen.getByRole("button", { name: "Sign out anyway" }))
    expect(onSignOut).toHaveBeenCalled()
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
    // Brief W1 as F1: the reason and the login in the overline, the frame's title.
    expect(screen.getByText("Locked // Out of view // me@example.com")).toBeInTheDocument()
    expect(screen.getByRole("heading", { level: 1, name: "Lockdown is locked." })).toBeInTheDocument()
    await userEvent.type(screen.getByLabelText("Master password"), "correct horse")
    await userEvent.click(screen.getByRole("button", { name: "Unlock" }))
    expect(s.unlock).toHaveBeenCalledWith("correct horse")
    expect(onUnlocked).toHaveBeenCalled()
  })
})
