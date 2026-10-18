/** "lockdown/0.1.0 (macos)" → "Lockdown 0.1.0 on macOS". */
export function clientWords(client: string | null): string | null {
  if (!client) return null
  const m = /^lockdown\/(\S+)(?: \((\w+)\))?/.exec(client)
  if (!m) return client
  const os: Record<string, string> = { macos: "macOS", windows: "Windows", linux: "Linux", ios: "iOS", android: "Android" }
  const where = m[2] ? ` on ${os[m[2]] ?? m[2]}` : ""
  return `Lockdown ${m[1]}${where}`
}
