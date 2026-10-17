import type { ReactNode } from "react"

import { Button, Icon, cn } from "@unicron/ui"

export type Area = "vault" | "status"

interface AppShellProps {
  active: Area
  onNavigate: (area: Area) => void
  onLock: () => void
  children: ReactNode
}

/** A narrow rail with two areas and Lock. Neutral until the design brief's sidebar lands. */
export function AppShell({ active, onNavigate, onLock, children }: AppShellProps) {
  const item = (area: Area, icon: Parameters<typeof Icon>[0]["name"], label: string) => (
    <Button
      variant={active === area ? "secondary" : "ghost"}
      size="sm"
      className={cn("justify-start", active === area && "bg-surface-selected")}
      aria-current={active === area ? "page" : undefined}
      onClick={() => onNavigate(area)}
    >
      <Icon name={icon} data-icon="inline-start" />
      {label}
    </Button>
  )
  return (
    <div className="flex h-full min-h-0 flex-col md:flex-row">
      <nav aria-label="Areas" className="flex shrink-0 items-center gap-1 border-b bg-surface-chrome p-2 md:w-44 md:flex-col md:items-stretch md:border-r md:border-b-0">
        {item("vault", "key", "Vault")}
        {item("status", "shield", "Status")}
        <div className="md:mt-auto">
          <Button variant="ghost" size="sm" className="justify-start" onClick={onLock}>
            <Icon name="lock" data-icon="inline-start" />
            Lock
          </Button>
        </div>
      </nav>
      <main className="flex min-h-0 min-w-0 flex-1 flex-col overflow-y-auto">{children}</main>
    </div>
  )
}
