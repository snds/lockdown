# Passkeys through the browser extension: first slice

_Status: built, 2026-10-18, under D122 ("desktop and web first"). Implements the extension-override half of `03-architecture.md` §4 ("Passkey provider path by platform": the fallback where no OS provider exists), the Passkey anatomy of `02-model.md` §4, the passkey rows of `04-security.md` §12 (second deliberate action) and the Signal API handling the research brief asks for (hide, never delete, with undo). Builds on `16-browser-extension.md` (the channel, pairing, the inline menu, three checks per fill) and `09-extension-boundary.md` (`fill_check`, related origins)._

## For future agent

- **TL;DR:** Lockdown is now a WebAuthn authenticator for pages, through the extension and the running desktop app. A script in the page's own world wraps `navigator.credentials.create/get` and the Signal API in the top frame; a bridge in the extension's isolated world carries each request to the background; the background asks the app; the core (`lockdown-passkey` for the bytes, `lockdown-app::passkeys` for the rules) checks the RP ID against the browser-reported page, runs the `Approve` gate for user verification, makes or signs, and stores the passkey as a field group on the Login it belongs to, where it syncs, versions and exports like every field. The person says yes only in Lockdown's own extension-origin sheet (or the inline menu, for conditional mediation), armed by IntersectionObserver v2, with the second deliberate action for a sign-in. Every sheet has "Use another device or security key", which hands the untouched request to whatever was installed before (the browser's own authenticator, or another manager's wrapper).
- **Read with:** `crates/passkey/src/lib.rs` (the authenticator), `crates/app/src/passkeys.rs` (the rules), `crates/app/src/session/passkeys.rs` (the field layout), `apps/desktop/src-tauri/src/browser/passkeys.rs` (the endpoint ops and the in-app approval), `extension/src/main/webauthn.ts` (the page side and why it carries no authority), `extension/src/background/passkeys.ts`, `extension/src/content/passkeys.ts`, `extension/src/pages/frame.ts` (the sheet).
- **Do not:** take an origin, an RP ID decision or an approval from anything the page's world sends; let a passkey's `key` part leave `lockdown-app` (only the vault seals it; `reveal` refuses every passkey field); make passkeys from a frame or answer one there; delete a passkey on a Signal; offer a crown-jewel passkey to a page; freeze or make non-configurable the wrapped `navigator.credentials` members (that is what breaks other managers).

## 1. What was built, per row

