import { Fragment, useState, type ReactElement } from "react"

import { AdaptiveMenu, AdaptiveMenuGroup, AdaptiveMenuItem, AdaptiveMenuSeparator, Button, DropdownMenuGroup, DropdownMenuItem, DropdownMenuLabel, Icon } from "@unicron/ui"

import { templateIcon } from "@/app/template-model"
import { useTemplates } from "@/app/templates"
import type { TemplateView } from "@/core/types"

import { kindIcon } from "./list-model"
import { TemplatesManager } from "./template-designer"

/** The menu's groups: the everyday kinds, the identities and documents, the technical ones, the rest. */
const GROUPS: Array<[string, readonly string[]]> = [
  ["Sign-ins and money", ["login", "code", "card", "bank-account", "crypto-wallet", "note"]],
  ["Identities and documents", ["identity", "document", "membership"]],
  ["Networks and machines", ["wi-fi", "server", "database", "ssh-key", "api-credential", "software-licence"]],
]

/** Which group a template sits in; the custom item and anything new go last. */
function groupOf(t: TemplateView): string {
  return GROUPS.find(([, kinds]) => kinds.includes(t.kind))?.[0] ?? "Other"
}

interface NewItemMenuProps {
  onPick: (template: string) => void
  onNotice?: (text: string) => void
  /**
   * The control that opens it. Left out, the menu draws its own "New item"
   * button. The shell passes its own (the sidebar's primary, the phone's
   * create button) and holds `open`.
   */
  trigger?: ReactElement<Record<string, unknown>>
  open?: boolean
  onOpenChange?: (open: boolean) => void
}

/**
 * "New item": every template a person makes by hand (D110), its kinds'
 * variants (a Passport, a Vehicle, a Router; D231) beside them, and the
 * person's own templates (D232), each with its name and icon; then
 * "Your templates…" to make or change those. A passkey is made by a site
 * through a provider and is not in the list. `onPick` gets the template's
 * id (`login`, `identity/passport`, `user:<id>`). A menu on a desktop, a
 * sheet of the same rows on a phone.
 */
export function NewItemMenu({ onPick, onNotice = () => {}, trigger, open, onOpenChange }: NewItemMenuProps) {
  const { templates, error, userTemplates } = useTemplates()
  const [managing, setManaging] = useState(false)
  const kinds = (templates?.kinds ?? []).filter((t) => t.creatable)
  const groups = [...GROUPS.map(([g]) => g), "Other"].map((g) => [g, kinds.filter((t) => groupOf(t) === g)] as const).filter(([, list]) => list.length > 0)
  return (
    <>
      <AdaptiveMenu
        title="New item"
        height="full"
        align="end"
        open={open}
        onOpenChange={onOpenChange}
        trigger={
          trigger ?? (
            <Button size="sm">
              <Icon name="add" data-icon="inline-start" />
              New item
            </Button>
          )
        }
      >
        {!templates && <AdaptiveMenuGroup label={error ? "The item kinds didn't load" : "Loading…"}>{null}</AdaptiveMenuGroup>}
        {groups.map(([name, list], i) => (
          <Fragment key={name}>
            {i > 0 && <AdaptiveMenuSeparator />}
            <AdaptiveMenuGroup label={name}>
              {list.map((t) => (
                <AdaptiveMenuItem key={t.id ?? t.kind} icon={templateIcon(t.icon, kindIcon(t.kind))} onSelect={() => onPick(t.id ?? t.kind)}>
                  {t.name}
                </AdaptiveMenuItem>
              ))}
            </AdaptiveMenuGroup>
          </Fragment>
        ))}
        {templates && <AdaptiveMenuSeparator />}
        {templates && (
          <AdaptiveMenuGroup label="Your templates">
            {userTemplates.map((t) => (
              <AdaptiveMenuItem key={t.id} icon={templateIcon(t.icon, "categories")} onSelect={() => onPick(`user:${t.id}`)}>
                {t.name}
              </AdaptiveMenuItem>
            ))}
            <AdaptiveMenuItem icon="edit" onSelect={() => setManaging(true)}>
              {userTemplates.length ? "Change your templates…" : "Make a template…"}
            </AdaptiveMenuItem>
          </AdaptiveMenuGroup>
        )}
      </AdaptiveMenu>
      <TemplatesManager open={managing} onOpenChange={setManaging} onNotice={onNotice} />
    </>
  )
}

/** The four kinds quick create offers (States / 03): the ones made most. */
const QUICK_KINDS = ["login", "card", "note", "code"] as const

/**
 * Quick create's rows (States / 03): Login, Card, Secure note and One-time
 * code, each by the name its template carries. Menu rows: put them in the
 * menu the create button opens on a long press.
 */
export function QuickCreateItems({ onPick }: { onPick: (template: string) => void }) {
  const { templates } = useTemplates()
  // The kind's own template, not one of its variants (a Passport, a Router).
  const kinds = (templates?.kinds ?? []).filter((t) => t.creatable && !t.variant)
  const rows = QUICK_KINDS.map((k) => kinds.find((t) => t.kind === k)).filter((t): t is TemplateView => Boolean(t))
  return (
    <DropdownMenuGroup>
      <DropdownMenuLabel>Create // long press</DropdownMenuLabel>
      {rows.map((t) => (
        <DropdownMenuItem key={t.kind} onClick={() => onPick(t.kind)}>
          <Icon name={templateIcon(t.icon, kindIcon(t.kind))} />
          {t.name}
        </DropdownMenuItem>
      ))}
    </DropdownMenuGroup>
  )
}
