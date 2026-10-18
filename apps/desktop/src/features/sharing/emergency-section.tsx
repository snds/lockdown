import { useState, type FormEvent } from "react"

import {
  Badge,
  Button,
  Checkbox,
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
  FieldLabel,
  FieldSet,
  FieldLegend,
  Icon,
  Item,
  ItemActions,
  ItemContent,
  ItemDescription,
  ItemGroup,
  ItemTitle,
  NativeSelect,
  NativeSelectOption,
} from "@unicron/ui"

import { useGate } from "@/app/gate"
import { describeError } from "@/core/client"
import type { VaultView } from "@/core/types"
import { whenLocal } from "@/lib/format"

import { emergencyActions, emergencyWords, useSharingClient, type ContactView, type EmergencyAction, type EmergencyView } from "./sharing-client"

interface EmergencySectionProps {
  grants: EmergencyView[]
  contacts: ContactView[]
  vaults: VaultView[]
  online: boolean
  onGrants: (grants: EmergencyView[]) => void
  onChanged: () => void
  onNotice: (text: string) => void
}

const WAITS = [1, 2, 3, 7, 14, 30]

/**
 * Emergency access (docs/plan/18-sharing.md §6): someone you trust, whose
 * keys checked out against the log and whose fingerprint you compared,
 * can ask for your vaults; you have the waiting period to stop it, and
 * only after it runs out does the server hand over the keys you sealed to
 * them. Every step is signed and listed.
 */