| Row | State | What is real | What is missing |
|---|---|---|---|
| `extension-passkey-fallback` | done (Chromium; Firefox untested) | `webauthn.js` in the top frame's main world at `document_start` wraps `create`, `get`, `isConditionalMediationAvailable`, `getClientCapabilities` and the three Signal methods, chaining onto whatever was installed before (§3). Lockdown answers when the browser is paired, the app unlocked and **Settings → Browsers → Offer Lockdown's passkeys in browsers** is on (default on: no Lockdown OS provider exists on any platform yet, D122). It steps aside, passing the request unchanged, when it has no passkey for the site, when the site hints a security key or asks for a cross-platform authenticator, when the app is unreachable or unpaired, or when the person picks "Use another device or security key". The coexistence convention is §3.3. How the page's call reaches the extension without being able to forge an approval is §3.1. | Firefox is not driven by a test (Playwright can't load an add-on there); frames go to the browser's own (§8); Windows has no transport to the app yet, so there the browser's own always answers. |
| `passkey-sync` (create and use) | done (extension path) | The core makes an **ES256** credential (`ring`, already in the tree through rustls; EdDSA is signed with when an imported passkey holds one, never made, D200), stores it on the Login, signs assertions with the **sign count always zero** (02-model §4: no counter, so it moves under CXF and syncs without a counter to fork), sets **BE and BS** (it syncs), and runs user verification through the `Approve` gate (D207). The private key is generated, sealed and used inside `lockdown-app`; nothing outside it ever reads the `key` part. A passkey made on one device signs on another after a sync (`a_passkey_made_on_one_device_signs_in_on_another_after_a_sync`), and survives a CXF export and import. | The OS providers (macOS AutoFill, Windows plug-in, Android, iOS) stay deferred (D122); phone-as-authenticator (hybrid) is not this slice. |
| `login-items` | done | A passkey made in the browser joins the live, everyday Login whose user name matches and whose website is the RP's site or matches the page; with none, a new Login for the page's origin, titled with the RP's name, sign-in method "Passkey". A passkey for the same site and user handle replaces the old one where it is (WebAuthn §6.3.2 step 7). | Choosing a different Login than the one Lockdown picks (the sheet says which one it will be). |
| `passkey-management` | done | Each passkey on an item shows its provider and **provider ID (AAGUID)**, the algorithm, **BE/BS** as "synced with your vault" or "on this device only", when it was made, when this computer **last used** it (device-only, D151's record), a site's hide with **Undo**, and **Delete**, which warns when it is the account's only way in that Lockdown holds (no password, no other visible passkey, not signed in through another account) and then says "Delete anyway". A delete is a new version, so history keeps the old one; a crown jewel's goes through the `Use` gate. The editor keeps a passkey as one read-only line, so its parts can't be removed one at a time. | Last use is per computer by design (02-model §10). |
| `signal-api` | done (Chromium) | `signalUnknownCredential` hides that credential; `signalAllAcceptedCredentials` hides that user's credentials the list leaves out and brings back the ones it hid if they are listed again; `signalCurrentUserDetails` updates the passkey's user and display names (a new version). Nothing is deleted. Signals reach only the page's own site (no related origins). A hidden passkey isn't offered, its Login joins **Needs attention** (`passkey-hidden-by-site`), and the item shows "hidden by the site" with Undo. The browser's own Signal handling (Chrome, Edge) still runs: the wrapper passes every Signal on. | Firefox has no Signal API of its own; the wrapper adds the three methods there, untested. Signals that arrive while Lockdown is locked or unpaired are dropped, not queued. |
| `conditional-create` | partial | **Conditional mediation for sign-in** works: a page's `get({mediation: "conditional"})` registers with the extension, and focusing its sign-in field (`autocomplete="… webauthn"`, or any field the inline menu treats as a login field) lists its passkeys first, with its logins, in the existing closed-shadow top-layer menu; a pick takes the second deliberate action. "Use a passkey from another device" in that menu hands over to the browser's own (modal) picker; with no Lockdown passkey for the site, the browser's own autofill runs as if Lockdown weren't there. **Conditional create** (the automatic upgrade) has a safe path and is built: only when this core itself filled that Login's password into that host within the last five minutes and the user names match, with no prompt and UP = UV = 0, forgotten on lock (D210); otherwise the request goes to the browser's own. | The OS-level automatic upgrade (iOS 18, macOS 15) is deferred with the OS providers (D122), so the row stays partial. |
| `clickjacking-defense` (passkeys) | done | The sheet and the menu are extension-origin frames in a closed shadow root in the top layer, armed only after 400 ms uncovered (IOv2 on Chromium; the bridge's hit test of the sheet's host on Firefox), deaf to untrusted events. Saving a passkey takes one armed click; signing in takes two (pick, then "Sign in"), and the extension's wasm `fill_check` and the core's both ask for the second (`FillKind::Passkey`). A page that covers the sheet leaves its buttons disabled (`red team: a page that covers the sheet cannot click through it`). | Firefox's hit test does not see a `pointer-events:none` decoy over the sheet itself (the same gap as the menu, 16 §7). |

## 2. The authenticator (`crates/passkey`)

| Piece | What it does |
|---|---|
| `key` | `CredentialKey::generate()` makes a P-256 key and its PKCS#8 v1 document with the public key (RFC 5915, what CXF carries); `from_pkcs8` reads P-256 (with its public half) or Ed25519; `cose_public_key` writes the COSE_Key in CTAP2 canonical order (`{1: 2, 3: -7, -1: 1, -2: x, -3: y}`); `spki` gives `SubjectPublicKeyInfo` DER for the page's `getPublicKey()`; `sign` gives ASN.1 DER for ES256, raw 64 bytes for Ed25519. |
| `rp` | `Caller::of(page)`: the browser-reported address must be a secure context with a domain name (`https`, or `localhost` and `*.localhost`; an IP address is refused, as browsers do). `rp_id_for`: no RP ID means the caller's host; a named one must equal the host or be a registrable-domain suffix of it, never a public suffix (HTML's algorithm, PSL from the `psl` crate); else the caller must be in the RP's related-origins document under the matcher's five-label cap. |
| `client_data` | `clientDataJSON` serialised exactly as WebAuthn L3 §5.8.1.1 says a client does (member order, CCDToString escaping), so a relying party's limited-verification parser reads it. The origin is the caller's, from the browser. |
| `make_credential` / `get_assertion` | Authenticator data (`rpIdHash ‖ flags ‖ 0000 ‖ [AAGUID ‖ length ‖ credential id ‖ COSE key]`), the `none` attestation object (`fmt`, `attStmt`, `authData`, canonical order) and the signature over `authData ‖ SHA-256(clientDataJSON)`. A credential id is 16 random bytes; Lockdown's AAGUID is `07017dfd-2717-4c97-92c2-b3d6513f46ce`. |

