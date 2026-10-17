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
} from "@unicron/ui"

import { useSettings } from "@/app/settings"
import { describeError, useClient } from "@/core/client"
import type { OncePage } from "@/core/types"

interface AccountDialogProps {
  kind: "register" | "login" | null
  onClose: () => void
  onDone: (lines: string[], page: OncePage | null) => void
}

/** Register an account on a server, or log in to one. The typed password goes out once and the form resets. */
export function AccountDialog({ kind, onClose, onDone }: AccountDialogProps) {
  const client = useClient()
  const { settings } = useSettings()
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const submit = async (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    const form = e.currentTarget
    const data = new FormData(form)
    const text = (n: string) => String(data.get(n) ?? "").trim()
    const input = { server: text("server"), login: text("login"), password: String(data.get("password") ?? ""), secretKey: text("secretKey") || undefined }
    setBusy(true)
    setError(null)
    try {
      if (kind === "register") {
        const page = await client.accountRegister(input)
        form.reset()
        onDone(page.lines, page)
      } else {
        const lines = await client.accountLogin(input)
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
            <DialogTitle>{kind === "register" ? "Register an account" : "Log in"}</DialogTitle>
            <DialogDescription>
              {kind === "register"
                ? "An account syncs this vault between your devices. If no recovery kit exists yet, one is made now and shown once."
                : "A fresh session on this device. Give the Secret Key too to finish protecting an older account."}
            </DialogDescription>
          </DialogHeader>
          <FieldGroup>
            <Field>
              <FieldLabel htmlFor="acct-server">Server</FieldLabel>
              <Input id="acct-server" name="server" type="url" placeholder="https://" defaultValue={settings.server ?? ""} required autoComplete="off" />
              <FieldDescription>Yours or ours; a self-hosted server is a first-class choice.</FieldDescription>
            </Field>
            <Field>
              <FieldLabel htmlFor="acct-login">Login name</FieldLabel>
              <Input id="acct-login" name="login" required autoComplete="username" />
            </Field>
            <Field>
              <FieldLabel htmlFor="acct-password">Account password</FieldLabel>
              <Input id="acct-password" name="password" type="password" required autoComplete={kind === "register" ? "new-password" : "current-password"} />
            </Field>
            <Field>
              <FieldLabel htmlFor="acct-secret-key">Secret Key</FieldLabel>
              <Input id="acct-secret-key" name="secretKey" type="password" autoComplete="off" />
              <FieldDescription>Only when a recovery kit already exists.</FieldDescription>
            </Field>
          </FieldGroup>
          {error && <FieldError role="alert">{error}</FieldError>}
          <DialogActions>
            <DialogAction kind="cancel" type="button" onClick={onClose} disabled={busy}>
              Cancel
            </DialogAction>
            <DialogAction kind="confirm" type="submit" disabled={busy}>
              {kind === "register" ? "Register" : "Log in"}
            </DialogAction>
          </DialogActions>
        </form>
      </DialogContent>
    </Dialog>
  )
}
