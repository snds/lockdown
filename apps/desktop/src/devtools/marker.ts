/**
 * A string only a build with the developer tools carries; the release
 * workflow fails a tag build whose files contain it (docs/plan/40-devtools.md
 * §5). The Rust side has the same one, `lockdown_devtools::MARKER`.
 */
export const MARKER = "lockdown-devtools-6f1c2a"