## 3. The page side

### 3.1 How the page's call reaches the extension, and why the page can't forge an approval

```text
 page's world:  navigator.credentials.get(...)  →  webauthn.js (wrapper, document_start, top frame)
                    │ MessagePort (offered with window.postMessage at document_start; first one kept)
 isolated world:    passkeys.js (bridge, document_start, top frame)
                    │ runtime.sendMessage: {t: "pk_begin", op, mediation, request}
 background:        sender.url and tabs.get give the page; the app is asked (passkey_options / _plan)
                    │ tabs.sendMessage {pk_done | pk_measure} back to the bridge
 sheet frame:       frame.html#passkey:<request id>   (extension origin, closed shadow root, top layer)
                    │ runtime.sendMessage {pk_sheet_pick | pk_sheet_create | …}: the only yes
 app:               passkey_get / passkey_create: the core checks again, verifies, signs
```

The wrapper shares its world with the page, so the page can read it, call it, post the same messages, even take the port. That is accepted, because **nothing on that path approves anything**:

1. A request carries the page's own options and no authority. The page's address comes from the browser (`sender.url`, `tabs.get`), never from a message, and only a request from the tab's top frame (`frameId 0`) is answered.
2. The core checks the RP ID against that address and writes the origin into `clientDataJSON` itself, so whatever comes back is bound to the page's own origin and RP ID.
3. The only "yes" is a trusted click in the extension-origin sheet or menu, whose messages the background accepts only from `frame.html`, in the same tab, carrying the request id the background generated (the frame learns it from its own URL, inside the closed shadow root). A pick is checked against what that sheet was offered.
4. Presence is measured, not claimed: IOv2 (or the hit test) for the sheet, the field's D67 measurement for the inline menu, then `fill_check` in the extension and again in the core.

So a page that forges a request gets at most what it could ask for honestly: a passkey for its own origin, after the person said yes in Lockdown's frame. A page that forges an answer to its own wrapper fools only itself.

### 3.2 What the person sees

| Request | Sheet or menu | Words |
|---|---|---|
| `create`, modal | sheet | "Save a passkey for Fixture RP in Lockdown?" · "As sean@example.com, in your login "Local site"." (or "in a new login for example.com") · **Save passkey** · Cancel · Use another device or security key |
| `create`, an excluded credential is in Lockdown | sheet | "Lockdown already holds a passkey for this account on example.com." · OK → `InvalidStateError` |
| `get`, modal | sheet | "Sign in to example.com with a passkey" · a row per passkey (title, user) · Use another device or security key → then "Sign in to example.com as sean?" · **Sign in** · Back |
| `get`, conditional | inline menu on the sign-in field | rows "Passkey · sean" before the logins · "Use a passkey from another device" → then the same second action |
| any, Lockdown locked | sheet | "Lockdown is locked. Unlock it on this computer to use your passkeys here." · Try again · Use another device or security key |
| a refusal | sheet | the core's words · OK → `NotAllowedError` |

