import type { ItemDetail } from "@/core/types"
import { TEMPLATES } from "@/test/templates-fixture"

import { buildChanges, customRow, holdsValue, hostOf, inSentence, initialRows, isEssential, wipeRows } from "./editor-model"

const wifi = TEMPLATES.kinds.find((t) => t.kind === "wi-fi")!
const login = TEMPLATES.kinds.find((t) => t.kind === "login")!

const decode = (bytes?: number[]) => (bytes ? new TextDecoder().decode(Uint8Array.from(bytes)) : undefined)

const existingLogin: ItemDetail = {
  id: "i1",
  kind: "login",
  state: "live",
  version: 3,
  edited_at: 0,
  crown_jewel: false,
  fields: [
    { field_id: "title", label: "title", kind: "text", hidden: false, value: "Example", mask: null },
    { field_id: "user", label: "username", kind: "text", hidden: false, value: "me", mask: null },
    { field_id: "pw", label: "password", kind: "hidden", hidden: true, value: null, mask: "••••••••" },
    { field_id: "url.1", label: "website", kind: "url", hidden: false, value: "https://m.example.com", mask: null },
    { field_id: "passkey.0.key", label: "private key", kind: "passkey", hidden: true, value: null, mask: "••••••••" },
    { field_id: "custom.0", label: "Door code", kind: "hidden", hidden: true, value: null, mask: "••••••••" },
  ],
}

describe("the editor's model (D110)", () => {
  it("a new login's password is made by the core on save, by default; an edit keeps the one it has (Lockdown 29)", () => {
    expect(initialRows(login, null).find((r) => r.fieldId === "pw")?.secret).toBe("generate")
    expect(buildChanges(initialRows(login, null))).toContainEqual({ field_id: "pw", action: "generate" })
    expect(initialRows(login, existingLogin).find((r) => r.fieldId === "pw")?.secret).toBe("keep")
    // Other kinds start empty, as before.
    expect(initialRows(wifi, null).find((r) => r.fieldId === "wifi.passphrase")?.secret).toBe("type")
  })

  it("a new item sends only what was filled in, secrets as bytes, a generator as `generate`", () => {
    const rows = initialRows(wifi, null)
    expect(rows.map((r) => r.fieldId)).toEqual(["wifi.ssid", "wifi.passphrase", "wifi.security", "wifi.hidden", "notes"])
    const filled = rows.map((r) =>
      r.fieldId === "wifi.ssid" ? { ...r, text: "home-net" } : r.fieldId === "wifi.passphrase" ? { ...r, secret: "generate" as const } : r.fieldId === "wifi.hidden" ? { ...r, flag: true } : r,
    )
    expect(buildChanges(filled)).toEqual([
      { field_id: "wifi.ssid", action: "set", text: "home-net" },
      { field_id: "wifi.passphrase", action: "generate" },
      { field_id: "wifi.hidden", action: "set", flag: true },
    ])
    const typed = filled.map((r) => (r.fieldId === "wifi.passphrase" ? { ...r, secret: "type" as const, text: "s3cret" } : r))
    const pw = buildChanges(typed).find((c) => c.field_id === "wifi.passphrase")!
    expect(pw.text).toBeUndefined()
    expect(decode(pw.secret)).toBe("s3cret")
  })

  it("an edit names every field: keep for an untouched secret (no bytes), keep for a provider's passkey, set or remove for the rest", () => {
    const rows = initialRows(login, existingLogin)
    expect(rows.map((r) => r.fieldId)).toEqual(["user", "pw", "code.0", "url.0", "notes", "url.1", "passkey.0.key", "custom.0"])
    expect(rows.find((r) => r.fieldId === "pw")!.text).toBe("")
    const changed = rows.map((r) => (r.fieldId === "url.1" ? { ...r, text: "" } : r.fieldId === "user" ? { ...r, text: "me@work" } : r))
    const changes = buildChanges(changed)
    expect(changes).toEqual([
      { field_id: "user", action: "set", text: "me@work" },
      { field_id: "pw", action: "keep" },
      { field_id: "url.1", action: "remove" },
      { field_id: "passkey.0.key", action: "keep" },
      { field_id: "custom.0", action: "keep" },
    ])
    expect(JSON.stringify(changes)).not.toContain("secret")
  })

  it("a field added by hand carries its label and kind; wiping drops typed secrets", () => {
    const rows = [...initialRows(login, null), { ...customRow(0, "Recovery email", "text"), text: "me@example.org" }, { ...customRow(1, "PIN", "hidden"), text: "0000" }]
    const changes = buildChanges(rows)
    expect(changes).toContainEqual({ label: "Recovery email", kind: "text", action: "set", text: "me@example.org" })
    const pin = changes.find((c) => c.label === "PIN")!
    expect(pin.kind).toBe("hidden")
    expect(decode(pin.secret)).toBe("0000")
    expect(wipeRows(rows).find((r) => r.label === "PIN")!.text).toBe("")
    expect(wipeRows(rows).find((r) => r.label === "Recovery email")!.text).toBe("me@example.org")
  })

  it("a passkey is one line in the editor, every part kept, none removable on its own", () => {
    const withPasskey: ItemDetail = {
      ...existingLogin,
      fields: [
        ...existingLogin.fields.filter((f) => f.field_id !== "passkey.0.key"),
        { field_id: "passkey.0.rp_id", label: "relying party", kind: "text", hidden: false, value: "example.com", mask: null },
        { field_id: "passkey.0.user_name", label: "user name", kind: "text", hidden: false, value: "me", mask: null },
        { field_id: "passkey.0.key", label: "private key", kind: "passkey", hidden: true, value: null, mask: "••••••••" },
        { field_id: "passkey.0.backed_up", label: "synced", kind: "boolean", hidden: false, value: "true", mask: null },
      ],
    }
    const rows = initialRows(login, withPasskey)
    const passkey = rows.filter((r) => r.fieldId?.startsWith("passkey."))
    expect(passkey.filter((r) => !r.quiet).map((r) => [r.label, r.managed])).toEqual([["Passkey for example.com", true]])
    expect(passkey.every((r) => !r.editable)).toBe(true)
    const changes = buildChanges(rows).filter((c) => c.field_id?.startsWith("passkey."))
    expect(changes).toEqual([
      { field_id: "passkey.0.rp_id", action: "keep" },
      { field_id: "passkey.0.user_name", action: "keep" },
      { field_id: "passkey.0.key", action: "keep" },
      { field_id: "passkey.0.backed_up", action: "keep" },
    ])
  })
})

