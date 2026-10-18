import type { ReactNode } from "react"

import { Button, Icon, NativeSelect, NativeSelectOption, Separator, type IconName } from "@unicron/ui"

import type { PersonaInfo } from "@/core/types"
import type { Scope } from "@/features/organization/org-model"
import { VIEW_LABELS, type ListView } from "@/features/vault/list-model"

/** The areas of the window (brief §3): the vault's views, and the screens (Sharing: docs/plan/18-sharing.md). */
export type Area = "vault" | "codes" | "security" | "devices" | "recovery" | "status" | "settings" | "sharing" | "people"

export interface Nav {
  area: Area
  /** Which list the vault shows; only read while `area` is `vault`. */
  view: ListView
  /** A vault, folder or tag the list is narrowed to (features/organization). */
  scope?: Scope
}

interface AppShellProps {
  nav: Nav
  personas: PersonaInfo[]
  counts: Record<ListView, number>
  onNavigate: (nav: Nav) => void
  onLock: () => void
  /** The vaults, folders and tags sections, under the smart views. */
  organization?: ReactNode
  /** The screens under the views; the desktop's five unless a shell names fewer (the web vault). */
  screens?: ReadonlyArray<ShellScreen>
  /** A line above Lock (the last-backup indicator, docs/plan/23-bridges.md §10). */
  footer?: ReactNode
  children: ReactNode
}

/** One screen in the sidebar: where it goes, its icon, its name. */
export type ShellScreen = readonly [Area, IconName, string]

const VIEWS: Array<[ListView, IconName]> = [
  ["all", "inventory_2"],
  ["favourites", "star"],
  ["passkeys", "passkey"],
  ["codes", "timer"],
  ["wi-fi", "wifi"],
  ["contacts", "contact_page"],
  ["expiring", "schedule"],
  ["attention", "warning"],
  ["archived", "archive"],
  ["deleted", "delete"],
]

const SCREENS: ReadonlyArray<ShellScreen> = [
  ["security", "security", "Security"],
  ["sharing", "group", "Sharing"],
  ["devices", "devices", "Devices"],
  ["recovery", "health_and_safety", "Recovery"],
  ["status", "shield", "Status"],
  ["settings", "settings", "Settings"],
]

/**
 * The sidebar of brief §3: persona switcher (a read-only list for now),
 * All items and the smart views with counts, then Security · Sharing ·
 * Devices · Recovery · Status · Settings, and Lock. Codes is a view of the vault that opens its
 * own screen, because a code is usually part of a Login (brief §3). Rows
 * take the density knobs through `data-row`.
 */
export function AppShell({ nav, personas, counts, onNavigate, onLock, organization, screens = SCREENS, footer, children }: AppShellProps) {
  const active = personas.find((p) => p.active)
  return (
    <div className="flex h-full min-h-0 flex-col md:flex-row">
      <nav
        aria-label="Sidebar"
        className="flex shrink-0 flex-row items-center gap-1 overflow-x-auto border-b bg-surface-chrome p-2 md:w-(--c-shell-sidebar-width) md:flex-col md:items-stretch md:overflow-y-auto md:border-r md:border-b-0"
      >
        <div className="hidden px-1 pb-2 md:block">
          <label htmlFor="persona" className="sr-only">
            Persona
          </label>
          <NativeSelect id="persona" value={active?.id ?? ""} disabled aria-describedby="persona-note" onChange={() => {}}>
            {personas.map((p) => (
              <NativeSelectOption key={p.id} value={p.id}>
                {p.label} · {p.kind}
              </NativeSelectOption>
            ))}
          </NativeSelect>
          <p id="persona-note" className="sr-only">
            Switching personas comes in a later slice; this list is read-only.
          </p>
        </div>
        {VIEWS.map(([view, icon]) => (
          <NavRow
            key={view}
            icon={icon}
            label={VIEW_LABELS[view]}
            count={counts[view]}
            current={view === "codes" ? nav.area === "codes" : nav.area === "vault" && nav.view === view && !nav.scope}
            tone={view === "codes" ? "codes" : view === "attention" && counts.attention > 0 ? "attention" : undefined}
            onClick={() => onNavigate(view === "codes" ? { area: "codes", view: "codes" } : { area: "vault", view })}
          />
        ))}
        {organization}
        <Separator className="my-1 hidden md:block" />
        {screens.map(([area, icon, label]) => (
          <NavRow key={area} icon={icon} label={label} current={nav.area === area} onClick={() => onNavigate({ area, view: nav.view })} />
        ))}
        <div className="md:mt-auto">
          {footer && <div className="hidden md:block">{footer}</div>}
          <NavRow icon="lock" label="Lock" current={false} onClick={onLock} />
        </div>
      </nav>
      <main className="flex min-h-0 min-w-0 flex-1 flex-col overflow-y-auto">{children}</main>
    </div>
  )
}

interface NavRowProps {
  icon: IconName
  label: string
  count?: number
  current: boolean
  tone?: "codes" | "attention"
  /** How many levels deep (a folder inside a folder, a nested tag). */
  depth?: number
  onClick: () => void
}

/** One sidebar entry. The count is spoken as words, never shown alone. */
export function NavRow({ icon, label, count, current, tone, depth = 0, onClick }: NavRowProps) {
  return (
    <Button
      variant={current ? "secondary" : "ghost"}
      size="sm"
      data-row=""
      data-tone={tone}
      className="w-full justify-start data-[tone=attention]:text-attention-text data-[tone=codes]:text-area-codes-text"
      aria-current={current ? "page" : undefined}
      onClick={onClick}
    >
      {depth > 0 && <span aria-hidden="true" className="inline-block shrink-0" style={{ width: `${depth * 0.75}rem` }} />}
      <Icon name={icon} data-icon="inline-start" />
      <span className="flex-1 truncate text-left">{label}</span>
      {count !== undefined && count > 0 && (
        <span className="text-xs text-muted-foreground" aria-label={`${count} ${count === 1 ? "item" : "items"}`}>
          {count}
        </span>
      )}
    </Button>
  )
}
