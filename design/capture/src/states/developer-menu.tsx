import { DEVELOPER_MENU } from "@/devtools/menu"

/** The Developer menu's items, drawn where the OS would draw them. */
export function DeveloperMenu() {
  return (
    <main className="flex min-h-svh items-start justify-center bg-background p-6">
      <section aria-label="Developer menu (as the OS draws it)" className="w-64 rounded-lg border bg-popover p-1 text-sm text-popover-foreground shadow-md">
        <h1 className="px-2 py-1 text-xs font-medium text-muted-foreground">Developer</h1>
        <ul role="menu" aria-label="Developer">
          {DEVELOPER_MENU.map((m) => (
            <li key={m.action} role="menuitem" className={`rounded-sm px-2 py-1.5 ${m.action === "reset" ? "mt-1 border-t pt-2" : ""}`}>
              {m.label}
            </li>
          ))}
        </ul>
      </section>
    </main>
  )
}
