import { useEffect, useLayoutEffect, useMemo, useRef, useState, type FormEvent } from "react"

import {
  Button,
  Checkbox,
  ConfirmDialog,
  EmptyState,
  Field,
  FieldLabel,
  FieldLegend,
  FieldRow,
  FieldSet,
  FloatButton,
  ListRow,
  LogRow,
  NativeSelect,
  NativeSelectOption,
  SectionHeader,
  useViewport,
} from "@unicron/ui"

import { useDetailBottomRow } from "@/app/detail-bottom-row"
import { useGate } from "@/app/gate"
import { useNeedsYouSource, type NeedsYouEntry } from "@/app/needs-you"
import { usePlaces } from "@/app/places"
import { describeError } from "@/core/client"
import type { VaultView } from "@/core/types"
import { count, rowDate } from "@/lib/format"

import {
  STATE_WORDS,
  asksForYours,
  dateTime,
  daysWords,
  grantActions,
  grantLine,
  requestLine,
  requestTitle,
  standingLine,
  stoppedWords,
  useShownGrant,
  wantGrant,
  type GrantAction,
} from "./emergency-model"
import { OFFLINE_REASON } from "./people-section"
import { ShareLayer } from "./share-layer"
import { useSharingClient, type ContactView, type EmergencyView } from "./sharing-client"
import { SharingPanes, paneHeading } from "./sharing-panes"

const WAITS = [1, 2, 3, 7, 14, 30]

interface EmergencyTabProps {
  grants: EmergencyView[]
  contacts: ContactView[]
  vaults: VaultView[]
  /** The server can be used now (online, and no split view pausing emergency steps). */
  online: boolean
  /** Not online at all: the disabled controls say so. A split view's pause is the page's Banner's to explain. */
  offline?: boolean
  /** The grant open, by id. */
  selected: string | null
  onSelect: (grant: string | null) => void
  onGrants: (grants: EmergencyView[]) => void
  onChanged: () => void
  onNotice: (text: string) => void
  /** OPEN PEOPLE, when nobody is compared yet. */
  onOpenPeople: () => void
}

/**
 * Sharing → EMERGENCY (brief S5, doc 91): someone you trust, whose keys
 * checked out against the log and whose fingerprint you compared, can ask
 * for your vaults; you have the waiting period to stop it, and only after
 * it runs out does the server hand over the keys you sealed to them
 * (docs/plan/18-sharing.md §6). A list of the grants, both ways, and one
 * grant's detail: where it stands, its Field Rows, what you can do, and
 * WHAT HAPPENED as Log Rows.
 *
 * A request for your vaults is needs-you row 4 (`EmergencyRequests`), the
 * screen's one yellow; on the grant's own detail its solid action is STOP
 * IT, which acts at once with no gate (D196, D221). Ending asks first.
 */
export function EmergencyTab({ grants, contacts, vaults, online, offline = !online, selected, onSelect, onGrants, onChanged, onNotice, onOpenPeople }: EmergencyTabProps) {
  const [adding, setAdding] = useState(false)
  const open = grants.find((g) => g.grant_id === selected) ?? null
  const mine = grants.filter((g) => g.mine)
  const theirs = grants.filter((g) => !g.mine)
  const reason = offline ? OFFLINE_REASON : undefined
  const row = (g: EmergencyView) => (
    <ListRow
      key={g.grant_id}
      role="listitem"
      size="compact"
      selected={g.grant_id === selected}
      title={g.other}
      meta={grantLine(g)}
      metaStyle="label"
      status={STATE_WORDS[g.state]}
      onClick={() => onSelect(g.grant_id)}
    />
  )
  const list = (
    <>
      <p className="type-body text-text-secondary">Someone you trust can ask for your vaults. You get a waiting period to stop it.</p>
      <div>
        <Button type="button" variant="secondary" disabled={!online} disabledReason={reason} onClick={() => setAdding(true)}>
          Add emergency contact
        </Button>
      </div>
      {grants.length === 0 && <EmptyState size="sm" overline="Emergency // 00" title="No emergency contacts." body="Name someone whose fingerprint you compared." />}
      {mine.length > 0 && (
        <section aria-label="Your emergency contacts" className="flex flex-col">
          <SectionHeader title="Your emergency contacts" count={mine.length} />
          <div role="list" aria-label="Your emergency contacts">
            {mine.map(row)}
          </div>
        </section>
      )}
      {theirs.length > 0 && (
        <section aria-label="People who trust you" className="flex flex-col">
          <SectionHeader title="People who trust you" count={theirs.length} />
          <div role="list" aria-label="People who trust you">
            {theirs.map(row)}
          </div>
        </section>
      )}
    </>
  )
  return (
    <>
      <SharingPanes
        name="Emergency"
        backLabel="Emergency"
        onBack={() => onSelect(null)}
        emptyBody="Pick a contact to see where it stands."
        list={list}
        detail={open ? <GrantDetail key={open.grant_id} grant={open} vaults={vaults} online={online} reason={reason} onGrants={onGrants} onChanged={onChanged} onNotice={onNotice} /> : null}
      />
      <AddEmergency
        open={adding}
        contacts={contacts}
        vaults={vaults}
        onOpenPeople={() => {
          setAdding(false)
          onOpenPeople()
        }}
        onClose={() => setAdding(false)}
        onAdded={(list, login) => {
          setAdding(false)
          onGrants(list)
          onNotice(`Asked ${login} to be your emergency contact`)
        }}
      />
    </>
  )
}

