import { Button, DropdownMenu, DropdownMenuContent, DropdownMenuGroup, DropdownMenuItem, DropdownMenuLabel, DropdownMenuTrigger, Icon } from "@unicron/ui"

import { useTemplates } from "@/app/templates"

import { kindIcon } from "./list-model"

/**
 * "New item": every kind a person makes by hand, from the core's templates
 * (D110), each with its plain name and its icon. A passkey is made by a
 * site through a provider and is not in the list.
 */
export function NewItemMenu({ onPick }: { onPick: (kind: string) => void }) {
  const { templates, error } = useTemplates()
  const kinds = (templates?.kinds ?? []).filter((t) => t.creatable)
  return (
    <DropdownMenu>
      <DropdownMenuTrigger render={<Button size="sm" />}>
        <Icon name="add" data-icon="inline-start" />
        New item
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end">
        <DropdownMenuGroup>
          <DropdownMenuLabel>{error ? "The item kinds didn't load" : templates ? "New item" : "Loading…"}</DropdownMenuLabel>
          {kinds.map((t) => (
            <DropdownMenuItem key={t.kind} onClick={() => onPick(t.kind)}>
              <Icon name={kindIcon(t.kind)} data-icon="inline-start" />
              {t.name}
            </DropdownMenuItem>
          ))}
        </DropdownMenuGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}
