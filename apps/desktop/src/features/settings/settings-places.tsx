import { useState, type ReactNode } from "react"

import { AppFrameInset, AppFramePageHead, NavItem, SearchField, SectionHeader, SettingsFrame, useAppFrame, type NavMarkProp } from "@unicron/ui"

import { useCrumb } from "@/app/crumbs"
import { NeedsYouSlot, useNeedsYouBlock } from "@/app/needs-you"
import { deviceName, settingsSections, usePlaces, type SettingsSection } from "@/app/places"
import { isMac } from "@/features/palette/palette-model"
import { count } from "@/lib/format"

import { findSettings } from "./settings-index"

/** The Settings groups' overlines; the device's is its real kind (cross-app decisions §5). */
const GROUP_TITLES = { account: "Account", vault: "Vault", lockdown: "Lockdown" } as const

/** Section heads' lines where the brief words them (T0, T10); the rest are the places table's. */
const LINES: Partial<Record<SettingsSection, string>> = {
  "this-device": "How Lockdown locks and copies on this computer.",
}

interface SettingsPlacesProps {
  /** On a phone: the device's own rows, drawn first, above "Find a setting" and the list of every other section (brief T0, Mobile / 16). */
  overview?: ReactNode
  /** A mark on a section's row: Recovery while the kit is untested, About while an update is ready (brief T0). */
  marks?: Partial<Record<SettingsSection, NavMarkProp>>
  /** The open section's content. */
  children: (shown: SettingsSection) => ReactNode
}

/**
 * Settings as places (brief T0, doc 64, doc 79): unicron's `SettingsFrame`
 * over the sections `places.ts` lists for this build, with the open one
 * read from and written to the shell's `Nav`. The desktop app and the web
 * vault both mount it and give it their own sections' content.
 *
 * - Expanded: the section list beside the open section (This Mac first),
 *   and the breadcrumb `SETTINGS // THIS MAC`.
 * - Phone and medium: the drawn rows first (`overview`), then "Find a setting",
 *   `ALL SETTINGS NN` and the other sections; each pushes.
 * - "Find a setting" from ten sections up: what it finds shows under the
 *   field, each a way to its section.
 */
export function SettingsPlaces({ overview, marks, children }: SettingsPlacesProps) {
  const { nav, surface, go } = usePlaces()
  const frame = useAppFrame()
  const needsYou = useNeedsYouBlock()
  const [query, setQuery] = useState("")
  const device = deviceName(surface, isMac())
  const all = settingsSections(surface)
  // Narrower than two columns (a phone, and medium width as found-3 built it) the device's rows are the overview, so its section is not listed twice.
  const drawsOverview = !frame.split && Boolean(overview)
  const listed = drawsOverview ? all.filter((p) => p.nav.section !== "this-device") : all
  const labelOf = (section: SettingsSection | undefined, label: string) => (section === "this-device" ? device : label)
  const sections = listed.map((p) => ({
    value: p.nav.section as string,
    label: labelOf(p.nav.section, p.label),
    icon: p.icon,
    group: p.settingsGroup === "device" ? device : GROUP_TITLES[p.settingsGroup ?? "account"],
    description: surface === "web" && p.nav.section === "this-device" ? "How Lockdown locks and copies in this tab." : (LINES[p.nav.section as SettingsSection] ?? p.description),
    mark: p.nav.section ? marks?.[p.nav.section] : undefined,
  }))
  const open = nav.section && listed.some((p) => p.nav.section === nav.section) ? nav.section : null
  const shown = open ?? (frame.split ? (listed[0]?.nav.section ?? null) : null)
  // The first section shows without being chosen: the breadcrumb still names it.
  useCrumb(frame.split && !open && shown ? [labelOf(shown, sections[0]?.label ?? "")] : null)

  const sectionLabel = (section: SettingsSection) => labelOf(section, all.find((p) => p.nav.section === section)?.label ?? "")
  const found = findSettings(
    surface,
    query,
    // Every section, the phone's drawn rows too: what they hold is found as well.
    all.map((p) => ({ section: p.nav.section as SettingsSection, label: labelOf(p.nav.section, p.label), description: p.description })),
  )
  const search = (
    <div className="flex flex-col gap-6">
      <SearchField aria-label="Find a setting" placeholder="Find a setting" value={query} onValueChange={setQuery} />
      {query.trim() && (
        <div className="flex flex-col gap-1" role="region" aria-label="Settings found">
          <SectionHeader level={2} title="Found" count={found.length} />
          {found.length === 0 ? (
            <p className="type-body text-text-secondary py-3">No setting for {query.trim()}.</p>
          ) : (
            <ul role="list" className="flex flex-col gap-1">
              {found.map((f) => (
                <li key={`${f.section}:${f.name}`}>
                  <NavItem
                    label={f.name}
                    description={f.name === sectionLabel(f.section) ? undefined : sectionLabel(f.section)}
                    icon={all.find((p) => p.nav.section === f.section)?.icon}
                    onClick={() => {
                      setQuery("")
                      if (drawsOverview && f.section === "this-device") {
                        // On a phone the device's rows are the top of this screen, not a section of their own.
                        go("settings")
                        document.querySelector('[data-slot="settings-frame-overview"]')?.scrollIntoView({ block: "start" })
                      } else go(`settings-${f.section}`)
                    }}
                  />
                </li>
              ))}
            </ul>
          )}
        </div>
      )}
      {drawsOverview && <SectionHeader level={2} title="All settings" count={count(listed.length)} />}
    </div>
  )

  return (
    <SettingsFrame
      sections={sections}
      value={open}
      onValueChange={(value) => go(value ? `settings-${value}` : "settings")}
      search={search}
      overview={drawsOverview ? <AppFrameInset>{overview}</AppFrameInset> : undefined}
      // The head stands over the 204 section list, so its title is display/compact, as Desktop / 12 draws it; lg would cut it.
      head={frame.phone ? undefined : <AppFramePageHead title="Settings" size="md" gap="sm" />}
    >
      {shown ? (
        <AppFrameInset>
          {/* A section pushed at medium width: the frame's section head is a level below and no top bar names it,
              so the screen's one h1 is the section's name (brief 4.0). The phone's top bar carries it there. */}
          {open && !frame.split && !frame.phone && <h1 className="sr-only">{sectionLabel(open)}</h1>}
          {/* What waits on the person, at the top of the section (brief X3). */}
          {needsYou ? (
            <div className="flex flex-col gap-6">
              <NeedsYouSlot />
              {children(shown)}
            </div>
          ) : (
            children(shown)
          )}
        </AppFrameInset>
      ) : null}
    </SettingsFrame>
  )
}
