import { render, screen } from "@testing-library/react"

import App from "@/App"
import { ClientProvider } from "@/core/client"
import { fakeClient } from "@/test/fake-client"

// The host's edges: no Tauri here.
vi.mock("@tauri-apps/api/core", () => ({ invoke: () => Promise.reject({ code: "refused", message: "not in this test" }) }))
vi.mock("@tauri-apps/api/event", () => ({ listen: () => Promise.resolve(() => {}) }))

/*
 * Brief X5, Lock (Q15, D107 changed): while locked the app draws the unlock
 * screen alone. Nothing of the shell is drawn, not even its shape: no
 * sidebar, no status line, no washed skeleton of the list.
 */
describe("locked", () => {
  it("draws the unlock screen alone, and nothing of the shell", async () => {
    const client = fakeClient({
      lockState: () => Promise.resolve({ locked: true, reason: "requested", idle_secs: 0, idle_timeout_secs: null, hidden_timeout_secs: null }),
      probe: () => Promise.reject({ code: "refused", message: "The vault file can't be read." }),
    })
    render(
      <ClientProvider value={client}>
        <App />
      </ClientProvider>,
    )
    expect(await screen.findByText("The vault file can't be read.")).toBeInTheDocument()
    expect(document.querySelector('[data-slot="locked-wash"]')).toBeNull()
    // The closed app is Unicron's focus frame (contract 5.1b #1; screens-1, doc 76): the frame's box, its body and its
    // polite announcer come with it. None of the shell's parts does: no sidebar, rail, status line, rows or columns.
    expect(document.querySelector('[data-slot="app-frame-focus"]')).not.toBeNull()
    expect(document.querySelector('[data-slot^="app-frame-"]:not([data-slot^="app-frame-focus"]):not([data-slot="app-frame-body"]):not([data-slot="app-frame-announcer"])')).toBeNull()
    expect(screen.queryByRole("navigation")).toBeNull()
    expect(document.querySelector('[data-slot="needs-you"]')).toBeNull()
  })
})
