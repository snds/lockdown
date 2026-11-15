import { useCallback, useEffect, useState, type FormEvent } from "react"

import {
  AdaptiveDialog,
  Button,
  ConfirmDialog,
  FieldRow,
  FieldRowAction,
  RadioGroup,
  RadioRow,
  SectionHeader,
  TextField,
  ToggleRow,
} from "@unicron/ui"

import { useGate } from "@/app/gate"
import { usePlaces } from "@/app/places"
import { describeError, useClient } from "@/core/client"
import type { ApprovalRoute, DeviceInfo, LockState, RevealGesture, Status } from "@/core/types"
import { countName, rowDate } from "@/lib/format"
import { useLiveClient, type LiveClient } from "@/features/sessions/live-client"
import { useFirstRunClient, type QuickUnlock } from "@/features/unlock/first-run-client"

import { CLIPBOARD_CHOICES, ChangeSetting, SaveError, idleChoices, idleKey, idleSecs, useGroupLevel, useRowSize, useSettingSave } from "./setting-rows"

/** The reveal choices with today's sentences (D97). */
const REVEAL: ReadonlyArray<[RevealGesture, string, string]> = [
  ["hold", "Hold to reveal", "Hold shows a value only while you press; release hides it."],
  ["toggle", "Tap to reveal", "Tap shows it until you tap again or 30 seconds pass, for anyone who cannot hold a pointer still."],
]

/** The approvals route as a row's value, and today's sentence for it. */
export function approvalWords(route: ApprovalRoute | null): { value: string; line: string } {
  if (route === null) return { value: "Checking…", line: "Checking what this computer offers." }
  if (route.route === "platform") return { value: route.method, line: `Using or approving protected things is confirmed with ${route.method}. Revealing one asks for your master password.` }
  if (route.route === "password") return { value: "Master password", line: "This computer has no built-in check, so protected things ask for your master password." }
  return {
    value: "This window",
    line: "No account yet and no built-in check on this computer: protected things ask you to confirm in this window. Register an account to protect them with a password.",
  }
}

/** The kit's state in a row (brief T1). The kit is shown once and never kept, so it has no date and no Print. */
export function kitWords(recovery: string): string {
  if (recovery === "drilled") return "Tested"
  if (recovery === "made") return "Made, not tested"
  return "None yet"
}

interface SecurityRowsProps {
  surface: "desktop" | "web"
  /** The lock timers changed: the shell reads the core's lock state again. */
  onLock: (state: LockState) => void
}

/**
 * The SECURITY group of This Mac (brief T1), and of This browser in the
 * web vault: Auto-lock, the out-of-view lock, quick unlock and the
 * approvals route (desktop), Clipboard, Reveal and the name on the lock
 * screen (desktop). Every control saves at once (T0).
 */
