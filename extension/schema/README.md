# Matcher boundary schema (v1)

The JSON shapes `lockdown-wasm` takes and returns, as JSON Schema (draft 2020-12). The Rust source of truth is `crates/matcher/src/wire.rs`; `crates/matcher/tests/wire_schema.rs` checks that every field a response carries is declared here and every `required` field is present, so the two cannot drift silently.

| File | Call | Direction |
|---|---|---|
| `decide-request.schema.json` | `decide(request)` | page → core |
| `decide-response.schema.json` | `decide(request)` | core → page |
| `fill-request.schema.json` | `fill_check(request)` | page → core |
| `fill-response.schema.json` | `fill_check(request)` | core → page |
| `related-origins-request.schema.json` | `related_origins(request)` | page → core |
| `related-origins-response.schema.json` | `related_origins(request)` | core → page |
| `code-response.schema.json` | `WasmSession.code(handle, now_secs)` | core → page |
| `wire-error.schema.json` | any call, as the thrown `Error`'s message (JSON text) | core → page |

Stability rule: within one `v`, fields are only added, never renamed, removed or retyped; readers ignore fields they do not know. A breaking change bumps `v` and the old shape stays readable for one release. See `docs/plan/09-extension-boundary.md`.
