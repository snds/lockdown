import { useCallback, useEffect, useState, type FormEvent } from "react"

import {
  Badge,
  Button,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  Field,
  FieldDescription,
  FieldError,
  FieldLabel,
  Icon,
  Input,
  Item,
  ItemActions,
  ItemContent,
  ItemDescription,
  ItemGroup,
  ItemTitle,
  NativeSelect,
  NativeSelectOption,
  Spinner,
} from "@unicron/ui"

import { useGate } from "@/app/gate"
import { LabelledAction, LabelledActions } from "@/components/labelled-actions"
import { describeError } from "@/core/client"
import type { Access, VaultView } from "@/core/types"
import { whenLocal } from "@/lib/format"

import { RIGHT_NAMES, useSharingClient, type ContactView, type HouseholdView, type MemberView, type VaultSharingView } from "./sharing-client"

const RIGHTS: Access[] = ["fill-only", "view", "edit", "manage"]

interface VaultsSectionProps {
  vaults: VaultView[]
  contacts: ContactView[]
  online: boolean
  onChanged: () => void
  onNotice: (text: string) => void
}

/** The person's vaults and those shared with them, each with who has it and Manage sharing. */
export function VaultsSection({ vaults, contacts, online, onChanged, onNotice }: VaultsSectionProps) {
  const [open, setOpen] = useState<VaultView | null>(null)
  const shareable = vaults.filter((v) => !v.sharing || v.sharing.kind === "shared" || v.sharing.kind === "family")
  return (
    <section aria-labelledby="sharing-vaults" className="flex flex-col gap-2">
      <h2 id="sharing-vaults" className="text-base font-semibold">
        Vaults
      </h2>
      <ItemGroup role="list" aria-label="Vaults">
        {shareable.map((v) => (
          <Item key={v.id} variant="outline" size="sm" role="listitem">
            <ItemContent>
              <ItemTitle>
                {v.name}
                {v.sharing && <Badge variant="secondary">{v.sharing.kind === "family" ? "Household" : "Shared"}</Badge>}
              </ItemTitle>
              <ItemDescription>{vaultWords(v)}</ItemDescription>
            </ItemContent>
            <ItemActions>
              <Button variant="outline" size="sm" disabled={!online} onClick={() => setOpen(v)}>
                <Icon name="group" data-icon="inline-start" />
                {v.sharing && !v.sharing.owned ? "Who has it" : "Manage sharing"}
              </Button>
            </ItemActions>
          </Item>
        ))}
      </ItemGroup>
      {open && (
        <VaultSharingDialog
          vault={open}
          contacts={contacts}
          onClose={() => {
            setOpen(null)
            onChanged()
          }}
          onNotice={onNotice}
        />
      )}
    </section>
  )
}

function vaultWords(v: VaultView): string {
  const s = v.sharing
  if (!s) return `Only you · ${v.items === 1 ? "1 item" : `${v.items} items`}`
  const people = s.people === 1 ? "1 person" : `${s.people} people`
  if (s.owned) return `Yours, shared with ${people}`
  return `${s.owner ?? "Someone"}'s · you can ${s.right === "fill-only" ? "fill only" : s.right} · ${people}`
}

interface VaultSharingDialogProps {
  vault: VaultView
  contacts: ContactView[]
  onClose: () => void
  onNotice: (text: string) => void
}

/**
 * One vault's members and their rights (docs/plan/18-sharing.md §4).
 * Adding someone hands them the vault's key, wrapped to the key their
 * record in the log names, through the `Approve` gate. Removing someone
 * moves the vault to a new key at once and hands it to everyone who
 * stays. Only a manager sees the controls; the server refuses the rest
 * anyway. Every change is a signed line in the history.
 */
