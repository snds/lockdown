import { useCallback, useEffect, useState, type FormEvent } from "react"

import {
  AdaptiveDialog,
  Badge,
  Banner,
  Button,
  ConfirmDialog,
  DropdownMenuItem,
  Field,
  FieldLabel,
  ListRow,
  LogRow,
  NativeSelect,
  NativeSelectOption,
  RadioGroup,
  RadioRow,
  SectionHeader,
  Skeleton,
  TextField,
  useAppFrame,
} from "@unicron/ui"

import { useGate } from "@/app/gate"
import { describeError } from "@/core/client"
import type { Access, ItemSummary, VaultView } from "@/core/types"
import { count, countName, fullDate, rowDate } from "@/lib/format"

import { StopSharing } from "./item-share"
import { OFFLINE_REASON } from "./people-section"
import { RIGHT_NAMES, RIGHT_WORDS, useSharingClient, type ContactView, type MemberView, type SharingOverview, type VaultSharingView } from "./sharing-client"
import { SharingPanes, paneHeading } from "./sharing-panes"

const RIGHTS: Access[] = ["fill-only", "view", "edit", "manage"]

/** A right as one word, for the right-hand side of a row. */
const RIGHT_WORD: Record<Access, string> = { "fill-only": "Fill only", view: "View", edit: "Edit", manage: "Manage" }

/** D187's sentence for a right, without its name: "see, copy and fill". */
function rightSentence(r: Access): string {
  const words = RIGHT_WORDS[r]
  const at = words.indexOf(": ")
  return at < 0 ? words : words.slice(at + 2)
}

/** The vaults Sharing lists: yours and the ones shared with you, not items received one by one or emergency access. */
export function shareableVaults(vaults: VaultView[]): VaultView[] {
  return vaults.filter((v) => !v.sharing || v.sharing.kind === "shared" || v.sharing.kind === "family")
}

/** A vault's mono line, in today's words: "Only you // 231 items", "Yours // shared with 3 people", "alice's // 3 people". */
function vaultLine(v: VaultView): string {
  const s = v.sharing
  const household = s?.kind === "family" ? " // household" : ""
  if (!s) return `Only you // ${countName(v.items, "item")}`
  const people = countName(s.people, "person", "people")
  if (s.owned) return `Yours // shared with ${people}${household}`
  return `${s.owner ?? "Someone"}'s // ${people}${household}`
}

interface VaultsTabProps {
  overview: SharingOverview
  vaults: VaultView[]
  rows: ItemSummary[]
  /** The vault open, by id. */
  selected: string | null
  onSelect: (vaultId: string | null) => void
  onOpenItem: (id: string) => void
  onOpenPeople: () => void
  onChanged: () => void
  onNotice: (text: string) => void
  onError: (words: string) => void
}

/**
 * Sharing → VAULTS (brief S3): see who has each vault, change what they
 * can do, and stop. The list holds the vaults (a household says so), NEW
 * HOUSEHOLD, and the items shared one by one, sent and received; a vault
 * opens its members as the detail, found-3's members place.
 */
