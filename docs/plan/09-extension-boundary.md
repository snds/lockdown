# The extension boundary — v0.1 (as built)

_Status: as built, 2026-10-17. Implements `07-workspace.md` §4 step 5 over `03-architecture.md` §3 (bindings), §4 (extension modes), `04-security.md` §12 (clickjack-hardened autofill) and `06-roadmap.md` workstream 10 (the matching set). Companion to `02-model.md` §10 (the URL rules)._

## For future agent

- **TL;DR:** The matcher now has one JSON wire shape (`lockdown_matcher::wire`), `lockdown-wasm` exports it over `wasm-bindgen` (`decide`, `fill_check`, `related_origins`, `load_matcher_data`, `matcher_data_status`, `set_matcher_data_floor`, `WasmSession` for one-time codes), the curated groups and the confusables table ship as one Ed25519-signed CBOR file the matcher verifies before reading, a generated 1,533-row matching set over 60 real sites passes end to end, and a Playwright harness replays the clickjacking red team in a real Chromium against the wasm build. CI has an `extension` job that builds the wasm and runs the harness.
- **Read with:** `extension/README.md` (how to run it), `extension/schema/README.md` (the message table), `crates/matcher/src/signed.rs` (the file format, in code), `crates/matcher/data/dev/README.md` (the dev-key rule), `crates/matcher/tests/sites/gen.rs` (how the set is built).
- **Do not:** add a field to a wire message without adding it to the schema file (a test fails), hand-edit `matcher-data.signed.cbor` or `publisher.pub` (run the example), lower `matcher_data` or reuse a number, trust `dev-2026-10` in a release build, or pass a secret through `decide`/`fill_check` (they take addresses only).

## 1. What crosses the boundary

Three surfaces call the same Rust (`03-architecture.md` §3): the MV3 extension through wasm, the Tauri web view through a command, the Swift and Kotlin shells through UniFFI. `lockdown_matcher::wire` is the one request/response shape all of them speak, so the schema is owned by the matcher and the bindings are thin.

| Call | In | Out | What it is |
|---|---|---|---|
| `decide` | `{saved, current, rule?}` | `{v, decision, saved, current}` | How a saved address relates to the page: `exact`, `base_domain`, `equivalent`, `lookalike`, `no_match`. The two `OriginView`s say what was compared (lowercase punycode host, port, path, registrable domain). |
| `fill_check` | `{top, frame, field_visible, user_confirmed, kind, decision}` | `{v, verdict, reason?}` | May the extension fill now: `fill`, `confirm`, `refuse`. The page reports what it measured; the core decides. Order: match decision, visibility, cross-origin frame, then the second-action rule for `code`, `card`, `passkey`. |
| `related_origins` | `{rp_id, origins, fetched_at, now, max_age_secs, origin}` | `{v, allowed, effective}` | WebAuthn related origins: the five-label cap and freshness. Passkeys only. |
| `load_matcher_data` | signed bytes | `{v, version, newest_loaded, key_id, published, groups, confusables}` | Verifies and installs a signed data file (§2). Throws `bad_signature`, `unknown_key`, `downgrade` or `bad_data`; the data in force is unchanged then. |
| `matcher_data_status` | — | the same object | The data in force and the floor. `newest_loaded` is what the host persists (§4). |
| `set_matcher_data_floor` | a version | the floor in force | The host's remembered newest version from an earlier run. Never lowers. |
| `WasmSession.import_code_uri` | an `otpauth://` or `steam://` URI | a handle (number) | The one inbound crossing of a secret: the seed goes into Rust memory once and the page gets a number. |
| `WasmSession.code` | handle, now | `{v, code, seconds_left?, issuer?, account?}` | The digits to fill. The seed never comes back. |
| `WasmSession.lock` | — | — | Wipes every seed and secret. |
| errors | — | thrown `Error` whose message is `{v, code, message}` | Codes: `not_a_url`, `bad_data`, `bad_request`, `bad_signature`, `unknown_key`, `downgrade`, `invalid_handle`. |

**Rules.**

- Every response carries `v`, the schema version, today `1`. Within one `v`, fields are only ever **added**; never renamed, removed or retyped; readers ignore fields they do not know. A breaking change bumps `v` and the old shape stays readable for one release. (D49 below.)
- Field names are `snake_case`, enums are lowercase words. Numbers are JSON numbers (the wasm serialiser is set to JSON-compatible mode, so `u64` versions arrive as numbers, not `BigInt`). Handles are `u32`, seconds are plain numbers.
- The published form is JSON Schema (draft 2020-12) under `extension/schema/`. `crates/matcher/tests/wire_schema.rs` serialises every response type and fails when a field is emitted that the schema does not declare, a required field is not emitted, or an enum value is missing.
- Nothing secret crosses `decide`, `fill_check` or `related_origins`: addresses, rules, verdicts and their plain-language reasons. The verdict text is written for the person and shown as is. (D52 below.)