export function SecurityRows({ surface, onLock }: SecurityRowsProps) {
  const level = useGroupLevel()
  const client = useClient()
  const frc = useFirstRunClient()
  const size = useRowSize()
  const relock = useCallback(async () => onLock(await client.lockState()), [client, onLock])
  const { settings, save, busy, errorOf } = useSettingSave(relock)
  const [approvals, setApprovals] = useState<ApprovalRoute | null>(null)
  const [quick, setQuick] = useState<QuickUnlock | null>(null)
  const [name, setName] = useState<string | null>(null)
  const desktop = surface === "desktop"
  const web = !desktop

  useEffect(() => {
    if (!desktop) return
    client.approvalRoute("use").then(setApprovals, () => setApprovals(null))
  }, [client, desktop])
  useEffect(() => {
    if (!frc) return
    frc.quickUnlock().then(setQuick, () => setQuick(null))
    client.probe().then((p) => setName(p.name ?? null), () => setName(null))
  }, [frc, client])

  // On the web, a tab with no idle time yet shows the default it runs on (10 minutes).
  const idle = web ? (settings.idle_lock_secs ?? 600) : settings.idle_lock_secs
  const approval = approvalWords(approvals)
  const quickName = quick?.kind === "touch_id" ? "Touch ID" : quick?.kind === "windows_hello" ? "Windows Hello" : null

  return (
    <div className="flex flex-col">
      <SectionHeader level={level} title="Security" />
      <ChangeSetting
        label="Auto-lock"
        choices={idleChoices(surface)}
        current={idleKey(idle)}
        onPick={(key) => void save("idle", { idle_lock_secs: idleSecs(key) })}
        busy={busy === "idle"}
        error={errorOf("idle")}
      />
      <div>
        <ToggleRow
          label={web ? "Lock when this tab is out of view" : "Lock when the window is out of view"}
          description="After 5 minutes."
          checked={settings.hidden_lock_secs !== null}
          onCheckedChange={(on) => void save("hidden", { hidden_lock_secs: on ? 300 : null })}
          disabled={busy === "hidden"}
        />
        <SaveError text={errorOf("hidden")} />
      </div>
      {quickName && (
        // No switch: the core has no setting that turns quick unlock on, so the row says where it stands (app spec, "no data").
        <FieldRow
          size={size}
          label={quickName}
          description={quick?.available ? "Opens the vault. Lockdown still asks for your master password at each start and every 14 days." : (quick?.why ?? undefined)}
        >
          {quick?.available ? "Available" : "Not yet"}
        </FieldRow>
      )}
      {desktop && (
        <FieldRow size={size} label="Approvals" description={approval.line}>
          {approval.value}
        </FieldRow>
      )}
      <ChangeSetting
        label="Clipboard"
        choices={CLIPBOARD_CHOICES}
        current={String(settings.clipboard_clear_secs)}
        onPick={(key) => void save("clipboard", { clipboard_clear_secs: Number(key) })}
        description={
          web
            ? "In a browser the copied value passes through this page. It is cleared if the clipboard still holds it, unless you copied something from Lockdown since."
            : "Cleared only if the clipboard still holds what Lockdown copied."
        }
        busy={busy === "clipboard"}
        error={errorOf("clipboard")}
      />
      <div>
        <FieldRow size="stacked" label="Reveal">
          <RadioGroup aria-label="Reveal" value={settings.reveal} onValueChange={(v) => v !== settings.reveal && void save("reveal", { reveal: v as RevealGesture })} disabled={busy === "reveal"}>
            {REVEAL.map(([value, label, line], i) => (
              <RadioRow key={value} value={value} label={label} description={line} rule={i === 0 ? "none" : "top"} />
            ))}
          </RadioGroup>
        </FieldRow>
        <SaveError text={errorOf("reveal")} />
      </div>
      {frc && name !== null && <NameRow name={name} onSaved={setName} save={(value) => frc.setName(value)} />}
      <p className="type-caption text-text-secondary pt-3">
        {web ? "Unlocking asks for the master password. Closing the tab ends everything at once." : "The vault also locks when this computer wakes from sleep."}
      </p>
    </div>
  )
}

/** YOUR NAME (brief T1): the name on the lock screen, changed in a name prompt with its own Save. */
function NameRow({ name, onSaved, save }: { name: string; onSaved: (name: string) => void; save: (name: string) => Promise<string> }) {
  const size = useRowSize()
  const [open, setOpen] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [saving, setSaving] = useState(false)

  const submit = (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    const value = String(new FormData(e.currentTarget).get("name") ?? "").trim()
    setSaving(true)
    setError(null)
    save(value).then(
      (saved) => {
        onSaved(saved)
        setOpen(false)
      },
      (err) => setError(describeError(err)),
    ).finally(() => setSaving(false))
  }

  return (
    <>
      <FieldRow
        size={size}
        label="Your name"
        description="Shown when Lockdown is locked. It stays on this computer."
        action={
          <FieldRowAction
            onClick={() => {
              setError(null)
              setOpen(true)
            }}
          >
            Change
          </FieldRowAction>
        }
      >
        {name || "Not set"}
      </FieldRow>
      <AdaptiveDialog
        open={open}
        onOpenChange={setOpen}
        size="question"
        overline="Your name"
        title="Change your name"
        description="Shown when Lockdown is locked. It stays on this computer."
      >
        <form onSubmit={submit} className="flex flex-col gap-6">
          <TextField label="Name" name="name" defaultValue={name} autoComplete="name" error={error ?? undefined} autoFocus />
          <div className="flex justify-end gap-3">
            <Button type="button" variant="secondary" onClick={() => setOpen(false)} disabled={saving}>
              Cancel
            </Button>
            <Button type="submit" loading={saving}>
              Save name
            </Button>
          </div>
        </form>
      </AdaptiveDialog>
    </>
  )
}

