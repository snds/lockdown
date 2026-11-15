import { useState, type FormEvent, type ReactNode } from "react"

import { Button, Field, FieldLabel, FieldRow, FloatButton, NativeSelect, NativeSelectOption } from "@unicron/ui"
import { SecretField } from "@unicron/ui/security/secret-field"

import { useSettings } from "@/app/settings"
import { describeError } from "@/core/client"

import { ShareLayer } from "./share-layer"
import { useSharingClient, utf8 } from "./sharing-client"

interface LinkOnceProps {
  /** The link, decoded from the shell's bytes (which were wiped) by the layer that holds it. */
  link: string
  label?: string
  /** What to tell the person about passing it on. */
  note?: string
}

/** The characters kept at the link's end when it is cut in the middle: the key's last part stays readable. */
const TAIL = 10

/** The words after a copy, with the seconds from the copy's own `clears_in_secs` (brief I7). */
export const copiedWords = (secs: number) => `Copied. The clipboard clears in ${secs} seconds.`

/** The caption under a share link shown once (brief I7). */
export const SHOWN_ONCE = "Send it in a way you trust. It isn't shown again after you close this."

/**
 * A link that carries its key (a share link, a request link), shown once
 * (brief I7): the link in a read-only mono line, cut in the middle so its
 * start and its end both show, and COPY LINK as the layer's one primary,
 * full width under it. Copy goes the way every secret's copy goes: the
 * shell writes the clipboard and clears it on the settings' timer, only
 * while it still holds this copy (D85, `link_copy`). The layer that holds
 * the link drops the string when it closes.
 */
export function LinkOnce({ link, label = "The link", note = SHOWN_ONCE }: LinkOnceProps) {
  const sharing = useSharingClient()
  const { settings } = useSettings()
  const [copied, setCopied] = useState<number | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  const copy = async () => {
    setError(null)
    setBusy(true)
    try {
      const c = await sharing.copyLink(utf8(link), settings.clipboard_clear_secs)
      setCopied(c.clears_in_secs)
    } catch (e) {
      setError(describeError(e))
    } finally {
      setBusy(false)
    }
  }
  const head = link.length > TAIL * 2 ? link.slice(0, -TAIL) : link
  const tail = link.length > TAIL * 2 ? link.slice(-TAIL) : ""
  return (
    <div className="flex flex-col gap-4">
      <FieldRow size="stacked" label={label} rule="none">
        {/* Cut in the middle: the start shrinks with an ellipsis, the end stays. The text is the whole link. */}
        <span data-testid="new-link" className="type-code-inline flex min-w-0 max-w-full">
          <span className="min-w-0 truncate">{head}</span>
          <span className="shrink-0">{tail}</span>
        </span>
      </FieldRow>
      {/* Full width: a grid cell stretches the button (no className on unicron's part). */}
      <div className="grid">
        <Button type="button" loading={busy} onClick={() => void copy()}>
          Copy link
        </Button>
      </div>
      <p aria-live="polite" className="type-caption text-text-secondary">
        {copied !== null ? copiedWords(copied) : note}
      </p>
      {error && (
        <p role="alert" className="type-caption text-critical-text">
          {error}
        </p>
      )}
    </div>
  )
}

/** The lifetimes and open counts a link offers (D192). */
export const LINK_DAYS = [1, 3, 7, 14, 30]
const OPENS: Array<[string, string]> = [
  ["Once", "1"],
  ["3 times", "3"],
  ["10 times", "10"],
  ["Until it expires", ""],
]

/** `1 day`, `7 days`. */
const dayWords = (d: number) => (d === 1 ? "1 day" : `${d} days`)

/** "Lasts": how many days a link opens for. Read from the form as `days`. */
export function LastsField({ id }: { id: string }) {
  return (
    <Field>
      <FieldLabel htmlFor={id}>Lasts</FieldLabel>
      <NativeSelect id={id} name="days" defaultValue="7">
        {LINK_DAYS.map((d) => (
          <NativeSelectOption key={d} value={String(d)}>
            {dayWords(d)}
          </NativeSelectOption>
        ))}
      </NativeSelect>
    </Field>
  )
}