### 3.3 The coexistence convention (proposed, D204)

Chrome and Firefox have no provider API for extensions, so every manager wraps the page's WebAuthn functions, and they collide (foundation §1.5). Lockdown follows, and proposes to other vendors, five rules:

1. **Wrap, never replace.** Keep whatever `navigator.credentials.get` / `create` (and the static `PublicKeyCredential` members) were when Lockdown installed: the browser's own, or another manager's wrapper.
2. **Hand over unchanged.** When the person picks another authenticator, when Lockdown has nothing for the site, when the request is for a security key, or when Lockdown is unreachable, call the previous function with the page's own options object (signal included), as if Lockdown weren't there.
3. **Install as an ordinary property.** Writable, configurable, never frozen, never `configurable: false`, so a later manager can wrap Lockdown in turn.
4. **Mark the wrapper.** `fn[Symbol.for("webauthn.provider.override")] = {provider: "Lockdown", version: 1, next}`, so another manager can see the chain and choose to chain onto it rather than displace it.
5. **Pass Signals on.** Hear a Signal and also call the previous method, so the browser's and other managers' handling still runs.

### 3.4 Smaller rules

- Top frame only: the scripts are not injected into frames, so a frame's WebAuthn is the browser's own (with its permissions policy).
- A modal request times out per the page's `timeout`, clamped to 30 s–10 min (default 5 min); a conditional one waits up to 10 min. `AbortSignal` aborts: the sheet closes and the page gets the signal's `AbortError`.
- One modal and one conditional request per tab; a newer one replaces the older.
- What the page gets back is a real `PublicKeyCredential` by prototype (`instanceof` holds) with own properties: `id`, `rawId`, `response` (`clientDataJSON`, `attestationObject`, `getTransports()` = `internal, hybrid`, `getAuthenticatorData()`, `getPublicKey()`, `getPublicKeyAlgorithm()`, or `authenticatorData`, `signature`, `userHandle`), `authenticatorAttachment: "platform"`, `getClientExtensionResults()` (`credProps.rk = true` when asked) and `toJSON()`.
- `isConditionalMediationAvailable()` is `true` while Lockdown answers, else the browser's own answer; `getClientCapabilities()` adds `conditionalGet`, `conditionalCreate`, `relatedOrigins` and the three Signal capabilities while Lockdown answers.
- Related origins: when the page names an RP ID it isn't under, the background fetches `https://<rp id>/.well-known/webauthn` (no credentials, no redirects, ≤ 64 KiB) and hands the document to the core, which applies the cap.

## 4. Messages (extension ↔ app)

Added to the table in `16-browser-extension.md` §4. All need the pairing; all answer `{outcome: "done", result}`, `{outcome: "confirm", reason}` or `{outcome: "refused", name, message}` (`name` is the DOMException the page gets). With the setting off, each answers the error code `passkeys_off`. `status` gains `passkeys: bool`. Binary values are base64url.

| op | In | Out (`result`) | Secret in the answer |
|---|---|---|---|
| `passkey_options` | `page {top, frame, related?}`, `request {rp_id?, challenge, allow[], user_verification}` | `{rp_id, options: [{item_id, credential_id, title, user_name, user_display_name}]}` | no |
| `passkey_plan` | `page`, `request {rp_id?, rp_name, user_id, user_name, user_display_name, challenge, algs[], exclude[], user_verification, cred_props, conditional}` | `{rp_id, excluded, item_id?, title, user_name}` | no |
| `passkey_create` | `page {…, visible, user_confirmed}`, `request` | `{item_id, credential_id, client_data_json, attestation_object, authenticator_data, public_key, public_key_algorithm, transports, user_verified, resident_key?}` | no (public key only) |
| `passkey_get` | `page {…, visible, user_confirmed}`, `request`, `item_id`, `credential_id`, `user_name` | `{item_id, credential_id, client_data_json, authenticator_data, signature, user_handle, user_verified}` | no (a signature) |
| `passkey_signal` | `page` (address), `signal {signal: unknown_credential \| all_accepted_credentials \| current_user_details, …}` | `{changed}` | no |

