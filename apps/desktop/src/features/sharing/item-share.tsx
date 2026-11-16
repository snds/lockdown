import { useEffect, useState, type FormEvent } from "react"

import {
  AdaptiveDialog,
  Badge,
  Button,
  ConfirmDialog,
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
  EmptyState,
  Field,
  FieldLabel,
  FieldLegend,
  FieldSet,
  FloatButton,
  Icon,
  ListRow,
  NativeSelect,
  NativeSelectOption,
  RadioGroup,
  RadioRow,
} from "@unicron/ui"

import { useGate } from "@/app/gate"
import { usePlaces } from "@/app/places"
import { describeError } from "@/core/client"
import { decodeAndWipe } from "@/core/secret-bytes"
import type { Access, ItemDetail } from "@/core/types"
import { count } from "@/lib/format"

import { LinkFields, LinkLayer, linkOptions } from "./link-once"
import { ShareLayer } from "./share-layer"
import { RIGHT_WORDS, useSharingClient, utf8, type ContactView } from "./sharing-client"

interface ItemShareProps {
  item: ItemDetail
  title: string
  onNotice: (text: string) => void
  /**
   * Opened from somewhere else (the item's ⋯ → Share…): with these, ItemShare
   * draws no button of its own and shows its two rows, "With someone…" and
   * "As a link…", while `open` is true.
   */
  open?: boolean
  onOpenChange?: (open: boolean) => void
}

type Open = "person" | "link" | null

/** What a fill-only share says in place of every hidden value (brief I7, D187). */
export const FILL_ONLY_WORDS = "Fills in the browser only; never shown or copied."

/** Whether an item may be shared from here: the person's own, never a crown jewel, never in the bin (brief I7). */
export const canShare = (item: ItemDetail) => !item.access && !item.crown_jewel && item.state !== "trashed"

/**
 * Share one item (brief I7, doc 91): with someone pinned here, to view or
 * to fill only, or as a link anyone with it can open a copy through. Only
 * the person's own items offer it, never a crown jewel, never in the web
 * vault. Both Forms are full screens on a phone, and their yes goes
 * through the gate. A link is shown once, with COPY LINK as the layer's
 * one primary.
 */
export function ItemShare({ item, title, onNotice, open, onOpenChange }: ItemShareProps) {
  const [form, setForm] = useState<Open>(null)
  const controlled = open !== undefined
  if (!canShare(item)) return null
  const start = (f: Exclude<Open, null>) => {
    onOpenChange?.(false)
    setForm(f)
  }
  return (
    <>
      {controlled ? (
        <AdaptiveDialog
          open={open}
          onOpenChange={(o) => onOpenChange?.(o)}
          size="question"
          overline={`${title} // share`}
          title={`Share ${title}`}
          footer={
            <Button type="button" variant="ghost" onClick={() => onOpenChange?.(false)}>
              Cancel
            </Button>
          }
        >
          <div role="list" aria-label={`Ways to share ${title}`} className="flex flex-col">
            <ListRow role="listitem" title="With someone…" meta="Only this item, locked to their key" metaStyle="label" onClick={() => start("person")} />
            <ListRow role="listitem" title="As a link…" meta="Anyone with it can open a copy" metaStyle="label" onClick={() => start("link")} />
          </div>
        </AdaptiveDialog>
      ) : (
        <DropdownMenu>
          <DropdownMenuTrigger render={<Button variant="secondary" size="sm" />}>
            <Icon name="share" data-icon="inline-start" />
            Share
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            <DropdownMenuItem onClick={() => setForm("person")}>
              <Icon name="user--follow" data-icon="inline-start" />
              With someone…
            </DropdownMenuItem>
            <DropdownMenuItem onClick={() => setForm("link")}>
              <Icon name="link" data-icon="inline-start" />
              As a link…
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      )}
      <SharePerson open={form === "person"} item={item} title={title} onClose={() => setForm(null)} onNotice={onNotice} />
      <ShareLink open={form === "link"} item={item} title={title} onClose={() => setForm(null)} />
    </>
  )
}

/** "bob@example.com // not compared" in the With select (brief I7). */
const withWords = (c: ContactView) => (c.verified ? c.login : `${c.login} // not compared`)

/**
 * With someone (brief I7), a Form: With (the people pinned on this
 * device), They can (View, or Fill only), the caption, and SHARE through
 * the gate ("Share" extended on a phone). Nobody pinned: an Empty State
 * with OPEN PEOPLE.
 */
