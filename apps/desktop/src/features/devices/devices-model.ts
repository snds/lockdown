import type { DeviceInfo } from "@/core/types"
import type { SessionView } from "@/features/sessions/live-client"

const PLATFORMS: Record<string, string> = {
  macos: "macOS",
  ios: "iOS",
  ipados: "iPadOS",
  windows: "Windows",
  linux: "Linux",
  android: "Android",
}

/** A platform as people write it: "macos" → "macOS" (brief T2). Anything else as it came. */
export function platformName(platform: string): string {
  return PLATFORMS[platform.toLowerCase()] ?? platform
}

/** When a device was last used: its newest session's last use, or null when it has none live. */
export function lastUse(device: DeviceInfo, sessions: SessionView[] | null | undefined): number | null {
  let last: number | null = null
  for (const s of sessions ?? []) if (s.device_id === device.id && (last === null || s.last_seen > last)) last = s.last_seen
  return last
}
