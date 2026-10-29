import { PaneHeader } from "@/components/master-detail"
import { PqStatusLine } from "@/features/security/pq-status-line"

/**
 * The web vault's Security screen: the desktop's post-quantum status line
 * (docs/plan/30-pq-status-line.md), each device it waits for a link to
 * Devices. The desktop's password checks aren't on the web yet.
 */
export function WebSecurityView({ onDevice }: { onDevice: (id: string) => void }) {
  return (
    <div className="mx-auto flex w-full max-w-3xl flex-col gap-4 p-4 md:p-6">
      <PaneHeader title="Security" />
      <PqStatusLine onDevice={onDevice} />
      <p className="text-sm text-muted-foreground">Checks for weak, reused and breached passwords are in the desktop app.</p>
    </div>
  )
}
