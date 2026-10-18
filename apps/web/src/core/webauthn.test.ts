import { describe, expect, it, vi } from "vitest"

import { cameraScanner } from "./camera-scan"
import type { Link } from "./link"
import { webDepthClient } from "./web-depth-client"
import { answerWithKey, createAnswer, createOptions, getOptions, keyPageFromHash, type CreateRequest, type GetRequest } from "./webauthn"

const get: GetRequest = { pending_id: "p", challenge: [1, 2, 3], rp_id: "vault.example", allow: [[9, 9]], timeout_ms: 1000, expires_at: 1, device_label: "Firefox on Linux" }
const create: CreateRequest = { pending_id: "p", challenge: [4], rp_id: "vault.example", rp_name: "Lockdown", user_id: [7, 7], user_name: "me@example.com", exclude: [[1]], timeout_ms: 1000 }

const ab = (bytes: number[]) => new Uint8Array(bytes).buffer

describe("WebAuthn between the worker and the browser", () => {
  it("builds the options the browser takes from the worker's request", () => {
    const g = getOptions(get).publicKey!
    expect(Array.from(g.challenge as Uint8Array)).toEqual([1, 2, 3])
    expect(g.rpId).toBe("vault.example")
    expect(Array.from(g.allowCredentials![0]!.id as Uint8Array)).toEqual([9, 9])
    const c = createOptions(create).publicKey!
    expect(c.rp).toEqual({ id: "vault.example", name: "Lockdown" })
    expect(c.pubKeyCredParams.map((p) => p.alg)).toEqual([-7, -8])
    expect(c.attestation).toBe("none")
    expect(Array.from(c.excludeCredentials![0]!.id as Uint8Array)).toEqual([1])
  })

  it("hands the credential back as arrays, and nothing when the person backs out", async () => {
    const assertion = { rawId: ab([9, 9]), response: { clientDataJSON: ab([1]), authenticatorData: ab([2]), signature: ab([3]) } } as unknown as PublicKeyCredential
    const credentials = { get: vi.fn(async () => assertion) } as unknown as CredentialsContainer
    expect(await answerWithKey(credentials, get)).toEqual({ credentialId: [9, 9], clientDataJson: [1], authenticatorData: [2], signature: [3] })
    const none = { get: vi.fn(async () => null) } as unknown as CredentialsContainer
    expect(await answerWithKey(none, get)).toBeNull()
    const made = { rawId: ab([5]), response: { clientDataJSON: ab([6]), attestationObject: ab([7]) } } as unknown as PublicKeyCredential
    expect(createAnswer(made, "Blue key")).toEqual({ name: "Blue key", clientDataJson: [6], attestationObject: [7] })
  })

  it("reads only the two key pages from the address", () => {
    const id = "6f1c2b8e-3d4a-4f5b-9c6d-7e8f9a0b1c2d"
    expect(keyPageFromHash(`#/second-factor/${id}`)).toEqual({ kind: "second-factor", id })
    expect(keyPageFromHash(`#/security-key/${id}`)).toEqual({ kind: "security-key", id })
    expect(keyPageFromHash(`#/security-key/${id}?x=<script>`)).toBeNull()
    expect(keyPageFromHash("#/other/abc")).toBeNull()
  })
})

describe("the web vault's security-depth client", () => {
  it("keeps the desktop's command names and joins on this page's server only", async () => {
    const call = vi.fn(async () => "https://vault.example/vault/#/security-key/x")
    const link = { call, callBytes: vi.fn() } as unknown as Link
    const open = vi.fn()
    const depth = webDepthClient(link, open)
    await depth.setExposure("i", "ask", { password: "pw" })
    expect(call).toHaveBeenLastCalledWith("set_exposure", { id: "i", tier: "ask", gate: { password: "pw" } })
    await depth.joinStart("https://elsewhere.example", "7KQ2M-9XH4P")
    expect(call).toHaveBeenLastCalledWith("web_join_start", { invite: "7KQ2M-9XH4P" })
    await depth.securityKeyAdd()
    expect(open).toHaveBeenCalledWith("https://vault.example/vault/#/security-key/x")
    await expect(depth.accountRegisterOptedOut({ server: "s", login: "l", password: "p" }, true)).rejects.toMatchObject({ code: "refused" })
  })

  it("offers no camera scan where the browser can't read QR codes", () => {
    expect(cameraScanner({ navigator: {} } as unknown as Window & typeof globalThis)).toBeUndefined()
  })
})
