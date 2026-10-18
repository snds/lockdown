import { useEffect, useState } from "react"

import { Badge, Button, Icon } from "@unicron/ui"

import { useClient } from "@/core/client"
import type { Finding } from "@/core/types"

import { FindingFix } from "./finding-actions"
import { PlaybookDialog } from "./playbook-dialog"
import { SEVERITIES, SEVERITY_TEXT, findingsForItem } from "./security-model"
import { usePlaybooks } from "./use-playbooks"

interface ItemSecurityProps {
  itemId: string
  onNotice: (text: string) => void
}

/**
 * An item's own findings on its detail: a website that looks like another
 * site's first and loudest (the matcher's lookalike decision, before you
 * sign in there), then anything else the security center says about this
 * item, each with What to do. Nothing shows when there is nothing to say;
 * a report that fails to load shows nothing rather than a false all-clear.
 */
export function ItemSecurity({ itemId, onNotice }: ItemSecurityProps) {
  const client = useClient()
  const playbooks = usePlaybooks()
  const [findings, setFindings] = useState<Finding[]>([])
  const [open, setOpen] = useState<Finding | null>(null)

  useEffect(() => {
    let live = true
    client
      .securityReport()
      .then((r) => live && setFindings(findingsForItem(r, itemId)))
      .catch(() => live && setFindings([]))
    return () => {
      live = false
    }
  }, [client, itemId])

  if (findings.length === 0) return null
  const lookalike = findings.find((f) => f.kind === "lookalike")
  const rest = findings.filter((f) => f !== lookalike)
  const heading = (f: Finding) => SEVERITIES.find((s) => s.severity === f.severity)?.heading ?? ""
  return (
    <section aria-label="Security" className="flex flex-col gap-2 rounded-xl border p-3 text-sm">
      {lookalike && (
        <div role="alert" className="flex flex-col gap-2">
          <p className={`flex items-center gap-2 font-semibold ${SEVERITY_TEXT[lookalike.severity]}`}>
            <Icon name="report" />
            Careful: this website looks like another site
          </p>
          <p>{lookalike.detail} Don't sign in there unless you're sure it's the real site.</p>
          <div className="flex flex-wrap gap-2">
            <Button size="sm" variant="outline" onClick={() => setOpen(lookalike)}>
              <Icon name="help" data-icon="inline-start" />
              What to do
            </Button>
          </div>
        </div>
      )}
      {rest.length > 0 && (
        <ul aria-label="Security findings" className="flex flex-col gap-2">
          {rest.map((f) => (
            <li key={f.id} className="flex flex-wrap items-center gap-2">
              <Badge variant={f.severity === "urgent" ? "destructive" : "outline"}>{heading(f)}</Badge>
              <span className="min-w-0 flex-1">
                <span className="font-medium">{f.title}.</span> {f.detail}
              </span>
              <Button size="sm" variant="ghost" onClick={() => setOpen(f)}>
                What to do
              </Button>
              <FindingFix finding={f} onNotice={onNotice} />
            </li>
          ))}
        </ul>
      )}
      <PlaybookDialog
        finding={open}
        playbook={open ? (playbooks?.[open.playbook] ?? null) : null}
        onClose={() => setOpen(null)}
        onNotice={onNotice}
      />
    </section>
  )
}