export function VaultSharingDialog({ vault, contacts, onClose, onNotice }: VaultSharingDialogProps) {
  const sharing = useSharingClient()
  const gate = useGate()
  const [view, setView] = useState<VaultSharingView | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const [removing, setRemoving] = useState<MemberView | null>(null)

  useEffect(() => {
    let live = true
    sharing
      .vault(vault.id)
      .then((v) => live && setView(v))
      .catch((e) => live && setError(describeError(e)))
    return () => {
      live = false
    }
  }, [sharing, vault.id])

  const act = useCallback(async (call: () => Promise<VaultSharingView>, words: string) => {
    setError(null)
    setBusy(true)
    try {
      setView(await call())
      onNotice(words)
    } catch (e) {
      setError(describeError(e))
    } finally {
      setBusy(false)
    }
  }, [onNotice])

  const add = (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    const data = new FormData(e.currentTarget)
    const login = String(data.get("who") ?? "")
    const right = String(data.get("right") ?? "view") as Access
    if (!login) return
    const family = view?.kind === "family"
    void act(
      () => gate.run((g) => (family ? sharing.addToHousehold(vault.id, login, right, g) : sharing.shareVault(vault.id, login, right, g))),
      `${login} can open "${vault.name}" now (${RIGHT_NAMES[right].toLowerCase()}).`,
    )
  }
  const members = view?.members ?? []
  const candidates = contacts.filter((c) => !members.some((m) => m.account_id === c.account_id))

  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Sharing "{vault.name}"</DialogTitle>
          <DialogDescription>
            Everyone here holds the vault's key. What each can do is checked on their device and refused by the server: fill only and view
            can't change anything.
          </DialogDescription>
        </DialogHeader>
        {!view && !error && <Spinner />}
        {view && (
          <div className="flex flex-col gap-3">
            {view.blocked && (
              <p role="note" className="text-sm text-attention-text">
                {view.blocked}
              </p>
            )}
            {view.shared && (
              <ItemGroup role="list" aria-label="Members">
                {members.map((m) => (
                  <Item key={m.account_id} variant="outline" size="sm" role="listitem">
                    <ItemContent>
                      <ItemTitle>
                        {m.login}
                        {m.you && <Badge variant="outline">You</Badge>}
                        {m.owner && <Badge variant="secondary">Owner</Badge>}
                      </ItemTitle>
                      <ItemDescription>{m.right_words}</ItemDescription>
                    </ItemContent>
                    {view.can_manage && !m.owner && !m.you && (
                      <ItemActions>
                        <label htmlFor={`right-${m.account_id}`} className="sr-only">
                          What {m.login} can do
                        </label>
                        <NativeSelect
                          id={`right-${m.account_id}`}
                          value={m.right}
                          disabled={busy}
                          onChange={(e) => {
                            const right = e.target.value as Access
                            void act(() => sharing.setRight(vault.id, m.account_id, right), `${m.login}: ${RIGHT_NAMES[right].toLowerCase()}.`)
                          }}
                        >
                          {RIGHTS.map((r) => (
                            <NativeSelectOption key={r} value={r}>
                              {RIGHT_NAMES[r]}
                            </NativeSelectOption>
                          ))}
                        </NativeSelect>
                        <Button variant="ghost-destructive" size="sm" disabled={busy} onClick={() => setRemoving(m)}>
                          Remove
                        </Button>
                      </ItemActions>
                    )}
                  </Item>
                ))}
              </ItemGroup>
            )}
            {(!view.shared || view.can_manage) && !view.blocked && (
              <form id="vault-add" className="flex flex-wrap items-end gap-2" onSubmit={add}>
                {candidates.length === 0 ? (
                  <p className="text-sm text-muted-foreground">Look someone up under People first, so their keys are checked.</p>
                ) : (
                  <>
                    <Field>
                      <FieldLabel htmlFor="vault-add-who">Add</FieldLabel>
                      <NativeSelect id="vault-add-who" name="who" defaultValue={candidates[0]?.login}>
                        {candidates.map((c) => (
                          <NativeSelectOption key={c.account_id} value={c.login}>
                            {c.login}
                            {c.verified ? "" : " (fingerprint not compared)"}
                          </NativeSelectOption>
                        ))}
                      </NativeSelect>
                    </Field>
                    <Field>
                      <FieldLabel htmlFor="vault-add-right">Who can</FieldLabel>
                      <NativeSelect id="vault-add-right" name="right" defaultValue="view">
                        {RIGHTS.map((r) => (
                          <NativeSelectOption key={r} value={r}>
                            {RIGHT_NAMES[r]}
                          </NativeSelectOption>
                        ))}
                      </NativeSelect>
                    </Field>
                    <Button type="submit" size="sm" disabled={busy}>
                      <Icon name="person_add" data-icon="inline-start" />
                      Add
                    </Button>
                  </>
                )}
              </form>
            )}
            {view.history.length > 0 && (
              <section aria-label="History" className="flex flex-col gap-1">
                <h3 className="text-xs font-medium text-muted-foreground uppercase">History</h3>
                <ol className="flex flex-col gap-0.5 text-sm">
                  {view.history.map((h, i) => (
                    <li key={i}>
                      <span className="text-muted-foreground">{whenLocal(h.at)}</span> · {h.words}
                    </li>
                  ))}
                </ol>
              </section>
            )}
          </div>
        )}
        {error && <FieldError role="alert">{error}</FieldError>}
        <LabelledActions>
          <LabelledAction kind="cancel" onClick={onClose}>
            Done
          </LabelledAction>
        </LabelledActions>
      </DialogContent>
      <Dialog open={removing !== null} onOpenChange={(o) => !o && setRemoving(null)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Remove {removing?.login}?</DialogTitle>
            <DialogDescription>
              "{vault.name}" moves to a new key now and everyone else gets it; {removing?.login} can't open anything changed from here on.
              What they already saw, they may have kept.
            </DialogDescription>
          </DialogHeader>
          <LabelledActions>
            <LabelledAction kind="cancel" onClick={() => setRemoving(null)}>
              Cancel
            </LabelledAction>
            <LabelledAction
              kind="destructive"
              onClick={() => {
                const m = removing
                setRemoving(null)
                if (m) void act(() => sharing.removeMember(vault.id, m.account_id), `Removed ${m.login}; "${vault.name}" is on a new key.`)
              }}
            >
              Remove and change the key
            </LabelledAction>
          </LabelledActions>
        </DialogContent>
      </Dialog>
    </Dialog>
  )
}

