# 57: The web vault's plain join, fenced to the web vault

_Slice: `slice/fence-web-join`, built 2026-11-05. Sean's go on D644 (2026-11-04). Closes doc 56 §5 gap 1: the web vault's device-link join, `App::join_start` and `App::join_finish_signed_in`, writes the plain key file that only a web vault tab seals, and in a native build both functions still compiled and were public, though no native shell called them. Builds on D62 (wire shapes only grow), D530–D536 (doc 45, the desktop's sealed join), D640–D645 (doc 56, the `test-support` feature and its checks). Proposed decisions D650–D655 (§8)._

## For future agent

- **TL;DR:** The core's plain device-link join is compiled only for the web vault's target (`target_os = "wasi"`) and with lockdown-app's `test-support` feature. That covers `App::join_start`, `App::join_finish_signed_in`, the core's `LinkKeys::Plain` (the plain branch of `Joining`), `finish_joined`, and the three `Joining` accessors only they read. The web crate's own `WebVault::join_start`, `WebVault::join_finish` and the `web_join_start` / `web_join_finish` commands follow the same fence through a new lockdown-web `test-support` feature. That feature only passes the core's on, and only lockdown-web's own `[dev-dependencies]` turn it on. A native release of lockdown-app has only the sealed device-link join (`join_start_sealed`, `join_finish`). `packaging/plain-join/check.sh` proves it by reading the library's symbols, and CI runs it in `desktop-bare`.
- **Read with:** `crates/app/src/devices.rs` (`join_start`, `join_start_sealed`, `join_start_as`, `join_finish_signed_in`), `crates/app/src/session/devices_link.rs` (`LinkKeys`, `Joining::start`, `Joining::poll`, `finish_joined`), `crates/web/src/security.rs` (`WebVault::join_start`, `join_finish`), `crates/web/src/commands/depth.rs`, `crates/web/Cargo.toml`, `crates/app/tests/no_plain_joins.rs` (`only_dev_dependencies_turn_on_the_fixture`), `packaging/plain-join/check.sh`, `apps/web/e2e/device-link.spec.ts`, and the `desktop-bare` job in `.github/workflows/ci.yml`. Also doc 56 §2 and §5, doc 45 §3, doc 17.
- **Do not:**
  - Drop the fence to call `App::join_start` from a native shell. The desktop joins with `join_start_sealed`, then `join_finish`. A native caller of the plain join would write an unprotected key file.
  - Turn on lockdown-web's `test-support` outside `[dev-dependencies]`, or name it from another feature. It turns on the core's.
  - Change the fence to `target_arch = "wasm32"` without also changing the crate's other wasm cfgs. The crate spells "the web vault's build" as `target_os = "wasi"` everywhere (§1).
  - Give the web vault's join the password seal (doc 45 §3). Its tab seals the key file under the sign-in's export key.

## 1. The cfg, and why it is right

Every fenced item carries:

```rust
#[cfg(any(target_os = "wasi", feature = "test-support"))]
```

- **`target_os = "wasi"`** is the crate's existing spelling of "the web vault's build". lockdown-app already uses it in `lib.rs`, `depth.rs`, `confine.rs`, `bridges/clock.rs` and `session/depth.rs`, and lockdown-web uses it in `host.rs` and `lib.rs`. The web vault builds only for `wasm32-wasip1` (`apps/web/build-wasm.sh`). On that target `target_os = "wasi"` and `target_arch = "wasm32"` are both true, so either would compile the join there. `wasi` is the narrower one: a plain `wasm32-unknown-unknown` build (none exists today) wouldn't get the join by accident. No native target has `target_os = "wasi"`.
- **`feature = "test-support"`** is doc 56's feature. It is off by default, named by no other lockdown-app feature, and turned on only by `[dev-dependencies]`. Under the workspace's resolver, a dev-dependency's features reach only builds that build tests or examples.
- **No `test` in the list.** No unit test inside lockdown-app runs the plain device-link join, and lockdown-app's own dev-dependency turns on `test-support` for its test builds anyway.
- **The fence is on the target and a feature, never the profile.** So the dev profile's library has exactly the items a release has. That is why a dev build is a fair stand-in for a release in the symbol check (§3).

## 2. The pieces

| Piece | Where | What it does |
|---|---|---|
| `LinkKeys` | `crates/app/src/session/devices_link.rs` | Replaces `Joining`'s `sealed: bool`. `Sealed` keeps the keys in memory until `finish_sealed`. `Plain` is behind the fence and writes the plain key file at the start, then restores the identity the moment the Welcome checks out. Without the fence, `Joining::start` and `Joining::poll` have only the sealed arm. The plain code is not just unreachable in a native release: it is not there at all. |
| `finish_joined`, `Joining::server`, `Joining::secret_key`, `Joining::keys` | same | The plain finish and the three accessors only `join_finish_signed_in` reads, behind the fence. `finish_sealed` still refuses a plain join in the builds that have one. |
| `App::join_start`, `App::join_finish_signed_in` | `crates/app/src/devices.rs` | Behind the fence. `join_start_as` takes `LinkKeys`, and `join_start_sealed` passes `Sealed`. The confined-home refusal still applies only to the sealed join. Unchanged inside. Doc links to the fenced calls from items every build documents are plain text, so a native `cargo doc` has no broken links. |
| lockdown-web's `test-support` | `crates/web/Cargo.toml` | `test-support = ["lockdown-app/test-support"]`. It is turned on by the crate's own dev-dependency, `lockdown-web = { path = ".", features = ["test-support"] }`, the same self-reference lockdown-app uses. |
| The web vault's join calls | `crates/web/src/security.rs`, `crates/web/src/commands/depth.rs` | `WebVault::join_start`, `WebVault::join_finish`, `origin_for_join`, the imports only they use, and the `web_join_start` and `web_join_finish` arms of the dispatcher, behind the same fence. In the wasm build they are as before. A native lockdown-web without the feature answers those two names as unknown commands. `web_join_poll` and `web_join_cancel` stay in every build: they call `App::join_poll` and `App::join_cancel`, which the sealed join uses too and which write nothing. |
| The manifest scan | `crates/app/tests/no_plain_joins.rs`, `only_dev_dependencies_turn_on_the_fixture` | Now covers every crate's `test-support`, not only lockdown-app's. In every member manifest, a `[features]` line that mentions `test-support` must define a feature of that name. The test asserts the exact set: `crates/app` (`= []`) and `crates/web` (`= ["lockdown-app/test-support"]`). Every other line that mentions it must sit under a dev-dependencies section. The test asserts that exact set too: `bins/cli`, `crates/app`, `crates/web`. The old test only looked at lines starting with `lockdown-app`. A forwarding feature such as lockdown-web's would have slipped past it. |
| The symbol check | `packaging/plain-join/check.sh` | Builds lockdown-app's library natively twice and reads the functions each one defines (`nm -C --defined-only`, either symbol mangling). With `test-support` (the dry run), `App::join_start` and `App::join_finish_signed_in` must be there, which proves the check sees them. As a release resolves it (default features, no dev-dependencies), both must be missing and `App::join_start_sealed` must be there, which proves the library was read. Each rlib is copied out before the next build, because cargo writes both to the same path. Needs `jq` and `nm`. |
| CI | `desktop-bare` in `.github/workflows/ci.yml`, new step "A native lockdown-app has no plain device-link join" | Runs the script after doc 56's `cargo tree` step. That step already proves the desktop (`--features shell`), the server and the CLI resolve lockdown-app without `test-support`. |
| The e2e spec | `apps/web/e2e/device-link.spec.ts` | New. Before this slice, no browser test drove the device-link join end to end. A tab signed in with the kit opens Devices, chooses "Add a device" and reads the code. A second, empty tab types it under "With a device". Both show the same six digits. The first approves behind its password gate. The second finishes with the login name and the master password, no kit typed, and opens the vault the CLI filled. This proves the wasm build still has the plain join and that it joins as before. |

## 3. How the checks prove no native release has the plain join

1. **The source.** Every plain-join item is behind `any(target_os = "wasi", feature = "test-support")`. No native target is `wasi`, so a native build has the join only with `test-support` on.
2. **The manifests.** The scan shows only `[dev-dependencies]` turn on either crate's `test-support`. It shows no other feature names it. It shows only lockdown-web's feature passes it on.
3. **The resolved trees.** Doc 56's `desktop-bare` step runs `cargo tree -e features` without dev-dependencies. It shows `lockdown-app feature "test-support"` is absent for the desktop (`--features shell`), the server and the CLI. A dry run through the CLI's dev-dependencies sees it, which proves the step looks. None of the three depends on lockdown-web.
4. **The library itself.** `packaging/plain-join/check.sh` reads the compiled library's symbols: the join is present with the feature and absent without it, and the sealed join is present as a control. I checked the check by hand once by widening the cfg to every native build: it failed, naming both functions.

Together: a release build of the desktop, the server or the CLI compiles lockdown-app without `test-support` for a native target, and that library has no plain device-link join. The web vault's wasm build has it (`build:wasm`, the e2e spec).

## 4. Who changed

| Caller | Before | Now |
|---|---|---|
| `crates/web/src/security.rs` (`WebVault::join_start`, `join_finish`) | Called `App::join_start` and `App::join_finish_signed_in` in every build. | Fenced the same way. The wasm build is unchanged. Native builds have them only with lockdown-web's `test-support`. |
| `crates/web/tests/security_depth.rs` (`web_join_start`, through the dispatcher) | Ran the plain join natively. | Unchanged. lockdown-web's dev-dependency turns `test-support` on for it. |
| `apps/desktop/src-tauri/src/security_depth.rs` (`join_start`, `join_finish`) | `join_start_sealed`, `join_finish` (doc 45). | Unchanged. It never called the plain join. |
| `crates/app/tests/device_approval.rs`, `session_renewal.rs` | `join_start_sealed`. | Unchanged. |
| `bins/*`, `crates/devtools` | No caller. | Unchanged. |

No native caller needed to move to the sealed join: the only native callers were the web crate's own, which its native tests run through the fixture feature.

## 5. How to run it and test it

```sh
export CARGO_TARGET_DIR=… CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=line-tables-only
packaging/plain-join/check.sh                        # the symbol check (jq, nm)
cargo test -p lockdown-app --test no_plain_joins     # the manifest scan, doc 56's refusals
cargo test -p lockdown-web                           # the native web tests, the join through test-support
cargo clippy -p lockdown-web --target wasm32-wasip1 -- -D warnings
cargo clippy -p lockdown-web --lib -- -D warnings    # a native web build without the feature
cd apps/web && pnpm build:wasm && pnpm build && pnpm e2e   # device-link.spec.ts among the rest
```

## 6. Gaps

1. **A workspace build with tests compiles the join into every binary of that build.** This is doc 56 §5 gap 3, unchanged: `cargo test --workspace` unifies dev-dependency features. A release never builds tests.
2. **The symbol check reads a dev build.** The fence doesn't depend on the profile (§1), so the items are the same. But the script never builds a release, which keeps CI time and the disk small. A tagged release workflow could run the same script on its own artifacts with `--release` passed through. Not done.
3. **`open_or_create_plain` still has no compile-time fence** (doc 56 §5 gap 2, doc 45 §5 gap 5). It makes a local install, not a join, and the server's restore drill needs it. Unchanged.
4. **The script reads symbols with GNU `nm`.** That matches the Linux `desktop-bare` runner. On macOS, `nm -C` may not demangle Rust's v0 names. Running it there would need `rustfilt` or LLVM's `nm`. Not needed for CI.

## 7. unicron requests (host-contract gaps)

None. No screen, component, token or client interface changed. The e2e spec drives existing screens.

## 8. Audit notes

- **No new path releases a secret.** The slice only removes code from native builds. The wasm join is the same code as before. The `LinkKeys` enum replaces a bool with the same two meanings.
- **The wire shapes didn't change** (D62). The wasm module answers the same command names with the same shapes. The desktop's IPC is untouched. A native lockdown-web without the feature isn't shipped by anything.
- **Nothing new is logged.** The e2e spec types the test account's password into the page, as the other specs do.

## 9. Test counts

| Suite | Before | After |
|---|---|---|
| `lockdown-app` (all targets) | 249 | 249 (`only_dev_dependencies_turn_on_the_fixture` widened) |
| `lockdown-web` (native) | 24 | 24 |
| `apps/web` e2e (Chromium) | 12 | 13 (`device-link.spec.ts`) |

## 10. Proposed decisions

For the coordinator to record. **Flagged** rows are user-visible and need Sean. "Audit item" rows go to the external audit under D38.

| ID | Decision | Owner |
|---|---|---|
| D650 | **The web vault's plain device-link join is compiled only for the web vault and for tests** (builds D644 on Sean's go). `App::join_start`, `App::join_finish_signed_in`, the core's plain `LinkKeys` arm and `finish_joined` carry `#[cfg(any(target_os = "wasi", feature = "test-support"))]`, the crate's existing wasm spelling plus doc 56's feature. A native release has only the sealed device-link join. Plain version: no Lockdown you install on a computer contains the browser's way of joining, which leaves the key unprotected outside a browser tab. | Under D38; closes doc 56 gap 1; audit item |
| D651 | **lockdown-web gets a `test-support` feature that only passes the core's on**, turned on by its own `[dev-dependencies]` (a self-reference, as lockdown-app's). The web crate's `join_start`, `join_finish`, `web_join_start` and `web_join_finish` follow the same fence; `web_join_poll` and `web_join_cancel` stay, since the sealed join shares them. Plain version: the web vault's own tests can still run its join outside a browser. | Under D38 |
| D652 | **The manifest scan covers every crate's `test-support`**: only a feature of that name may mention it (exactly lockdown-app's and lockdown-web's), and every other mention is under dev-dependencies (exactly `bins/cli`, `crates/app`, `crates/web`). Plain version: a change that would ship test-only code fails the tests. | Under D38; audit item |
| D653 | **A symbol check proves a native lockdown-app has no plain device-link join**: `packaging/plain-join/check.sh` builds the library with and without `test-support`, requires the join in the first and its absence (with the sealed join present) in the second; CI's `desktop-bare` runs it. Plain version: the build itself is checked, not only its settings. | Under D38; audit item |
| D654 | **An e2e spec drives the web vault's device-link join end to end** (`apps/web/e2e/device-link.spec.ts`): a signed-in tab adds a device, an empty tab joins with the code, the digits match, the first approves behind its password, the second finishes with the password alone. Plain version: joining the web vault from another device is now tested in a real browser. | Accepted by default under D38 |
| D655 | **No wire shape changed** (D62): the wasm module, the desktop's IPC and the extension keep every command and shape; only a native lockdown-web built without its test feature, which nothing ships, lacks two command names. Plain version: older and newer parts keep understanding each other. | Accepted by default under D38 |
