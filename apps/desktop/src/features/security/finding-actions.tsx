import { useCallback, useMemo, useState, type ReactElement } from "react"

import { AdaptiveMenu, AdaptiveMenuItem, AdaptiveMenuSeparator, Button, FloatButton, Icon, type IconName } from "@unicron/ui"

import { describeError, useClient } from "@/core/client"
import type { ChangeLink, Finding } from "@/core/types"

import { SNOOZES, hostOf, snoozeUntil } from "./security-model"

/** The one action that fixes a finding (brief H2, "The actions by kind"). */
export interface Fix {
  label: string
  icon: IconName
  run: () => void
  busy: boolean
}

/** What the toast says once the change-password page opened, in today's three wordings (D129). */
export function changeLinkWords(link: ChangeLink): string {
  return link.kind === "well_known"
    ? "Opened the site's change-password page in your browser."
    : link.kind === "recipe"
      ? `Opened the site's password page in your browser.${link.note ? ` ${link.note}` : ""}`
      : "This site has no change-password page Lockdown could find, so its front page opened. Look for account or security settings."
}

interface FixOptions {
  onNotice: (text: string) => void
  /** For a new device's sign-in: Settings → Devices. */
  onDevices?: () => void
  /** For a look-alike: open the login, so its address can be checked. */
  onOpenItem?: (id: string) => void
}

/**
 * The fix for a finding, by kind: OPEN {SITE} for a weak, reused or breached
 * password (the shell opens the change-password page; the webview launches
 * nothing), TURN IT ON for a missing second step (the site's set-up page),
 * OPEN ITEM for a look-alike, GO TO DEVICES for a new sign-in, and OPEN
 * {SITE} for an exposed email when the site has a page to change it. `null`
 * when there is none.
 */
export function useFix(finding: Finding | null, { onNotice, onDevices, onOpenItem }: FixOptions): Fix | null {
  const client = useClient()
  const [busy, setBusy] = useState(false)
  const change = useCallback(
    async (itemId: string) => {
      setBusy(true)
      try {
        onNotice(changeLinkWords(await client.securityChangePassword(itemId)))
      } catch (e) {
        onNotice(describeError(e))
      } finally {
        setBusy(false)
      }
    },
    [client, onNotice],
  )
  const open = useCallback((url: string) => void client.openSite(url).catch((e) => onNotice(describeError(e))), [client, onNotice])
  return useMemo(() => {
    if (!finding) return null
    const f = finding
    const site = hostOf(f.site)
    const openSite = site ? `Open ${site}` : "Open site"
    switch (f.kind) {
      case "weak":
      case "reused":
      case "breached":
        return f.change_password && f.item_id ? { label: openSite, icon: "launch", run: () => void change(f.item_id!), busy } : null
      case "missing_2fa":
        return f.setup_url ? { label: "Turn it on", icon: "launch", run: () => open(f.setup_url!), busy: false } : null
      case "lookalike":
        return f.item_id && onOpenItem ? { label: "Open item", icon: "arrow--right", run: () => onOpenItem(f.item_id!), busy: false } : null
      case "new_sign_in":
        return onDevices ? { label: "Go to devices", icon: "devices", run: onDevices, busy: false } : null
      case "exposed_email":
        if (!f.change_password) return null
        if (f.item_id) return { label: openSite, icon: "launch", run: () => void change(f.item_id!), busy }
        return f.site ? { label: openSite, icon: "launch", run: () => open(f.site!), busy: false } : null
    }
  }, [finding, change, open, busy, onDevices, onOpenItem])
}

/** The fix as the detail's one solid button (as Desktop / 08 draws it). */
export function fixSolid(fix: Fix): ReactElement {
  return (
    <Button onClick={fix.run} loading={fix.busy}>
      <Icon name={fix.icon} data-icon="inline-start" />
      {fix.label}
    </Button>
  )
}

/** The fix as the phone's primary in the pushed screen's bottom row (Mobile / 10): the drawn launch button, named. */
export function fixFloat(fix: Fix): ReactElement {
  return <FloatButton kind="primary" size="l" icon={fix.icon} label={fix.label} onClick={fix.run} />
}

interface SkipMenuProps {
  finding: Finding
  /** `until: null` dismisses; a time snoozes. */
  onDismiss: (f: Finding, until: number | null) => void
  busy: boolean
  /** The control that opens it: SKIP FOR NOW (a desktop) or the top bar's ⋯ row. */
  trigger?: ReactElement<Record<string, unknown>>
}

/**
 * SKIP FOR NOW (brief H2): remind me in a week or a month, or dismiss, I've
 * dealt with it (for a missing second step: I already use two-step sign-in
 * here). A menu on a desktop, a sheet on a phone. An account alert can only
 * be dismissed (the core keeps no snooze for it).
 */
export function SkipMenu({ finding: f, onDismiss, busy, trigger }: SkipMenuProps) {
  return (
    <AdaptiveMenu
      title="Skip for now"
      overline={f.item_title ?? f.title}
      trigger={trigger ?? <Button variant="secondary" disabled={busy} aria-label={`Skip for now: ${f.item_title ?? f.title}`}>Skip for now</Button>}
    >
      {f.alert === null &&
        SNOOZES.map((s) => (
          <AdaptiveMenuItem key={s.days} icon="time" onSelect={() => onDismiss(f, snoozeUntil(s.days, Math.floor(Date.now() / 1000)))}>
            {s.label}
          </AdaptiveMenuItem>
        ))}
      {f.alert === null && <AdaptiveMenuSeparator />}
      <AdaptiveMenuItem icon="checkmark" onSelect={() => onDismiss(f, null)}>
        {f.kind === "missing_2fa" ? "I already use two-step sign-in here" : "Dismiss, I've dealt with it"}
      </AdaptiveMenuItem>
    </AdaptiveMenu>
  )
}
