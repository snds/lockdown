import { useCallback, useEffect, useState } from "react"

import { Badge, Banner, Button, EmptyState, Skeleton, Tabs, TabsList, TabsTrigger, useAppFrame } from "@unicron/ui"

import { NeedsYouSlot } from "@/app/needs-you"
import { ScreenHead } from "@/app/page-head"
import { membersPlace, usePlaces } from "@/app/places"
import { useSettings } from "@/app/settings"
import { describeError } from "@/core/client"
import type { ItemSummary } from "@/core/types"
import { useVaults } from "@/features/organization/vaults-context"
import { count } from "@/lib/format"

import { EmergencySection } from "./emergency-section"
import { KeyResets } from "./key-resets"
import { RequestLinksSection, ShareLinksSection } from "./links-section"
import { PeopleTab } from "./people-section"
import { SharingPaused } from "./sharing-paused"
import { useSharingClient, utf8, type SharingOverview } from "./sharing-client"
import { shareableVaults, VaultsTab } from "./vault-sharing"

/** The part of Sharing a tab shows (redesign brief §3: People, Vaults, Links, Emergency are places). */
export type SharingSection = "people" | "vaults" | "links" | "emergency"

const SECTION_LABELS: Record<SharingSection, string> = { people: "People", vaults: "Vaults", links: "Links", emergency: "Emergency" }
const SECTIONS = Object.keys(SECTION_LABELS) as SharingSection[]

interface SharingViewProps {
  /** The tab showing. Left out: the view keeps its own, starting on People. */
  section?: SharingSection
  /** Picks another tab. With it, the tabs and a vault's members are places (found-3). */
  onSection?: (section: SharingSection) => void
  /** One shared vault's members (its id): the Vaults tab with that vault open (found-3's members place). */
  vault?: string
  /** The rows, for the titles of items shared one by one. */
  rows: ItemSummary[]
  onOpenItem: (id: string) => void
  /** Something changed what the vault holds (a vault opened, a share stopped): reload. */
  onChanged: () => void
  onNotice: (text: string) => void
}

/** The page head's overline (brief S1): `SHARING // 03 PEOPLE // 02 SHARED VAULTS`. */
function overlineOf(o: SharingOverview, sharedVaults: number): string {
  return `Sharing // ${count(o.contacts.length)} ${o.contacts.length === 1 ? "person" : "people"} // ${count(sharedVaults)} shared ${sharedVaults === 1 ? "vault" : "vaults"}`
}

/**
 * Sharing (brief S1, doc 90): one Page split by what the person wants to
 * do, in four tabs, PEOPLE · VAULTS · LINKS · EMERGENCY, each a list and a
 * detail. The tabs are found-3's places; a vault's members are its members
 * place. What waits on the person here (someone who reset their account,
 * needs-you row 6) is the block under the page head, or the strip on a
 * phone. Offline, the lists stay readable and each action that needs the
 * server is disabled with its reason; a split view shows its critical
 * Banner. Routine removals are text buttons: red is kept for the Question
 * (R12). Until edges-8 the Links and Emergency tabs show today's sections.
 *
 * Nothing secret is drawn here except what a link or an answer holds,
 * which comes over one value at a time when the person asks.
 */