The wasm build: `extension/build-wasm.sh` compiles `lockdown-wasm` for `wasm32-unknown-unknown` and runs the `wasm-bindgen` CLI (`--target web`, an ES module a page, worker or offscreen document imports without a bundler) into `extension/pkg/`. The CLI version must equal the crate pin (`=0.2.129` in `crates/wasm/Cargo.toml`); the script checks and says what to install. Two build facts worth knowing: `getrandom` reaches the wasm build through the crypto stack and needs its `wasm_js` backend (a feature on 0.4; a feature **and** `--cfg getrandom_backend="wasm_js"` on 0.3), which the script passes through the target-scoped flags variable after folding in any `RUSTFLAGS`; and the `uuid` crate wants its `js` feature on this target. Both live in `crates/wasm/Cargo.toml` under a `wasm32` target table, so host builds never see them. The release `.wasm` is 1.7 MB before `wasm-opt`; `07-workspace.md` §5 q.1 (a `-primitives` split of the identity crate) is the lever for size.

## 2. The signed data file

The curated equivalent-domain groups and the confusables table were compiled into the matcher. They are data that changes more often than code and that an attacker would love to edit (add `evil.net` to the `paypal` group; map `l` to nothing), so they now ship as one signed file.

```text
envelope (CBOR map) {
  "format":    "lockdown-matcher-data",
  "v":         1,                     // envelope version
  "key_id":    "dev-2026-10",         // which trusted key signed it
  "payload":   bytes,                 // canonical CBOR of the payload
  "signature": bytes (64)             // Ed25519 over "lockdown-matcher-data/v1\0" || payload
}
payload (canonical CBOR map) {
  "matcher_data": 2,                  // monotonic data version
  "published":    "2026-10-17",
  "groups":       [ {name, domains}, … ],
  "confusables":  { map: {"а": "a", "1": "l", …}, folds: [["rn","m"], ["vv","w"], ["cl","d"]] },
  … unknown fields pass through …
}
```

- **Canonical** means RFC 8949 §4.2.1: shortest integer forms and map keys sorted by the bytewise order of their encodings. The signature covers the bytes as stored, so canonical form is for reproducibility (the same source signs to the same bytes; a test checks the committed file equals a fresh signing), not for security.
- **Domain separation.** The signed message is a fixed prefix plus the payload, so a signature over matcher data can never be presented as a signature over anything else that key signs.
- **The verify path** (`signed::verify`) decodes the envelope as exactly one CBOR item (trailing bytes refused), checks `format` and `v` by name, looks the `key_id` up in the build's `TrustedKeys`, verifies with `verify_strict`, and only then parses the payload. Refused: no envelope, a future format, an unknown key, a wrong key under a trusted id, an empty signature, any flipped bit (a test flips one bit at 97 positions across the file), an appended or dropped byte.
- **Unknown fields pass through.** `MatcherPayload.extra` keeps them as CBOR values; a re-signing of what came back is byte-identical. An older build carries a newer file's fields without dropping them.
- **Keys.** `TrustedKeys` is a map of id to Ed25519 public key. `TrustedKeys::builtin()` is what a build trusts for its embedded file; today that is only the development key `dev-2026-10`, whose seed and public half are checked in under `crates/matcher/data/dev/` with a README saying, loudly, that production keys live in the release signing service and that a release build must not trust a `dev-*` id. (D50 below.)
- **Version and downgrade.** `matcher_data` only goes up. `DataState` holds the data in force and the newest version it has loaded in this process; `load` refuses a lower one with `downgrade` and leaves the data in force unchanged; the same version loads again (a restart re-reading its file). A tampered file never moves the floor. (D51 below.)
- **Regenerating.** Edit `crates/matcher/data/matcher-data.json`, raise `matcher_data`, run `cargo run -p lockdown-matcher --example sign-data`. The example derives `publisher.pub` from the seed, signs, verifies what it wrote, and writes `matcher-data.signed.cbor`; `--check` exits 1 when the committed file is stale. The crate's tests fail on a stale file, a stale public key, or a built-in file that does not verify.
- **Source shape.** `matcher-data.json` is the editable source and an instance of `extension/schema/matcher-data.schema.json`. v2 dropped `githubusercontent.com` from the `github` group: the matching set showed it is a public suffix in the PSL (every user site under it is its own registrable domain), so it could never match by registrable domain, and nobody signs in there.