## 5. What the core decides (`crates/app/src/passkeys.rs`)

**Storage.** A passkey is the group `passkey.<n>.*` on its item (`session/passkeys.rs`): `rp_id`, `user_name`, `user_display_name` (text); `credential_id`, `user_handle`, `key` (passkey bytes; the key is the PKCS#8 document); and, new and additive, `aaguid`, `created` (text), `backup_eligible`, `backed_up` (boolean) and `hidden` (`unknown <secs>` or `not-accepted <secs>`). An import without the new parts reads as synced with no provider ID. Groups stay numbered `0..k` after a delete (an export walks them in order). Lockdown's own CXF carries the new parts in its item extension, so a move between Lockdown vaults keeps them; other formats carry the six CXF parts.

**User verification (D207).**

| Site asks | Platform check installed (Touch ID, Windows Hello) | Account password only | Neither |
|---|---|---|---|
| `required` | the check; UV = 1 | the app's window asks (D208); UV = 1 | refused: `NotAllowedError`, "this site asks Lockdown to check it's you, and this computer has neither…" |
| `preferred` | the check; UV = 1 | presence only; UV = 0 | presence only; UV = 0 |
| `discouraged` | presence only | presence only | presence only |

A "no" from the check is `NotAllowedError` with its words; nothing is saved or signed. The window's plain confirm is never verification, so it never sets UV.

**Presence.** A create needs `visible` and `user_confirmed` (one armed click); a get goes through `fill_check(FillKind::Passkey)`, so it needs the second deliberate action and refuses when covered. The asking frame must be the top frame.

**What a page may be offered.** Live Logins and Passkey items in every open vault, everyday class only (a crown jewel is never offered to a page, targeted or replaced by a create, or changed by a Signal, 02-model §3), not hidden by a Signal, in the allow list when there is one, ordered by title, user name and credential id.

**Uses.** A sign-in counts as a use of the item (D151) and records the passkey's own last use in the same sealed device-only file (`recency-<persona>.cbor`, a `passkeys` map added within v1).

## 6. The desktop

| Piece | Job |
|---|---|
| `features/passkeys/passkeys-block.tsx` | The passkeys on a Login or a Passkey item: provider and provider ID, algorithm, BE/BS, made, last used here, a site's hide with Undo, Delete with the only-sign-in warning. |
| `features/passkeys/passkey-approval-dialog.tsx` | A browser's passkey request that needs the account password: "Sign in to github.com as sean?", Approve (through the gate) or Decline; polled like pairing. |
| `features/browsers/browsers-section.tsx` | The "Offer Lockdown's passkeys in browsers" switch (`settings.browser_passkeys`). |
| `src-tauri/src/passkey_commands.rs` | `passkeys`, `passkey_delete`, `passkey_restore`, `browser_passkey_pending`, `browser_passkey_answer`, appended to the handler list. |
| `src-tauri/src/browser/passkeys.rs` | The five endpoint ops and the approval queue: a request waits at most two minutes, three at once per browser, on its connection's thread (never the core's). |

## 7. How to run and test

```sh
cargo test -p lockdown-passkey                  # the authenticator
cargo test -p lockdown-app --test passkeys      # the rules, a CXF round trip, a two-device sync
cargo test -p lockdown-desktop                  # the endpoint ops and the approval queue
cd apps/desktop && pnpm test                    # the management view, the approval dialog, the editor
cd extension && pnpm run build:wasm && pnpm run build:native && pnpm build && pnpm test   # passkeys.spec.ts among them
```

