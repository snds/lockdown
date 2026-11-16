import { useCallback, useMemo, useState, type FormEvent } from "react"

import {
  AdaptiveDialog,
  AdaptiveMenuItem,
  Badge,
  Banner,
  Button,
  ConfirmDialog,
  DigitGroups,
  EmptyState,
  FieldRow,
  FieldRowAction,
  FloatButton,
  ListRow,
  OverflowMenu,
  QrCode,
  SectionHeader,
  TextField,
  useAppFrame,
  useViewport,
} from "@unicron/ui"

import { useDetailBottomRow, useDetailTopBarHandUp } from "@/app/detail-bottom-row"
import { WaitingOnYou } from "@/app/needs-you"
import { describeError } from "@/core/client"
import { count, countName } from "@/lib/format"
import { PhraseIncomingList, SayPhrase } from "@/features/depth/key-check"
import { CameraView } from "@/features/scan/camera-view"
import { useScanClient, type Frame } from "@/features/scan/scan-client"

import { RESET_HINT } from "./key-resets-words"
import { useSharingClient, type ContactView, type KeyResetView, type SharingOverview } from "./sharing-client"
import { SharingPanes, paneHeading } from "./sharing-panes"

/** What every control that needs the server says while there is none (brief S1, #37). */
export const OFFLINE_REASON = "Needs the server. You're offline."

interface PeopleTabProps {
  overview: SharingOverview
  /** The person open, by account id. */
  selected: string | null
  onSelect: (account: string | null) => void
  onChanged: () => void
  onNotice: (text: string) => void
  /** Copies your code to paste; left out where the build has no clipboard path for it (the web vault). */
  onCopyCode?: () => Promise<void>
}

/**
 * Sharing → PEOPLE (brief S2): know that a person's keys are really theirs
 * before sharing anything with them. The list holds your fingerprint and
 * your code, Look up, the key checks others started with you, and the
 * people pinned on this device; a person opens their detail, where
 * COMPARE FINGERPRINTS is the one primary while they are not compared.
 *
 * Looking someone up checks their keys against the server's
 * key-transparency log before anything is pinned (docs/plan/18-sharing.md
 * §3). A pinned person whose keys later change is refused until they are
 * forgotten and looked up again, so a swapped key never slips in.
 */
export function PeopleTab({ overview, selected, onSelect, onChanged, onNotice, onCopyCode }: PeopleTabProps) {
  const [comparing, setComparing] = useState<ContactView | null>(null)
  const open = overview.contacts.find((c) => c.account_id === selected) ?? null
  const reset = open ? (overview.resets ?? []).find((r) => r.account_id === open.account_id) : undefined
  const lookedUp = useCallback(
    (c: ContactView) => {
      onNotice(`${c.login}'s keys checked out against the log`)
      onChanged()
      onSelect(c.account_id)
      setComparing(c)
    },
    [onChanged, onNotice, onSelect],
  )
  return (
    <>
      <SharingPanes
        name="People"
        backLabel="People"
        onBack={() => onSelect(null)}
        emptyBody="Pick a person to see them here."
        list={<PeopleList overview={overview} selected={selected} onSelect={onSelect} onLookedUp={lookedUp} onChanged={onChanged} onCopyCode={onCopyCode} />}
        detail={
          open ? (
            <PersonDetail
              key={open.account_id}
              contact={open}
              reset={reset}
              online={overview.online && !overview.paused}
              offline={!overview.online}
              onCompare={() => setComparing(open)}
              onChanged={onChanged}
              onForgotten={() => {
                onNotice(`Forgot ${open.login} on this device`)
                onSelect(null)
                onChanged()
              }}
            />
          ) : null
        }
      />
      <CompareFingerprints
        contact={comparing}
        onClose={() => setComparing(null)}
        onVerified={(c) => {
          setComparing(null)
          onNotice(`The fingerprints match: ${c.login} is verified`)
          onChanged()
        }}
      />
    </>
  )
}

