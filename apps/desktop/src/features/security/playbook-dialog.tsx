import { AdaptiveDialog, Button, SectionHeader } from "@unicron/ui"

import type { Finding, Playbook } from "@/core/types"

import { fixSolid, useFix } from "./finding-actions"
import { KIND_NAMES } from "./security-model"

/**
 * WHAT TO DO (brief H2 row 5): the playbook's steps as a numbered list in
 * the body role, the last saying how to confirm the fix. The steps are the
 * core's (every shell says the same), so they promise only what Lockdown
 * does. "The steps didn't load." in their place when they did not.
 */
export function PlaybookSteps({ playbook, heading = true }: { playbook: Playbook | null; heading?: boolean }) {
  return (
    <section aria-labelledby={heading ? "what-to-do" : undefined} aria-label={heading ? undefined : "What to do"} className="flex flex-col gap-3">
      {heading && <SectionHeader id="what-to-do" title="What to do" level={3} />}
      {playbook ? (
        <>
          <p className="type-body">{playbook.summary}</p>
          <ol aria-label="Steps" className="type-body flex list-decimal flex-col gap-2 ps-5">
            {playbook.steps.map((step) => (
              <li key={step}>{step}</li>
            ))}
          </ol>
        </>
      ) : (
        <p className="type-body text-muted-foreground">The steps didn't load.</p>
      )}
    </section>
  )
}

interface PlaybookDialogProps {
  finding: Finding | null
  playbook: Playbook | null
  onClose: () => void
  onNotice: (text: string) => void
  onOpenItem?: (id: string) => void
  onDevices?: () => void
}

/**
 * "What to do" from an item's own notice (brief I8, D830): the finding in
 * its own words, the playbook's steps, and the one action that fixes it
 * beside Close. Health reads the same steps in place (brief H2).
 */
export function PlaybookDialog({ finding, playbook, onClose, onNotice, onOpenItem, onDevices }: PlaybookDialogProps) {
  const fix = useFix(finding, { onNotice, onDevices, onOpenItem: onOpenItem ? (id) => (onClose(), onOpenItem(id)) : undefined })
  return (
    <AdaptiveDialog
      open={finding !== null}
      onOpenChange={(open) => !open && onClose()}
      size="review"
      overline={finding ? `Health // ${KIND_NAMES[finding.kind]}` : undefined}
      title={playbook?.title ?? finding?.title ?? ""}
      description={finding ? `${finding.item_title ? `${finding.item_title}: ` : ""}${finding.detail}` : undefined}
      footer={
        <>
          <Button variant={fix ? "secondary" : "default"} onClick={onClose}>
            Close
          </Button>
          {fix && fixSolid(fix)}
        </>
      }
    >
      {finding && <PlaybookSteps playbook={playbook} heading={false} />}
    </AdaptiveDialog>
  )
}
