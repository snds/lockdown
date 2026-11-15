import { useState, type FormEvent } from "react"

import { AdaptiveDialog, Banner, Button, ToggleRow } from "@unicron/ui"
import { SecretField } from "@unicron/ui/security/secret-field"

import { describeError, useClient } from "@/core/client"
import type { OncePage } from "@/core/types"

interface KitDialogProps {
  kind: "kit-create" | "kit-verify" | null
  hasAccount: boolean
  /** A new kit in place of the one that exists: the person has said yes to "Your old kit page stops working." (brief T3). */
  replace?: boolean
  onClose: () => void
  onDone: (lines: string[], page: OncePage | null) => void
}

/** Which of the drill's fields a refusal is about: the first one its sentence names; none, and it belongs to the whole form. */
export function drillFieldOf(message: string): "secretKey" | "recoveryCode" | "password" | null {
  const words = message.toLowerCase()
  const at = (["secretKey", "recoveryCode", "password"] as const)
    .map((field) => ({ field, i: words.indexOf(field === "secretKey" ? "secret key" : field === "recoveryCode" ? "recovery code" : "password") }))
    .filter((f) => f.i >= 0)
    .sort((a, b) => a.i - b.i)
  return at[0]?.field ?? null
}

/**
 * The recovery kit's two forms (brief T3, doc 81):
 *
 * - **The drill** (`kit-verify`): the Secret Key and the recovery code as
 *   SecretFields, the master password when there is an account (it also
 *   checks the Secret Key against the account's slot), and "Record a
 *   passing check as the drill". CHECK KIT. A refusal shows the core's
 *   sentence under the field it names, or above the buttons when it names
 *   none; the form stays.
 * - **A new kit with an account** (`kit-create`): the master password, which
 *   the account's slot needs for the new Secret Key. CREATE KIT. Without an
 *   account no form is needed: the Recovery section makes the kit at once.
 *
 * A dialog (form 640) on a desktop, a sheet on a phone. The typed values
 * leave with the form.
 */
export function KitDialog({ kind, hasAccount, replace = false, onClose, onDone }: KitDialogProps) {
  const client = useClient()
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<{ field: ReturnType<typeof drillFieldOf>; text: string } | null>(null)
  const [drill, setDrill] = useState(true)
  const [secretKey, setSecretKey] = useState("")
  const [recoveryCode, setRecoveryCode] = useState("")
  const [password, setPassword] = useState("")
  const verify = kind === "kit-verify"

  const clear = () => {
    setSecretKey("")
    setRecoveryCode("")
    setPassword("")
    setError(null)
  }
  const close = () => {
    clear()
    onClose()
  }

  const missing = verify ? !secretKey.trim() || !recoveryCode.trim() : hasAccount && !password
  const submit = async (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    if (missing) return
    setBusy(true)
    setError(null)
    try {
      if (verify) {
        const lines = await client.kitVerify({ secretKey: secretKey.trim(), recoveryCode: recoveryCode.trim(), password: password || undefined, drill })
        clear()
        onDone(lines, null)
      } else {
        const page = await client.kitCreate({ replace, password: password || undefined })
        clear()
        onDone(page.lines, page)
      }
    } catch (err) {
      const text = describeError(err)
      const field = drillFieldOf(text)
      // A field only shows what it is drawn with: the password field is there only with an account.
      setError({ field: field === "password" && !hasAccount ? null : verify || field === "password" ? field : null, text })
    } finally {
      setBusy(false)
    }
  }
  const under = (field: "secretKey" | "recoveryCode" | "password") => (error?.field === field ? error.text : undefined)

  return (
    <AdaptiveDialog
      open={kind !== null}
      onOpenChange={(o) => !o && close()}
      size="form"
      closeOnOutsideClick={false}
      overline={verify ? "Recovery drill" : "Recovery kit"}
      title={verify ? "Type your recovery kit" : "New recovery kit"}
      description={
        verify
          ? "Lockdown checks it opens this vault. Nothing changes unless you record the drill."
          : "Your account's slot takes the new Secret Key, so it needs your master password."
      }
      footer={
        <>
          <Button type="button" variant="secondary" onClick={close} disabled={busy}>
            Cancel
          </Button>
          <Button
            type="submit"
            form="kit-form"
            loading={busy}
            disabled={missing}
            disabledReason={missing ? (verify ? "Type both lines from your kit first." : "Type your master password first.") : undefined}
          >
            {verify ? "Check kit" : "Create kit"}
          </Button>
        </>
      }
    >
      <form id="kit-form" onSubmit={submit} className="flex flex-col gap-6" aria-label={verify ? "Recovery drill" : "New recovery kit"}>
        {verify && (
          <>
            <SecretField mode="edit" id="kit-secret-key" label="Secret Key" name="secret-key" mono value={secretKey} onChange={setSecretKey} autoComplete="off" error={under("secretKey")} />
            <SecretField mode="edit" id="kit-recovery-code" label="Recovery code" name="recovery-code" mono value={recoveryCode} onChange={setRecoveryCode} autoComplete="off" error={under("recoveryCode")} />
          </>
        )}
        {hasAccount && (
          <SecretField
            mode="edit"
            id="kit-password"
            label="Master password"
            name="password"
            value={password}
            onChange={setPassword}
            autoComplete="current-password"
            description={verify ? "Optional. Also checks the Secret Key against your account." : undefined}
            error={under("password")}
          />
        )}
        {verify && <ToggleRow label="Record a passing check as the drill" checked={drill} onCheckedChange={setDrill} />}
        {error && error.field === null && <Banner tone="critical" size="compact" title={error.text} />}
      </form>
    </AdaptiveDialog>
  )
}