What the matcher does with the file: `MatcherData` compiles the groups into `EquivalentDomains` and the table into `Confusables`; `decide_with(saved, current, &data)` uses both. `decide(saved, current, &equiv)` keeps its old signature for the CLI and uses the built-in table. `EquivalentDomains::curated()` now comes from the verified built-in file, so even the no-host path exercises the signature check once per process.

## 3. The matching set

`06-roadmap.md` workstream 10 asks for a 500-site autofill set as a product in its own right. The matching half of it is built: `crates/matcher/tests/sites/gen.rs` **constructs** every row from a stated rule, never by calling the matcher, so the set is an independent statement of what the rules mean; `tests/sites.rs` regenerates it, compares it to the committed `sites.json` (one row per line, so a review reads one case per line and a diff shows exactly which rows changed), and runs every row through `decide_with` under the built-in data.

| Family | Rows | What it covers |
|---|---|---|
| `exact` | 420 | same host under `exact_host`: case, trailing dot, scheme, path, query, fragment, userinfo, a port on the page, a port on the saved address |
| `subdomain` | 364 | depth under `exact_host` (no match) and `base_domain` (match); multi-label suffixes (`co.uk`, `co.jp`, `com.ar`, `com.au`); sharing a suffix is not sharing a site |
| `starts_with` | 20 | the prefix rule, with and without the trailing slash, another host |
| `equivalent` | 128 | every curated group, first domain to every other and back, another host; `never` beating the group and the same host |
| `lookalike` | 508 | digit swaps (`0`, `1`), `rn` for `m`, hyphen decoys both sides, the trusted domain as a leading and a middle sub-domain, a Cyrillic letter (punycode on the wire), one-character typos on labels of six or more; `never` on a lookalike; short labels get no typo guess; a suspicious label under the real domain is the real site |
| `unrelated` | 60 | thirty pairs of real sites, both rules |
| `host-kinds` | 33 | IPv4 and IPv6 literals, `localhost`, IANA IDN test names and a Latin IDN label, extension origins (`chrome-extension://`, `moz-extension://`), unparsable addresses (`file:`, `data:`, `about:`, `javascript:`, `blob:`, `mailto:`, empty, text), an impossible port, the saved host as userinfo |
| related origins | 22 | hits, misses, the five-label cap, http versus https, ports, `www`, a squat on a listed origin, stale and edge-of-fresh documents, an empty document, id case |

1,533 match rows and 22 related-origins rows; all pass. Sites: 60 real, public, top-traffic registrable domains (`gen.rs` `SITES`), plus the group members the curated data names. Refresh after a generator change with `LOCKDOWN_WRITE_SITES=1 cargo test -p lockdown-matcher --test sites`.

Two things the set found, both fixed in the same commit: the `githubusercontent.com` entry above, and `Origin::parse` reading `mailto:someone@example.com` as host `example.com` with userinfo `mailto:someone` (text without `://` was always treated as a bare host). An opaque scheme with no authority is now an error, while a typed `example.com:8443/app` or `localhost:3000` still reads as a host with a port.

What the set is not yet: the field-detection half of workstream 10 (which inputs on a real page are the username and password), the per-browser matrix, and nightly runs against live sites. Those come with the extension.

## 4. The browser harness and how the host persists the data version

**Harness.** `extension/test/clickjacking.spec.ts` (Playwright 1.56.1, Chromium) serves the fixtures and the wasm package from one process reachable as two origins, `http://localhost:<port>` and `http://127.0.0.1:<port>`, so a page can embed a real cross-origin frame. Every fixture document loads `harness.js`, which is the content-script side of the gate: it measures the field, builds the fill request, and asks the wasm core loaded from its own origin. The expectations are copied from `matcher::tests::never_fill_cross_origin_iframes_without_confirmation`:

| Case | Page | Measured `field_visible` | Verdict |
|---|---|---|---|
| cross-origin frame | field in a frame from `127.0.0.1` inside a page from `localhost` | true | `confirm`, reason "… inside a page from localhost …"; `fill` once `user_confirmed` |
| same-origin frame (control) | field in a frame from the same origin | true | `fill` |
| opacity-0 overlay | an invisible layer with `pointer-events: auto` over the field | false: the hit test finds the overlay | `refuse`, reason "… clickjacking …", even when confirmed |
| pointer-events:none overlay | an opaque decoy over the field that lets clicks through (the DEF CON 33 trick) | false: the painted-overlap scan finds it, although `elementFromPoint` returns the field | `refuse` |
| plain top-level field | no frame, no overlay | true | `fill`; `confirm` for `code`, `card`, `passkey`; `fill` for `identity` |
| lookalike | `paypal.com.evil.net` against a saved `www.paypal.com`, driven in the page | — | `decide` says `lookalike`; `fill_check` refuses whatever else is true |

