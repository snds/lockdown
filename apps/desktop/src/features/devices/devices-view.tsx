import { useCallback, useEffect, useRef, useState } from "react"

import {
  Badge,
  Button,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
  Icon,
  Item,
  ItemActions,
  ItemContent,
  ItemDescription,
  ItemGroup,
  ItemTitle,
  Spinner,
} from "@unicron/ui"

import { useGate } from "@/app/gate"
import { LabelledAction, LabelledActions } from "@/components/labelled-actions"
import { PaneHeader } from "@/components/master-detail"
import { describeError, useClient } from "@/core/client"
import type { DeviceInfo, Status } from "@/core/types"
import { dateLocal, shortId } from "@/lib/format"
import { SessionsSection } from "@/features/sessions/sessions-section"
import { AddDevice } from "@/features/depth/add-device"
import { SecurityKeysSection } from "@/features/depth/security-keys"

interface DevicesViewProps {
  status: Status
  onChanged: () => void
  onNotice: (text: string) => void
  /** A device to bring into view (a link from the Security screen's post-quantum line). */
  focus?: string
}

/**
 * The Devices screen (brief §3.2): every device of the identity, with
 * Revoke behind a confirm and the Approve gate (D101, "revoke needs a
 * fresh check"). Enrolling a new device happens on that device.
 */
export function DevicesView({ status, onChanged, onNotice, focus }: DevicesViewProps) {
  const client = useClient()
  const gate = useGate()
  const [devices, setDevices] = useState<DeviceInfo[] | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [revoking, setRevoking] = useState<DeviceInfo | null>(null)
  const [busy, setBusy] = useState(false)

  const focused = useRef<HTMLDivElement | null>(null)
  const load = useCallback(() => client.deviceList().then(setDevices).catch((e) => setError(describeError(e))), [client])
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

  const revoke = async (id: string) => {
    setBusy(true)
    setError(null)
    try {
      const lines = await gate.run((g) => client.deviceRevoke(id, g))
      onNotice(lines.join(" "))
      await load()
      onChanged()
    } catch (e) {
      setError(describeError(e))
    } finally {
      setBusy(false)
    }
  }

  const live = devices?.filter((d) => !d.revoked_at) ?? []
  return (
    <div className="mx-auto flex w-full max-w-3xl flex-col gap-4 p-4 md:p-6">
      <PaneHeader title={`Devices (${status.devices}${status.devices_revoked ? `, ${status.devices_revoked} revoked` : ""})`} />
      {error && (
        <p role="alert" className="text-sm text-destructive">
          {error}
        </p>
      )}
      {status.account && (
        <div>
          <AddDevice onNotice={onNotice} onChanged={() => void load().then(onChanged)} />
        </div>
      )}
      {!devices && !error && <Spinner />}
      {devices && live.length <= 1 && (
        <Empty>
          <EmptyHeader>
            <EmptyMedia variant="icon">
              <Icon name="devices" />
            </EmptyMedia>
            <EmptyTitle>Only this device</EmptyTitle>
            <EmptyDescription>
              Add a second device in your first week: choose “Add a device” here and join from the new one, or join there with your login name,
              password and Secret Key. A second device is what gets you back in when this one is lost.
            </EmptyDescription>
          </EmptyHeader>
        </Empty>
      )}
      <ItemGroup role="list" aria-label="Devices">
        {devices?.map((d) => (
          <Item
            key={d.id}
            role="listitem"
            size="sm"
            variant={d.id === focus ? "muted" : "outline"}
            data-row=""
            ref={d.id === focus ? focused : undefined}
            tabIndex={d.id === focus ? -1 : undefined}
            aria-current={d.id === focus ? "true" : undefined}
          >
            <ItemContent>
              <ItemTitle>
                {d.name}
                {d.this_device && <Badge variant="secondary">this device</Badge>}
                {d.revoked_at && <Badge variant="destructive">revoked {dateLocal(d.revoked_at)}</Badge>}
              </ItemTitle>
              <ItemDescription>
                {d.platform} · enrolled {dateLocal(d.enrolled_at)} · {shortId(d.id)}
              </ItemDescription>
            </ItemContent>
            {!d.this_device && !d.revoked_at && (
              <ItemActions>
                <Button size="sm" variant="ghost-destructive" onClick={() => setRevoking(d)} disabled={busy}>
                  Revoke
                </Button>
              </ItemActions>
            )}
          </Item>
        ))}
      </ItemGroup>
      {status.account && <SessionsSection onNotice={onNotice} />}
      {status.account && <SecurityKeysSection onNotice={onNotice} />}
      <Dialog open={revoking !== null} onOpenChange={(o) => !o && setRevoking(null)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Revoke {revoking?.name}?</DialogTitle>
            <DialogDescription>
              Nothing it signs from now on is accepted by the server or by any other device. Its earlier history still verifies. A revocation never
              comes back. You will be asked to confirm it is you.
            </DialogDescription>
          </DialogHeader>
          <LabelledActions>
            <LabelledAction kind="cancel" onClick={() => setRevoking(null)}>
              Cancel
            </LabelledAction>
            <LabelledAction
              kind="destructive"
              onClick={() => {
                const id = revoking?.id
                setRevoking(null)
                if (id) void revoke(id)
              }}
            >
              Revoke
            </LabelledAction>
          </LabelledActions>
        </DialogContent>
      </Dialog>
    </div>
  )
}
