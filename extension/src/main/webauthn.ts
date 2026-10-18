// The one script Lockdown runs in the page's own world (docs/plan/
// 19-passkeys.md §3): it wraps navigator.credentials.create/get and the
// Signal API so Lockdown can be the authenticator where no OS provider
// exists. It runs at document_start in the top frame only, before any page
// script, and holds nothing: no key, no vault data, no authority.
//
// Why the page can't forge an approval: this script shares its world with
// the page, so the page can read it, call it, impersonate it, or post the
// same messages to the extension. None of that matters, because nothing
// on this side approves anything. A request is only a request; the
// extension takes the page's address from the browser (never from here),
// the core checks the RP ID against that address and writes the origin
// into the client data itself, and the only "yes" is a trusted click in
// Lockdown's own extension-origin frame, armed by IntersectionObserver v2
// and a second deliberate action. A page that forges a request gets at
// most what it could ask for honestly: a passkey for its own origin, after
// the person said yes in Lockdown's frame.
//
// Coexistence with other managers' overrides (D-row in the doc): wrap,
// never replace; keep whatever navigator.credentials.get/create was
// installed before (another manager's wrapper, or the browser's own) and
// hand the request to it unchanged when the person picks "Use another
// device or security key", when Lockdown has nothing for the site, or when
// Lockdown isn't reachable; install as an ordinary writable, configurable
// property, never frozen; and mark the wrapper with
// `Symbol.for("webauthn.provider.override")` naming the provider and the
// next function in the chain, so a later override can chain onto Lockdown
// the same way.

