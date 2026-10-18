import { useEffect, useState } from "react"

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
  Spinner,
} from "@unicron/ui"

import { describeError, useClient } from "@/core/client"

import { useBridges, type SetupGuide } from "./bridges-client"

/**
 * "Set-up guide" on a site that offers two-step sign-in (docs/plan/23-bridges.md
 * §3): what the site offers, its own note quoted as the directory has it,
 * Lockdown's steps, the site's pages, and who the details come from. Hidden
 * where the host has no bridges client (the web vault).
 */
export function GuideButton({ itemId, onNotice }: { itemId: string; onNotice: (text: string) => void }) {
  const bridges = useBridges()
  const [guide, setGuide] = useState<SetupGuide | null>(null)
  const [busy, setBusy] = useState(false)
  if (!bridges) return null
  const open = async () => {
    setBusy(true)
    try {
      const g = await bridges.setupGuide(itemId)
      if (g) setGuide(g)
      else onNotice("Lockdown has no set-up guide for this site.")
    } catch (e) {
      onNotice(describeError(e))
    } finally {
      setBusy(false)
    }
  }
  return (
    <>
      <Button size="sm" variant="outline" onClick={() => void open()} disabled={busy}>
        {busy ? <Spinner data-icon="inline-start" /> : <Icon name="menu_book" data-icon="inline-start" />}
        Set-up guide
      </Button>
      <SetupGuideDialog guide={guide} onClose={() => setGuide(null)} onNotice={onNotice} />
    </>
  )
}

export function SetupGuideDialog({
  guide,
  onClose,
  onNotice,
}: {
  guide: SetupGuide | null
  onClose: () => void
  onNotice: (text: string) => void
}) {
  const client = useClient()
  const open = (url: string) => void client.openSite(url).catch((e) => onNotice(describeError(e)))
  return (
    <Dialog open={guide !== null} onOpenChange={(o) => !o && onClose()}>
      <DialogContent size="lg">
        {guide && (
          <>
            <DialogHeader>
              <DialogTitle>Set up two-step sign-in for {guide.site}</DialogTitle>
              <DialogDescription>
                {guide.methods.length > 0 ? `${guide.site} offers ${guide.methods.join(", ")}.` : `${guide.site} offers a second step.`}
                {guide.passkeys ? ` ${guide.passkeys}` : ""}
              </DialogDescription>
            </DialogHeader>
            <div className="flex flex-col gap-3 text-sm">
              {guide.notes && (
                <figure className="rounded-md border p-3">
                  <blockquote>{guide.notes}</blockquote>
                  <figcaption className="mt-1 text-xs text-muted-foreground">{guide.site}'s note, from 2fa.directory</figcaption>
                </figure>
              )}
              {guide.site_apps.length > 0 && <p>The site also has its own: {guide.site_apps.join(", ")}.</p>}
              <ol aria-label="Steps" className="flex list-decimal flex-col gap-2 pl-5">
                {guide.steps.map((s) => (
                  <li key={s}>{s}</li>
                ))}
              </ol>
              <div className="flex flex-wrap gap-2">
                {guide.recovery_url && (
                  <Button size="sm" variant="ghost" onClick={() => open(guide.recovery_url ?? "")}>
                    <Icon name="open_in_new" data-icon="inline-start" />
                    Account recovery page
                  </Button>
                )}
                {guide.passkey_url && (
                  <Button size="sm" variant="ghost" onClick={() => open(guide.passkey_url ?? "")}>
                    <Icon name="open_in_new" data-icon="inline-start" />
                    About passkeys there
                  </Button>
                )}
              </div>
              <p className="text-xs text-muted-foreground">
                {guide.attribution.map((a) => `${a.name} (${a.licence})`).join(" and ")}. Built into Lockdown; looking a site up tells no one which sites you use.
              </p>
            </div>
            <DialogActions>
              <DialogAction kind="cancel" onClick={onClose}>
                Close
              </DialogAction>
              {guide.setup_url && (
                <Button onClick={() => open(guide.setup_url ?? "")}>
                  <Icon name="open_in_new" data-icon="inline-start" />
                  Set it up
                </Button>
              )}
            </DialogActions>
          </>
        )}
      </DialogContent>
    </Dialog>
  )
}

/** The Passkeys Directory's attribution line beside the 2FA one, on the Security screen. */
export function GuideAttribution() {
  const bridges = useBridges()
  const [line, setLine] = useState<string | null>(null)
  useEffect(() => {
    let live = true
    bridges
      ?.guideSources()
      .then((s) => {
        const others = s.sources.filter((a) => a.name !== "2fa.directory")
        if (live && others.length > 0)
          setLine(
            `Passkey support and set-up notes come from ${others.map((a) => `the ${a.name} (${a.licence})`).join(" and ")} and 2fa.directory, in Lockdown's signed site data of ${s.published}.`,
          )
      })
      .catch(() => {})
    return () => {
      live = false
    }
  }, [bridges])
  return line ? <p className="text-xs text-muted-foreground">{line}</p> : null
}
