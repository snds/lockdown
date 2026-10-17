import { useCallback, useState } from "react"

import { Button, Icon } from "@unicron/ui"
import { LargeTypeView } from "@unicron/ui/security/large-type-view"

import { useGate } from "@/app/gate"
import { useClient } from "@/core/client"

/** How long large type stays up before it hides itself (brief §3.2; `--c-large-type-lifetime`). */
export const LARGE_TYPE_LIFETIME_S = 120

interface LargeTypeButtonProps {
  /** What is shown, never the value: "Wi-Fi password · Home". */
  title: string
  /** One-shot bytes, as SecretField's `getSecret`. */
  getSecret: () => Promise<Uint8Array>
  groupSize?: 3 | 4
}

/**
 * "Show large type": unicron's LargeTypeView as a fullscreen sheet, read
 * across a room or typed into a TV (brief §3.2). The value comes from the
 * host as bytes and hides after two minutes, on blur and on lock. Keeping
 * the screen awake and raising brightness are a phone's hooks; on desktop
 * they are no-ops, left out here.
 */
export function LargeTypeButton({ title, getSecret, groupSize = 3 }: LargeTypeButtonProps) {
  const [open, setOpen] = useState(false)
  return (
    <>
      <Button variant="ghost" size="sm" onClick={() => setOpen(true)}>
        <Icon name="format_size" data-icon="inline-start" />
        Show large type
      </Button>
      <LargeTypeView open={open} onOpenChange={setOpen} getSecret={getSecret} title={title} groupSize={groupSize} lifetime={LARGE_TYPE_LIFETIME_S} />
    </>
  )
}

/** Large type for one hidden field of an item, through the core's `reveal`. */
export function FieldLargeType({ itemId, fieldId, title }: { itemId: string; fieldId: string; title: string }) {
  const client = useClient()
  const gate = useGate()
  const getSecret = useCallback(() => gate.run((g) => client.reveal(itemId, fieldId, g)), [client, gate, itemId, fieldId])
  return <LargeTypeButton title={title} getSecret={getSecret} />
}
