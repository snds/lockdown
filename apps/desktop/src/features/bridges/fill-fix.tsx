import { useEffect, useRef, useState } from "react"

import {
  AdaptiveDialog,
  Button,
  Field,
  FieldLabel,
  FieldRow,
  FieldRowAction,
  ListRow,
  NativeSelect,
  NativeSelectOption,
  SectionHeader,
  TextField,
  ToggleRow,
} from "@unicron/ui"

import { describeError } from "@/core/client"

import { useBridges, type FieldRole, type FillFix } from "./bridges-client"
import { ROLE_WORDS } from "./bridges-model"

interface FillFixPanelProps {
  itemId: string
  crownJewel: boolean
  canEdit: boolean
  /** Kept for the callers that pass it: the row is the answer, so nothing toasts. */
  onNotice?: (t: string) => void
  /** The login's first website's host, for the Form's title ("Filling on store.example.com"). */
  host?: string | null
  /** Field Row's layout: `stacked` on a phone and in a pane under 480 px (brief 4.0). */
  size?: "default" | "stacked"
}

const nameOf = (m: FillFix["fields"][number]["match"]) => m.name ?? m.id ?? m.autocomplete ?? ""

/** What the row says (brief I4): Lockdown decides, never fills here, or how many fields are fixed. */
function fillFixWords(fix: FillFix): string {
  if (fix.never) return "Never fills here"
  const n = fix.fields.length
  if (n === 0) return "Lockdown decides"
  return n === 1 ? "1 field fixed" : `${n} fields fixed`
}

/**
 * FILLING ON THIS SITE on a Login (docs/plan/23-bridges.md §5, brief I4,
 * review R9): one Field Row saying what is set, and CHANGE opening a small
 * Form: never fill here, and the fields the site's page names wrongly, by
 * the field's name, each with REMOVE, then a name, what it is, and ADD
 * FIX. Kept on the login, so it syncs. Hidden without a bridges client
 * (the web vault) or for a crown jewel (never filled). Each change saves at
 * once; the row is the answer, so nothing toasts; a failure stays in the
 * Form in the core's words.
 */
export function FillFixPanel({ itemId, crownJewel, canEdit, host, size = "default" }: FillFixPanelProps) {
  const bridges = useBridges()
  const [fix, setFix] = useState<FillFix | null>(null)
  const [open, setOpen] = useState(false)
  const [name, setName] = useState("")
  const [role, setRole] = useState<FieldRole>("username")
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const nameRef = useRef<HTMLInputElement | null>(null)

  useEffect(() => {
    if (!bridges || crownJewel) return
    let live = true
    bridges
      .siteFix(itemId)
      .then((f) => live && setFix(f))
      .catch(() => live && setFix(null))
    return () => {
      live = false
    }
  }, [bridges, itemId, crownJewel])
  if (!bridges || crownJewel || !fix) return null

  const save = async (next: FillFix) => {
    setBusy(true)
    setError(null)
    try {
      setFix(await bridges.siteFixSet(itemId, next))
      return true
    } catch (e) {
      setError(describeError(e))
      return false
    } finally {
      setBusy(false)
    }
  }
  const add = async () => {
    const typed = name.trim()
    if (!typed) return
    if (await save({ ...fix, fields: [...fix.fields, { match: { name: typed }, role }] })) setName("")
  }
  const where = host ? `Filling on ${host}` : "Filling on this site"
  return (
    <>
      <FieldRow size={size} label="Filling on this site" action={canEdit ? <FieldRowAction onClick={() => setOpen(true)}>Change</FieldRowAction> : undefined}>
        {fillFixWords(fix)}
      </FieldRow>
      <AdaptiveDialog
        open={open}
        onOpenChange={(o) => !busy && setOpen(o)}
        // Focus starts in the field a fix is typed into: the Form's usual job.
        initialFocus={nameRef}
        title={where}
        description="The browser extension's popup can set these too, from the fields it finds on the page."
        footer={
          <Button type="button" onClick={() => setOpen(false)} disabled={busy}>
            Done
          </Button>
        }
      >
        <div className="flex flex-col gap-6">
          <ToggleRow label="Never fill on this site" description="Lockdown won't offer this login on its pages." checked={fix.never} disabled={busy} rule="none" onCheckedChange={(on) => void save({ ...fix, never: on })} />
          <section aria-label="Fields fixed" className="flex flex-col">
            <SectionHeader title="Fields fixed" count={fix.fields.length} level={3} />
            {fix.fields.length > 0 && (
              <div role="list" aria-label="Fields fixed">
                {fix.fields.map((f) => (
                  <ListRow
                    key={nameOf(f.match)}
                    role="listitem"
                    size="compact"
                    title={`The field "${nameOf(f.match)}" is ${ROLE_WORDS[f.role]}`}
                    action={
                      <Button type="button" variant="ghost" size="sm" disabled={busy} aria-label={`Remove the fix for ${nameOf(f.match)}`} onClick={() => void save({ ...fix, fields: fix.fields.filter((x) => x !== f) })}>
                        Remove
                      </Button>
                    }
                  />
                ))}
              </div>
            )}
          </section>
          <form
            className="flex flex-col gap-4"
            onSubmit={(e) => {
              e.preventDefault()
              void add()
            }}
          >
            <TextField ref={nameRef} label="Field name on the page" value={name} onChange={(e) => setName(e.target.value)} placeholder="login_id" autoComplete="off" spellCheck={false} />
            <Field>
              <FieldLabel htmlFor={`fix-role-${itemId}`}>What it is</FieldLabel>
              <NativeSelect id={`fix-role-${itemId}`} value={role} onChange={(e) => setRole(e.target.value as FieldRole)}>
                {(Object.keys(ROLE_WORDS) as FieldRole[]).map((r) => (
                  <NativeSelectOption key={r} value={r}>
                    {ROLE_WORDS[r].replace(/^./, (c) => c.toUpperCase())}
                  </NativeSelectOption>
                ))}
              </NativeSelect>
            </Field>
            {error && (
              <p role="alert" className="type-caption text-critical-text">
                {error}
              </p>
            )}
            <div>
              <Button type="submit" variant="secondary" disabled={busy || !name.trim()} loading={busy}>
                Add fix
              </Button>
            </div>
          </form>
        </div>
      </AdaptiveDialog>
    </>
  )
}