describe("the editor's zones (brief E2)", () => {
  it("puts each kind's essentials first, and the rest under More options", () => {
    const essentials = (t: typeof login) => initialRows(t, null).filter((r) => isEssential(t, r)).map((r) => r.fieldId)
    expect(essentials(login)).toEqual(["user", "pw", "url.0"])
    expect(essentials(wifi)).toEqual(["wifi.ssid", "wifi.passphrase", "wifi.security"])
    // A kind with no list of its own: its first section and every required field, never Notes.
    const passport = TEMPLATES.kinds.find((t) => t.id === "identity/passport")!
    expect(essentials(passport)).toEqual(["passport.number", "passport.country", "expires_at"])
  })

  it("knows what holds a value, for opening More options on an edit", () => {
    const [user] = initialRows(login, existingLogin)
    expect(holdsValue(user!)).toBe(true)
    expect(holdsValue(customRow(0, "Hint", "text"))).toBe(false)
  })

  it("writes a kind's name inside a sentence and a website as its host", () => {
    expect(inSentence("Payment card")).toBe("payment card")
    expect(inSentence("Wi-Fi network")).toBe("Wi-Fi network")
    expect(inSentence("SSH key")).toBe("SSH key")
    expect(inSentence("Driver's licence")).toBe("driver's licence")
    expect(hostOf("https://www.github.example/login")).toBe("github.example")
    expect(hostOf("github.example")).toBe("github.example")
    expect(hostOf("")).toBe("")
  })
})
