import type { ReactNode } from "react"

import { AppFrame, Toaster } from "@unicron/ui"

import { NeedsYouSlot, NeedsYouStrip, WaitingHost, type NeedsYouPlace } from "@/app/needs-you"

export { ARMED_MS } from "./requests"

const everywhere: NeedsYouPlace = { sharing: false, item: null }

/**
 * For the request tests (doc 84): the shell's needs-you around a request
 * source, as the unlocked app draws it: the block wider, the strip on a
 * phone, and the toasts.
 */
export function RequestHost({ phone = false, children }: { phone?: boolean; children: ReactNode }) {
  return (
    <AppFrame layout={phone ? "phone" : "expanded"}>
      <WaitingHost place={everywhere}>
        <NeedsYouSlot />
        <NeedsYouStrip />
        {children}
      </WaitingHost>
      <Toaster />
    </AppFrame>
  )
}
