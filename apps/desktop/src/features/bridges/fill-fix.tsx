import { useEffect, useState } from "react"

import {
  Button,
  Field,
  FieldDescription,
  FieldLabel,
  Icon,
  Input,
  NativeSelect,
  NativeSelectOption,
  Switch,
} from "@unicron/ui"

import { describeError } from "@/core/client"

import { useBridges, type FieldRole, type FillFix } from "./bridges-client"
import { ROLE_WORDS } from "./bridges-model"

/**
 * "Filling on this site" on a Login (docs/plan/23-bridges.md §5): never
 * fill here, and fields the site's page names wrongly, by the field's
 * name. Kept on the login, so it syncs. Hidden without a bridges client
 * or for a protected item (never filled).
 */
export function FillFixPanel({ itemId, crownJewel, canEdit, onNotice }: { itemId: string; crownJewel: boolean; canEdit: boolean; onNotice: (t: string) => void }) {
  const bridges = useBridges()
  const [fix, setFix] = useState<FillFix | null>(null)
  const [name, setName] = useState("")
  const [role, setRole] = useState<FieldRole>("username")
  const [busy, setBusy] = useState(false)

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

  const save = async (next: FillFix, notice: string) => {
    setBusy(true)
    try {
      setFix(await bridges.siteFixSet(itemId, next))
      onNotice(notice)
    } catch (e) {
      onNotice(describeError(e))
    } finally {
      setBusy(false)
    }
  }
  const nameOf = (m: FillFix["fields"][number]["match"]) => m.name ?? m.id ?? m.autocomplete ?? ""
  return (
    <section aria-labelledby={`fill-fix-${itemId}`} className="flex flex-col gap-2">
      <h3 id={`fill-fix-${itemId}`} className="text-sm font-semibold">
        Filling on this site
      </h3>
      <Field orientation="horizontal">
        <FieldLabel htmlFor={`never-${itemId}`}>Never fill on this site</FieldLabel>
        <Switch
          id={`never-${itemId}`}
          checked={fix.never}
          disabled={busy || !canEdit}
          onCheckedChange={(on) => void save({ ...fix, never: on }, on ? "Lockdown won't offer to fill on this site." : "Lockdown fills here again.")}
        />
      </Field>
      {fix.fields.length > 0 && (
        <ul aria-label="Fields fixed" className="flex flex-col gap-1 text-sm">
          {fix.fields.map((f) => (
            <li key={nameOf(f.match)} className="flex items-center gap-2">
              <span className="flex-1">
                The field "{nameOf(f.match)}" is {ROLE_WORDS[f.role]}
              </span>
              <Button
                size="sm"
                variant="ghost"
                disabled={busy || !canEdit}
                onClick={() => void save({ ...fix, fields: fix.fields.filter((x) => x !== f) }, "Fix removed.")}
                aria-label={`Remove the fix for ${nameOf(f.match)}`}
              >
                <Icon name="close" />
              </Button>
            </li>
          ))}
        </ul>
      )}
      {canEdit && (
        <div className="flex flex-wrap items-end gap-2">
          <Field>
            <FieldLabel htmlFor={`fix-name-${itemId}`}>Field name on the page</FieldLabel>
            <Input id={`fix-name-${itemId}`} value={name} onChange={(e) => setName(e.target.value)} placeholder="login_id" autoComplete="off" />
          </Field>
          <Field>
            <FieldLabel htmlFor={`fix-role-${itemId}`}>Is</FieldLabel>
            <NativeSelect id={`fix-role-${itemId}`} value={role} onChange={(e) => setRole(e.target.value as FieldRole)}>
              {(Object.keys(ROLE_WORDS) as FieldRole[]).map((r) => (
                <NativeSelectOption key={r} value={r}>
                  {ROLE_WORDS[r]}
                </NativeSelectOption>
              ))}
            </NativeSelect>
          </Field>
          <Button
            variant="outline"
            size="sm"
            disabled={busy || !name.trim()}
            onClick={() => {
              const next = { ...fix, fields: [...fix.fields, { match: { name: name.trim() }, role }] }
              setName("")
              void save(next, "Saved. The browser extension uses it on this site.")
            }}
          >
            Add fix
          </Button>
        </div>
      )}
      <FieldDescription>The browser extension's popup can set these too, from the fields it finds on the page.</FieldDescription>
    </section>
  )
}