/** The list pane: your fingerprint and code, Look up, key checks for you, and the people pinned here. */
function PeopleList({ overview, selected, onSelect, onLookedUp, onChanged, onCopyCode }: {
  overview: SharingOverview
  selected: string | null
  onSelect: (account: string) => void
  onLookedUp: (c: ContactView) => void
  onChanged: () => void
  onCopyCode?: () => Promise<void>
}) {
  const sharing = useSharingClient()
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const [showing, setShowing] = useState(false)
  const [copied, setCopied] = useState(false)
  const online = overview.online
  // A split view pauses look-ups; the page's Banner says why, so the button stays quiet about it.
  const canLookUp = online && !overview.paused
  const lookup = async (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    const form = e.currentTarget
    const login = String(new FormData(form).get("login") ?? "").trim()
    if (!login) return
    setError(null)
    setBusy(true)
    try {
      const c = await sharing.lookup(login)
      form.reset()
      onLookedUp(c)
    } catch (err) {
      setError(describeError(err))
    } finally {
      setBusy(false)
    }
  }
  const copy = async () => {
    if (!onCopyCode) return
    await onCopyCode()
    setCopied(true)
    window.setTimeout(() => setCopied(false), 2000)
  }
  return (
    <>
      <WaitingOnYou />
      {overview.fingerprint && (
        <div className="flex flex-col">
          <FieldRow
            size="stacked"
            label="Your fingerprint"
            rule="none"
            action={<FieldRowAction onClick={() => setShowing(true)}>Show code</FieldRowAction>}
          >
            <DigitGroups value={overview.fingerprint} size="sm" label="Your fingerprint" />
          </FieldRow>
          <FieldRow
            size="stacked"
            label="Your code"
            description="Read this out, or send the code, when someone compares fingerprints with you."
            action={onCopyCode ? <FieldRowAction onClick={() => void copy().catch(() => undefined)}>{copied ? "Copied" : "Copy"}</FieldRowAction> : undefined}
          >
            <span aria-label="Your code to paste" className="type-code-inline min-w-0 truncate">
              {overview.fingerprint_qr}
            </span>
          </FieldRow>
        </div>
      )}
      <form className="flex items-end gap-2" aria-label="Look someone up" onSubmit={(e) => void lookup(e)}>
        <div className="min-w-0 flex-1">
          <TextField
            label="Look someone up by their sign-in"
            name="login"
            type="email"
            autoComplete="off"
            placeholder="name@example.com"
            error={error ?? undefined}
            disabled={!online}
            disabledReason={!online ? OFFLINE_REASON : undefined}
          />
        </div>
        <Button type="submit" variant="secondary" disabled={!canLookUp} loading={busy}>
          Look up
        </Button>
      </form>
      <PhraseIncomingList online={online} onVerified={onChanged} />
      {overview.contacts.length === 0 ? (
        <EmptyState size="sm" overline={`People // ${count(0)}`} title="Nobody yet." body="Look someone up to share with them or name them an emergency contact." />
      ) : (
        <section aria-label="People pinned here" className="flex flex-col">
          <SectionHeader title="People" count={overview.contacts.length} />
          <div role="list" aria-label="People">
            {overview.contacts.map((c) => (
              <ListRow
                key={c.account_id}
                role="listitem"
                size="compact"
                selected={c.account_id === selected}
                title={c.login}
                meta={countName(c.devices, "device")}
                metaStyle="label"
                status={
                  <>
                    <span className="sr-only">Fingerprint </span>
                    {c.verified ? "compared" : "not compared"}
                  </>
                }
                onClick={() => onSelect(c.account_id)}
              />
            ))}
          </div>
        </section>
      )}
      <AdaptiveDialog
        open={showing}
        onOpenChange={setShowing}
        size="question"
        overline="Sharing // your fingerprint"
        title="Your code"
        description="Read this out when someone compares fingerprints with you."
        footer={
          <Button type="button" variant="secondary" onClick={() => setShowing(false)}>
            Done
          </Button>
        }
      >
        <div className="flex flex-col items-start gap-4">
          <QrCode value={overview.fingerprint_qr} label="QR code of your fingerprint, for someone comparing with you" />
          <DigitGroups value={overview.fingerprint} size="md" label="Your fingerprint" />
        </div>
      </AdaptiveDialog>
    </>
  )
}