/** PASSKEYS (brief T1): the switch that exists; the system-wide provider is deferred (D122). */
export function PasskeyRows() {
  const level = useGroupLevel()
  const { settings, save, busy, errorOf } = useSettingSave()
  return (
    <div className="flex flex-col">
      <SectionHeader level={level} title="Passkeys" />
      <ToggleRow
        label="Offer Lockdown's passkeys in browsers"
        description="A paired browser offers the passkeys in Lockdown and can save new ones here."
        checked={settings.browser_passkeys ?? true}
        onCheckedChange={(on) => void save("passkeys", { browser_passkeys: on })}
        disabled={busy === "passkeys"}
      />
      <SaveError text={errorOf("passkeys")} />
    </div>
  )
}

/** The devices This Mac lists: live ones, this one first, at most three. */
export function firstDevices(devices: DeviceInfo[]): DeviceInfo[] {
  const live = devices.filter((d) => !d.revoked_at)
  return [...live.filter((d) => d.this_device), ...live.filter((d) => !d.this_device)].slice(0, 3)
}

/**
 * DEVICES NN (brief T1): the first three devices, this one first, each other
 * one with REVOKE behind a Question and the gate (D101); ALL opens Devices.
 * A device has no "last seen" in the core, so the others show when they were added.
 */
export function DeviceRows({ onChanged, onNotice }: { onChanged: () => void; onNotice: (text: string) => void }) {
  const level = useGroupLevel()
  const client = useClient()
  const gate = useGate()
  const { go } = usePlaces()
  const size = useRowSize()
  const [devices, setDevices] = useState<DeviceInfo[] | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [revoking, setRevoking] = useState<DeviceInfo | null>(null)
  const [revokeError, setRevokeError] = useState<string | null>(null)

  const load = useCallback(
    () =>
      client.deviceList().then(
        (d) => {
          setDevices(d)
          setError(null)
        },
        (e) => setError(describeError(e)),
      ),
    [client],
  )
  useEffect(() => {
    void load()
  }, [load])

  const live = devices?.filter((d) => !d.revoked_at) ?? []
  const revoke = async (d: DeviceInfo) => {
    setRevokeError(null)
    try {
      const lines = await gate.run((g) => client.deviceRevoke(d.id, g))
      onNotice(lines.join(" "))
      await load()
      onChanged()
    } catch (e) {
      setRevokeError(describeError(e))
      throw e
    }
  }

  return (
    <div className="flex flex-col">
      <SectionHeader
        level={level}
        title="Devices"
        count={devices ? live.length : undefined}
        action={
          <Button variant="ghost" size="sm" onClick={() => go("settings-devices")}>
            All
          </Button>
        }
      />
      {error && (
        <p role="alert" className="type-caption text-critical-text py-3">
          {error}
        </p>
      )}
      {devices &&
        firstDevices(devices).map((d) => (
          <FieldRow
            key={d.id}
            size={size}
            label={d.name}
            action={
              d.this_device ? undefined : (
                <FieldRowAction
                  onClick={() => {
                    setRevokeError(null)
                    setRevoking(d)
                  }}
                >
                  Revoke
                </FieldRowAction>
              )
            }
          >
            {d.this_device ? "Now" : `Added ${rowDate(d.enrolled_at)}`}
          </FieldRow>
        ))}
      <ConfirmDialog
        open={revoking !== null}
        onOpenChange={(o) => !o && setRevoking(null)}
        overline={revoking?.name}
        title={`Revoke ${revoking?.name ?? "this device"}?`}
        description="Nothing it signs from now on is accepted by the server or by any other device. A revocation never comes back. You will be asked to confirm it is you."
        confirmLabel="Revoke"
        tone="destructive"
        error={revokeError}
        onConfirm={() => (revoking ? revoke(revoking) : undefined)}
      />
    </div>
  )
}