export function VaultsTab({ overview, vaults, rows, selected, onSelect, onOpenItem, onOpenPeople, onChanged, onNotice }: VaultsTabProps) {
  const sharing = useSharingClient()
  const shown = shareableVaults(vaults)
  const open = shown.find((v) => v.id === selected) ?? null
  const [naming, setNaming] = useState(false)
  const online = overview.online
  const titleOf = (id: string) => rows.find((r) => r.id === id)?.title ?? "An item"
  // Stopping asks first (brief I7, StopSharing); a refusal shows in the Question.
  const [stopping, setStopping] = useState<{ share_id: string; login: string; item_id: string } | null>(null)
  const unshare = async (shareId: string, login: string) => {
    await sharing.unshareItem(shareId)
    onNotice(`Stopped sharing with ${login}`)
    onChanged()
  }
  const canStartHousehold = overview.households.every((h) => !h.organizing)
  const items = overview.sent.length + overview.received.length

  const list = (
    <>
      <section aria-label="Vaults" className="flex flex-col">
        <SectionHeader
          title="Vaults"
          count={shown.length}
          action={
            canStartHousehold ? (
              <Button type="button" size="sm" variant="ghost" disabled={!online} disabledReason={!online ? OFFLINE_REASON : undefined} onClick={() => setNaming(true)}>
                New household
              </Button>
            ) : undefined
          }
        />
        <div role="list" aria-label="Vaults">
          {shown.map((v) => (
            <ListRow
              key={v.id}
              role="listitem"
              size="compact"
              selected={v.id === selected}
              title={v.name}
              meta={vaultLine(v)}
              status={!v.sharing || v.sharing.owned ? "Owner" : RIGHT_WORD[v.sharing.right]}
              onClick={() => onSelect(v.id)}
            />
          ))}
        </div>
      </section>
      {items > 0 && (
        <section aria-label="Items shared one by one" className="flex flex-col">
          <SectionHeader title="Items shared one by one" count={items} />
          <div role="list" aria-label="Items shared one by one">
            {overview.sent.map((s) => (
              <ListRow
                key={s.share_id}
                role="listitem"
                size="compact"
                title={titleOf(s.item_id)}
                meta={`With ${s.login} // ${RIGHT_WORD[s.right].toLowerCase()} // since ${fullDate(s.created_at)}`}
                action={
                  <Button type="button" size="sm" variant="ghost" disabled={!online} disabledReason={!online ? OFFLINE_REASON : undefined} onClick={() => setStopping(s)}>
                    Stop sharing
                  </Button>
                }
                onClick={() => onOpenItem(s.item_id)}
              />
            ))}
            {overview.received.map((r) => (
              <ListRow
                key={r.share_id}
                role="listitem"
                size="compact"
                title={titleOf(r.item_id)}
                meta={`From ${r.from} // ${RIGHT_WORD[r.right].toLowerCase()}`}
                action={
                  <Button type="button" size="sm" variant="ghost" onClick={() => onOpenItem(r.item_id)}>
                    Open
                  </Button>
                }
              />
            ))}
          </div>
        </section>
      )}
      <StopSharing share={stopping} title={stopping ? titleOf(stopping.item_id) : ""} onClose={() => setStopping(null)} onStop={unshare} />
      <NewHousehold
        open={naming}
        onClose={() => setNaming(false)}
        onMade={(name) => {
          setNaming(false)
          onNotice(`${name} is ready. Add the people in your household to it`)
          onChanged()
        }}
      />
    </>
  )

  return (
    <SharingPanes
      name="Vaults"
      backLabel="Vaults"
      onBack={() => onSelect(null)}
      emptyBody="Pick a vault to see who has it."
      list={list}
      detail={open ? <VaultMembers key={open.id} vault={open} contacts={overview.contacts} online={online} onChanged={onChanged} onNotice={onNotice} onOpenPeople={onOpenPeople} /> : null}
    />
  )
}

/** NEW HOUSEHOLD's name prompt (brief S3, X5): one field and D194's hint. */
function NewHousehold({ open, onClose, onMade }: { open: boolean; onClose: () => void; onMade: (name: string) => void }) {
  const sharing = useSharingClient()
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const submit = async (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    const name = String(new FormData(e.currentTarget).get("name") ?? "").trim()
    if (!name) return
    setError(null)
    setBusy(true)
    try {
      await sharing.createHousehold(name)
      onMade(name)
    } catch (err) {
      setError(describeError(err))
    } finally {
      setBusy(false)
    }
  }
  return (
    <AdaptiveDialog
      open={open}
      onOpenChange={(o) => {
        if (!o) {
          setError(null)
          onClose()
        }
      }}
      size="question"
      overline="Sharing // vaults"
      title="New household"
      footer={
        <>
          <Button type="button" variant="ghost" onClick={onClose}>
            Cancel
          </Button>
          <Button type="submit" form="household-new" loading={busy}>
            Create household
          </Button>
        </>
      }
    >
      <form id="household-new" aria-label="New household" onSubmit={(e) => void submit(e)}>
        <TextField label="Name" name="name" autoComplete="off" placeholder="Home" helper="A shared vault you organize. Billing isn't part of Lockdown." error={error ?? undefined} />
      </form>
    </AdaptiveDialog>
  )
}

interface VaultMembersProps {
  vault: VaultView
  contacts: ContactView[]
  online: boolean
  onChanged: () => void
  onNotice: (text: string) => void
  onOpenPeople: () => void
}