/** The words of the Question before a step that asks (brief S5). */
function questionOf(g: EmergencyView, a: GrantAction): { title: string; description: string; confirm: string } {
  if (a.action === "revoke")
    return {
      title: `End emergency access for ${g.other}?`,
      description:
        g.state === "released"
          ? "They already opened your vaults. Ending it stops new access, and you should change what matters."
          : "The keys you sealed for them are dropped, and they can't ask again.",
      confirm: "End it",
    }
  if (a.action === "request")
    return {
      title: `Ask for ${g.other}'s vaults?`,
      description: `The ${daysWords(g.wait_days)} wait starts now, and they are told. They can stop it.`,
      confirm: "Ask for access",
    }
  return { title: `Step down for ${g.other}?`, description: "You stop being their emergency contact. They can name you again.", confirm: "Step down" }
}

/** One grant (brief S5): where it stands, WAITING PERIOD, VAULTS, OPENS, what you can do, WHAT HAPPENED. */
function GrantDetail({ grant, vaults, online, reason, onGrants, onChanged, onNotice }: {
  grant: EmergencyView
  vaults: VaultView[]
  online: boolean
  reason?: string
  onGrants: (grants: EmergencyView[]) => void
  onChanged: () => void
  onNotice: (text: string) => void
}) {
  const sharing = useSharingClient()
  const stacked = useViewport().widthClass === "compact"
  const [busy, setBusy] = useState(false)
  const [asking, setAsking] = useState<GrantAction | null>(null)
  const [error, setError] = useState<string | null>(null)
  const actions = grantActions(grant)
  const primary = actions.find((a) => a.emphasis === "primary")

  const run = async (a: GrantAction) => {
    setError(null)
    setBusy(true)
    try {
      if (a.action === "open") {
        onGrants(await sharing.emergencyOpen(grant.grant_id))
        onNotice(`${grant.other}'s vaults are open to read, in the sidebar`)
        onChanged()
      } else {
        onGrants(await sharing.emergencyStep(grant.grant_id, a.action))
        onNotice(DONE_WORDS[a.action](grant))
      }
    } catch (e) {
      setError(describeError(e))
      throw e
    } finally {
      setBusy(false)
    }
  }
  const press = (a: GrantAction) => (a.asks ? setAsking(a) : void run(a).catch(() => undefined))
  const latest = useRef(() => {})
  useLayoutEffect(() => {
    latest.current = () => primary && press(primary)
  })
  const pa = primary?.action
  const pl = primary?.label

  // On a phone, ACCEPT and OPEN THEIR VAULTS sit at the end of the detail bottom row, with their word (brief S5).
  const end = useMemo(
    () => (pa && pl ? <FloatButton label={pl} icon={pa === "open" ? "unlocked" : "checkmark"} kind="primary" size="l" extended disabled={!online || busy} onClick={() => latest.current()} /> : undefined),
    [pa, pl, online, busy],
  )
  const inRow = useDetailBottomRow({ end })
  const names = grant.vaults.map((id) => vaults.find((v) => v.id === id)?.name).filter((n): n is string => Boolean(n))
  const size = stacked ? "stacked" : "default"
  const question = asking ? questionOf(grant, asking) : null
  const standing = standingLine(grant)

  return (
    <div className="flex flex-col gap-6">
      {paneHeading(`Emergency access // ${STATE_WORDS[grant.state]}`, grant.other, grant.mine ? "Your emergency contact" : "Trusts you")}
      {standing && <p className="type-body text-text-body">{standing}</p>}
      <div className="flex flex-col">
        <FieldRow size={size} label="Waiting period" rule="none">
          {daysWords(grant.wait_days)}
        </FieldRow>
        <FieldRow size={size} label="Vaults">
          {names.length === grant.vaults.length && names.length > 0 ? names.join(", ") : `${count(grant.vaults.length)} ${grant.vaults.length === 1 ? "vault" : "vaults"}`}
        </FieldRow>
        {grant.release_at && (
          <FieldRow size={size} label="Opens">
            {dateTime(grant.release_at)}
          </FieldRow>
        )}
      </div>
      {actions.length > 0 && (
        <div className="flex flex-wrap items-start gap-2">
          {actions.map((a) =>
            a.emphasis === "primary" && inRow ? null : (
              <Button
                key={a.action}
                type="button"
                variant={a.emphasis === "primary" ? "default" : a.emphasis === "secondary" ? "secondary" : a.emphasis === "critical" ? "ghost-destructive" : "ghost"}
                disabled={!online || busy}
                disabledReason={reason}
                loading={busy && a.emphasis === "primary"}
                onClick={() => press(a)}
              >
                {a.label}
              </Button>
            ),
          )}
        </div>
      )}
      {error && (
        <p role="alert" className="type-caption text-critical-text">
          {error}
        </p>
      )}
      {grant.history.length > 0 && (
        <section aria-label="What happened" className="flex flex-col">
          <SectionHeader title="What happened" count={grant.history.length} />
          <ol aria-label="What happened" className="flex flex-col">
            {grant.history.map((h, i) => (
              <LogRow key={i} render={<li />} time={rowDate(h.at)} dateTime={new Date(h.at * 1000).toISOString()}>
                {h.words}
              </LogRow>
            ))}
          </ol>
        </section>
      )}
      <ConfirmDialog
        open={asking !== null}
        onOpenChange={(o) => !o && setAsking(null)}
        overline={`Emergency access // ${grant.other}`}
        title={question?.title ?? ""}
        description={question?.description}
        confirmLabel={question?.confirm ?? ""}
        tone={asking?.action === "request" ? "default" : "destructive"}
        error={error ?? undefined}
        onConfirm={async () => {
          const a = asking
          if (!a) return
          await run(a)
          setAsking(null)
        }}
      />
    </div>
  )
}

