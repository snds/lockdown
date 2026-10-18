import { useState } from "react"

import { Button, DropdownMenu, DropdownMenuContent, DropdownMenuGroup, DropdownMenuItem, DropdownMenuLabel, DropdownMenuSeparator, DropdownMenuTrigger, Icon, type IconName } from "@unicron/ui"

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

/**
 * "New item": every template a person makes by hand (D110), its kinds'
 * variants (a Passport, a Vehicle, a Router; D231) beside them, and the
 * person's own templates (D232), each with its name and icon; then
 * "Your templates…" to make or change those. A passkey is made by a site
 * through a provider and is not in the list. `onPick` gets the template's
 * id (`login`, `identity/passport`, `user:<id>`).
 */
export function NewItemMenu({ onPick, onNotice = () => {} }: { onPick: (template: string) => void; onNotice?: (text: string) => void }) {
  const { templates, error, userTemplates } = useTemplates()
  const [managing, setManaging] = useState(false)
  const kinds = (templates?.kinds ?? []).filter((t) => t.creatable)
  const groups = [...GROUPS.map(([g]) => g), "Other"].map((g) => [g, kinds.filter((t) => groupOf(t) === g)] as const).filter(([, list]) => list.length > 0)
  return (
    <>
      <DropdownMenu>
        <DropdownMenuTrigger render={<Button size="sm" />}>
          <Icon name="add" data-icon="inline-start" />
          New item
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end">
          {!templates && (
            <DropdownMenuGroup>
              <DropdownMenuLabel>{error ? "The item kinds didn't load" : "Loading…"}</DropdownMenuLabel>
            </DropdownMenuGroup>
          )}
          {groups.map(([name, list], i) => (
            <DropdownMenuGroup key={name}>
              {i > 0 && <DropdownMenuSeparator />}
              <DropdownMenuLabel>{name}</DropdownMenuLabel>
              {list.map((t) => (
                <DropdownMenuItem key={t.id ?? t.kind} onClick={() => onPick(t.id ?? t.kind)}>
                  <Icon name={(t.icon as IconName | undefined) ?? kindIcon(t.kind)} data-icon="inline-start" />
                  {t.name}
                </DropdownMenuItem>
              ))}
            </DropdownMenuGroup>
          ))}
          {templates && (
            <DropdownMenuGroup>
              <DropdownMenuSeparator />
              <DropdownMenuLabel>Your templates</DropdownMenuLabel>
              {userTemplates.map((t) => (
                <DropdownMenuItem key={t.id} onClick={() => onPick(`user:${t.id}`)}>
                  <Icon name={(t.icon || "category") as IconName} data-icon="inline-start" />
                  {t.name}
                </DropdownMenuItem>
              ))}
              <DropdownMenuItem onClick={() => setManaging(true)}>
                <Icon name="edit_note" data-icon="inline-start" />
                {userTemplates.length ? "Change your templates…" : "Make a template…"}
              </DropdownMenuItem>
            </DropdownMenuGroup>
          )}
        </DropdownMenuContent>
      </DropdownMenu>
      <TemplatesManager open={managing} onOpenChange={setManaging} onNotice={onNotice} />
    </>
  )
}
