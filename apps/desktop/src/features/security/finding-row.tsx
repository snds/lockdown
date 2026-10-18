import {
  Button,
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
  Icon,
  Item,
  ItemActions,
  ItemContent,
  ItemDescription,
  ItemMedia,
  ItemTitle,
} from "@unicron/ui"

import type { Finding } from "@/core/types"
import { whenLocal } from "@/lib/format"

import { FindingFix } from "./finding-actions"
import { KIND_ICONS, SNOOZES, dismissedWords, snoozeUntil } from "./security-model"

interface FindingRowProps {
  finding: Finding
  busy: boolean
  onWhatToDo: (f: Finding) => void
  onOpenItem: (id: string) => void
  /** `until: null` dismisses; a time snoozes. */
  onDismiss: (f: Finding, until: number | null) => void
  onRestore: (f: Finding) => void
  onNotice: (text: string) => void
  onDevices?: () => void
}

/**
 * One finding: what it is, which item, the particulars, then What to do
 * (the playbook), the fix where there is one (Change password, Turn it
 * on, Go to Devices), Open item, and Dismiss with its snoozes. A
 * dismissed finding offers Bring back instead.
 */
export function FindingRow({ finding: f, busy, onWhatToDo, onOpenItem, onDismiss, onRestore, onNotice, onDevices }: FindingRowProps) {
  const where = [f.item_title, f.at ? whenLocal(f.at) : null].filter(Boolean).join(" · ")
  return (
    <Item role="listitem" variant="outline" size="sm">
      <ItemMedia variant="icon">
        <Icon name={KIND_ICONS[f.kind]} />
      </ItemMedia>
      <ItemContent>
        <ItemTitle>{f.title}</ItemTitle>
        {where && <ItemDescription>{where}</ItemDescription>}
        <ItemDescription>{f.dismissed ? `${dismissedWords(f, whenLocal)}. ${f.detail}` : f.detail}</ItemDescription>
      </ItemContent>
      <ItemActions>
        {f.dismissed ? (
          <Button size="sm" variant="ghost" onClick={() => onRestore(f)} disabled={busy}>
            <Icon name="undo" data-icon="inline-start" />
            Bring back
          </Button>
        ) : (
          <>
            <Button size="sm" variant="outline" onClick={() => onWhatToDo(f)}>
              <Icon name="help" data-icon="inline-start" />
              What to do
            </Button>
            <FindingFix finding={f} onNotice={onNotice} onDevices={onDevices} />
            {f.item_id && (
              <Button size="sm" variant="ghost" onClick={() => f.item_id && onOpenItem(f.item_id)}>
                Open item
              </Button>
            )}
            <DropdownMenu>
              <DropdownMenuTrigger render={<Button size="sm" variant="ghost" disabled={busy} aria-label={`Dismiss: ${f.title}`} />}>
                <Icon name="snooze" data-icon="inline-start" />
                Dismiss
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end">
                {f.alert === null &&
                  SNOOZES.map((s) => (
                    <DropdownMenuItem key={s.days} onClick={() => onDismiss(f, snoozeUntil(s.days, Math.floor(Date.now() / 1000)))}>
                      {s.label}
                    </DropdownMenuItem>
                  ))}
                {f.alert === null && <DropdownMenuSeparator />}
                <DropdownMenuItem onClick={() => onDismiss(f, null)}>
                  {f.kind === "missing_2fa" ? "I already use two-step sign-in here" : "Dismiss, I've dealt with it"}
                </DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          </>
        )}
      </ItemActions>
    </Item>
  )
}