| Layer | Where | Count | What |
|---|---|---|---|
| Authenticator | `crates/passkey` | 13 | base64url and CBOR forms; ES256 made, signed and verified with `ring`'s verifier; Ed25519 imports sign; the COSE key's members and order; the attestation object read back as an RP does; the sign count zero; ES256 chosen when the RP lists EdDSA first; client data order and escaping; secure-context and RP ID rules (public suffixes, private suffixes such as `github.io`, IPs, `localhost`); related origins and the five-label cap; provider names |
| Core | `crates/app/tests/passkeys.rs` | 10 | made on the right Login and verified as an RP does; where a new one goes, replacement, exclusion, Security / NotAllowed / NotSupported / Type refusals, covered pages, frames; crown jewels never offered, replaced or hidden by a page; allow lists; user verification through the gate (and refused rather than faked); Signals hide with undo, rename, never cross sites; delete and the only-sign-in flag; the upgrade rule; CXF export and import then sign; a sync to a second device then sign |
| Core (changed) | `recency.rs`, `settings.rs` | 2 extended | the passkey's last use in the sealed record; the new setting |
| Endpoint | `apps/desktop/src-tauri/src/browser/` | 4 | every op over the protocol with nothing secret in an answer, a SecurityError by name, a Signal, the setting off; pairing first; the approval queue (wait, decline, expiry, limit) and an answer through the gate |
| Desktop UI | `features/passkeys/*.test.tsx`, `editor-model.test.tsx` | 7 | the management view's facts, Undo, the only-sign-in warning and "Delete anyway", the ordinary delete; the approval dialog's Approve and Decline; a passkey as one kept line in the editor |
| Browser | `extension/test/passkeys.spec.ts` | 9 | in Chromium, through the real host and endpoint, against a real RP page that parses the attestation and verifies every signature with WebCrypto: create on the right Login with UV from the platform check; sign-in with the second action; conditional mediation in the inline menu; UV required and a "no"; a covered sheet stays disabled; an excluded credential is `InvalidStateError`; Signals rename and hide, nothing offered while hidden, Undo; "Use another device or security key" registers on the browser's own (virtual) authenticator; the setting off steps aside and back on |
| Manifests | `extension/test/manifest.spec.ts` | 1 | the wrapper in `MAIN`, the bridge isolated, both top frame only at `document_start`, neither web-accessible |

The suite starts the endpoint fixture with `--platform-check`, a stand-in for Touch ID that answers `platform yes|no` and prints each prompt, so user verification is seen happening.

## 8. Security notes and gaps

- **The key.** Made by `ring` in the core, sealed into the vault as a field, read back only by `Session::passkey_key` (crate-internal) to sign, and wiped on drop. `reveal` and `show(reveal)` refuse every passkey field; no op answers with it; the webview never sees it.
- **JavaScript holds the signature and the public key** for the moment of the answer; neither is secret.
- **Same-user malware** that can drive the browser could ask for a passkey sheet like any page; the sheet still needs the person's click, and the site's own origin is what gets signed.
- **Frames** get the browser's own authenticator, even same-origin ones: the sheet would be clipped in a small frame and IOv2 would never arm it.
- **The service worker** holds open requests in memory; if the browser stops it mid-request, the page times out with `NotAllowedError`.
- **Not supported:** RS256 and EdDSA for new passkeys, PRF (`prf-unlock` is its own row), `largeBlob`, enterprise attestation; an imported P-256 key whose PKCS#8 lacks its public half (refused in words: `ring` can't derive it).
- **The approval dialog** doesn't raise the app's window, as pairing doesn't.
- **The socket** still carries plain JSON (D162); passkey answers are public data, but a session key remains the next step there.
- **Firefox:** same code, MV3 `world: "MAIN"` content scripts (Firefox 128, the manifest's floor); no IOv2 (the bridge's hit test stands in); no native Signal API; not driven by a test. Whether a page's CSP can block a declared main-world content script on Firefox is **[uncertain]** (foundation §1.5 reports CSP breaking injected scripts; declared ones are not injected through the DOM).

## 9. Windows and Safari (deferred)

