import { useEffect, useState } from "react"

import { Button, Icon, SectionHeader, Skeleton } from "@unicron/ui"

import { describeError, useClient } from "@/core/client"

import { useBridges, type SetupGuide } from "./bridges-client"

/** "{site} offers {methods}." and the passkey line, as the guide's first sentence. */
export function offersWords(guide: SetupGuide): string {
  const offers = guide.methods.length > 0 ? `${guide.site} offers ${guide.methods.join(", ")}.` : `${guide.site} offers a second step.`
  return guide.passkeys ? `${offers} ${guide.passkeys}` : offers
}

/**
 * SET-UP GUIDE, read in place in a finding (brief H2 row 6;
 * docs/plan/23-bridges.md §3): what the site offers, its own note quoted as
 * the directory has it, Lockdown's steps, the site's pages as text links,
 * and who the details come from. Nothing where the host has no bridges
 * client (the web vault) or the site has no guide.
 */
export function SetupGuideBlock({ itemId, onNotice }: { itemId: string; onNotice: (text: string) => void }) {
  const bridges = useBridges()
  const client = useClient()
  const [guide, setGuide] = useState<SetupGuide | null | undefined>(undefined)
  useEffect(() => {
    if (!bridges) return
    let live = true
    bridges
      .setupGuide(itemId)
      .then((g) => live && setGuide(g))
      .catch(() => live && setGuide(null))
    return () => {
      live = false
    }
  }, [bridges, itemId])
  if (!bridges || guide === null) return null
  const open = (url: string) => void client.openSite(url).catch((e) => onNotice(describeError(e)))
  return (
    <section aria-labelledby="setup-guide" className="flex flex-col gap-3">
      <SectionHeader id="setup-guide" title="Set-up guide" level={3} />
      {guide === undefined ? (
        <Skeleton variant="row" />
      ) : (
        <>
          <p className="type-body">{offersWords(guide)}</p>
          {guide.notes && (
            <figure className="flex flex-col gap-1">
              <blockquote className="type-body text-muted-foreground">“{guide.notes}”</blockquote>
              <figcaption className="type-caption text-muted-foreground">{guide.site}'s note, from 2fa.directory</figcaption>
            </figure>
          )}
          {guide.site_apps.length > 0 && <p className="type-body">The site also has its own: {guide.site_apps.join(", ")}.</p>}
          <ol aria-label="Set-up steps" className="type-body flex list-decimal flex-col gap-2 ps-5">
            {guide.steps.map((s) => (
              <li key={s}>{s}</li>
            ))}
          </ol>
          {(guide.recovery_url || guide.passkey_url) && (
            <div className="flex flex-wrap gap-2">
              {guide.recovery_url && (
                <Button size="sm" variant="ghost" onClick={() => open(guide.recovery_url ?? "")}>
                  <Icon name="launch" data-icon="inline-start" />
                  Account recovery page
                </Button>
              )}
              {guide.passkey_url && (
                <Button size="sm" variant="ghost" onClick={() => open(guide.passkey_url ?? "")}>
                  <Icon name="launch" data-icon="inline-start" />
                  About passkeys there
                </Button>
              )}
            </div>
          )}
          <p className="type-caption text-muted-foreground">
            {guide.attribution.map((a) => `${a.name} (${a.licence})`).join(" and ")}. Built into Lockdown; looking a site up tells no one which sites you use.
          </p>
        </>
      )}
    </section>
  )
}

/** The Passkeys Directory's attribution line beside the 2FA one, at the foot of Health. */
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
  return line ? <p className="type-caption text-muted-foreground">{line}</p> : null
}
