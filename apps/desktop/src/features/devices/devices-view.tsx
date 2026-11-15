import { useCallback, useEffect, useRef, useState } from "react"

import { Badge, Banner, Button, ConfirmDialog, FieldRow, FieldRowAction, SectionHeader, Spinner, useViewport } from "@unicron/ui"

import { useGate } from "@/app/gate"
import { usePlaces } from "@/app/places"
import { describeError, useClient } from "@/core/client"
import type { DeviceInfo, Status } from "@/core/types"
import { fullDate, rowDate } from "@/lib/format"
import { isCancelled } from "@/features/approvals/requests"
import { AddDevice } from "@/features/depth/add-device"
import { SecurityKeysSection } from "@/features/depth/security-keys"
import { SessionsSection, sessionDevice, useSessionList } from "@/features/sessions/sessions-section"
import type { SessionView } from "@/features/sessions/live-client"

import { lastUse, platformName } from "./devices-model"

interface DevicesViewProps {
  status: Status
  onChanged: () => void
  onNotice: (text: string) => void
  /** A device to bring into view (a link from the Health screen's post-quantum line). */
  focus?: string
}

/** This device first, then the live ones as the core lists them, the revoked ones last. */
function ordered(devices: DeviceInfo[]): DeviceInfo[] {
  const rank = (d: DeviceInfo) => (d.this_device ? 0 : d.revoked_at ? 2 : 1)
  return [...devices].sort((a, b) => rank(a) - rank(b))
}

/** A device's value: "Now" for this one, its last use from its session, or the day it was added. */
function deviceValue(d: DeviceInfo, sessions: SessionView[] | null | undefined): string {
  if (d.this_device) return "Now"
  const last = d.revoked_at ? null : lastUse(d, sessions)
  return last ? rowDate(last) : `Added ${fullDate(d.enrolled_at)}`
}

/**
 * Settings → Devices (brief T2, doc 85): every device of the identity as a
 * Field Row, this one first, with its platform, its last use (from the
 * session list) and a REVOKE on a live device that is not this one. REVOKE
 * asks a Question first (brief X5); the core then runs its own `Approve`
 * gate (D101, "revoke needs a fresh check"), which the app does not add
 * to. Under the list, ADD DEVICE; then Signed in and Security keys where
 * the account has a server; the ids at the foot, under Details (R14).
 */
export function DevicesView({ status, onChanged, onNotice, focus }: DevicesViewProps) {
  const client = useClient()
  const gate = useGate()
  const { go } = usePlaces()
  // A phone puts the label over the value, so a date or a name is never cut (FieldRow `stacked`).
  const rowSize = useViewport().widthClass === "compact" ? "stacked" : "default"
  const sessions = useSessionList()
  const [devices, setDevices] = useState<DeviceInfo[] | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [revoking, setRevoking] = useState<DeviceInfo | null>(null)
  const [busy, setBusy] = useState(false)

  const focused = useRef<HTMLDivElement | null>(null)
  const load = useCallback(
    () =>
      client
        .deviceList()
        .then(setDevices)
        .catch((e) => setError(describeError(e))),
    [client],
  )
  useEffect(() => {
    void load()
  }, [load])

  // The device a link named: scrolled to and focused once the list is in.
  useEffect(() => {
    const row = focused.current
    if (!row) return
    row.scrollIntoView?.({ block: "center" })
    row.focus()
  }, [devices, focus])

  const revoke = async (d: DeviceInfo) => {
    setBusy(true)
    setError(null)
    try {
      const lines = await gate.run((g) => client.deviceRevoke(d.id, g))
      onNotice(lines.join(" "))
      await load()
      onChanged()
    } catch (e) {
      if (!isCancelled(e)) setError(describeError(e))
    } finally {
      setBusy(false)
    }
  }

  const server = !!status.account && !status.account.local
  const local = !!status.account?.local
  const live = devices?.filter((d) => !d.revoked_at) ?? []
  return (
    <div className="flex w-full flex-col gap-(--c-section-gap)">
      {error && <Banner size="compact" title={error} />}
      <section aria-label="Devices" className="flex flex-col">
        <SectionHeader title="Devices" count={devices ? live.length : undefined} />
        {!devices && !error && <Spinner />}
        <div role="list" aria-label="Devices">
          {devices &&
            ordered(devices).map((d) => (
              <div
                key={d.id}
                role="listitem"
                ref={d.id === focus ? focused : undefined}
                tabIndex={d.id === focus ? -1 : undefined}
                aria-current={d.id === focus ? "true" : undefined}
                className={d.id === focus ? "bg-primary-soft edge-l outline-none" : undefined}
              >
                <FieldRow
                  size={rowSize}
                  label={d.name}
                  description={platformName(d.platform)}
                  badge={
                    d.this_device ? <Badge>This device</Badge> : d.revoked_at ? <Badge tone="critical">{`Revoked ${fullDate(d.revoked_at)}`}</Badge> : undefined
                  }
                  action={
                    !d.this_device && !d.revoked_at ? (
                      <FieldRowAction variant="ghost-destructive" onClick={() => setRevoking(d)} disabled={busy}>
                        Revoke
                      </FieldRowAction>
                    ) : undefined
                  }
                >
                  {deviceValue(d, sessions?.sessions)}
                </FieldRow>
              </div>
            ))}
        </div>
        {devices && live.length <= 1 && !local && (
          <p className="type-body text-text-secondary py-3">Only this device. A second device is what gets you back in when this one is lost.</p>
        )}
        {server && (
          <div className="flex flex-col items-start gap-2 pt-3">
            <AddDevice onNotice={onNotice} onChanged={() => void load().then(onChanged)} />
          </div>
        )}
        {local && (
          <div className="flex flex-col items-start gap-2 pt-3">
            <p className="type-body text-text-secondary">To use Lockdown on another device, both need a server they can reach.</p>
            <Button variant="secondary" onClick={() => go("settings-server")}>
              Set up server
            </Button>
          </div>
        )}
      </section>
      {server && <SessionsSection onNotice={onNotice} list={sessions} />}
      {server && <SecurityKeysSection onNotice={onNotice} />}
      {devices && devices.length > 0 && (
        <SectionHeader title="Details" collapsible summary="Device and session ids">
          {devices.map((d) => (
            <FieldRow size={rowSize} key={d.id} label={d.name}>
              {d.id}
            </FieldRow>
          ))}
          {server &&
            sessions?.sessions?.map((s) => (
              <FieldRow size={rowSize} key={s.id} label={`Session // ${sessionDevice(s)}`}>
                {s.id}
              </FieldRow>
            ))}
        </SectionHeader>
      )}
      <ConfirmDialog
        open={revoking !== null}
        onOpenChange={(o) => !o && setRevoking(null)}
        overline={revoking ? `${revoking.name} // ${platformName(revoking.platform)}` : "Device"}
        title={`Revoke ${revoking?.name ?? "this device"}?`}
        description="It can't open the vault or sync from now on. What it signed before still verifies. A revocation never comes back."
        confirmLabel="Revoke"
        tone="destructive"
        onConfirm={() => {
          const d = revoking
          setRevoking(null)
          if (d) void revoke(d)
        }}
      />
    </div>
  )
}
