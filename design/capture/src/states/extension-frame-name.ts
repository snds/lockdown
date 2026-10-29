/**
 * An in-page frame's iframe is named `ld-extframe:<mode>:<scenario>:<scheme>` by
 * its test page; the scheme is the one the review build chose (light or dark).
 */
export function extensionFrameName(name: string): { mode: string; scenario: string; scheme: "light" | "dark" } | null {
  const m = /^ld-extframe:([a-z]+):([a-z0-9-]+):(light|dark)$/.exec(name)
  return m ? { mode: m[1]!, scenario: m[2]!, scheme: m[3] as "light" | "dark" } : null
}
