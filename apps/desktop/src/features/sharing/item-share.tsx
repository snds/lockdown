import { useEffect, useState, type FormEvent } from "react"

import {
  Badge,
  Button,
  Dialog,
  DialogAction,
  DialogActions,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
  Field,
  FieldDescription,
  FieldError,
  FieldGroup,
  FieldLabel,
  Icon,
  Input,
  NativeSelect,
  NativeSelectOption,
  ToggleGroup,
  ToggleGroupItem,
} from "@unicron/ui"

import { useGate } from "@/app/gate"
import { describeError } from "@/core/client"
import { decodeAndWipe } from "@/core/secret-bytes"
import type { Access, ItemDetail } from "@/core/types"

import { LinkOnce } from "./link-once"
import { RIGHT_WORDS, useSharingClient, utf8, type ContactView } from "./sharing-client"

interface ItemShareProps {
  item: ItemDetail
  title: string
  onNotice: (text: string) => void
}

type Open = "person" | "link" | null

/**
 * Sharing one item from its detail (docs/plan/18-sharing.md §5): with
 * someone pinned here, to view or to fill only, in two taps (who, then
 * Share), or as a share link anyone with it can open. Only the person's
 * own items offer it, never a crown jewel.
 */
export function ItemShare({ item, title, onNotice }: ItemShareProps) {
  const [open, setOpen] = useState<Open>(null)
  if (item.access || item.crown_jewel || item.state === "trashed") return null
  return (
    <>
      <DropdownMenu>
        <DropdownMenuTrigger render={<Button variant="outline" size="sm" />}>
          <Icon name="share" data-icon="inline-start" />
          Share
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end">
          <DropdownMenuItem onClick={() => setOpen("person")}>
            <Icon name="person_add" data-icon="inline-start" />
            With someone…
          </DropdownMenuItem>
          <DropdownMenuItem onClick={() => setOpen("link")}>
            <Icon name="link" data-icon="inline-start" />
            As a share link…
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
      <SharePersonDialog open={open === "person"} item={item} title={title} onClose={() => setOpen(null)} onNotice={onNotice} />
      <ShareLinkDialog open={open === "link"} item={item} title={title} onClose={() => setOpen(null)} />
    </>
  )
}

function SharePersonDialog({ open, item, title, onClose, onNotice }: { open: boolean; item: ItemDetail; title: string; onClose: () => void; onNotice: (t: string) => void }) {
  const sharing = useSharingClient()
  const gate = useGate()
  const [contacts, setContacts] = useState<ContactView[] | null>(null)
  const [who, setWho] = useState("")
  const [right, setRight] = useState<"view" | "fill-only">("view")
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    if (!open) return
    let live = true
    sharing
      .overview()
      .then((o) => {
        if (!live) return
        setContacts(o.contacts)
        setWho((w) => w || o.contacts[0]?.login || "")
      })
      .catch((e) => live && setError(describeError(e)))
    return () => {
      live = false
    }
  }, [open, sharing])

  const share = async () => {
    setError(null)
    try {
      await gate.run((g) => sharing.shareItem(item.id, who, right, g))
      onNotice(`Shared "${title}" with ${who}.`)
      onClose()
    } catch (e) {
      setError(describeError(e))
    }
  }
  const pick = contacts?.find((c) => c.login === who)
  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Share "{title}"</DialogTitle>
          <DialogDescription>
            Only this item goes, locked to their key. You can stop sharing it at any time; it then moves to a new key.
          </DialogDescription>
        </DialogHeader>
        {contacts && contacts.length === 0 ? (
          <p className="text-sm text-muted-foreground">Look someone up on the Sharing screen first, so their keys are checked.</p>
        ) : (
          <FieldGroup>
            <Field>
              <FieldLabel htmlFor="share-who">With</FieldLabel>
              <NativeSelect id="share-who" value={who} onChange={(e) => setWho(e.target.value)}>
                {(contacts ?? []).map((c) => (
                  <NativeSelectOption key={c.account_id} value={c.login}>
                    {c.login}
                    {c.verified ? "" : " (fingerprint not compared)"}
                  </NativeSelectOption>
                ))}
              </NativeSelect>
              {pick && !pick.verified && (
                <FieldDescription>You haven't compared fingerprints with {pick.login} yet; their keys checked out against the server's log.</FieldDescription>
              )}
            </Field>
            <Field>
              <FieldLabel>They can</FieldLabel>
              <ToggleGroup value={[right]} onValueChange={(v: unknown[]) => v[0] && setRight(v[0] as "view" | "fill-only")} aria-label="What they can do">
                <ToggleGroupItem value="view">View</ToggleGroupItem>
                <ToggleGroupItem value="fill-only">Fill only</ToggleGroupItem>
              </ToggleGroup>
              <FieldDescription>{RIGHT_WORDS[right as Access]}</FieldDescription>
            </Field>
          </FieldGroup>
        )}
        {error && <FieldError role="alert">{error}</FieldError>}
        <DialogActions>
          <DialogAction kind="cancel" onClick={onClose}>
            Cancel
          </DialogAction>
          <DialogAction kind="confirm" disabled={!who} onClick={() => void share()}>
            Share
          </DialogAction>
        </DialogActions>
      </DialogContent>
    </Dialog>
  )
}