/**
 * One vault's members and their rights (docs/plan/18-sharing.md §4, brief
 * S3) as a detail pane. Adding someone hands them the vault's key, wrapped
 * to the key their record in the log names, through the `Approve` gate.
 * Removing someone asks first and moves the vault to a new key at once
 * (D196: no gate for taking access away). Only a manager sees the
 * controls; the server refuses the rest anyway. While the core holds new
 * members back (a split view, D572 and D573) the controls are hidden. Every
 * change is a signed line in the history.
 */
export function VaultMembers({ vault, contacts, online, onChanged, onNotice, onOpenPeople }: VaultMembersProps) {
  const sharing = useSharingClient()
  const gate = useGate()
  const [view, setView] = useState<VaultSharingView | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const [removing, setRemoving] = useState<MemberView | null>(null)
  const [changing, setChanging] = useState<MemberView | null>(null)

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

  const act = useCallback(
    async (call: () => Promise<VaultSharingView>, words: string) => {
      setError(null)
      setBusy(true)
      try {
        setView(await call())
        onNotice(words)
        onChanged()
      } catch (e) {
        setError(describeError(e))
        throw e
      } finally {
        setBusy(false)
      }
    },
    [onNotice, onChanged],
  )

  const add = (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    const data = new FormData(e.currentTarget)
    const login = String(data.get("who") ?? "")
    const right = String(data.get("right") ?? "view") as Access
    if (!login) return
    const family = view?.kind === "family"
    void act(
      () => gate.run((g) => (family ? sharing.addToHousehold(vault.id, login, right, g) : sharing.shareVault(vault.id, login, right, g))),
      `${login} can open ${vault.name} now`,
    ).catch(() => undefined)
  }

  const members = view?.members ?? []
  const candidates = contacts.filter((c) => !members.some((m) => m.account_id === c.account_id))
  const others = members.filter((m) => !m.you).length
  const shared = view?.shared ?? false
  const overline = shared ? `Vault // shared // ${countName(members.length, "person", "people")}` : "Vault // only you"
  const held = view?.held ?? null
  const manages = view !== null && !held && !view.blocked && (!view.shared || view.can_manage)
  const reason = !online ? OFFLINE_REASON : undefined
  const verifiedOf = (m: MemberView) => m.you || m.verified
  // A phone has no room for two text actions beside the right: they go behind the row's ⋯ (cross-app §10).
  const phone = useAppFrame().phone

  return (
    <div className="flex flex-col gap-6">
      {paneHeading(overline, vault.name, "Everyone here holds the vault's key. What each can do is checked on their device and by the server.")}
      {!view && !error && (
        <div className="flex flex-col gap-2" aria-label="Loading members">
          <Skeleton variant="row" />
          <Skeleton variant="row" />
        </div>
      )}
      {error && <Banner tone="critical" size="compact" title="Not done." body={error} />}
      {view?.blocked && <Banner tone="info" size="compact" title="Can't be shared." body={view.blocked} />}
      {held && <Banner tone="info" size="compact" body={held} />}
      {view && shared && (
        <section aria-label="Members" className="flex flex-col">
          <SectionHeader title="Members" count={members.length} />
          <div role="list" aria-label="Members">
            {members.map((m) => {
              const controls = manages && view.can_manage && !m.owner && !m.you
              return (
                <ListRow
                  key={m.account_id}
                  role="listitem"
                  size="compact"
                  title={m.login}
                  marks={
                    <span className="flex flex-wrap gap-1">
                      {m.you && <Badge>You</Badge>}
                      {m.owner && <Badge>Owner</Badge>}
                      {!verifiedOf(m) && <Badge tone="warning">Not compared</Badge>}
                    </span>
                  }
                  status={RIGHT_WORD[m.right]}
                  menu={
                    controls && phone ? (
                      <>
                        <DropdownMenuItem disabled={!online || busy} onClick={() => setChanging(m)}>
                          Change
                        </DropdownMenuItem>
                        <DropdownMenuItem disabled={!online || busy} onClick={() => setRemoving(m)}>
                          Remove
                        </DropdownMenuItem>
                      </>
                    ) : undefined
                  }
                  menuLabel={`Change or remove ${m.login}`}
                  action={
                    controls && !phone ? (
                      <span className="flex items-center gap-1">
                        <Button type="button" size="sm" variant="ghost" disabled={!online || busy} disabledReason={reason} onClick={() => setChanging(m)}>
                          Change
                        </Button>
                        <Button type="button" size="sm" variant="ghost" disabled={!online || busy} disabledReason={reason} onClick={() => setRemoving(m)}>
                          Remove
                        </Button>
                      </span>
                    ) : undefined
                  }
                />
              )
            })}
          </div>
        </section>
      )}
      {view && !shared && !view.blocked && <p className="type-body text-text-secondary">Only you.</p>}
      {manages && (
        <section aria-label="Add someone" className="flex flex-col gap-3">
          <SectionHeader title="Add someone" />
          {candidates.length === 0 ? (
            <div className="flex flex-col items-start gap-3">
              <p className="type-body text-text-secondary">Look someone up under People first.</p>
              <Button type="button" variant="secondary" onClick={onOpenPeople}>
                Open people
              </Button>
            </div>
          ) : (
            <form id="vault-add" aria-label="Add someone" className="flex flex-wrap items-end gap-3" onSubmit={add}>
              <div className="min-w-48 flex-1">
                <Field>
                <FieldLabel htmlFor="vault-add-who">Who</FieldLabel>
                <NativeSelect id="vault-add-who" name="who" defaultValue={candidates[0]?.login}>
                  {candidates.map((c) => (
                    <NativeSelectOption key={c.account_id} value={c.login}>
                      {c.login}
                      {c.verified ? "" : " (not compared)"}
                    </NativeSelectOption>
                  ))}
                </NativeSelect>
                </Field>
              </div>
              <div className="min-w-40">
                <Field>
                <FieldLabel htmlFor="vault-add-right">They can</FieldLabel>
                <NativeSelect id="vault-add-right" name="right" defaultValue="view">
                  {RIGHTS.map((r) => (
                    <NativeSelectOption key={r} value={r}>
                      {RIGHT_NAMES[r]}
                    </NativeSelectOption>
                  ))}
                </NativeSelect>
                </Field>
              </div>
              <Button type="submit" variant="secondary" disabled={!online || busy} disabledReason={reason} loading={busy}>
                Add
              </Button>
            </form>
          )}
        </section>
      )}
      {view && view.history.length > 0 && (
        <SectionHeader title="History" count={view.history.length} collapsible summary={`Last change ${rowDate(view.history[view.history.length - 1]!.at)}`}>
          <ol aria-label="History" className="flex flex-col">
            {view.history.map((h, i) => (
              <LogRow key={i} render={<li />} time={rowDate(h.at)} dateTime={new Date(h.at * 1000).toISOString()}>
                {h.words}
              </LogRow>
            ))}
          </ol>
        </SectionHeader>
      )}
      <AdaptiveDialog
        open={changing !== null}
        onOpenChange={(o) => !o && setChanging(null)}
        size="question"
        overline={`${vault.name} // ${changing?.login ?? ""}`}
        title={`What can ${changing?.login ?? ""} do?`}
        footer={
          <Button type="button" variant="ghost" onClick={() => setChanging(null)}>
            Cancel
          </Button>
        }
      >
        {changing && (
          <RadioGroup
            aria-label={`What ${changing.login} can do`}
            value={changing.right}
            onValueChange={(v) => {
              const m = changing
              const right = v as Access
              setChanging(null)
              if (right !== m.right) void act(() => sharing.setRight(vault.id, m.account_id, right), `${m.login} can ${RIGHT_WORD[right].toLowerCase()} now`).catch(() => undefined)
            }}
          >
            {RIGHTS.map((r) => (
              <RadioRow key={r} value={r} label={RIGHT_WORD[r]} description={rightSentence(r)} />
            ))}
          </RadioGroup>
        )}
      </AdaptiveDialog>
      <ConfirmDialog
        open={removing !== null}
        onOpenChange={(o) => !o && setRemoving(null)}
        overline={`${vault.name} // ${count(others)} ${others === 1 ? "other" : "others"}`}
        title={`Remove ${removing?.login ?? ""}?`}
        description={`"${vault.name}" moves to a new key now and everyone else gets it. What they already saw, they may have kept.`}
        confirmLabel="Remove and change key"
        tone="destructive"
        onConfirm={() => {
          const m = removing
          if (!m) return
          return act(() => sharing.removeMember(vault.id, m.account_id), `Removed ${m.login}; ${vault.name} is on a new key`)
        }}
      />
    </div>
  )
}
