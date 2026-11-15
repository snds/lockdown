import type { SettingsSection, Surface } from "@/app/places"

/**
 * What "Find a setting" finds (brief T0): the settings in each section, by
 * the names their rows show. A section is also found by its own name and
 * line. Each surface lists only the sections it has.
 */
export const SETTINGS_INDEX: Record<SettingsSection, readonly string[]> = {
  "this-device": [
    "Auto-lock",
    "Lock when the window is out of view",
    "Touch ID",
    "Windows Hello",
    "Approvals",
    "Clipboard",
    "Reveal",
    "Your name",
    "Offer Lockdown's passkeys in browsers",
    "Recovery kit",
    "Other sessions",
  ],
  browsers: ["Extension", "Browsers on this computer", "Connect", "Paired browsers"],
  "quick-access": ["Open quick access from anywhere", "Shortcut", "Out of swap"],
  appearance: ["Theme", "Density"],
  devices: ["Devices", "Add a device", "Revoke", "Sessions", "Security keys"],
  recovery: ["Recovery kit", "Drill", "Emergency sheet", "Master password reminder"],
  status: ["Status", "Check now", "Account", "Sync"],
  server: ["Server", "Restore drills", "Log mirrors", "Move the vault"],
  "import-export": ["Import", "Export"],
  backups: ["Backups", "Restore", "Crown jewels in backups"],
  aliases: ["Alias services", "Add service", "Watch my aliases for data breaches"],
  codes: ["Correct for a wrong clock", "One-time codes"],
  "security-checks": ["Check my passwords against known data breaches", "Breach list", "Find each site's change-password page before opening it"],
  about: ["About", "Version", "Check for updates", "Check for updates automatically", "Restart to update", "Reload app"],
}

/** This browser's own rows in the web vault (brief T1, W1). */
const WEB_THIS_DEVICE: readonly string[] = ["Auto-lock", "Lock when this tab is out of view", "Clipboard", "Reveal", "Signed in as", "Sign out of this tab"]

export interface Found {
  section: SettingsSection
  /** The setting's own name, or the section's when the section itself matched. */
  name: string
}

/** The settings and sections with a word starting with each word typed, in list order. */
export function findSettings(surface: Surface, query: string, sections: ReadonlyArray<{ section: SettingsSection; label: string; description?: string }>): Found[] {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean)
  if (words.length === 0) return []
  // A word matches the start of a word, and the app's own name is in too many lines to tell them apart.
  const hit = (text: string) => {
    const own = text.toLowerCase().replace(/lockdown('s)?/g, " ")
    return words.every((w) => own.split(/[^a-z0-9]+/).some((part) => part.startsWith(w.replace(/[^a-z0-9]/g, ""))))
  }
  const out: Found[] = []
  for (const s of sections) {
    if (hit(`${s.label} ${s.description ?? ""}`)) out.push({ section: s.section, name: s.label })
    const names = surface === "web" && s.section === "this-device" ? WEB_THIS_DEVICE : SETTINGS_INDEX[s.section]
    for (const name of names) if (name !== s.label && hit(name)) out.push({ section: s.section, name })
  }
  return out
}
