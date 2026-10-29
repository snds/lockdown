import { useState } from "react"

import { Badge, Button, Icon, Item, ItemActions, ItemContent, ItemDescription, ItemGroup, ItemTitle } from "@unicron/ui"

import { PaneHeader } from "@/components/master-detail"
import { EmergencySheetButton } from "@/features/bridges/emergency-sheet"
import { decodeAndWipe } from "@/core/secret-bytes"
import type { OncePage as OncePageData, Status } from "@/core/types"
import { KitDialog } from "@/features/account/kit-dialog"
import { DrillSection } from "@/features/depth/reminders"
import { OncePage, type ShownPage } from "@/features/status/once-page"

interface RecoveryViewProps {
  status: Status
  onChanged: () => void
  onNotice: (text: string) => void
  /** Opens Sharing, where emergency access lives; without it the row only says where. */
  onEmergency?: () => void
}

/**
 * The Recovery screen (brief §3.2): the kit's status, the drill's entry
 * point, and the way to emergency access, which lives under Sharing. The kit page is shown once, through
 * the once-page dialog (LargeTypeView's placeholder for the kit).
 */
export function RecoveryView({ status, onChanged, onNotice, onEmergency }: RecoveryViewProps) {
  const [dialog, setDialog] = useState<"kit-create" | "kit-verify" | null>(null)
  const [once, setOnce] = useState<ShownPage | null>(null)
  const kit = status.recovery
  const showOnce = (page: OncePageData) => setOnce({ title: "Your recovery kit", lines: page.lines, text: page.page.length ? decodeAndWipe(page.page) : null })

  return (
    <div className="mx-auto flex w-full max-w-3xl flex-col gap-(--c-section-gap) p-(--c-page-gutter-narrow) md:p-(--c-page-gutter)">
      <PaneHeader title="Recovery">
        <EmergencySheetButton onNotice={onNotice} />
      </PaneHeader>
      <ItemGroup>
        <Item variant="outline" role="listitem" data-row="">
          <ItemContent>
            <ItemTitle>
              Recovery kit
              <Badge variant={kit === "none" ? "outline" : "secondary"}>{kit === "none" ? "not made yet" : kit === "drilled" ? "made and drilled" : "made, not drilled"}</Badge>
            </ItemTitle>
            <ItemDescription>One printed page: a Secret Key and a recovery code. The whole kit gets you back in when every device is gone.</ItemDescription>
          </ItemContent>
          <ItemActions>
            <Button size="sm" variant={kit === "none" ? "default" : "outline"} onClick={() => setDialog("kit-create")}>
              {kit === "none" ? "Make the kit" : "Replace"}
            </Button>
          </ItemActions>
        </Item>
        <Item variant="outline" role="listitem" data-row="">
          <ItemContent>
            <ItemTitle>
              Recovery drill
              <Badge variant={kit === "drilled" ? "secondary" : "outline"}>{kit === "drilled" ? "passed" : "not run yet"}</Badge>
            </ItemTitle>
            <ItemDescription>
              Type the kit back in and prove it opens this identity. Most people learn their recovery fails at the worst moment; the drill finds out now.
            </ItemDescription>
          </ItemContent>
          <ItemActions>
            <Button size="sm" variant={kit === "made" ? "default" : "outline"} disabled={kit === "none"} onClick={() => setDialog("kit-verify")}>
              <Icon name="health_and_safety" data-icon="inline-start" />
              Run the drill
            </Button>
          </ItemActions>
        </Item>
        <Item variant="outline" role="listitem" data-row="">
          <ItemContent>
            <ItemTitle>Trusted contacts</ItemTitle>
            <ItemDescription>
              Someone you trust can ask for your vaults if something happens to you, and you get a waiting period to stop it. Set it up in Sharing →
              Emergency access.
            </ItemDescription>
          </ItemContent>
          {onEmergency && (
            <ItemActions>
              <Button size="sm" variant="outline" onClick={onEmergency}>
                Open Emergency access
              </Button>
            </ItemActions>
          )}
        </Item>
      </ItemGroup>
      {kit !== "none" && <DrillSection key={kit} />}
      <KitDialog
        kind={dialog}
        hasAccount={status.account !== null}
        onClose={() => setDialog(null)}
        onDone={(lines, page) => {
          setDialog(null)
          if (page) showOnce(page)
          if (lines.length) onNotice(lines.join(" "))
          onChanged()
        }}
      />
      <OncePage page={once} onClose={() => setOnce(null)} />
    </div>
  )
}