- **Windows.** The extension reaches the app only over the Unix socket today (16 §7: no named-pipe transport in the identity crate), so on Windows every passkey request goes to the browser's own, which is Windows Hello or the Windows passkey plug-in model. Lockdown's own path there is the `IPluginAuthenticator` COM plug-in with Windows Hello verification and MSIX packaging (`windows-passkey-plugin`, deferred under D122); once it exists, the app turns `passkeys` off in `status` on that machine and the extension steps aside for it, which this design already does. Until then, the named-pipe transport would make this extension path work on Windows; Windows Hello pre-empting extension overrides on Firefox (foundation §1.5) is the known hazard.
- **Safari.** A Safari Web Extension has no passkey provider API either, and Apple's path is the AutoFill credential provider extension (`ASCredentialProviderViewController`, passkeys from macOS 14 and iOS 17, conditional registration from macOS 15 and iOS 18), which the native app owns (`macos-autofill`, deferred). The main-world wrapper should not ship in the Safari build: Safari's own sheet already offers the OS provider, so the Safari extension would leave passkeys to it.

## 10. unicron

Nothing blocking. The desktop's passkey list is unicron `Item`, `Badge`, `Button`, `Dialog` and `Switch`; the sheet in the page is the extension's plain stylesheet for the reasons in 16 §9. A credential row (provider, flags, dates, a hidden state with Undo) is a candidate unicron pattern once the iOS and Android providers need the same view.

## 11. Proposed decisions

For the coordinator to record. **Flagged** rows are user-visible and need Sean.