function ShareLinkDialog({ open, item, title, onClose }: { open: boolean; item: ItemDetail; title: string; onClose: () => void }) {
  const sharing = useSharingClient()
  const gate = useGate()
  const [link, setLink] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)

  const close = () => {
    setLink(null)
    setError(null)
    onClose()
  }
  const submit = async (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    const data = new FormData(e.currentTarget)
    const days = Number(data.get("days") ?? 7)
    const views = String(data.get("views") ?? "")
    const password = String(data.get("password") ?? "")
    e.currentTarget.reset()
    setError(null)
    try {
      const bytes = await gate.run((g) => sharing.sendItem(item.id, days, views ? Number(views) : null, password ? utf8(password) : null, g))
      setLink(decodeAndWipe(bytes))
    } catch (err) {
      setError(describeError(err))
    }
  }
  return (
    <Dialog open={open} onOpenChange={(o) => !o && close()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>A share link for "{title}"</DialogTitle>
          <DialogDescription>
            Anyone with the link can open a copy of this item as it is now, until it expires. It is locked on this computer; the key is the part
            after <code>#</code>, which never reaches the server.
          </DialogDescription>
        </DialogHeader>
        {link ? (
          <LinkOnce link={link} />
        ) : (
          <form id="share-link" onSubmit={(e) => void submit(e)}>
            <LinkFields />
          </form>
        )}
        {error && <FieldError role="alert">{error}</FieldError>}
        <DialogActions>
          <DialogAction kind="cancel" onClick={close}>
            {link ? "Done" : "Cancel"}
          </DialogAction>
          {!link && (
            <DialogAction kind="confirm" type="submit" form="share-link">
              Make the link
            </DialogAction>
          )}
        </DialogActions>
      </DialogContent>
    </Dialog>
  )
}

/** A share link's options: how long, how many opens, an optional password. */
export function LinkFields() {
  return (
    <FieldGroup>
      <Field>
        <FieldLabel htmlFor="link-days">Lasts</FieldLabel>
        <NativeSelect id="link-days" name="days" defaultValue="7">
          {[1, 3, 7, 14, 30].map((d) => (
            <NativeSelectOption key={d} value={String(d)}>
              {d === 1 ? "1 day" : `${d} days`}
            </NativeSelectOption>
          ))}
        </NativeSelect>
      </Field>
      <Field>
        <FieldLabel htmlFor="link-views">Opens at most</FieldLabel>
        <NativeSelect id="link-views" name="views" defaultValue="1">
          <NativeSelectOption value="1">Once</NativeSelectOption>
          <NativeSelectOption value="3">3 times</NativeSelectOption>
          <NativeSelectOption value="10">10 times</NativeSelectOption>
          <NativeSelectOption value="">Until it expires</NativeSelectOption>
        </NativeSelect>
      </Field>
      <Field>
        <FieldLabel htmlFor="link-password">Password (optional)</FieldLabel>
        <Input id="link-password" name="password" type="password" autoComplete="off" />
        <FieldDescription>Tell it to them another way; without it the link doesn't open.</FieldDescription>
      </Field>
    </FieldGroup>
  )
}

/** "Shared with you" on an item that isn't the person's own. */
export function AccessNote({ access }: { access: Access }) {
  return (
    <p className="flex items-center gap-2 text-sm">
      <Badge variant="secondary">
        <Icon name="group" data-icon="inline-start" />
        Shared with you
      </Badge>
      <span className="text-muted-foreground">{RIGHT_WORDS[access]}</span>
    </p>
  )
}
