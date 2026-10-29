import type { MenuAction } from "./devtools-client"

/**
 * The Developer menu as the shell builds it (crates/devtools/src/menu.rs),
 * in order; a Rust test keeps the two equal. The menu itself is the OS's;
 * this list is what the review harness draws in its place.
 */
export const DEVELOPER_MENU: { label: string; action: MenuAction }[] = [
  { label: "Open the demo", action: "demo_open" },
  { label: "Reset demo data", action: "demo_reset" },
  { label: "Load a demo pack…", action: "demo_load" },
  { label: "Reset Lockdown…", action: "reset" },
]