export function SharingView({ section, onSection, vault, rows, onOpenItem, onChanged, onNotice }: SharingViewProps) {
  const sharing = useSharingClient()
  const { settings } = useSettings()
  const { vaults, reload: reloadVaults } = useVaults()
  const { go } = usePlaces()
  const frame = useAppFrame()
  const [ownTab, setOwnTab] = useState<SharingSection>("people")
  const tab = section ?? ownTab
  const routed = onSection !== undefined
  const [person, setPerson] = useState<string | null>(null)
  const [ownVault, setOwnVault] = useState<string | null>(null)
  const openVault = routed ? (vault ?? null) : ownVault
  const [overview, setOverview] = useState<SharingOverview | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [failed, setFailed] = useState<string | null>(null)
  const [links, setLinks] = useState<number | null>(null)

  const pick = (t: SharingSection) => (routed ? onSection(t) : setOwnTab(t))
  const selectVault = useCallback((id: string | null) => (routed ? go(id ? membersPlace(id) : "sharing-vaults") : setOwnVault(id)), [routed, go])

  const load = useCallback(() => {
    sharing
      .overview()
      .then((o) => {
        setOverview(o)
        setError(null)
      })
      .catch((e) => setError(describeError(e)))
    // The Links tab's count: the links and requests made here (a read the Links tab makes too).
    void Promise.all([sharing.sends(), sharing.requests()])
      .then(([s, r]) => setLinks(s.length + r.length))
      .catch(() => setLinks(null))
  }, [sharing])
  // Read again after every sync, which a pull and "Check again" run (brief S1: a pull reads the overview again).
  const synced = rows.length === 0 ? null : rows
  useEffect(load, [load, synced])

  const changed = useCallback(() => {
    load()
    reloadVaults()
    onChanged()
  }, [load, reloadVaults, onChanged])

  const shown = shareableVaults(vaults)
  const sharedVaults = shown.filter((v) => v.sharing).length
  const counts: Record<SharingSection, number | null> = {
    people: overview?.contacts.length ?? null,
    vaults: overview ? shown.length : null,
    links,
    emergency: overview?.emergency.length ?? null,
  }
  const pushed = !frame.split && ((tab === "people" && person !== null) || (tab === "vaults" && openVault !== null))

  const copyCode = async () => {
    if (!overview) return
    await sharing.copyLink(utf8(overview.fingerprint_qr), settings.clipboard_clear_secs)
  }

  return (
    <div className="flex w-full flex-col gap-6">
      {!pushed && <ScreenHead overline={overview ? overlineOf(overview, sharedVaults) : "Sharing"} title="Sharing" />}
      {pushed && !frame.phone && <NeedsYouSlot />}
      {!pushed && (
        <div className="no-scrollbar -mt-2 overflow-x-auto">
          <Tabs value={tab} onValueChange={(v) => pick(v as SharingSection)}>
            <TabsList variant="line" aria-label="Sharing">
              {SECTIONS.map((s) => (
                <TabsTrigger key={s} value={s}>
                  {SECTION_LABELS[s]}
                  {/* The count is seen, not part of the tab's name: "People", as the places name it. */}
                  {counts[s] !== null && (
                    <span aria-hidden="true" className="text-text-secondary">
                      {count(counts[s])}
                    </span>
                  )}
                </TabsTrigger>
              ))}
            </TabsList>
          </Tabs>
        </div>
      )}
      {error && overview && (
        <Banner
          tone="critical"
          title="Sharing didn't load."
          body={error}
          action={
            <Button type="button" variant="secondary" size="sm" onClick={load}>
              Try again
            </Button>
          }
        />
      )}
      {error && !overview && <EmptyState tone="critical" overline="Sharing" title="Sharing didn't load." body={error} actionLabel="Try again" onAction={load} />}
      {!overview && !error && (
        <div className="flex flex-col gap-2" aria-label="Loading Sharing">
          <Skeleton variant="row" />
          <Skeleton variant="row" />
          <Skeleton variant="row" />
        </div>
      )}
      {overview && (
        <>
          <SharingPaused text={overview.paused} />
          {!overview.online && <Banner tone="info" size="compact" title="You're offline." body="Sharing needs the server." />}
          {failed && <Banner tone="critical" size="compact" title="Not done." body={failed} onDismiss={() => setFailed(null)} />}
          <KeyResets resets={overview.resets ?? []} online={overview.online} onChanged={changed} onNotice={onNotice} onError={setFailed} />
          {tab === "people" && <PeopleTab overview={overview} selected={person} onSelect={setPerson} onChanged={changed} onNotice={onNotice} onCopyCode={copyCode} />}
          {tab === "vaults" && (
            <VaultsTab
              overview={overview}
              vaults={vaults}
              rows={rows}
              selected={openVault}
              onSelect={selectVault}
              onOpenItem={onOpenItem}
              onOpenPeople={() => pick("people")}
              onChanged={changed}
              onNotice={onNotice}
              onError={setFailed}
            />
          )}
          {tab === "links" && (
            <div className="flex flex-col gap-6">
              <ShareLinksSection online={overview.online} onNotice={onNotice} />
              <RequestLinksSection online={overview.online} onNotice={onNotice} />
            </div>
          )}
          {tab === "emergency" && (
            <div className="flex flex-col gap-6">
              {overview.waiting_on_you > 0 && (
                <p role="alert" className="type-body flex items-center gap-2">
                  <Badge tone="critical">Emergency</Badge>
                  {overview.waiting_on_you === 1
                    ? "Someone asked for emergency access to your vaults. Stop it below if you didn't expect it."
                    : `${overview.waiting_on_you} people asked for emergency access to your vaults. Stop it below if you didn't expect it.`}
                </p>
              )}
              <EmergencySection
                grants={overview.emergency}
                contacts={overview.contacts}
                vaults={vaults}
                online={overview.online}
                onGrants={(emergency) => setOverview((o) => (o ? { ...o, emergency } : o))}
                onChanged={changed}
                onNotice={onNotice}
              />
            </div>
          )}
        </>
      )}
    </div>
  )
}

/**
 * Sharing for a vault kept on this computer only (brief S1, docs/plan/39-first-run.md
 * §1.8): the whole page is one Empty State, no server words beyond it, and
 * SET UP SERVER, which opens Settings → Server.
 */
export function SharingLocalOnly() {
  const { go } = usePlaces()
  return (
    <div className="flex w-full flex-col gap-6">
      <h1 className="sr-only">Sharing</h1>
      <EmptyState
        overline="Sharing"
        title="Sharing needs a server."
        body="You and the people you share with need a server you can all reach."
        actionLabel="Set up server"
        onAction={() => go("settings-server")}
      />
    </div>
  )
}