/**
 * ACCOUNT (brief T1): the recovery kit's state with OPEN (Recovery), and
 * the other sessions with END ALL behind a Question. This device stays
 * signed in, so the words say "other". A local account shows the kit only.
 */
export function AccountRows({ status, onNotice }: { status: Status; onNotice: (text: string) => void }) {
  const level = useGroupLevel()
  const { go } = usePlaces()
  const size = useRowSize()
  const live = useLiveClient()
  const remote = Boolean(status.account && !status.account.local)
  return (
    <div className="flex flex-col">
      <SectionHeader level={level} title="Account" />
      <FieldRow
        size={size}
        label="Recovery kit"
        action={<FieldRowAction onClick={() => go("settings-recovery")}>Open</FieldRowAction>}
      >
        {kitWords(status.recovery)}
      </FieldRow>
      {remote && live && <OtherSessions live={live} onNotice={onNotice} />}
    </div>
  )
}

function OtherSessions({ live, onNotice }: { live: LiveClient; onNotice: (text: string) => void }) {
  const size = useRowSize()
  const [others, setOthers] = useState<number | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [asking, setAsking] = useState(false)
  const load = useCallback(
    () =>
      live.sessionList().then(
        (s) => {
          setOthers(s.filter((x) => !x.current).length)
          setError(null)
        },
        (e) => setError(describeError(e)),
      ),
    [live],
  )
  useEffect(() => {
    void load()
  }, [load])

  const endAll = async () => {
    setError(null)
    try {
      const n = await live.sessionEndOthers()
      onNotice(n === 1 ? "Ended 1 other session" : `Ended ${n} other sessions`)
      await load()
    } catch (e) {
      setError(describeError(e))
      throw e
    }
  }

  return (
    <div>
      <FieldRow
        size={size}
        label="Other sessions"
        action={others ? <FieldRowAction onClick={() => setAsking(true)}>End all</FieldRowAction> : undefined}
      >
        {others === null ? "Checking…" : others === 0 ? "None" : countName(others, "other", "others")}
      </FieldRow>
      {!asking && <SaveError text={error} />}
      <ConfirmDialog
        open={asking}
        onOpenChange={setAsking}
        title="End all other sessions?"
        description={`${others === null ? "Every other device" : `${countName(others, "device")}`} must log in again with the password. This device stays signed in.`}
        confirmLabel="End all"
        tone="destructive"
        error={asking ? error : null}
        onConfirm={endAll}
      />
    </div>
  )
}

/**
 * This Mac (brief T1, Desktop / 12 in one column, Q10): Security, then
 * Passkeys, then Devices, then Account.
 */
export function ThisDevice({ status, onLock, onChanged, onNotice }: { status: Status; onLock: (state: LockState) => void; onChanged: () => void; onNotice: (text: string) => void }) {
  return (
    <div className="flex flex-col gap-8">
      <SecurityRows surface="desktop" onLock={onLock} />
      <PasskeyRows />
      <DeviceRows onChanged={onChanged} onNotice={onNotice} />
      <AccountRows status={status} onNotice={onNotice} />
    </div>
  )
}
