import { DEVELOPER_MENU } from "@/devtools/menu"

/**
 * The Developer menu's items, drawn where the OS would draw them: a menu's
 * look from unicron's roles (raised surface, a rule, the overline for its
 * name), since the real one is the OS's own.
 */
export function DeveloperMenu() {
  return (
    <main className="bg-surface-ground flex min-h-svh items-start justify-center p-6">
      <section aria-label="Developer menu (as the OS draws it)" className="bg-surface-raised rule-box w-64 rounded-sm p-1">
        <h1 className="type-overline text-text-label px-2 py-2">Developer</h1>
        <ul role="menu" aria-label="Developer">
          {DEVELOPER_MENU.map((m) => (
            <li key={m.action} role="menuitem" className={`type-ui text-text-body rounded-sm px-2 py-2 ${m.action === "reset" ? "rule-t mt-1" : ""}`}>
              {m.label}
            </li>
          ))}
        </ul>
      </section>
    </main>
  )
}
