import { useState } from "react"

import { ConfirmDialog, FieldRow, FieldRowAction } from "@unicron/ui"

import { decodeAndWipe } from "@/core/secret-bytes"
import { describeError, useClient } from "@/core/client"
import type { OncePage as OncePageData, Status } from "@/core/types"
import { KitDialog } from "@/features/account/kit-dialog"
import { EmergencySheetRow } from "@/features/bridges/emergency-sheet"
import { DrillSection, PasswordReminderSetting, useRecoveryFacts } from "@/features/depth/reminders"
import { SaveError, useRowSize } from "@/features/settings/setting-rows"
import { OncePage, type ShownPage } from "@/features/status/once-page"
import { fullDate } from "@/lib/format"

interface RecoveryViewProps {
  status: Status
  onChanged: () => void
  onNotice: (text: string) => void
  /** Opens Sharing → Emergency access; without it the row only says where. */
  onEmergency?: () => void
}

/** RECOVERY KIT's value (brief T3): the core's word, and the last drill's date when the reminders know it. */
export function kitValue(recovery: string, lastDrilled: number | null | undefined): string {
  if (recovery === "drilled") return lastDrilled ? `Tested ${fullDate(lastDrilled)}` : "Tested"
  if (recovery === "made") return "Made, not tested"
  return "None yet"
}

/**
 * Settings → Recovery (brief T3, doc 81): make sure the person can get back
 * in. Rows, top to bottom:
 *
 * - RECOVERY KIT: "None yet", "Made, not tested" or "Tested 2026.10.01".
 *   NEW KIT. When a kit exists it asks first ("Your old kit page stops
 *   working.", a Question with the default yes, X5); then, with an
 *   account, the master password; then the kit page, shown once.
 * - RECOVERY DRILL: "Passed 2026.10.01" or "Not run yet". RUN DRILL opens
 *   the drill form; a pass is a toast in the core's words.
 * - MASTER PASSWORD REMINDER (only where unlocking does not ask for it).
 * - EMERGENCY SHEET: OPEN, the print page.
 * - EMERGENCY ACCESS: OPEN, Sharing → Emergency access (S5), which is built
 *   (review R5: no "later").
 * - WHAT THE DRILL CHECKS NN.
 *
 * Not in the web vault (W1): its Settings has no Recovery section.
 */
export function RecoveryView({ status, onChanged, onNotice, onEmergency }: RecoveryViewProps) {
  const client = useClient()
  const facts = useRecoveryFacts()
  const size = useRowSize()
  const [dialog, setDialog] = useState<"kit-create" | "kit-verify" | null>(null)
  const [asking, setAsking] = useState(false)
  const [replace, setReplace] = useState(false)
  const [making, setMaking] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [once, setOnce] = useState<ShownPage | null>(null)
  const kit = status.recovery
  const hasAccount = status.account !== null
  const lastDrilled = facts.reminders?.drill.last_drilled_at ?? null
  const drilled = kit === "drilled"

  const showOnce = (page: OncePageData) => setOnce({ title: "Your recovery kit", lines: page.lines, text: page.page.length ? decodeAndWipe(page.page) : null })
  const made = (page: OncePageData) => {
    showOnce(page)
    facts.reload()
    onChanged()
  }
  /** Without an account the kit needs no password: it is made at once. */
  const makeNow = async (replacing: boolean) => {
    setMaking(true)
    setError(null)
    try {
      made(await client.kitCreate({ replace: replacing }))
    } catch (e) {
      setError(describeError(e))
      throw e
    } finally {
      setMaking(false)
    }
  }
  const newKit = () => {
    setError(null)
    if (kit !== "none") setAsking(true)
    else if (hasAccount) {
      setReplace(false)
      setDialog("kit-create")
    } else void makeNow(false).catch(() => undefined)
  }

  return (
    <div className="flex flex-col gap-8">
      <div className="flex flex-col">
        <div className="flex flex-col">
          <FieldRow
            size={size}
            label="Recovery kit"
            description="A Secret Key and a recovery code on one page. With it you get back in when every device is gone."
            action={
              <FieldRowAction loading={making && !asking} onClick={newKit}>
                New kit
              </FieldRowAction>
            }
          >
            {kitValue(kit, lastDrilled)}
          </FieldRow>
          {!asking && <SaveError text={error} />}
        </div>
        <FieldRow
          size={size}
          label="Recovery drill"
          description="Type the kit back in and prove it opens this vault."
          disabledReason={kit === "none" ? "Make a kit first." : undefined}
          action={<FieldRowAction onClick={() => setDialog("kit-verify")}>Run drill</FieldRowAction>}
        >
          {drilled && lastDrilled ? `Passed ${fullDate(lastDrilled)}` : drilled ? "Passed" : "Not run yet"}
        </FieldRow>
        <PasswordReminderSetting facts={facts} />
        <EmergencySheetRow />
        <FieldRow
          size={size}
          label="Emergency access"
          description="Someone you trust can ask for your vaults, after a wait you can stop."
          action={onEmergency ? <FieldRowAction onClick={onEmergency}>Open</FieldRowAction> : undefined}
        >
          {onEmergency ? undefined : "In Sharing → Emergency access"}
        </FieldRow>
      </div>
      {kit !== "none" && <DrillSection facts={facts} />}
      {/* NEW KIT over a kit that exists: a choice worth a second look, so the yes is the default one (X5). */}
      <ConfirmDialog
        open={asking}
        onOpenChange={(o) => !o && setAsking(false)}
        overline="Recovery kit"
        title="Replace your recovery kit?"
        description="Your old kit page stops working."
        confirmLabel="Replace kit"
        error={asking ? error : null}
        onConfirm={async () => {
          if (hasAccount) {
            setAsking(false)
            setReplace(true)
            setDialog("kit-create")
            return
          }
          await makeNow(true)
          setAsking(false)
        }}
        onCancel={() => setError(null)}
      />
      <KitDialog
        kind={dialog}
        hasAccount={hasAccount}
        replace={replace}
        onClose={() => setDialog(null)}
        onDone={(lines, page) => {
          const verified = dialog === "kit-verify"
          setDialog(null)
          if (page) made(page)
          else {
            facts.reload()
            onChanged()
          }
          // A drill's pass, in the core's words (brief T3); a new kit's notes are on its page.
          if (verified && lines.length) onNotice(lines.join(" "))
        }}
      />
      <OncePage page={once} onClose={() => setOnce(null)} />
    </div>
  )
}