/**
 * One person (brief S2): who they are, their fingerprint and devices, what
 * support needs under DETAILS, and the three actions. On a phone COMPARE
 * sits at the end of the bottom row as an extended float button; the other
 * two are in the top bar's ⋯.
 */
function PersonDetail({ contact, reset, online, offline, onCompare, onChanged, onForgotten }: {
  contact: ContactView
  reset?: KeyResetView
  /** The server can be used now: online, and no split view pausing look-ups. */
  online: boolean
  offline: boolean
  onCompare: () => void
  onChanged: () => void
  onForgotten: () => void
}) {
  const sharing = useSharingClient()
  const frame = useAppFrame()
  const stacked = useViewport().widthClass === "compact"
  const [forgetting, setForgetting] = useState(false)
  const [phrase, setPhrase] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const reason = offline ? OFFLINE_REASON : undefined
  const compare = !contact.verified
  // The phone's chrome: COMPARE at the end of the bottom row, with its word; the rest behind ⋯.
  const end = useMemo(() => (compare ? <FloatButton label="Compare" icon="fingerprint-recognition" kind="primary" size="l" extended onClick={onCompare} /> : undefined), [compare, onCompare])
  const inRow = useDetailBottomRow({ end })
  const more = useMemo(
    () => (
      <OverflowMenu label={`More actions for ${contact.login}`} trigger={<FloatButton label={`More actions for ${contact.login}`} icon="overflow-menu--horizontal" size="l" />}>
        {!contact.verified && online && <AdaptiveMenuItem onSelect={() => setPhrase(true)}>Check with a phrase</AdaptiveMenuItem>}
        <AdaptiveMenuItem variant="destructive" onSelect={() => setForgetting(true)}>
          Forget
        </AdaptiveMenuItem>
      </OverflowMenu>
    ),
    [contact.login, contact.verified, online],
  )
  const inTopBar = useDetailTopBarHandUp(frame.phone ? more : null)
  const forget = async () => {
    try {
      await sharing.forget(contact.account_id)
      onForgotten()
    } catch (e) {
      setError(describeError(e))
      throw e
    }
  }
  return (
    <div className="flex flex-col gap-6">
      {paneHeading(`Person // ${contact.verified ? "compared" : "not compared"}`, contact.login)}
      {reset && <Banner tone="info" size="compact" icon="user--follow" title="Waiting on you." body={RESET_HINT} />}
      <div className="flex flex-col">
        <FieldRow size={stacked ? "stacked" : "default"} label="Fingerprint" rule="none" badge={contact.verified ? <Badge tone="success">Compared</Badge> : <Badge>Not compared</Badge>}>
          <DigitGroups value={contact.fingerprint} size="sm" label={`${contact.login}'s fingerprint`} />
        </FieldRow>
        <FieldRow size={stacked ? "stacked" : "default"} label="Devices">
          {count(contact.devices)}
        </FieldRow>
      </div>
      {(!inRow || !inTopBar) && (
        <div className="flex flex-wrap items-start gap-2">
          {compare && !inRow && (
            <Button type="button" onClick={onCompare}>
              Compare fingerprints
            </Button>
          )}
          {!inTopBar && !contact.verified && (
            <Button type="button" variant="secondary" disabled={!online} disabledReason={reason} onClick={() => setPhrase(true)}>
              Check with a phrase
            </Button>
          )}
          {!inTopBar && (
            <Button type="button" variant="ghost-destructive" onClick={() => setForgetting(true)}>
              Forget
            </Button>
          )}
        </div>
      )}
      <SectionHeader title="Details" collapsible summary={`Log entry ${contact.log_index}`}>
        <div className="flex flex-col">
          <FieldRow size={stacked ? "stacked" : "default"} label="Log entry" rule="none">
            {String(contact.log_index)}
          </FieldRow>
          <FieldRow size={stacked ? "stacked" : "default"} label="Account">
            {contact.account_id}
          </FieldRow>
          <FieldRow size={stacked ? "stacked" : "default"} label="Quantum-safe keys">
            {contact.post_quantum ? "Yes" : "No"}
          </FieldRow>
        </div>
      </SectionHeader>
      <SayPhrase login={contact.login} open={phrase} onClose={() => setPhrase(false)} onVerified={onChanged} />
      <ConfirmDialog
        open={forgetting}
        onOpenChange={(o) => {
          if (!o) {
            setForgetting(false)
            setError(null)
          }
        }}
        overline={`Person // ${contact.login}`}
        title={`Forget ${contact.login}?`}
        description="Their pinned keys are dropped from this device. What you already share with them stays shared; remove them from a vault to stop that. Looking them up again pins whatever keys the log has then."
        confirmLabel="Forget"
        tone="destructive"
        error={error ?? undefined}
        onConfirm={forget}
      />
    </div>
  )
}