function SharePerson({ open, item, title, onClose, onNotice }: { open: boolean; item: ItemDetail; title: string; onClose: () => void; onNotice: (t: string) => void }) {
  const sharing = useSharingClient()
  const gate = useGate()
  const { go } = usePlaces()
  const [contacts, setContacts] = useState<ContactView[] | null>(null)
  const [right, setRight] = useState<"view" | "fill-only">("view")
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  useEffect(() => {
    if (!open) return
    let live = true
    sharing
      .overview()
      .then((o) => live && setContacts(o.contacts))
      .catch((e) => live && setError(describeError(e)))
    return () => {
      live = false
    }
  }, [open, sharing])

  const close = () => {
    setError(null)
    setRight("view")
    onClose()
  }
  const share = async (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    const who = String(new FormData(e.currentTarget).get("who") ?? "")
    if (!who) return
    setError(null)
    setBusy(true)
    try {
      await gate.run((g) => sharing.shareItem(item.id, who, right, g))
      onNotice(`Shared ${title} with ${who}`)
      close()
    } catch (err) {
      setError(describeError(err))
    } finally {
      setBusy(false)
    }
  }
  const nobody = contacts !== null && contacts.length === 0
  return (
    <ShareLayer
      open={open}
      onClose={close}
      overline={`${title} // share`}
      title={`Share ${title}`}
      footer={
        <>
          <Button type="button" variant="ghost" onClick={close}>
            Cancel
          </Button>
          {!nobody && (
            <Button type="submit" form="share-person" disabled={contacts === null} loading={busy}>
              Share
            </Button>
          )}
        </>
      }
      phoneAction={nobody ? undefined : <FloatButton label="Share" icon="share" kind="primary" size="l" extended type="submit" form="share-person" disabled={contacts === null || busy} />}
    >
      {nobody ? (
        <EmptyState
          size="sm"
          overline={`People // ${count(0)}`}
          title="Look someone up first."
          body="Only people whose keys checked out can get an item."
          actionLabel="Open people"
          onAction={() => {
            close()
            go("sharing-people")
          }}
        />
      ) : (
        <form id="share-person" aria-label={`Share ${title}`} className="flex flex-col gap-4" onSubmit={(e) => void share(e)}>
          <Field>
            <FieldLabel htmlFor="share-who">With</FieldLabel>
            <NativeSelect id="share-who" name="who" key={contacts?.length ?? 0} defaultValue={contacts?.[0]?.login}>
              {(contacts ?? []).map((c) => (
                <NativeSelectOption key={c.account_id} value={c.login}>
                  {withWords(c)}
                </NativeSelectOption>
              ))}
            </NativeSelect>
          </Field>
          <FieldSet>
            <FieldLegend variant="label">They can</FieldLegend>
            <RadioGroup aria-label="They can" value={right} onValueChange={(v) => setRight(v as "view" | "fill-only")}>
              <RadioRow value="view" label="View" description="See, copy and fill" />
              <RadioRow value="fill-only" label="Fill only" description="Lockdown fills it in the browser. It is never shown or copied." />
            </RadioGroup>
          </FieldSet>
          <p className="type-caption text-text-secondary">Only this item goes, locked to their key. You can stop at any time.</p>
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

/** As a link (brief I7): Lasts, Opens at most, a password; CREATE LINK through the gate; then the link once. */
function ShareLink({ open, item, title, onClose }: { open: boolean; item: ItemDetail; title: string; onClose: () => void }) {
  const sharing = useSharingClient()
  const gate = useGate()
  const [password, setPassword] = useState("")
  return (
    <LinkLayer
      open={open}
      onClose={onClose}
      overline={`${title} // link`}
      title={`A link to ${title}`}
      formId="share-link"
      caption="Anyone with the link can open a copy of this item as it is now."
      onReset={() => setPassword("")}
      onMake={async (form) => {
        const { days, views } = linkOptions(form)
        const pw = password ? utf8(password) : null
        return decodeAndWipe(await gate.run((g) => sharing.sendItem(item.id, days, views, pw, g)))
      }}
    >
      <LinkFields idPrefix="link" password={password} onPassword={setPassword} />
    </LinkLayer>
  )
}

/**
 * Stopping one item's share asks first (brief I7): "Stop sharing with
 * bob@example.com?", the item moves to a new key, STOP SHARING. No gate:
 * taking access away never asks for it (D196).
 */
export function StopSharing({ share, title, onClose, onStop }: {
  /** The share to stop, or null when nothing is asked. */
  share: { share_id: string; login: string } | null
  /** The item's title, for the overline. */
  title: string
  onClose: () => void
  /** Stops it; a refusal shows in the Question. */
  onStop: (shareId: string, login: string) => Promise<void>
}) {
  const [error, setError] = useState<string | null>(null)
  return (
    <ConfirmDialog
      open={share !== null}
      onOpenChange={(o) => {
        if (!o) {
          setError(null)
          onClose()
        }
      }}
      overline={`${title} // shared`}
      title={`Stop sharing with ${share?.login ?? ""}?`}
      description="The item moves to a new key. What they already saw, they may have kept."
      confirmLabel="Stop sharing"
      tone="destructive"
      error={error ?? undefined}
      onConfirm={async () => {
        if (!share) return
        try {
          await onStop(share.share_id, share.login)
        } catch (e) {
          setError(describeError(e))
          throw e
        }
      }}
    />
  )
}

/**
 * "Shared with you" on an item that isn't the person's own, with what the
 * right allows (brief I7). A fill-only item also says FILL_ONLY_WORDS in
 * place of each hidden value; the detail pane draws that.
 */
export function AccessNote({ access }: { access: Access }) {
  return (
    <p className="type-body flex flex-wrap items-center gap-2">
      <Badge>Shared with you</Badge>
      <span className="text-text-secondary">{RIGHT_WORDS[access]}</span>
    </p>
  )
}
