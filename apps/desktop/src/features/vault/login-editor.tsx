import { useState, type FormEvent } from "react"

import {
  DialogAction,
  DialogActions,
  Field,
  FieldDescription,
  FieldError,
  FieldGroup,
  FieldLabel,
  Input,
  Sheet,
  SheetContent,
  SheetDescription,
  SheetHeader,
  SheetTitle,
} from "@unicron/ui"

import { describeError, useClient } from "@/core/client"
import { encodeSecret } from "@/core/secret-bytes"
import type { ItemDetail } from "@/core/types"

interface LoginEditorProps {
  open: boolean
  /** The item to change, or `null` to add one. */
  item: ItemDetail | null
  onClose: () => void
  onSaved: (id: string) => void
}

function plain(item: ItemDetail | null, fieldId: string): string {
  return item?.fields.find((f) => f.field_id === fieldId)?.value ?? ""
}

/**
 * Add or edit a login in a sheet. The password input's value is sent as
 * bytes once, on save, and the form is reset at once; while someone types
 * it is a webview string, which a webview cannot avoid (the native shells
 * will use native fields).
 */
export function LoginEditor({ open, item, onClose, onSaved }: LoginEditorProps) {
  const client = useClient()
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const editing = item !== null

  const submit = async (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    const form = e.currentTarget
    const data = new FormData(form)
    const text = (name: string) => String(data.get(name) ?? "").trim()
    const password = String(data.get("password") ?? "")
    setBusy(true)
    setError(null)
    try {
      let id: string
      if (editing) {
        await client.editLogin({
          id: item.id,
          title: text("title") || undefined,
          username: text("username") || undefined,
          password: password ? encodeSecret(password) : undefined,
        })
        id = item.id
      } else {
        id = await client.addLogin({
          site: text("site"),
          username: text("username"),
          password: encodeSecret(password),
          title: text("title") || undefined,
        })
      }
      form.reset()
      onSaved(id)
    } catch (err) {
      setError(describeError(err))
    } finally {
      setBusy(false)
    }
  }

  return (
    <Sheet open={open} onOpenChange={(o) => !o && onClose()}>
      <SheetContent>
        <form onSubmit={submit} className="flex h-full flex-col gap-4 p-4">
          <SheetHeader className="p-0">
            <SheetTitle>{editing ? "Edit login" : "New login"}</SheetTitle>
            <SheetDescription>
              {editing ? "Saves a new version; the old one stays in history." : "A site, a username and a password."}
            </SheetDescription>
          </SheetHeader>
          <FieldGroup>
            {!editing && (
              <Field>
                <FieldLabel htmlFor="login-site">Site</FieldLabel>
                <Input id="login-site" name="site" type="url" placeholder="https://" required autoComplete="off" />
              </Field>
            )}
            <Field>
              <FieldLabel htmlFor="login-title">Title</FieldLabel>
              <Input id="login-title" name="title" defaultValue={plain(item, "title")} autoComplete="off" />
              {!editing && <FieldDescription>Left empty, the site's host is used.</FieldDescription>}
            </Field>
            <Field>
              <FieldLabel htmlFor="login-username">Username</FieldLabel>
              <Input id="login-username" name="username" defaultValue={plain(item, "user")} required={!editing} autoComplete="off" />
            </Field>
            <Field>
              <FieldLabel htmlFor="login-password">Password</FieldLabel>
              <Input id="login-password" name="password" type="password" required={!editing} autoComplete="new-password" />
              {editing && <FieldDescription>Left empty, the password stays as it is.</FieldDescription>}
            </Field>
          </FieldGroup>
          {error && (
            <FieldError role="alert" className="text-sm">
              {error}
            </FieldError>
          )}
          <DialogActions className="mt-auto">
            <DialogAction kind="cancel" type="button" onClick={onClose} disabled={busy}>
              Cancel
            </DialogAction>
            <DialogAction kind="confirm" type="submit" disabled={busy}>
              {editing ? "Save" : "Add"}
            </DialogAction>
          </DialogActions>
        </form>
      </SheetContent>
    </Sheet>
  )
}
