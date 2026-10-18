import type { MirrorCheck } from "./server-client"

/** What a mirror's last word means, in a short line. */
export function mirrorLine(c: MirrorCheck): string {
  switch (c.state) {
    case "agrees":
      return `Agrees with this server's log (its copy has ${c.size} ${c.size === 1 ? "entry" : "entries"}).`
    case "split":
      return c.message
    case "unusable":
      return `Can't vouch for this server's log: ${c.message}.`
    case "unreachable":
      return `Not checked: ${c.message}.`
  }
}

/** The mirrors that caught a split view, for a warning anywhere. */
export function splits(checks: readonly MirrorCheck[] | undefined | null): MirrorCheck[] {
  return (checks ?? []).filter((c) => c.state === "split")
}
