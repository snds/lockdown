import { useState } from "react"

import { Button, Field, FieldDescription, FieldError, FieldLabel, Icon, Input } from "@unicron/ui"

import { useSettings } from "@/app/settings"
import { describeError } from "@/core/client"

import { useSharingClient, utf8 } from "./sharing-client"

interface LinkOnceProps {
  /** The link, decoded from the shell's bytes (which were wiped) by the dialog that holds it. */
  link: string
  label?: string
  /** What to tell the person about passing it on. */
  note?: string
}

/**
 * A link that carries its key (a share link, a request link) is shown
 * once, here. Copy goes the way every secret's copy goes: the shell writes
 * the clipboard and clears it on the settings' timer, only while it still
 * holds this copy (D85, `link_copy`). The dialog that holds the link drops
 * the string when it closes.
 */
export function LinkOnce({ link, label = "The link", note }: LinkOnceProps) {
  const sharing = useSharingClient()
  const { settings } = useSettings()
  const [copied, setCopied] = useState<number | null>(null)
  const [error, setError] = useState<string | null>(null)

  const copy = async () => {
    setError(null)
    try {
      const c = await sharing.copyLink(utf8(link), settings.clipboard_clear_secs)
      setCopied(c.clears_in_secs)
    } catch (e) {
      setError(describeError(e))
    }
  }
  return (
    <Field>
      <FieldLabel htmlFor="link-once">{label}</FieldLabel>
      <Input id="link-once" readOnly value={link} spellCheck={false} onFocus={(e) => e.currentTarget.select()} />
      <div>
        <Button variant="outline" size="sm" onClick={() => void copy()}>
          <Icon name="content_copy" data-icon="inline-start" />
          Copy the link
        </Button>
      </div>
      <FieldDescription aria-live="polite">
        {copied !== null
          ? `Copied. The clipboard clears in ${copied} seconds.`
          : (note ?? "Send it in a way you trust. It isn't shown again after you close this.")}
      </FieldDescription>
      {error && <FieldError role="alert">{error}</FieldError>}
    </Field>
  )
}