interface HouseholdsSectionProps {
  households: HouseholdView[]
  vaults: VaultView[]
  contacts: ContactView[]
  online: boolean
  onChanged: () => void
  onNotice: (text: string) => void
}

/**
 * A household (docs/plan/18-sharing.md §6): one shared vault whose
 * organizer manages it, for the people who live together. There is no
 * billing in Lockdown; a household is only the vault and its members.
 */
export function HouseholdsSection({ households, vaults, contacts, online, onChanged, onNotice }: HouseholdsSectionProps) {
  const sharing = useSharingClient()
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const [open, setOpen] = useState<VaultView | null>(null)

  const create = async (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    const form = e.currentTarget
    const name = String(new FormData(form).get("name") ?? "").trim()
    if (!name) return
    setError(null)
    setBusy(true)
    try {
      await sharing.createHousehold(name)
      form.reset()
      onNotice(`"${name}" is ready. Add the people in your household to it.`)
      onChanged()
    } catch (err) {
      setError(describeError(err))
    } finally {
      setBusy(false)
    }
  }
  return (
    <section aria-labelledby="sharing-households" className="flex flex-col gap-2">
      <h2 id="sharing-households" className="text-base font-semibold">
        Household
      </h2>
      {households.length > 0 && (
        <ItemGroup role="list" aria-label="Households">
          {households.map((h) => {
            const vault = vaults.find((v) => v.id === h.vault_id)
            return (
              <Item key={h.vault_id} variant="outline" size="sm" role="listitem">
                <ItemContent>
                  <ItemTitle>{h.name}</ItemTitle>
                  <ItemDescription>
                    {h.organizing ? "You organize it" : `Organized by ${h.organizer}`} · {h.members.map((m) => m.login).join(", ")}
                  </ItemDescription>
                </ItemContent>
                {vault && (
                  <ItemActions>
                    <Button variant="outline" size="sm" disabled={!online} onClick={() => setOpen(vault)}>
                      {h.organizing ? "Manage" : "Who's in it"}
                    </Button>
                  </ItemActions>
                )}
              </Item>
            )
          })}
        </ItemGroup>
      )}
      {households.every((h) => !h.organizing) && (
        <form className="flex flex-wrap items-end gap-2" onSubmit={(e) => void create(e)}>
          <Field>
            <FieldLabel htmlFor="household-name">Start a household</FieldLabel>
            <Input id="household-name" name="name" autoComplete="off" placeholder="Home" disabled={!online} />
            <FieldDescription>A shared vault you organize. Billing isn't part of Lockdown.</FieldDescription>
          </Field>
          <Button type="submit" variant="outline" size="sm" disabled={!online || busy}>
            <Icon name="home" data-icon="inline-start" />
            Create
          </Button>
        </form>
      )}
      {error && <FieldError role="alert">{error}</FieldError>}
      {open && (
        <VaultSharingDialog
          vault={open}
          contacts={contacts}
          onClose={() => {
            setOpen(null)
            onChanged()
          }}
          onNotice={onNotice}
        />
      )}
    </section>
  )
}
