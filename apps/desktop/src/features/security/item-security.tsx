import { useEffect, useState, type ReactNode } from "react"

import { Banner, Button } from "@unicron/ui"

import { usePlaces } from "@/app/places"
import { useClient } from "@/core/client"
import type { Finding } from "@/core/types"

import { WhatToDo } from "./playbook-dialog"
import { findingsForItem } from "./security-model"
import { usePlaybooks } from "./use-playbooks"

interface ItemSecurityProps {
  itemId: string
  onNotice: (text: string) => void
  /**
   * The item's own state Banner (archived), when it has one. It ranks under a
   * site that looks like another and over every other finding (brief I8).
   */
  state?: ReactNode
}

/**
 * What a finding says on its item's Banner: a site that looks like another's
 * is critical, first and loudest (D130); an urgent finding is critical; the
 * rest are warnings. Color never says it alone: each tone has its own glyph.
 */
function findingTone(f: Finding): "critical" | "warning" {
  return f.kind === "lookalike" || f.severity === "urgent" ? "critical" : "warning"
}

/** The findings in the order the notice slot ranks them: the look-alike first, then by severity as Health lists them. */
function rankFindings(findings: readonly Finding[]): Finding[] {
  const rank = (f: Finding) => (f.kind === "lookalike" ? 0 : f.severity === "urgent" ? 1 : f.severity === "important" ? 2 : 3)
  return findings
    .map((f, i) => [f, i] as const)
    .sort((a, b) => rank(a[0]) - rank(b[0]) || a[1] - b[1])
    .map(([f]) => f)
}

/**
 * The notice slot of an item's detail (brief I8): at most one Banner, the
 * first that applies of a site that looks like another (critical), the
 * item's own state, and its other findings (critical when urgent, warning
 * otherwise), each with WHAT TO DO opening the finding's playbook, where its
 * fix is. What else the security center says about this item becomes one
 * line under the Banner, "02 more in Health", which goes to Health.
 * Nothing shows when there is nothing to say; a report that fails to load
 * shows only the state, never a false all-clear.
 */
export function ItemSecurity({ itemId, onNotice, state }: ItemSecurityProps) {
  const client = useClient()
  const playbooks = usePlaybooks()
  const { go } = usePlaces()
  const [findings, setFindings] = useState<Finding[]>([])
  const [open, setOpen] = useState<Finding | null>(null)

  useEffect(() => {
    let live = true
    client
      .securityReport()
      .then((r) => live && setFindings(rankFindings(findingsForItem(r, itemId))))
      .catch(() => live && setFindings([]))
    return () => {
      live = false
    }
  }, [client, itemId])

  const lookalike = findings[0]?.kind === "lookalike" ? findings[0] : null
  // The one Banner: the look-alike, else the item's state, else its first finding.
  const shown = lookalike ?? (state ? null : (findings[0] ?? null))
  const more = findings.filter((f) => f !== shown).length
  if (!shown && !state) return null
  return (
    <section aria-label="Security" className="flex flex-col gap-2">
      {shown ? (
        <Banner
          tone={findingTone(shown)}
          title={shown.title}
          body={shown.kind === "lookalike" ? `${shown.detail} Don't sign in there unless you're sure it's the real site.` : shown.detail}
          action={
            <Button variant="secondary" size="sm" onClick={() => setOpen(shown)}>
              What to do
            </Button>
          }
        />
      ) : (
        state
      )}
      {more > 0 && (
        <p className="type-caption text-support-foreground">
          <Button variant="link" size="inline" onClick={() => go("health")}>
            {`${more.toLocaleString("en-US")} more in Health`}
          </Button>
        </p>
      )}
      <WhatToDo
        finding={open}
        playbook={open ? (playbooks?.[open.playbook] ?? null) : null}
        onClose={() => setOpen(null)}
        onNotice={onNotice}
      />
    </section>
  )
}