export function EmergencySection({ grants, contacts, vaults, online, onGrants, onChanged, onNotice }: EmergencySectionProps) {
  const sharing = useSharingClient()
  const [adding, setAdding] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const [revoking, setRevoking] = useState<EmergencyView | null>(null)

  const step = async (g: EmergencyView, action: EmergencyAction | "open") => {
    setError(null)
    setBusy(true)
    try {
      if (action === "open") {
        onGrants(await sharing.emergencyOpen(g.grant_id))
        onNotice(`${g.other}'s vaults are open to read, in the sidebar.`)
        onChanged()
      } else {
        onGrants(await sharing.emergencyStep(g.grant_id, action))
        onNotice(action === "stop" ? `Stopped. ${g.other} can ask again later.` : "Done.")
      }
    } catch (e) {
      setError(describeError(e))
    } finally {
      setBusy(false)
    }
  }
  const mine = grants.filter((g) => g.mine)
  const theirs = grants.filter((g) => !g.mine)

  return (
    <section aria-labelledby="sharing-emergency" className="flex flex-col gap-2">
      <h2 id="sharing-emergency" className="text-base font-semibold">
        Emergency access
      </h2>
      <p className="text-sm text-muted-foreground">
        Someone you trust can ask for your vaults if something happens to you. You get the waiting period to stop it; nothing opens before it
        ends.
      </p>
      <div>
        <Button variant="outline" size="sm" disabled={!online} onClick={() => setAdding(true)}>
          <Icon name="emergency" data-icon="inline-start" />
          Add an emergency contact…
        </Button>
      </div>
      {mine.length > 0 && <GrantList label="Your emergency contacts" grants={mine} busy={busy} onStep={step} onRevoke={setRevoking} />}
      {theirs.length > 0 && <GrantList label="People who trust you" grants={theirs} busy={busy} onStep={step} onRevoke={setRevoking} />}
      {error && <FieldError role="alert">{error}</FieldError>}
      {adding && (
        <AddEmergencyDialog
          contacts={contacts}
          vaults={vaults}
          onClose={() => setAdding(false)}
          onAdded={(list, login) => {
            setAdding(false)
            onGrants(list)
            onNotice(`Asked ${login} to be your emergency contact.`)
          }}
        />
      )}
      <Dialog open={revoking !== null} onOpenChange={(o) => !o && setRevoking(null)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>End emergency access for {revoking?.other}?</DialogTitle>
            <DialogDescription>
              {revoking?.state === "released"
                ? "They already opened your vaults; ending it stops new access, and you should change what matters."
                : "The keys you sealed for them are dropped and they can't ask again."}
            </DialogDescription>
          </DialogHeader>
          <DialogActions>
            <DialogAction kind="cancel" onClick={() => setRevoking(null)}>
              Cancel
            </DialogAction>
            <DialogAction
              kind="destructive"
              onClick={() => {
                const g = revoking
                setRevoking(null)
                if (g) void step(g, "revoke")
              }}
            >
              End it
            </DialogAction>
          </DialogActions>
        </DialogContent>
      </Dialog>
    </section>
  )
}

function GrantList({
  label,
  grants,
  busy,
  onStep,
  onRevoke,
}: {
  label: string
  grants: EmergencyView[]
  busy: boolean
  onStep: (g: EmergencyView, a: EmergencyAction | "open") => void
  onRevoke: (g: EmergencyView) => void
}) {
  return (
    <ItemGroup role="list" aria-label={label}>
      {grants.map((g) => (
        <Item key={g.grant_id} variant="outline" size="sm" role="listitem">
          <ItemContent>
            <ItemTitle>
              {g.other}
              {g.mine && g.state === "requested" && <Badge variant="destructive">Asked for access</Badge>}
            </ItemTitle>
            <ItemDescription>
              {emergencyWords(g)}
              {g.release_at && g.state === "requested" ? ` It opens ${whenLocal(g.release_at)}.` : ""} Wait: {g.wait_days}{" "}
              {g.wait_days === 1 ? "day" : "days"}.
            </ItemDescription>
            {g.history.length > 0 && (
              <ol aria-label={`What happened with ${g.other}`} className="mt-1 flex flex-col gap-0.5 text-xs text-muted-foreground">
                {g.history.map((h, i) => (
                  <li key={i}>
                    {whenLocal(h.at)} · {h.words}
                  </li>
                ))}
              </ol>
            )}
          </ItemContent>
          <ItemActions>
            {emergencyActions(g).map((a) => (
              <Button
                key={a.action}
                size="sm"
                variant={a.destructive ? "ghost-destructive" : "outline"}
                disabled={busy}
                onClick={() => (a.action === "revoke" ? onRevoke(g) : onStep(g, a.action))}
              >
                {a.label}
              </Button>
            ))}
          </ItemActions>
        </Item>
      ))}
    </ItemGroup>
  )
}

function AddEmergencyDialog({
  contacts,
  vaults,
  onClose,
  onAdded,
}: {
  contacts: ContactView[]
  vaults: VaultView[]
  onClose: () => void
  onAdded: (list: EmergencyView[], login: string) => void
}) {
  const sharing = useSharingClient()
  const gate = useGate()
  const verified = contacts.filter((c) => c.verified)
  const own = vaults.filter((v) => !v.sharing || v.sharing.owned)
  const [picked, setPicked] = useState<string[]>(own.filter((v) => v.primary).map((v) => v.id))
  const [error, setError] = useState<string | null>(null)

  const submit = async (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    const data = new FormData(e.currentTarget)
    const login = String(data.get("who") ?? "")
    const days = Number(data.get("wait") ?? 7)
    if (!login || picked.length === 0) return
    setError(null)
    try {
      onAdded(await gate.run((g) => sharing.addEmergency(login, days, picked, g)), login)
    } catch (err) {
      setError(describeError(err))
    }
  }
  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Add an emergency contact</DialogTitle>
          <DialogDescription>
            The vaults you pick are sealed to their key now and held by the server, which hands them over only after they ask and the wait
            runs out without you stopping it.
          </DialogDescription>
        </DialogHeader>
        {verified.length === 0 ? (
          <p className="text-sm text-muted-foreground">
            Only someone whose fingerprint you compared can be an emergency contact. Look them up under People and compare first.
          </p>
        ) : (
          <form id="emergency-add" className="flex flex-col gap-3" onSubmit={(e) => void submit(e)}>
            <Field>
              <FieldLabel htmlFor="emergency-who">Who</FieldLabel>
              <NativeSelect id="emergency-who" name="who" defaultValue={verified[0]?.login}>
                {verified.map((c) => (
                  <NativeSelectOption key={c.account_id} value={c.login}>
                    {c.login}
                  </NativeSelectOption>
                ))}
              </NativeSelect>
            </Field>
            <Field>
              <FieldLabel htmlFor="emergency-wait">Waiting period</FieldLabel>
              <NativeSelect id="emergency-wait" name="wait" defaultValue="7">
                {WAITS.map((d) => (
                  <NativeSelectOption key={d} value={String(d)}>
                    {d === 1 ? "1 day" : `${d} days`}
                  </NativeSelectOption>
                ))}
              </NativeSelect>
              <FieldDescription>How long you have to stop it after they ask.</FieldDescription>
            </Field>
            <FieldSet>
              <FieldLegend variant="label">Which vaults</FieldLegend>
              {own.map((v) => (
                <Field key={v.id} orientation="horizontal">
                  <Checkbox
                    id={`emergency-vault-${v.id}`}
                    checked={picked.includes(v.id)}
                    onCheckedChange={(on) => setPicked((p) => (on === true ? [...p, v.id] : p.filter((x) => x !== v.id)))}
                  />
                  <FieldLabel htmlFor={`emergency-vault-${v.id}`}>{v.name}</FieldLabel>
                </Field>
              ))}
            </FieldSet>
          </form>
        )}
        {error && <FieldError role="alert">{error}</FieldError>}
        <DialogActions>
          <DialogAction kind="cancel" onClick={onClose}>
            Cancel
          </DialogAction>
          {verified.length > 0 && (
            <DialogAction kind="confirm" type="submit" form="emergency-add" disabled={picked.length === 0}>
              Ask them
            </DialogAction>
          )}
        </DialogActions>
      </DialogContent>
    </Dialog>
  )
}