| ID | Decision | Owner |
|---|---|---|
| D200 | **Lockdown's authenticator**: new passkeys are ES256 only (P-256 through `ring`, already linked by the TLS stack; EdDSA is signed with when an imported passkey holds one, never made, so a passkey stays portable to every manager); always discoverable; attestation `none` with Lockdown's AAGUID `07017dfd-2717-4c97-92c2-b3d6513f46ce` in the attested data; the sign count always zero (02-model §4); BE and BS set because the passkey syncs; 16-byte random credential ids. Plain version: Lockdown makes the most widely accepted kind of passkey, says it made it, and keeps no counter, so the passkey works on every device you sync. | Accepted by default under D38; audit item |
| D201 | **A passkey lives on the Login it belongs to**: the field group `passkey.<n>.*` (adds `aaguid`, `created`, `backup_eligible`, `backed_up`, `hidden` to the import's six parts); a new one joins the live everyday Login with the same user name whose website is the site, else a new Login for the page's origin titled with the site's name, sign-in method Passkey; the same site and user handle replace in place; `reveal` refuses every passkey field. Plain version: your passkey is saved in the same record as that account's password, and its key can't be shown. | Accepted by default under D38; **flagged for Sean** (where a new passkey goes) |
| D202 | **Who asks**: only the tab's top frame, as the browser reports it; a secure context with a domain name (`https`, `localhost`); the RP ID equal to the host or a registrable suffix of it, never a public suffix; otherwise a related origin from the RP's `/.well-known/webauthn` under the five-label cap; the client data built in the core from that origin. Plain version: a site can only use passkeys that belong to it, and the browser, not the page, says which site it is. | Accepted by default under D38; audit item |
| D203 | **How the page's call reaches the extension**: a main-world wrapper at `document_start` in the top frame, a MessagePort to the isolated bridge, runtime messages to the background; the path carries no authority (§3.1); the only yes is a trusted, armed click in the extension-origin sheet or menu, accepted only from `frame.html` in the same tab with the request id the background made. Plain version: a page can ask, but only you can answer, and only in Lockdown's own box. | Accepted by default under D38; audit item |
| D204 | **The coexistence convention** (§3.3): wrap, never replace; hand over unchanged; install writable and configurable; mark the wrapper with `Symbol.for("webauthn.provider.override")` `{provider, version, next}`; pass Signals on. Proposed to other managers as is. Plain version: Lockdown takes turns with other password managers instead of locking them out. | Accepted by default under D38 |
| D205 | **When Lockdown answers**: paired, unlocked, and Settings → Browsers → "Offer Lockdown's passkeys in browsers" on (default on everywhere until Lockdown's OS providers exist); it steps aside when it has nothing for the site, for security-key and cross-platform requests, when unreachable, and on "Use another device or security key", which every sheet has; locked, the sheet says so with Try again. Plain version: Lockdown offers its passkeys in the browser and always lets you pick something else. | **Flagged for Sean** (the switch and its default; the sheet's words) |
| D206 | **Presence**: saving a passkey takes one armed click on "Save passkey"; signing in takes two (pick, then "Sign in"), as a card fill does; both armed by IOv2 after 400 ms and refused when covered; checked by `fill_check(Passkey)` in the extension and again in the core. Plain version: nothing signs you in on a hover or a hidden click. | Accepted by default under D38; **flagged for Sean** (the two taps to sign in) |
| D207 | **User verification through the `Approve` gate** (§5 table): `required` runs it in full and is refused, never faked, where the machine has neither a platform check nor an account password; `preferred` uses the platform check where installed and is presence only otherwise; `discouraged` is presence only; UV is set only when a real check ran. Plain version: when a site wants proof it's you, Lockdown asks for your fingerprint or password, and says so when it can't. | **Flagged for Sean** (a fingerprint on most sign-ins where Touch ID or Hello exists) |
| D208 | **Approval in the app for password-only machines**: a `required` request waits up to two minutes (three per browser) while the app's window asks "Sign in to github.com as sean?" with Approve (the account password through the gate) or Decline; the password never reaches the browser. Plain version: on a computer without a fingerprint reader, you confirm in Lockdown's window. | **Flagged for Sean** (the dialog and its words) |
| D209 | **Conditional mediation**: a page's conditional `get` lists its Lockdown passkeys first in the inline menu on its sign-in field, with "Use a passkey from another device" handing over to the browser's own picker; with none in Lockdown, the browser's own autofill runs untouched. Plain version: your passkey shows up in the same little menu as your password. | **Flagged for Sean** (passkeys before passwords in the menu) |
| D210 | **The automatic upgrade (conditional create)** happens only when this core filled that Login's password into that site in the last five minutes and the user names match; no prompt, UP and UV clear, the record forgotten on lock; anything else goes to the browser's own. Plain version: right after Lockdown signs you in with a password, the site may quietly add a passkey to that same login. | **Flagged for Sean** (a passkey saved without a prompt) |
| D211 | **Signals**: unknown-credential hides that passkey; all-accepted hides that user's unlisted passkeys and restores those it hid when listed again; current-user-details renames (a new version); nothing is deleted; only the page's own site; Undo in the item; Needs attention shows the Login. Plain version: when a site says a passkey is gone, Lockdown hides it and lets you bring it back. | Accepted by default under D38; **flagged for Sean** ("hidden by the site", Undo) |
| D212 | **The management view**: provider and provider ID from the AAGUID (a short built-in list: Lockdown, Google Password Manager, iCloud Keychain, Bitwarden, 1Password, Proton Pass; otherwise "Another provider"), algorithm, BE/BS in words, made, last used on this computer only, and a delete that warns when the passkey is the account's only way in (no password, no other visible passkey, not through another account) and keeps the old version in history. Plain version: see who made each passkey and when you last used it, and get a warning before you delete your only way into an account. | **Flagged for Sean** (the warning's words) |
| D213 | **Crown-jewel passkeys** are never offered to a page, never receive a new passkey from one, are never replaced by one (refused in words) and are never changed by a site's Signal (02-model §3; their changes go through the gate in the app); signing in with one from the browser waits for a flow inside the app. Plain version: your most sensitive logins don't sign in from the browser, and a site can't change them. | Accepted by default under D38 |
| D214 | **Additive wire changes** (D62): the extension↔app ops `passkey_options`, `passkey_plan`, `passkey_create`, `passkey_get`, `passkey_signal`; `status.passkeys`; `settings.browser_passkeys`; the recency file's `passkeys` map within v1; the `lockdown-passkey` crate. Plain version: older and newer parts keep understanding each other. | Accepted by default under D38 |
