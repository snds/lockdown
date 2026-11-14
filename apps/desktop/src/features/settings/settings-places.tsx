import type { ReactNode } from "react"

import { AppFrameInset, AppFramePageHead, SettingsFrame, useAppFrame } from "@unicron/ui"

import { deviceName, settingsSections, usePlaces, type SettingsSection } from "@/app/places"
import { isMac } from "@/features/palette/palette-model"

/** The Settings groups' overlines; the device's is its real kind (cross-app decisions §5). */
const GROUP_TITLES = { account: "Account", vault: "Vault", lockdown: "Lockdown" } as const

interface SettingsPlacesProps {
  /** On a phone and at medium width: the device's own rows, drawn first, above the list of every other section (brief T0). */
  overview?: ReactNode
  /** The open section's content. */
  children: (shown: SettingsSection) => ReactNode
}

/**
 * Settings as places (brief T0, doc 64): unicron's `SettingsFrame` over the
 * sections `places.ts` lists for this build, with the open one read from
 * and written to the shell's `Nav`. The desktop app and the web vault both
 * mount it and give it their own sections' content. With room for two
 * columns the section list stands beside the open section; narrower, the
 * device's rows come first, then the list, and a section is a pushed screen.
 */
export function SettingsPlaces({ overview, children }: SettingsPlacesProps) {
  const { nav, surface, go } = usePlaces()
  const frame = useAppFrame()
  const device = deviceName(surface, isMac())
  const all = settingsSections(surface)
  // Narrower than two columns the device's rows are the overview, so its section is not listed twice.
  const listed = frame.split || !overview ? all : all.filter((p) => p.nav.section !== "this-device")
  const sections = listed.map((p) => ({
    value: p.nav.section as string,
    label: p.nav.section === "this-device" ? device : p.label,
    icon: p.icon,
    group: p.settingsGroup === "device" ? device : GROUP_TITLES[p.settingsGroup ?? "account"],
    description: p.description,
  }))
  const open = nav.section && listed.some((p) => p.nav.section === nav.section) ? nav.section : null
  const shown = open ?? (frame.split ? (listed[0]?.nav.section ?? null) : null)
  return (
    <SettingsFrame
      sections={sections}
      value={open}
      onValueChange={(value) => go(value ? `settings-${value}` : "settings")}
      overview={overview ? <AppFrameInset>{overview}</AppFrameInset> : undefined}
      head={frame.phone ? undefined : <AppFramePageHead title="Settings" size="sm" gap="sm" />}
    >
      {shown ? <AppFrameInset>{children(shown)}</AppFrameInset> : null}
    </SettingsFrame>
  )
}
