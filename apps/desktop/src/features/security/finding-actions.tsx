import { useState } from "react"

import { Button, Icon } from "@unicron/ui"

import { describeError, useClient } from "@/core/client"
import type { Finding } from "@/core/types"

interface FixProps {
  finding: Finding
  onNotice: (text: string) => void
  /** For a new device's sign-in: go to the Devices screen. */
  onDevices?: () => void
}

/**
 * The one thing that fixes a finding, where there is one: open the site's
 * change-password page (the shell opens it; the webview launches
 * nothing), open the site's two-step set-up page, or go to Devices.
 */
export function FindingFix({ finding, onNotice, onDevices }: FixProps) {
  const client = useClient()
  const [busy, setBusy] = useState(false)

  if (finding.change_password && finding.item_id) {
    const itemId = finding.item_id
    const change = async () => {
      setBusy(true)
      try {
        const link = await client.securityChangePassword(itemId)
        onNotice(
          link.kind === "well_known"
            ? "Opened the site's change-password page in your browser."
            : "This site has no change-password page Lockdown could find, so its front page opened. Look for account or security settings.",
        )
      } catch (e) {
        onNotice(describeError(e))
      } finally {
        setBusy(false)
      }
    }
    return (
      <Button size="sm" variant="outline" onClick={() => void change()} disabled={busy}>
        <Icon name="open_in_new" data-icon="inline-start" />
        Change password
      </Button>
    )
  }
  if (finding.setup_url) {
    const url = finding.setup_url
    return (
      <Button size="sm" variant="outline" onClick={() => void client.openSite(url).catch((e) => onNotice(describeError(e)))}>
        <Icon name="open_in_new" data-icon="inline-start" />
        Turn it on
      </Button>
    )
  }
  if (finding.kind === "new_sign_in" && onDevices) {
    return (
      <Button size="sm" variant="outline" onClick={onDevices}>
        <Icon name="devices" data-icon="inline-start" />
        Go to Devices
      </Button>
    )
  }
  return null
}
