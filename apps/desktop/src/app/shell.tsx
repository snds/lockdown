import type { ReactNode } from "react"

import { Button, Icon, NativeSelect, NativeSelectOption, Separator, type IconName } from "@unicron/ui"

import type { PersonaInfo } from "@/core/types"
import { VIEW_LABELS, type ListView } from "@/features/vault/list-model"

/** The areas of the window (brief §3): the vault's views, and four screens. */
export type Area = "vault" | "codes" | "security" | "devices" | "recovery" | "status" | "settings"

export interface Nav {
  area: Area
  /** Which list the vault shows; only read while `area` is `vault`. */
  view: ListView
}

interface AppShellProps {
  nav: Nav
  personas: PersonaInfo[]
  counts: Record<ListView, number>
  onNavigate: (nav: Nav) => void
  onLock: () => void
  children: ReactNode
}

const VIEWS: Array<[ListView, IconName]> = [
  ["all", "inventory_2"],
  ["passkeys", "passkey"],
  ["codes", "timer"],
  ["wi-fi", "wifi"],
  ["expiring", "schedule"],
  ["attention", "warning"],
  ["deleted", "delete"],
]

const SCREENS: Array<[Area, IconName, string]> = [
  ["security", "security", "Security"],
  ["devices", "devices", "Devices"],
  ["recovery", "health_and_safety", "Recovery"],
  ["status", "shield", "Status"],
  ["settings", "settings", "Settings"],
]

/**
 * The sidebar of brief §3: persona switcher (a read-only list for now),
 * All items and the smart views with counts, then Security · Devices ·
 * Recovery · Status · Settings, and Lock. Codes is a view of the vault that opens its
 * own screen, because a code is usually part of a Login (brief §3). Rows
 * take the density knobs through `data-row`.
 */
export function AppShell({ nav, personas, counts, onNavigate, onLock, children }: AppShellProps) {
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
            current={view === "codes" ? nav.area === "codes" : nav.area === "vault" && nav.view === view}
            tone={view === "codes" ? "codes" : view === "attention" && counts.attention > 0 ? "attention" : undefined}
            onClick={() => onNavigate(view === "codes" ? { area: "codes", view: "codes" } : { area: "vault", view })}
          />
        ))}
        <Separator className="my-1 hidden md:block" />
        {SCREENS.map(([area, icon, label]) => (
          <NavRow key={area} icon={icon} label={label} current={nav.area === area} onClick={() => onNavigate({ area, view: nav.view })} />
        ))}
        <div className="md:mt-auto">
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
  onClick: () => void
}

/** One sidebar entry. The count is spoken as words, never shown alone. */
function NavRow({ icon, label, count, current, tone, onClick }: NavRowProps) {
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