/**
 * Compare fingerprints (brief S2): a Form. They read theirs out, or show
 * their QR, or send the code under it; what they read is checked here
 * against the keys pinned for them, and THEY MATCH marks them verified.
 * A mismatch says so under the field, in the core's words.
 */
function CompareFingerprints({ contact, onClose, onVerified }: { contact: ContactView | null; onClose: () => void; onVerified: (c: ContactView) => void }) {
  const sharing = useSharingClient()
  const scanner = useScanClient()
  const frame = useAppFrame()
  const [error, setError] = useState<string | null>(null)
  const [scanning, setScanning] = useState(false)
  const [busy, setBusy] = useState(false)
  const check = async (typed: string) => {
    if (!contact) return
    setError(null)
    setBusy(true)
    try {
      onVerified(await sharing.verify(contact.account_id, typed))
    } catch (err) {
      setError(describeError(err))
    } finally {
      setBusy(false)
    }
  }
  const submit = async (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    await check(String(new FormData(e.currentTarget).get("fingerprint") ?? ""))
  }
  // Their screen's QR code, read by the core from the camera's frames (D190's `lockdown:contact:v1:` payload).
  const onFrame = async (f: Frame) => {
    const qr = await scanner.contactFrame(f)
    if (!qr) return false
    setScanning(false)
    await check(qr)
    return true
  }
  const close = () => {
    setError(null)
    setScanning(false)
    onClose()
  }
  return (
    <AdaptiveDialog
      open={contact !== null}
      onOpenChange={(o) => !o && close()}
      size="form"
      overline="People // compare"
      title={`Compare fingerprints with ${contact?.login ?? ""}`}
      description="Ask them to read out their fingerprint, in person or on a call you trust."
      closeOnOutsideClick={false}
      footer={
        <>
          <Button type="button" variant="ghost" onClick={close}>
            Not now
          </Button>
          {frame.phone ? (
            <FloatButton label="They match" icon="checkmark" kind="primary" size="l" extended type="submit" form="compare-fingerprint" disabled={busy} />
          ) : (
            <Button type="submit" form="compare-fingerprint" loading={busy}>
              They match
            </Button>
          )}
        </>
      }
    >
      <div className="flex flex-col items-start gap-4">
        <form id="compare-fingerprint" aria-label="Compare fingerprints" className="w-full" onSubmit={(e) => void submit(e)}>
          <TextField
            label="What they read out"
            name="fingerprint"
            inputMode="numeric"
            autoComplete="off"
            helper={error ? undefined : "Spaces don't matter. The code under their QR can be pasted here too."}
            invalid={error !== null}
            aria-describedby={error ? "compare-error" : undefined}
          />
          {/* The mismatch, under the field, in the core's words; said at once. */}
          {error && (
            <p id="compare-error" role="alert" className="type-caption text-critical-text pt-1">
              {error}
            </p>
          )}
        </form>
        {scanning ? (
          <CameraView label="The camera's picture of their QR code" onFrame={onFrame} onError={setError} />
        ) : (
          <Button type="button" variant="ghost" onClick={() => setScanning(true)}>
            Scan their QR code
          </Button>
        )}
      </div>
    </AdaptiveDialog>
  )
}