/** The toast after a step, naming who (brief X2: no full stop). */
const DONE_WORDS: Record<GrantAction["action"], (g: EmergencyView) => string> = {
  accept: (g) => `You're ${g.other}'s emergency contact`,
  decline: (g) => (g.state === "invited" ? `Declined ${g.other}'s request` : `Stepped down for ${g.other}`),
  request: (g) => `Asked for ${g.other}'s vaults. The wait starts now`,
  stop: stoppedWords,
  revoke: (g) => `Ended emergency access for ${g.other}`,
  open: (g) => `${g.other}'s vaults are open to read, in the sidebar`,
}

/**
 * Needs-you row 4 (brief X3): someone you named asked for your vaults and
 * the wait runs. EMERGENCY ACCESS // OPENS NOV 20, "{them} asked for your
 * vaults.", REVIEW, which opens the grant; on that grant's own screen the
 * solid action is STOP IT, which stops it at once: taking access away
 * never asks for the gate (D196, D221). No text action. Offline, or while
 * a split view pauses emergency steps, STOP IT would fail, so it waits
 * and REVIEW still opens the grant.
 */
export function EmergencyRequests({ grants, online, onGrants, onReview, onNotice, onError }: {
  grants: EmergencyView[]
  online: boolean
  onGrants: (grants: EmergencyView[]) => void
  /** Opens the grant (Sharing → Emergency, its detail). */
  onReview: (grant: string) => void
  onNotice: (text: string) => void
  onError?: (words: string) => void
}) {
  return (
    <>
      {grants.filter(asksForYours).map((g) => (
        <EmergencyRequest key={g.grant_id} grant={g} online={online} onGrants={onGrants} onReview={onReview} onNotice={onNotice} onError={onError} />
      ))}
    </>
  )
}

function EmergencyRequest({ grant, online, onGrants, onReview, onNotice, onError }: {
  grant: EmergencyView
  online: boolean
  onGrants: (grants: EmergencyView[]) => void
  onReview: (grant: string) => void
  onNotice: (text: string) => void
  onError?: (words: string) => void
}) {
  const sharing = useSharingClient()
  const shown = useShownGrant() === grant.grant_id
  const [busy, setBusy] = useState(false)
  const entry = useMemo<NeedsYouEntry>(() => {
    const stop = async () => {
      setBusy(true)
      try {
        onGrants(await sharing.emergencyStep(grant.grant_id, "stop"))
        onNotice(stoppedWords(grant))
      } catch (e) {
        onError?.(describeError(e))
      } finally {
        setBusy(false)
      }
    }
    return {
      id: `emergency:${grant.grant_id}`,
      priority: 4,
      title: requestTitle(grant),
      line: requestLine(grant),
      action: shown && online ? { label: "Stop it", onClick: () => void stop(), busy } : { label: "Review", onClick: () => onReview(grant.grant_id) },
      shows: "every",
    }
  }, [grant, shown, online, busy, sharing, onGrants, onNotice, onError, onReview])
  useNeedsYouSource(entry)
  return null
}

