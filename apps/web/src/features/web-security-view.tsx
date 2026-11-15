import { SecurityView } from "@/features/security/security-view"

/**
 * The web vault's Health (brief H1, W1): what the web client can do. No
 * hero, no metrics, no Fix groups (the password checks are the desktop's,
 * and an info Banner says so); the Needs attention and Expiring lists, and
 * PROTECTIONS with Devices, the quantum-safe line (each device it waits for
 * a link to Devices) and Duplicates, which the web client finds and merges.
 */
export function WebSecurityView({ onDevice }: { onDevice: (id: string) => void }) {
  return <SecurityView surface="web" onDevices={(id) => (id ? onDevice(id) : onDevice(""))} />
}
