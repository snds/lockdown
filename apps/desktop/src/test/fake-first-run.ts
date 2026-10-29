import type { Status } from "@/core/types"
import type { Candidate, FirstRunClient } from "@/features/unlock/first-run-client"

/** A ready Lockdown server, as discovery reports one. */
export function readyServer(address: string, over: Partial<Candidate> = {}): Candidate {
  return {
    address,
    sources: [{ kind: "network" }],
    state: "ready",
    reason: null,
    fingerprint: "12345 67890 13579 24680 11223 34455",
    version: "0.1.0",
    policy_key: false,
    locked: false,
    in_use: false,
    ...over,
  }
}

/**
 * A first-run client: discovery finds nothing, Touch ID isn't offered and
 * the stage and checks succeed, unless the test says otherwise; every
 * other method refuses.
 */
export function fakeFirstRun(overrides: Partial<FirstRunClient> = {}): FirstRunClient {
  const refuse = () => Promise.reject({ code: "refused", message: "not in this test" })
  const base = new Proxy({} as FirstRunClient, { get: () => refuse })
  const defaults: Partial<FirstRunClient> = {
    candidates: () => Promise.resolve([]),
    domain: () => Promise.resolve([]),
    quickUnlock: () => Promise.resolve({ kind: "touch_id", available: false, why: "Touch ID unlock needs the signed Lockdown app." }),
    stage: () => Promise.resolve(),
    checkMasterPassword: () => Promise.resolve(),
    confirm: (address) => Promise.resolve(readyServer(address)),
  }
  return Object.assign(Object.create(base), defaults, overrides)
}

/** A local account's kit page, as the core writes it. */
export const KIT_PAGE = `LOCKDOWN RECOVERY KIT
=====================
Keep this page somewhere safe, away from this computer. It is the only way
back into your vault if you forget your master password.

Secret Key:     A3-K7QF2M-9XWD4R-HT6PL-C8ZB3-NVQ5
Recovery code:  RC4F-9K2M-XQ7H-TDPL-8B3Z-NW5V-6YEA-J2KD

Profile:        0190e6a0-0000-7000-8000-000000000001
Made:           2026-10-29 on Lockdown on test

To use this kit, open Lockdown on this computer and choose
"Forgot your password? Use your recovery kit".
`

/** What the core reports once the vault opened. */
export const opened = { devices: 1, recovery: "created", account: { local: true } } as unknown as Status