;(() => {
  const creds = navigator.credentials as CredentialsContainer | undefined
  if (!creds || typeof PublicKeyCredential === "undefined" || window.top !== window) return

  const MARK = Symbol.for("webauthn.provider.override")
  const nextGet = creds.get
  const nextCreate = creds.create
  const PKC = PublicKeyCredential as unknown as Record<string, unknown>
  const nextSignal = {
    signalUnknownCredential: PKC.signalUnknownCredential as ((o: unknown) => Promise<void>) | undefined,
    signalAllAcceptedCredentials: PKC.signalAllAcceptedCredentials as ((o: unknown) => Promise<void>) | undefined,
    signalCurrentUserDetails: PKC.signalCurrentUserDetails as ((o: unknown) => Promise<void>) | undefined,
  }
  const nextConditional = PKC.isConditionalMediationAvailable as (() => Promise<boolean>) | undefined
  const nextCapabilities = PKC.getClientCapabilities as (() => Promise<Record<string, boolean>>) | undefined

  // --- the channel to Lockdown's isolated content script -------------------

  let port: MessagePort | null = null
  let connected = false
  let markReady: () => void = () => {}
  const ready = new Promise<void>((resolve) => (markReady = resolve))
  const waiting = new Map<number, (msg: Answer) => void>()
  let nextId = 1

  interface Answer {
    t: "res" | "ack"
    id?: number
    handoff?: "modal" | "same"
    active?: boolean
    credential?: Wire
    error?: { name: string; message: string }
  }

  function onAnswer(e: MessageEvent) {
    const msg = e.data as Answer
    if (msg?.t !== "res" || typeof msg.id !== "number") return
    const w = waiting.get(msg.id)
    if (w) {
      waiting.delete(msg.id)
      w(msg)
    }
  }

  // Introduce a port to the isolated script; each try transfers a fresh
  // one, and the first to be acknowledged is kept. The isolated script
  // runs at document_start too, so this ends before any page script runs.
  let tries = 0
  const hello = () => {
    if (connected || tries++ > 20) return
    const ch = new MessageChannel()
    ch.port1.onmessage = (e: MessageEvent) => {
      if (connected || (e.data as Answer)?.t !== "ack") return
      connected = true
      port = ch.port1
      port.onmessage = onAnswer
      markReady()
    }
    window.postMessage({ lockdown_passkeys: "hello" }, "*", [ch.port2])
    setTimeout(hello, 50)
  }
  hello()

  /** Waits a moment for the channel, for a page that asks at once. */
  async function linked(): Promise<boolean> {
    if (!connected) await Promise.race([ready, new Promise((r) => setTimeout(r, 500))])
    return connected
  }

  function ask(msg: Record<string, unknown>, timeoutMs: number, signal?: AbortSignal | null): Promise<Answer> {
    const id = nextId++
    return new Promise((resolve) => {
      const done = (a: Answer) => {
        clearTimeout(timer)
        signal?.removeEventListener("abort", onAbort)
        resolve(a)
      }
      const timer = setTimeout(() => {
        waiting.delete(id)
        port?.postMessage({ t: "abort", id })
        done({ t: "res", id, error: { name: "NotAllowedError", message: "The request timed out." } })
      }, timeoutMs)
      const onAbort = () => {
        waiting.delete(id)
        port?.postMessage({ t: "abort", id })
        done({ t: "res", id, error: { name: "AbortError", message: "The request was aborted." } })
      }
      signal?.addEventListener("abort", onAbort, { once: true })
      waiting.set(id, done)
      port?.postMessage({ ...msg, id })
    })
  }

  // --- encoding -------------------------------------------------------------

  function bytes(src: unknown): Uint8Array | null {
    if (src instanceof ArrayBuffer) return new Uint8Array(src)
    if (ArrayBuffer.isView(src)) return new Uint8Array(src.buffer, src.byteOffset, src.byteLength)
    return null
  }

  function b64url(src: unknown): string {
    const b = bytes(src)
    if (!b) throw new TypeError("expected a BufferSource")
    let s = ""
    for (const x of b) s += String.fromCharCode(x)
    return btoa(s).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "")
  }

  function buf(text: string): ArrayBuffer {
    const s = atob(text.replace(/-/g, "+").replace(/_/g, "/"))
    const out = new Uint8Array(s.length)
    for (let i = 0; i < s.length; i++) out[i] = s.charCodeAt(i)
    return out.buffer
  }

  function timeoutOf(ms: unknown): number {
    const n = typeof ms === "number" && Number.isFinite(ms) ? ms : 300_000
    return Math.min(600_000, Math.max(30_000, n))
  }

  function fail(e: { name: string; message: string }): never {
    if (e.name === "TypeError") throw new TypeError(e.message)
    throw new DOMException(e.message, e.name)
  }

  // --- what the page gets back --------------------------------------------

  interface Wire {
    credential_id: string
    client_data_json: string
    attestation_object?: string
    authenticator_data: string
    public_key?: string
    public_key_algorithm?: number
    transports?: string[]
    resident_key?: boolean | null
    signature?: string
    user_handle?: string
  }

  function own(target: object, values: Record<string, unknown>) {
    for (const [k, v] of Object.entries(values)) Object.defineProperty(target, k, { value: v, enumerable: true, configurable: true })
  }

  function credential(w: Wire, created: boolean): PublicKeyCredential {
    const ext = created && w.resident_key != null ? { credProps: { rk: w.resident_key } } : {}
    const json: Record<string, unknown> = created
      ? {
          clientDataJSON: w.client_data_json,
          attestationObject: w.attestation_object,
          authenticatorData: w.authenticator_data,
          transports: w.transports ?? [],
          publicKey: w.public_key,
          publicKeyAlgorithm: w.public_key_algorithm,
        }
      : { clientDataJSON: w.client_data_json, authenticatorData: w.authenticator_data, signature: w.signature, userHandle: w.user_handle }
    const response = Object.create(created ? AuthenticatorAttestationResponse.prototype : AuthenticatorAssertionResponse.prototype)
    if (created) {
      own(response, {
        clientDataJSON: buf(w.client_data_json),
        attestationObject: buf(w.attestation_object ?? ""),
        getTransports: () => [...(w.transports ?? [])],
        getAuthenticatorData: () => buf(w.authenticator_data),
        getPublicKey: () => buf(w.public_key ?? ""),
        getPublicKeyAlgorithm: () => w.public_key_algorithm ?? -7,
        toJSON: () => ({ ...json }),
      })
    } else {
      own(response, {
        clientDataJSON: buf(w.client_data_json),
        authenticatorData: buf(w.authenticator_data),
        signature: buf(w.signature ?? ""),
        userHandle: w.user_handle ? buf(w.user_handle) : null,
        toJSON: () => ({ ...json }),
      })
    }
    const cred = Object.create(PublicKeyCredential.prototype)
    own(cred, {
      id: w.credential_id,
      rawId: buf(w.credential_id),
      type: "public-key",
      response,
      authenticatorAttachment: "platform",
      getClientExtensionResults: () => ({ ...ext }),
      toJSON: () => ({
        id: w.credential_id,
        rawId: w.credential_id,
        type: "public-key",
        response: { ...json },
        authenticatorAttachment: "platform",
        clientExtensionResults: { ...ext },
      }),
    })
    return cred as PublicKeyCredential
  }

  // --- the wrappers -------------------------------------------------------

  type Opts = CredentialCreationOptions & CredentialRequestOptions & { mediation?: string }

  function createRequest(o: Opts): Record<string, unknown> {
    const pk = o.publicKey as PublicKeyCredentialCreationOptions
    return {
      rp_id: pk.rp?.id ?? null,
      rp_name: String(pk.rp?.name ?? ""),
      user_id: b64url(pk.user?.id),
      user_name: String(pk.user?.name ?? ""),
      user_display_name: String(pk.user?.displayName ?? ""),
      challenge: b64url(pk.challenge),
      algs: (pk.pubKeyCredParams ?? []).filter((p) => p?.type === "public-key").map((p) => Number(p.alg)),
      exclude: (pk.excludeCredentials ?? []).map((c) => b64url(c.id)),
      user_verification: pk.authenticatorSelection?.userVerification ?? "preferred",
      cred_props: !!(pk.extensions as Record<string, unknown> | undefined)?.credProps,
      conditional: o.mediation === "conditional",
    }
  }

  function getRequest(o: Opts): Record<string, unknown> {
    const pk = o.publicKey as PublicKeyCredentialRequestOptions
    return {
      rp_id: pk.rpId ?? null,
      challenge: b64url(pk.challenge),
      allow: (pk.allowCredentials ?? []).map((c) => b64url(c.id)),
      user_verification: pk.userVerification ?? "preferred",
    }
  }

  /** Requests only another kind of authenticator can answer go straight on. */
  function forSomeoneElse(o: Opts, created: boolean): boolean {
    const pk = (o.publicKey ?? {}) as { hints?: string[]; authenticatorSelection?: { authenticatorAttachment?: string } }
    if (created && pk.authenticatorSelection?.authenticatorAttachment === "cross-platform") return true
    return Array.isArray(pk.hints) && pk.hints[0] === "security-key"
  }

  async function run(kind: "create" | "get", options: Opts | undefined): Promise<Credential | null> {
    const next = kind === "create" ? nextCreate : nextGet
    const pass = (o: Opts | undefined) => next.call(creds, o)
    if (!options?.publicKey || forSomeoneElse(options, kind === "create") || !(await linked())) return pass(options)
    if (options.signal?.aborted) fail({ name: "AbortError", message: "The request was aborted." })
    let request: Record<string, unknown>
    try {
      request = kind === "create" ? createRequest(options) : getRequest(options)
    } catch {
      // Something only the browser can judge (a malformed buffer): its words.
      return pass(options)
    }
    const timeout = options.mediation === "conditional" ? 600_000 : timeoutOf(options.publicKey.timeout)
    const answer = await ask({ t: "req", op: kind, mediation: options.mediation ?? "optional", request }, timeout, options.signal)
    if (answer.handoff === "same") return pass(options)
    if (answer.handoff === "modal") {
      // From the inline menu: the browser's own picker, not its autofill.
      const { mediation: _m, ...modal } = options
      return pass(modal)
    }
    if (answer.error) fail(answer.error)
    if (!answer.credential) fail({ name: "NotAllowedError", message: "Lockdown didn't answer." })
    return credential(answer.credential, kind === "create")
  }

  function install(target: object, name: string, value: (...a: never[]) => unknown, next: unknown) {
    Object.defineProperty(value, MARK, { value: Object.freeze({ provider: "Lockdown", version: 1, next }) })
    Object.defineProperty(target, name, { value, writable: true, configurable: true, enumerable: false })
  }

  install(creds, "get", function get(options?: CredentialRequestOptions) {
    return run("get", options as Opts)
  }, nextGet)
  install(creds, "create", function create(options?: CredentialCreationOptions) {
    return run("create", options as Opts)
  }, nextCreate)

  async function active(): Promise<boolean> {
    if (!(await linked())) return false
    const a = await ask({ t: "status" }, 5_000)
    return a.active === true
  }

  install(PublicKeyCredential, "isConditionalMediationAvailable", async function isConditionalMediationAvailable() {
    if (await active()) return true
    return nextConditional ? nextConditional.call(PublicKeyCredential) : false
  }, nextConditional)

  install(PublicKeyCredential, "getClientCapabilities", async function getClientCapabilities() {
    const theirs = nextCapabilities ? await nextCapabilities.call(PublicKeyCredential) : {}
    if (!(await active())) return theirs
    return {
      ...theirs,
      conditionalGet: true,
      conditionalCreate: true,
      passkeyPlatformAuthenticator: true,
      userVerifyingPlatformAuthenticator: theirs.userVerifyingPlatformAuthenticator ?? false,
      relatedOrigins: true,
      signalUnknownCredential: true,
      signalAllAcceptedCredentials: true,
      signalCurrentUserDetails: true,
    }
  }, nextCapabilities)

  // The Signal API: Lockdown hears it (and hides, never deletes), and so
  // does whoever was installed before, the browser included.
  const signals: [keyof typeof nextSignal, (o: Record<string, unknown>) => Record<string, unknown>][] = [
    ["signalUnknownCredential", (o) => ({ signal: "unknown_credential", rp_id: String(o.rpId ?? ""), credential_id: String(o.credentialId ?? "") })],
    [
      "signalAllAcceptedCredentials",
      (o) => ({
        signal: "all_accepted_credentials",
        rp_id: String(o.rpId ?? ""),
        user_id: String(o.userId ?? ""),
        all_accepted_credential_ids: Array.isArray(o.allAcceptedCredentialIds) ? o.allAcceptedCredentialIds.map(String) : [],
      }),
    ],
    [
      "signalCurrentUserDetails",
      (o) => ({ signal: "current_user_details", rp_id: String(o.rpId ?? ""), user_id: String(o.userId ?? ""), name: String(o.name ?? ""), display_name: String(o.displayName ?? "") }),
    ],
  ]
  for (const [name, shape] of signals) {
    const next = nextSignal[name]
    install(PublicKeyCredential, name, async function signal(options: Record<string, unknown>) {
      if (!options || typeof options !== "object") throw new TypeError(`${name} needs its options`)
      if (await linked()) port?.postMessage({ t: "signal", signal: shape(options) })
      if (next) return next.call(PublicKeyCredential, options)
    }, next)
  }
})()
