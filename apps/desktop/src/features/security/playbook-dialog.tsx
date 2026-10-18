import {
  Button,
  Dialog,
  DialogAction,
  DialogActions,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  Icon,
} from "@unicron/ui"

import type { Finding, Playbook } from "@/core/types"

import { FindingFix } from "./finding-actions"

interface PlaybookDialogProps {
  finding: Finding | null
  playbook: Playbook | null
  onClose: () => void
  onNotice: (text: string) => void
  onOpenItem?: (id: string) => void
  onDevices?: () => void
}

/**
 * "What to do": the finding in its own words, why it matters, and the
 * playbook's steps in order, with the one action that fixes it beside
 * Close. The steps are the core's (every shell says the same).
 */
export function PlaybookDialog({ finding, playbook, onClose, onNotice, onOpenItem, onDevices }: PlaybookDialogProps) {
  return (
    <Dialog open={finding !== null} onOpenChange={(open) => !open && onClose()}>
      <DialogContent size="lg">
        {finding && (
          <>
            <DialogHeader>
              <DialogTitle>{playbook?.title ?? finding.title}</DialogTitle>
              <DialogDescription>
                {finding.item_title ? `${finding.item_title}: ` : ""}
                {finding.detail}
              </DialogDescription>
            </DialogHeader>
            {playbook ? (
              <div className="flex flex-col gap-3 text-sm">
                <p>{playbook.summary}</p>
                <ol aria-label="Steps" className="flex list-decimal flex-col gap-2 pl-5">
                  {playbook.steps.map((step) => (
                    <li key={step}>{step}</li>
                  ))}
                </ol>
              </div>
            ) : (
              <p className="text-sm text-muted-foreground">The steps didn't load.</p>
            )}
            <DialogActions>
              <DialogAction kind="cancel" onClick={onClose}>
                Close
              </DialogAction>
              {finding.item_id && onOpenItem && (
                <Button
                  variant="ghost"
                  onClick={() => {
                    const id = finding.item_id
                    onClose()
                    if (id) onOpenItem(id)
                  }}
                >
                  <Icon name="arrow_forward" data-icon="inline-start" />
                  Open item
                </Button>
              )}
              <FindingFix finding={finding} onNotice={onNotice} onDevices={onDevices} />
            </DialogActions>
          </>
        )}
      </DialogContent>
    </Dialog>
  )
}