The visibility rule as measured (`harness.js` → `fieldVisibility`): a non-empty box, on screen, no ancestor `display:none`, `visibility:hidden` or opacity under 0.1, the element on top at five points (centre and four inset corners) is the field itself, and no painted `pointer-events:none` box that is neither ancestor nor descendant overlaps it. The last step exists because hit testing alone passes straight through the decoy. The frame learns the top-level origin from `location.ancestorOrigins`; a real extension gets it from the background page's `sender.tab`, which is why the request carries `top` as data rather than trusting the frame's own word.

**CI.** The `extension` job (`.github/workflows/ci.yml`) on `ubuntu-latest`: stable Rust with the `wasm32-unknown-unknown` target and `rust-cache`, `wasm-bindgen-cli@0.2.129` as a prebuilt binary (`taiki-e/install-action`), pnpm from the `packageManager` pin, Node 22 with the pnpm store cached, `pnpm install --frozen-lockfile`, `playwright install --with-deps chromium`, `pnpm run build:wasm`, `pnpm test`; traces are uploaded on failure. Locally: a warm wasm build and the six tests run in under ten seconds; a cold release build of the wasm is the long pole (47 s wall on a 4-CPU sandbox; `rust-cache` removes it from every run but the first), so the job sits well inside five minutes.

**How the host persists the data version.** The matcher has no file of its own to write to, by design (the same crate runs in a page). The contract with each host:

1. On start, the core holds the built-in file; `matcher_data_status().newest_loaded` equals its version.
2. The host reads the number it remembered last time and calls `set_matcher_data_floor(n)`. Never lower than the built-in version; `max` is applied either way.
3. When the host fetches a newer file, it calls `load_matcher_data(bytes)`. On success it persists `newest_loaded` **before** acting on the new data; on `downgrade` or `bad_signature` it keeps what it had and may report.
4. Where the number lives: the extension in `chrome.storage.local` (per profile, not synced, so a rolled-back profile notices nothing worse than a re-download); the desktop app beside `remembered.cbor` in Lockdown's data directory (D42: never inside the vault file); the shells in their app-private storage. It is one integer and it is not secret; what matters is that it is written forward-only and read before the first `load_matcher_data`.

## 5. Decisions proposed

For the coordinator to record in `decisions.md`. Each is a security-model detail under D38: the building session decides, one plain-language sentence, tagged audit item. Numbers follow D48 and may be renumbered on recording.

