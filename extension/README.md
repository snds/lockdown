# extension/ — the Lockdown browser extension

The MV3 extension for Chromium (Chrome, Edge, Brave, Chromium) and Firefox, built from one source, plus the wasm build of the matcher boundary it stands on and the browser-driven tests. It talks to the **running, unlocked Lockdown desktop app** through a native-messaging host; it holds no vault. Design, channel, pairing, gaps and proposed decisions: [`docs/plan/16-browser-extension.md`](../docs/plan/16-browser-extension.md). The boundary underneath: [`docs/plan/09-extension-boundary.md`](../docs/plan/09-extension-boundary.md).

| Path | What |
|---|---|
| `manifest.base.json` | The manifest, with a `chromium` and a `firefox` block for what differs (background key, the Basic Auth permission, the fixed id). |
| `scripts/build.mjs` | esbuild bundles into `dist/chromium/` and `dist/firefox/`, copies `static/` and the wasm, writes each manifest. |
| `src/background/` | `index.ts` routes and checks every message's sender; `link.ts` is the native port to the app; `pairkey.ts` keeps the pairing key as a non-extractable WebCrypto key; `matcher.ts` loads the wasm and runs `decide` / `fill_check` on every candidate before the app decides again. |
| `src/content/` | The content script: field detection (`detect.ts`), the visibility rule (`visibility.ts`, the same rule as the harness), writing approved values (`fill.ts`), the menu host in a closed shadow root in the top layer, and save capture (`index.ts`). |
| `src/pages/` | `frame.ts`: the in-page menu and save prompt (an extension-origin frame, armed only while IntersectionObserver v2 says it is uncovered). `popup.ts`: pairing, the page's logins, search, copy through the app, the lookalike stop, the change-password link. |
| `static/` | `frame.html`, `popup.html`, `ui.css`. |
| `build-wasm.sh` | Builds `lockdown-wasm` for `wasm32-unknown-unknown` and runs the pinned `wasm-bindgen` CLI into `pkg/` (ES module, `--target web`). |
| `schema/` | JSON Schema for every message across the matcher boundary. `crates/matcher/tests/wire_schema.rs` keeps it in step with the Rust structs. |
| `test/clickjacking.spec.ts`, `test/fixtures/{harness.js,top.html,frame.html}` | The red team against the wasm build alone, unchanged. |
| `test/extension.spec.ts`, `test/fixtures/ext/`, `test/desktop.ts` | End to end: the unpacked build in Chromium, the real `lockdown-native-host`, the real desktop endpoint and core over a throwaway vault (`apps/desktop/src-tauri/examples/browser-endpoint.rs`), local fixture pages. |
| `test/manifest.spec.ts` | What differs between the two manifests. |
| `test/server.ts` | The static server for every fixture, reachable as any name the suite maps to it; `/basic/` answers only with the fixture's Basic Auth credentials. |

## Run it

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.129 --locked   # must equal the crate pin in crates/wasm/Cargo.toml
cd extension
pnpm install --frozen-lockfile
pnpm exec playwright install --with-deps chromium          # once per machine
pnpm run build:wasm      # pkg/
pnpm run build:native    # lockdown-native-host and the endpoint fixture
pnpm typecheck
pnpm build               # dist/chromium, dist/firefox
pnpm test
```

CI runs the same steps in the `extension` job of `.github/workflows/ci.yml`.

## Use it with the desktop app

1. Build and run the desktop app with `lockdown-native-host` beside its executable (or `LOCKDOWN_NATIVE_HOST=<path>`), then Settings → Browsers → Connect: the app writes the host manifest where each browser reads it.
2. Load `dist/chromium` as an unpacked extension (its id is fixed by the manifest's public key: `dbdblnolpiehhlabpmhobmkbjipmebpe`), or `dist/firefox/manifest.json` as a temporary add-on.
3. Open the toolbar popup, choose Pair with Lockdown, and approve in the app when both show the same code.

## What crosses where

Between the page and the extension: nothing from the vault but the one value being filled, after the app's `fill` verdict, into the field it was approved for. Between the extension and the app: page addresses, measured facts, item summaries (title, username, site; a crown jewel by title only), verdicts and their words, and that one value. Never a key, a seed, or a list of the vault. The matcher boundary (`decide`, `fill_check`) carries addresses only, as before.
