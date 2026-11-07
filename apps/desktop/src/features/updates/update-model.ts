import type { UpdateStatus } from "./update-client"

/** The one status line under "Check for updates" (docs/plan/60-auto-update.md §6). */
export function updateLine(s: UpdateStatus): string {
  if (!s.enabled) return s.off ?? "Updates are off in this build."
  switch (s.state) {
    case "idle":
      return s.auto_check ? "Lockdown checks for updates when it opens and every six hours." : "Automatic checks are off. Check whenever you like."
    case "checking":
      return "Checking for updates…"
    case "up_to_date":
      return "Lockdown is up to date."
    case "downloading":
      return `Downloading version ${s.offered ?? "the update"}… ${s.percent ?? 0}%`
    case "ready":
      return `Version ${s.offered ?? "the update"} is ready. Restart to update, or it installs when you quit.`
    case "available":
      return `Version ${s.offered ?? "a newer build"} is out. ${s.link_why ?? ""}`.trim()
    case "failed":
      return s.error ?? "The update didn't work."
  }
}

/** The channel as Settings names it. */
export function channelName(s: UpdateStatus): string {
  return s.channel === "main" ? "main channel (every merged change)" : "stable channel"
}

/** Whether "Check for updates" can be pressed now. */
export function canCheck(s: UpdateStatus): boolean {
  return s.enabled && s.state !== "checking" && s.state !== "downloading"
}