/**
 * A share link's options (briefs I7 and S4): Lasts, Opens at most, and
 * the optional password in unicron's SecretField, edited. The form reads
 * `days` and `views`; the password comes from `password`.
 */
export function LinkFields({ idPrefix, password, onPassword }: { idPrefix: string; password: string; onPassword: (value: string) => void }) {
  return (
    <>
      <LastsField id={`${idPrefix}-days`} />
      <Field>
        <FieldLabel htmlFor={`${idPrefix}-views`}>Opens at most</FieldLabel>
        <NativeSelect id={`${idPrefix}-views`} name="views" defaultValue="1">
          {OPENS.map(([label, value]) => (
            <NativeSelectOption key={label} value={value}>
              {label}
            </NativeSelectOption>
          ))}
        </NativeSelect>
      </Field>
      <div className="flex flex-col gap-1">
        <SecretField mode="edit" label="Password (optional)" value={password} onChange={onPassword} autoComplete="off" />
        <p className="type-caption text-text-secondary">Tell it to them another way.</p>
      </div>
    </>
  )
}

/** What a link's options came to: days, opens (null: until it expires). */
export function linkOptions(form: HTMLFormElement): { days: number; views: number | null } {
  const data = new FormData(form)
  const views = String(data.get("views") ?? "1")
  return { days: Number(data.get("days") ?? 7), views: views === "" ? null : Number(views) }
}

interface LinkLayerProps {
  open: boolean
  onClose: () => void
  overline: string
  title: string
  /** One sentence above the fields, or left out. */
  description?: string
  /** The form's id, for its submit buttons. */
  formId: string
  /** The fields, inside the form. */
  children: ReactNode
  /** Makes the link from the form; answers it (the bytes already wiped), or throws the core's refusal. */
  onMake: (form: HTMLFormElement) => Promise<string>
  /** The caption under the link once it shows. */
  note?: string
  /** Something to say under the fields before the button (the item link's "Anyone with the link…"). */
  caption?: string
  /** Called once the layer closes, to drop typed secrets. */
  onReset?: () => void
}

/**
 * Making a link, then the link shown once, in the same layer (briefs I7
 * and S4). First the form with CREATE LINK as its one primary (an
 * extended "Create link" on a phone). Then the link: COPY LINK is the
 * layer's one primary; DONE closes it, secondary in the desktop footer
 * and a text button in the phone top bar's leading place, with no
 * primary at the top right on this step. Closing drops the link.
 */
export function LinkLayer({ open, onClose, overline, title, description, formId, children, onMake, note, caption, onReset }: LinkLayerProps) {
  const [link, setLink] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const close = () => {
    setLink(null)
    setError(null)
    onReset?.()
    onClose()
  }
  const submit = async (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    const form = e.currentTarget
    setError(null)
    setBusy(true)
    try {
      setLink(await onMake(form))
      onReset?.()
    } catch (err) {
      setError(describeError(err))
    } finally {
      setBusy(false)
    }
  }
  const shown = link !== null
  return (
    <ShareLayer
      open={open}
      onClose={close}
      overline={overline}
      title={title}
      description={shown ? undefined : description}
      footer={
        shown ? (
          <Button type="button" variant="secondary" onClick={close}>
            Done
          </Button>
        ) : (
          <>
            <Button type="button" variant="ghost" onClick={close}>
              Cancel
            </Button>
            <Button type="submit" form={formId} loading={busy}>
              Create link
            </Button>
          </>
        )
      }
      phoneStart={
        shown ? (
          <Button type="button" variant="ghost" onClick={close}>
            Done
          </Button>
        ) : undefined
      }
      phoneAction={shown ? undefined : <FloatButton label="Create link" icon="link" kind="primary" size="l" extended type="submit" form={formId} disabled={busy} />}
    >
      {shown ? (
        <LinkOnce link={link} note={note} />
      ) : (
        <form id={formId} aria-label={title} className="flex flex-col gap-4" onSubmit={(e) => void submit(e)}>
          {children}
          {caption && <p className="type-caption text-text-secondary">{caption}</p>}
          {/* A refusal (a split view pauses links, D328 as changed by D375) shows where the person is looking. */}
          {error && (
            <p role="alert" className="type-caption text-critical-text">
              {error}
            </p>
          )}
        </form>
      )}
    </ShareLayer>
  )
}
