import { useState, type FormEvent } from "react"

import { AdaptiveDialog, Banner, Button, TextField } from "@unicron/ui"
import { SecretField } from "@unicron/ui/security/secret-field"

import { useSettings } from "@/app/settings"
import { describeError, useClient } from "@/core/client"
import type { OncePage } from "@/core/types"

interface AccountDialogProps {
  kind: "register" | "login" | null
  /** Why a sign-in is needed, in the core's words (a session the server wouldn't renew). */
  reason?: string | null
  /**
   * Whether this device has a recovery kit, so the sign-in also asks for its
   * Secret Key (brief T4). Unknown (the shell's Banner opens the form without
   * the status at hand): the field is shown, and it stays optional.
   */
  hasKit?: boolean
  onClose: () => void
  onDone: (lines: string[], page: OncePage | null) => void
}

/**
 * The account forms (brief T4, doc 82): **Sign in** (the `login` kind:
 * Server, Login name, Master password, and the Secret Key when a kit
 * exists) and **Register** (the same, and a kit is made and shown once
 * when none exists yet). The shell's critical Banner (SIGN IN) and Status
 * open them. A Form: a 640 dialog on a desktop, a sheet on a phone. The
 * typed password leaves with the form, which clears on close and after
 * each answer.
 */
export function AccountDialog({ kind, reason, hasKit = true, onClose, onDone }: AccountDialogProps) {
  const client = useClient()
  const { settings } = useSettings()
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [password, setPassword] = useState("")
  const [secretKey, setSecretKey] = useState("")
  const register = kind === "register"

  const clear = () => {
    setPassword("")
    setSecretKey("")
    setError(null)
  }
  const close = () => {
    clear()
    onClose()
  }

  const submit = async (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    const form = e.currentTarget
    const data = new FormData(form)
    const text = (n: string) => String(data.get(n) ?? "").trim()
    const input = { server: text("server"), login: text("login"), password, secretKey: secretKey.trim() || undefined }
    setBusy(true)
    setError(null)
    try {
      if (register) {
        const page = await client.accountRegister(input)
        form.reset()
        clear()
        onDone(page.lines, page)
      } else {
        const lines = await client.accountLogin(input)
        form.reset()
        clear()
        onDone(lines, null)
      }
    } catch (err) {
      setError(describeError(err))
    } finally {
      setBusy(false)
    }
  }

  const missing = !password
  return (
    <AdaptiveDialog
      open={kind !== null}
      onOpenChange={(o) => !o && close()}
      size="form"
      closeOnOutsideClick={false}
      overline="Account"
      title={register ? "Register an account" : "Sign in"}
      description={
        register
          ? "An account syncs this vault between your devices. If no recovery kit exists yet, one is made now and shown once."
          : (reason ?? "A fresh session on this device.")
      }
      footer={
        <>
          <Button type="button" variant="secondary" onClick={close} disabled={busy}>
            Cancel
          </Button>
          <Button type="submit" form="account-form" loading={busy} disabled={missing} disabledReason={missing ? "Type your master password first." : undefined}>
            {register ? "Register" : "Sign in"}
          </Button>
        </>
      }
    >
      <form id="account-form" onSubmit={(e) => void submit(e)} className="flex flex-col gap-6" aria-label={register ? "Register an account" : "Sign in"}>
        <TextField
          id="acct-server"
          name="server"
          label="Server"
          type="url"
          inputMode="url"
          placeholder="https://"
          defaultValue={settings.server ?? ""}
          required
          autoComplete="off"
          helper="Yours or ours. A server you run yourself is a first-class choice."
        />
        <TextField id="acct-login" name="login" label="Login name" required autoComplete="username" />
        <SecretField
          mode="edit"
          id="acct-password"
          label="Master password"
          name="password"
          value={password}
          onChange={setPassword}
          autoComplete={register ? "new-password" : "current-password"}
        />
        {hasKit && (
          <SecretField
            mode="edit"
            id="acct-secret-key"
            label="Secret Key"
            name="secret-key"
            mono
            value={secretKey}
            onChange={setSecretKey}
            autoComplete="off"
            description={register ? "Only when a recovery kit already exists." : "From your recovery kit. Finishes protecting an older account."}
          />
        )}
        {error && <Banner tone="critical" size="compact" title={error} />}
      </form>
    </AdaptiveDialog>
  )
}
