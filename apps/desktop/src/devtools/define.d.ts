/**
 * Whether this build carries the developer tools (docs/plan/40-devtools.md):
 * set by vite.config.ts from LOCKDOWN_DEVTOOLS, false in a tagged release,
 * so every `import("@/devtools/…")` behind it is dropped from that bundle.
 */
declare const __LOCKDOWN_DEVTOOLS__: boolean