| ID | Date | Decision | Owner |
|---|---|---|---|
| D49 | 2026-10-17 | **Wire schema stability.** The matcher boundary (`lockdown_matcher::wire`, published as JSON Schema under `extension/schema/`) has one schema version `v` on every response. Within a `v`, fields are only added, never renamed, removed or retyped, and readers ignore unknown fields; a breaking change bumps `v` and the old shape stays readable for one release. The schema files are checked against the Rust structs by a test, so they cannot drift silently. Enums are lowercase `snake_case` words; numbers are JSON numbers; handles are `u32`. Plain version: the messages between the page and the core have a version number, and within a version we only ever add to them, so an older page and a newer core keep understanding each other. | Accepted by default under D38 (structure, not security) |
| D50 | 2026-10-17 | **Publisher keys and the dev key.** Signed matcher data is verified against a build's `TrustedKeys` (id → Ed25519 public key). The development pair lives in the repository (`crates/matcher/data/dev/`, id `dev-2026-10`) and signs the built-in file in development and CI; it is public and proves nothing. Production keys live in the release signing service only, never in this repository, its CI secrets or a laptop; the release build embeds their public halves under ids that are not `dev-*`, and a release build that trusts a `dev-*` id is a release blocker (the checklist greps for it). Rotation is additive: a new id joins `TrustedKeys`, files are re-signed, the old id is removed one release later. Plain version: the test key is in the open and only for testing; the real key lives where releases are signed, and the app shipped to people never trusts the test key. | Accepted by default under D38; audit item |
| D51 | 2026-10-17 | **Data version downgrade rule.** The payload carries `matcher_data`, a monotonic integer raised on every change and never reused. The matcher remembers the newest version it has loaded in-process (`DataState`) and refuses a lower one, leaving the data in force unchanged; the same version loads again; a file that fails verification never moves the floor. Persisting the floor across restarts is the host's job (§4): one forward-only integer in `chrome.storage.local`, beside `remembered.cbor`, or app-private storage, read and handed back through `set_matcher_data_floor` before the first load. Plain version: the list of "these sites share a login" can only ever move forward; an old copy of it cannot be slipped back in, and each app writes down the newest number it has seen. | Accepted by default under D38; audit item |
| D52 | 2026-10-17 | **What crosses the matcher boundary.** `decide`, `fill_check` and `related_origins` take addresses, rules and page measurements and return a decision or a verdict with plain-language text; nothing secret goes in or out of them. The one inbound secret crossing is `WasmSession.import_code_uri` (a one-time-code URI typed or scanned by the person), which returns a numeric handle; `code(handle, now)` returns the digits to fill and never the seed; `lock` wipes everything. Passwords, keys and seeds otherwise stay behind `SecretHandle` ids (03-architecture §3, `lockdown-ffi`). The page measures, the core decides: `field_visible`, `top` and `frame` are facts the extension reports, and the verdict is the core's. Plain version: the web page is only ever told "fill", "ask first" or "no, because …"; it never holds a password, and the only secret it hands in is a code seed, once, in exchange for a number. | Accepted by default under D38; audit item |
| D53 | 2026-10-17 | **The signed data format.** Groups and confusables ship as one CBOR envelope (`format`, `v`, `key_id`, `payload`, `signature`) around canonical CBOR (RFC 8949 §4.2.1) of the payload, signed with Ed25519 over a fixed domain-separation prefix plus the payload bytes, verified with `verify_strict`, decoded as exactly one CBOR item with no trailing bytes, format and version checked by name before the key is looked up, payload parsed only after the signature holds, unknown payload fields kept and re-emitted. Plain version: the data file is sealed with a signature that covers every byte, is labelled so it cannot be mistaken for anything else the key signs, and is not even opened until the seal checks out. | Accepted by default under D38; audit item |
| D54 | 2026-10-17 | **The visibility rule as measured.** "Visible" for `field_visible` means: a non-empty box on screen, no ancestor hidden or under 10% opacity, the element on top at the centre and four inset corners is the field itself, and no painted `pointer-events:none` box overlaps it. Hit testing alone is insufficient because a `pointer-events:none` decoy passes clicks through to the field under it; the overlap scan is what catches it. `Origin::parse` also now treats an opaque scheme with no authority (`mailto:`, `data:`, `about:`, `javascript:`) as "no host" rather than reading the text after the colon as a host. Plain version: before filling, the extension checks that the field is really the thing on top of the page at several points and that nothing see-through-to-clicks is painted over it. User-visible only as fewer fills on tricked pages. | Accepted by default under D38; audit item |

## 6. Open questions raised by the build

1. **Where the confusables table should grow.** The shipped table is the 34 characters the code carried plus three folds. The full Unicode confusables list is about 6,000 lines; shipping it whole in the signed file is cheap in bytes but the skeleton rule is per character, and `label_imitates` compares labels of the *saved* sites only, so the cost is bounded. Decide whether to generate the table from `confusables.txt` at data-build time (the obvious next step) or keep a curated subset.
2. **Lookalike edit-distance false positives** (`07-workspace.md` §5 q.3) stand. The set shows the rule is right on every generated typo and wrong on no real pair in the 30 it tries, which is not the same as wrong on none; a live run over the browser lab's logins will tell.
3. **`decide` versus `decide_with`.** The CLI still calls `decide(saved, current, &EquivalentDomains)` with the built-in table. Once the desktop app loads signed data it should call `decide_with`; the old signature can then go.
4. **Trust set per build.** `TrustedKeys::builtin()` returns the dev key today. The release pipeline needs a way to swap the body (a `cfg`, a build-time include of a public-keys file, or a feature); D50 fixes the policy, not the mechanism.
5. **Wasm size** is 1.7 MB release, dominated by the crypto stack the ffi facade pulls in even though the matcher boundary does not use it. The offscreen document loads it once, so it is a start-up cost, not a per-fill cost; the `-primitives` split (07-workspace §5 q.1) or a feature that leaves the crypto out of the matcher-only module would halve it.
6. **Firefox and Safari** are not in the harness. `location.ancestorOrigins` is Chromium and WebKit only; the Firefox path reads the top from the background page, which the real extension does anyway. The Playwright matrix widens with workstream 6.