/**
 * Row 4 for every screen: reads the grants at mount and every minute, and
 * registers each request for your vaults. REVIEW goes to Sharing →
 * Emergency and opens the grant. For the shell to mount once beside the
 * request sources (doc 91 §7); Sharing draws `EmergencyRequests` from its
 * own overview meanwhile, so the two must not both be mounted.
 */
export function EmergencyWatch({ enabled = true, onNotice }: { enabled?: boolean; onNotice: (text: string) => void }) {
  const sharing = useSharingClient()
  const { go } = usePlaces()
  const [grants, setGrants] = useState<EmergencyView[]>([])
  const [online, setOnline] = useState(false)
  useEffect(() => {
    if (!enabled) return
    let live = true
    const read = () =>
      sharing
        .overview()
        .then((o) => {
          if (!live) return
          setGrants(o.emergency)
          setOnline(o.online && !o.paused)
        })
        .catch(() => {})
    void read()
    const timer = window.setInterval(() => void read(), 60_000)
    return () => {
      live = false
      window.clearInterval(timer)
    }
  }, [enabled, sharing])
  if (!enabled) return null
  return (
    <EmergencyRequests
      grants={grants}
      online={online}
      onGrants={setGrants}
      onNotice={onNotice}
      onReview={(id) => {
        wantGrant(id)
        go("sharing-emergency")
      }}
    />
  )
}

/**
 * ADD EMERGENCY CONTACT (brief S5), a Form: Who (only people whose
 * fingerprint you compared), Waiting period, Which vaults, the caption,
 * and ASK THEM through the gate (an extended "Ask them" on a phone).
 */
function AddEmergency({ open, contacts, vaults, onOpenPeople, onClose, onAdded }: {
  open: boolean
  contacts: ContactView[]
  vaults: VaultView[]
  onOpenPeople: () => void
  onClose: () => void
  onAdded: (list: EmergencyView[], login: string) => void
}) {
  const sharing = useSharingClient()
  const gate = useGate()
  const verified = contacts.filter((c) => c.verified)
  const own = useMemo(() => vaults.filter((v) => !v.sharing || v.sharing.owned), [vaults])
  const [picked, setPicked] = useState<string[]>([])
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  useEffect(() => {
    if (open) setPicked(own.filter((v) => v.primary).map((v) => v.id))
  }, [open, own])
  const close = () => {
    setError(null)
    onClose()
  }
  const submit = async (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    const data = new FormData(e.currentTarget)
    const login = String(data.get("who") ?? "")
    const days = Number(data.get("wait") ?? 7)
    if (!login || picked.length === 0) return
    setError(null)
    setBusy(true)
    try {
      onAdded(await gate.run((g) => sharing.addEmergency(login, days, picked, g)), login)
    } catch (err) {
      setError(describeError(err))
    } finally {
      setBusy(false)
    }
  }
  const ready = verified.length > 0
  return (
    <ShareLayer
      open={open}
      onClose={close}
      overline="Emergency // add a contact"
      title="Add emergency contact"
      footer={
        <>
          <Button type="button" variant="ghost" onClick={close}>
            Cancel
          </Button>
          {ready && (
            <Button type="submit" form="emergency-add" disabled={picked.length === 0} loading={busy}>
              Ask them
            </Button>
          )}
        </>
      }
      phoneAction={ready ? <FloatButton label="Ask them" icon="send" kind="primary" size="l" extended type="submit" form="emergency-add" disabled={picked.length === 0 || busy} /> : undefined}
    >
      {!ready ? (
        <EmptyState size="sm" overline="Emergency // nobody compared" title="Compare fingerprints first." body="Only someone whose fingerprint you compared can be an emergency contact." actionLabel="Open people" onAction={onOpenPeople} />
      ) : (
        <form id="emergency-add" aria-label="Add emergency contact" className="flex flex-col gap-4" onSubmit={(e) => void submit(e)}>
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
          <div className="flex flex-col gap-1">
            <Field>
              <FieldLabel htmlFor="emergency-wait">Waiting period</FieldLabel>
              <NativeSelect id="emergency-wait" name="wait" defaultValue="7">
                {WAITS.map((d) => (
                  <NativeSelectOption key={d} value={String(d)}>
                    {daysWords(d)}
                  </NativeSelectOption>
                ))}
              </NativeSelect>
            </Field>
            <p className="type-caption text-text-secondary">How long you have to stop it after they ask.</p>
          </div>
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
          <p className="type-caption text-text-secondary">The vaults you pick are sealed to their key now. The server hands them over only after they ask and the wait runs out.</p>
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

