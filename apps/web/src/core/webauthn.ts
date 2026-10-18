/**
 * The browser's WebAuthn, between the worker's requests and its answers
 * (docs/plan/22-security-depth.md §4). The worker hands over a request
 * with its bytes as arrays of numbers (crates/web/src/security.rs); this
 * turns it into the options `navigator.credentials` takes, and the
 * credential that comes back into the arrays the worker passes to the
 * server, which checks everything. Nothing here decides anything.
 */

/** A waiting sign-in's request (`GetRequest`). */
export interface GetRequest {
  pending_id: string
  challenge: number[]
  rp_id: string
  allow: number[][]
  timeout_ms: number
  expires_at: number
  device_label: string
}

/** A key registration's request (`CreateRequest`). */
export interface CreateRequest {
  pending_id: string
  challenge: number[]
  rp_id: string
  rp_name: string
  user_id: number[]
  user_name: string
  exclude: number[][]
  timeout_ms: number
}

export interface GetAnswer {
  credentialId: number[]
  clientDataJson: number[]
  authenticatorData: number[]
  signature: number[]
}

export interface CreateAnswer {
  name: string
  clientDataJson: number[]
  attestationObject: number[]
}

const buf = (bytes: number[]) => new Uint8Array(bytes)
const arr = (b: ArrayBuffer) => Array.from(new Uint8Array(b))

export function getOptions(r: GetRequest): CredentialRequestOptions {
  return {
    publicKey: {
      challenge: buf(r.challenge),
      rpId: r.rp_id,
      allowCredentials: r.allow.map((id) => ({ type: "public-key", id: buf(id) })),
      timeout: r.timeout_ms,
      userVerification: "preferred",
    },
  }
}

export function createOptions(r: CreateRequest): CredentialCreationOptions {
  return {
    publicKey: {
      challenge: buf(r.challenge),
      rp: { id: r.rp_id, name: r.rp_name },
      user: { id: buf(r.user_id), name: r.user_name, displayName: r.user_name },
      // ES256 and Ed25519, the two the server checks.
      pubKeyCredParams: [
        { type: "public-key", alg: -7 },
        { type: "public-key", alg: -8 },
      ],
      excludeCredentials: r.exclude.map((id) => ({ type: "public-key", id: buf(id) })),
      timeout: r.timeout_ms,
      attestation: "none",
      authenticatorSelection: { userVerification: "preferred", residentKey: "discouraged" },
    },
  }
}

/** The assertion as the worker passes it on. */
export function getAnswer(c: PublicKeyCredential): GetAnswer {
  const r = c.response as AuthenticatorAssertionResponse
  return {
    credentialId: arr(c.rawId),
    clientDataJson: arr(r.clientDataJSON),
    authenticatorData: arr(r.authenticatorData),
    signature: arr(r.signature),
  }
}

/** The new credential as the worker passes it on. */
export function createAnswer(c: PublicKeyCredential, name: string): CreateAnswer {
  const r = c.response as AuthenticatorAttestationResponse
  return { name, clientDataJson: arr(r.clientDataJSON), attestationObject: arr(r.attestationObject) }
}

/** Uses a key for a waiting sign-in; `null` when the person backed out. */
export async function answerWithKey(credentials: CredentialsContainer, r: GetRequest): Promise<GetAnswer | null> {
  const c = (await credentials.get(getOptions(r))) as PublicKeyCredential | null
  return c ? getAnswer(c) : null
}

/** Makes a key's credential for the account; `null` when the person backed out. */
export async function registerKey(credentials: CredentialsContainer, r: CreateRequest, name: string): Promise<CreateAnswer | null> {
  const c = (await credentials.create(createOptions(r))) as PublicKeyCredential | null
  return c ? createAnswer(c, name) : null
}

/** Which key page the address names: `#/second-factor/<id>` or `#/security-key/<id>`. */
export function keyPageFromHash(hash: string): { kind: "second-factor" | "security-key"; id: string } | null {
  const m = /^#\/(second-factor|security-key)\/([0-9a-fA-F-]{8,64})$/.exec(hash)
  if (!m?.[1] || !m[2]) return null
  return { kind: m[1] as "second-factor" | "security-key", id: m[2] }
}
