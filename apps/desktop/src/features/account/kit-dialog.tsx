import { useState, type FormEvent } from "react"

import {
  Dialog,
  DialogAction,
  DialogActions,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  Field,
  FieldDescription,
  FieldError,
  FieldGroup,
  FieldLabel,
  Input,
  Switch,
} from "@unicron/ui"

import { describeError, useClient } from "@/core/client"
import type { OncePage } from "@/core/types"

interface KitDialogProps {
  kind: "kit-create" | "kit-verify" | null
  hasAccount: boolean
  onClose: () => void
  onDone: (lines: string[], page: OncePage | null) => void
}

/** Make or replace the recovery kit, or type it back in to verify it (and record the drill). */
export function KitDialog({ kind, hasAccount, onClose, onDone }: KitDialogProps) {
  const client = useClient()
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [replace, setReplace] = useState(false)
  const [drill, setDrill] = useState(true)
  const submit = async (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    const form = e.currentTarget
    const data = new FormData(form)
    const text = (n: string) => String(data.get(n) ?? "").trim()
    const password = String(data.get("password") ?? "") || undefined
    setBusy(true)
    setError(null)
    try {
      if (kind === "kit-create") {
        const page = await client.kitCreate({ replace, password })
        form.reset()
        onDone(page.lines, page)
      } else {
        const lines = await client.kitVerify({ secretKey: text("secretKey"), recoveryCode: text("recoveryCode"), password, drill })
        form.reset()
        onDone(lines, null)
      }
    } catch (err) {
      setError(describeError(err))
    } finally {
      setBusy(false)
    }
  }
  return (
    <Dialog open={kind !== null} onOpenChange={(o) => !o && onClose()}>
      <DialogContent>
        <form onSubmit={submit} className="flex flex-col gap-4">
          <DialogHeader>
            <DialogTitle>{kind === "kit-create" ? "Recovery kit" : "Recovery drill"}</DialogTitle>
            <DialogDescription>
              {kind === "kit-create"
                ? "Makes the page and shows it once. With an account, the account password is needed: its slot takes the new Secret Key."
                : "Type the kit back in to prove it opens this identity. Nothing changes unless the drill is recorded."}
            </DialogDescription>
          </DialogHeader>
          <FieldGroup>
            {kind === "kit-verify" && (
              <>
                <Field>
                  <FieldLabel htmlFor="kit-secret-key">Secret Key</FieldLabel>
                  <Input id="kit-secret-key" name="secretKey" type="password" required autoComplete="off" />
                </Field>
                <Field>
                  <FieldLabel htmlFor="kit-recovery-code">Recovery code</FieldLabel>
                  <Input id="kit-recovery-code" name="recoveryCode" type="password" required autoComplete="off" />
                </Field>
              </>
            )}
            {hasAccount && (
              <Field>
                <FieldLabel htmlFor="kit-password">Account password</FieldLabel>
                <Input id="kit-password" name="password" type="password" required={kind === "kit-create"} autoComplete="current-password" />
                {kind === "kit-verify" && <FieldDescription>Optional: also checks the Secret Key against the account slot.</FieldDescription>}
              </Field>
            )}
            {kind === "kit-create" ? (
              <Field orientation="horizontal">
                <FieldLabel htmlFor="kit-replace">Replace the kit that exists (the old page stops working)</FieldLabel>
                <Switch id="kit-replace" checked={replace} onCheckedChange={setReplace} />
              </Field>
            ) : (
              <Field orientation="horizontal">
                <FieldLabel htmlFor="kit-drill">Record a passing check as the recovery drill</FieldLabel>
                <Switch id="kit-drill" checked={drill} onCheckedChange={setDrill} />
              </Field>
            )}
          </FieldGroup>
          {error && <FieldError role="alert">{error}</FieldError>}
          <DialogActions>
            <DialogAction kind="cancel" type="button" onClick={onClose} disabled={busy}>
              Cancel
            </DialogAction>
            <DialogAction kind="confirm" type="submit" disabled={busy}>
              {kind === "kit-create" ? "Make the kit" : "Check the kit"}
            </DialogAction>
          </DialogActions>
        </form>
      </DialogContent>
    </Dialog>
  )
}
