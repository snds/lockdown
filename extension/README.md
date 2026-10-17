# extension/ — the matcher boundary and its browser harness

This directory is **not the extension**. It is the slice the extension will stand on: the wasm build of the matcher boundary, the JSON schema it speaks, and a browser-driven red team that proves the clickjacking rules hold in a real Chromium, not only in Rust. The MV3 extension itself (06-roadmap workstream 6) starts from here.

| Path | What |
|---|---|
| `build-wasm.sh` | Builds `lockdown-wasm` for `wasm32-unknown-unknown` and runs the pinned `wasm-bindgen` CLI into `pkg/` (ES module, `--target web`). |
| `schema/` | JSON Schema for every message across the boundary. `crates/matcher/tests/wire_schema.rs` keeps it in step with the Rust structs. |
| `test/fixtures/harness.js` | The content-script side of the fill gate: measures a field's visibility, builds the fill request, asks the wasm core. |
| `test/fixtures/top.html`, `frame.html` | The pages: a cross-origin frame, a same-origin frame, an opacity-0 overlay, a pointer-events:none overlay, a plain control. |
| `test/server.ts` | A static server for the fixtures and `pkg/`, reachable as `localhost` and `127.0.0.1` so one process is both the page and a cross-origin frame. |
| `test/clickjacking.spec.ts` | The Playwright test. Expectations are copied from `matcher::tests::never_fill_cross_origin_iframes_without_confirmation`. |
| `package.json`, `pnpm-lock.yaml` | Playwright 1.56.1 pinned; install with `pnpm install --frozen-lockfile`. |

## Run it

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.129 --locked   # must equal the crate pin in crates/wasm/Cargo.toml
cd extension
pnpm install --frozen-lockfile
pnpm exec playwright install --with-deps chromium          # once per machine
pnpm run build:wasm
pnpm test
```

CI runs the same steps in the `extension` job of `.github/workflows/ci.yml`.

## What the harness proves

| Case | What the page does | `field_visible` | Verdict | Same as Rust |
|---|---|---|---|---|
| cross-origin frame | the field is in a frame from another origin | true | `confirm` ("inside a page from …"); `fill` once confirmed | yes |
| same-origin frame | the field is in a frame from the same origin | true | `fill` | yes |
| opacity-0 overlay | an invisible, click-catching layer sits over the field | false (hit test finds the overlay) | `refuse` ("… clickjacking …"), even when confirmed | yes |
| pointer-events:none overlay | an opaque decoy sits over the field and lets clicks through | false (painted overlap scan finds it) | `refuse` | yes |
| control | a plain top-level field | true | `fill`; `confirm` for code, card and passkey | yes |
| lookalike | `paypal.com.evil.net` against a saved `www.paypal.com` | — | `refuse` whatever else is true | yes |

The visibility rule (`harness.js` → `fieldVisibility`) is: non-empty box, on screen, no ancestor hidden or see-through, the element on top at five points is the field itself, and no painted `pointer-events:none` box overlaps it. The last step exists because hit testing alone passes straight through the DEF CON 33 decoy.

## What crosses the boundary

Addresses, rules, verdicts and their plain-language reasons, data-file versions, and handle numbers. Never a key, a password or a code seed; `WasmSession.import_code_uri` is the one inbound crossing and returns a handle. Details and the decisions proposed: `docs/plan/09-extension-boundary.md`.
