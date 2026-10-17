import type { ReactNode } from "react"

import { CrownJewelSurface } from "@unicron/ui/security/crown-jewel"

import { useGate } from "@/app/gate"

interface ItemCrownJewelProps {
  /** Names the thing for the region and the gate's reason: the item's title. */
  title: string
  children: ReactNode
}

/**
 * unicron's CrownJewelSurface on this host (D98, D101): `host="app"` (the
 * placement is the guarantee: never a page), the proof routed through the
 * core's `approval_route` and judged by the core when the gated call
 * arrives (`app/gate.tsx`), the biometric named as the platform names it
 * (Touch ID, Windows Hello). Where this computer has no platform check
 * the route is the account password: `biometricName={null}`, and the
 * surface's copy and announcement say the password is asked for both use
 * and reveal. Nested SecretField, CodeDisplay and LargeTypeView read the
 * surface and switch to gated mode on their own (a SecretField's reveal
 * becomes a tap with the surface's 20 s lifetime).
 */
export function ItemCrownJewel({ title, children }: ItemCrownJewelProps) {
  const gate = useGate()
  const biometricName = gate.route?.route === "platform" ? gate.route.method : null
  return (
    <CrownJewelSurface
      host="app"
      appName="Lockdown"
      title={title}
      biometricName={biometricName}
      requestProof={(kind) => gate.requestProof(kind, `To ${kind === "use" ? "use" : "show"} ${title}`)}
    >
      {children}
    </CrownJewelSurface>
  )
}
