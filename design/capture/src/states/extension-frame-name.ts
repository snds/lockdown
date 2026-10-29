/** An in-page frame's iframe is named `ld-extframe:<mode>:<scenario>` by its test page. */
export function extensionFrameName(name: string): { mode: string; scenario: string } | null {
  const m = /^ld-extframe:([a-z]+):([a-z0-9-]+)$/.exec(name)
  return m ? { mode: m[1]!, scenario: m[2]! } : null
}
