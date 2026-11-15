// The toolbar popup on unicron 1.0 (brief B1, docs/plan/87-redesign-popup.md):
// pairing with the app, the logins for the page in front, quick access over
// the core's one search index (ranked, a typo forgiven, never a secret), copy
// through the desktop's clipboard path, a one-time code shown in place, per-
// site fill fixes and "Hide my email". It shows summaries only: title,
// username, site, kind (D113 for crown jewels). The in-page menu and prompts
// stay plain HTML (frame.ts): they load nothing a page could influence.
//
// The frame: 360 wide, as tall as its content up to 600, the ground surface,
// 12 px in. A top line holds the mark and the page's host, cut in the middle.
// Where the popup can't act (connecting, no app, not paired, pairing, locked)
// it says so in one centred column and offers the way out. Where it can, top
// to bottom: the look-alike stop, the page's logins, Hide my email, Search,
// and one status line. Every row is a compact ListRow with one action at its
// trailing edge (review R16). Reading text is 13 px or more; overlines are
// the only smaller text. Every look comes from unicron's props: no className
// on a unicron part (the popup's ratchet is 0); layout wrappers are plain
// elements.

import { useCallback, useEffect, useRef, useState, type KeyboardEvent, type ReactNode, type RefObject } from "react"

import {
  AppLockup,
  Badge,
  Banner,
  Button,
  CodeDisplay,
  DigitGroups,
  DropdownMenuItem,
  Icon,
  ListRow,
  NativeSelect,
  NativeSelectOption,
  SearchField,
  SectionHeader,
  Spinner,
  ToggleRow,
  TruncatedText,
} from "@unicron/ui"

import { OPEN_HOW, type AliasOption, type ChangeLink, type FieldMatch, type FieldRole, type FillFix, type LinkState, type OpenedApp, type PageField, type PageMatches, type PickAnswer, type SearchItem, type ShownCode, type SiteFixView } from "../shared/protocol"

const params = new URLSearchParams(location.search)
const tabParam = params.has("tab") ? Number(params.get("tab")) : undefined

function send<T>(msg: Record<string, unknown>): Promise<T> {
  return chrome.runtime.sendMessage(msg) as Promise<T>
}

type Say = (text: string) => void

/** A count as cross-app §13 writes it: two digits, three from 100, a comma from 1,000. */
const count = (n: number) => (n < 100 ? String(n).padStart(2, "0") : n.toLocaleString("en-US"))

/** How long a code shown here stays on screen. */
const SHOWN_CODE_MS = 30_000

type Site = { url?: string; tab_id?: number; matches: PageMatches | null }

/** The host the top line names: the matcher's, or the address's for a web page it had nothing for. */
function hostOf(site: Site | null): string {
  if (!site) return ""
  if (site.matches?.host) return site.matches.host
  try {
    const u = new URL(site.url ?? "")
    return /^https?:$/.test(u.protocol) ? u.host : ""
  } catch {
    return ""
  }
}

export function Popup() {
  const [state, setState] = useState<LinkState | null>(null)
  const [status, setStatus] = useState("")
  const [site, setSite] = useState<Site | null>(null)
  const start = useCallback(() => {
    setState(null)
    void send<LinkState>({ t: "status" }).then(setState)
  }, [])
  useEffect(start, [start])
  // Waiting on a pairing or an unlock: ask again each second until it changes.
  useEffect(() => {
    if (!state || state.link !== "ready" || !(state.pairing || state.locked)) return
    const poll = setInterval(async () => {
      const s = await send<LinkState>({ t: "status" })
      if (s.pairing) return
      setState(s)
    }, 1000)
    return () => clearInterval(poll)
  }, [state])
  const canAct = state?.link === "ready" && state.paired && !state.locked
  // The page in front, once the popup can act on it.
  useEffect(() => {
    if (!canAct) return
    void send<Site>({ t: "site", tab_id: tabParam }).then((s) => setSite(s ?? { matches: null }))
  }, [canAct])
  const host = canAct ? hostOf(site) : ""

  return (
    <main className="flex max-h-[600px] w-[360px] flex-col gap-3 overflow-y-auto bg-surface-ground p-3 text-foreground">
      <header className="flex min-h-8 items-center justify-between gap-3 pb-3 rule-b">
        <AppLockup name="Lockdown" showName={false} />
        {host && (
          <span className="min-w-0 type-overline text-muted-foreground" data-popup-host="">
            <TruncatedText cut="middle">{host}</TruncatedText>
          </span>
        )}
      </header>
      {!state ? (
        <Column>
          <h1 className="sr-only">Lockdown</h1>
          <p className="flex items-center gap-2 type-body text-muted-foreground">
            <Spinner /> Connecting to Lockdown…
          </p>
        </Column>
      ) : canAct ? (
        <Main site={site} say={setStatus} />
      ) : (
        <Link state={state} onChange={setState} onRetry={start} say={setStatus} />
      )}
      <p role="status" aria-live="polite" className={canAct ? "type-caption text-muted-foreground" : "type-caption text-center text-muted-foreground"}>
        {status}
      </p>
    </main>
  )
}

/** The states where the popup can't act: one centred column. */
function Column({ children }: { children: ReactNode }) {
  return <section className="flex flex-col items-center gap-3 py-6 text-center">{children}</section>
}

function Title({ children }: { children: ReactNode }) {
  return <h1 className="type-subtitle text-foreground">{children}</h1>
}

function Body({ children }: { children: ReactNode }) {
  return <p className="type-body text-muted-foreground">{children}</p>
}

/** The column's buttons, one under the other. */
function Ways({ children }: { children: ReactNode }) {
  return <div className="flex flex-col items-center gap-2 pt-1">{children}</div>
}

function Link({ state, onChange, onRetry, say }: { state: LinkState; onChange: (s: LinkState) => void; onRetry: () => void; say: Say }) {
  const [busy, setBusy] = useState(false)
  const pairing = state.link === "ready" && Boolean(state.pairing)
  // The pairing's live line: said once while the popup waits for the app's yes.
  useEffect(() => {
    if (!pairing) return
    say("Waiting for your approval in Lockdown…")
    return () => say("")
  }, [pairing, say])
  if (state.link !== "ready") {
    return (
      <Column>
        <Title>Lockdown isn't reachable.</Title>
        {state.message && <Body>{state.message}</Body>}
        <Ways>
          <OpenLockdown state={state} onChange={onChange} say={say} primary />
          <Button variant="ghost" onClick={onRetry}>
            Try again
          </Button>
        </Ways>
      </Column>
    )
  }
  if (state.pairing) {
    return (
      <Column>
        <Title>Check the code.</Title>
        <DigitGroups value={state.pairing.code} size="lg" label="Pairing code" />
        <Body>Check that Lockdown on this computer shows this code, then approve it there.</Body>
      </Column>
    )
  }
  if (!state.paired) {
    const pair = async () => {
      setBusy(true)
      const next = await send<LinkState & { error?: string }>({ t: "pair" })
      setBusy(false)
      if (next?.error) say(next.error)
      else onChange(next)
    }
    return (
      <Column>
        <Title>Pair this browser.</Title>
        <Body>{state.message || "Pair it with the Lockdown app on this computer to fill and save sign-ins."}</Body>
        <Ways>
          <Button loading={busy} onClick={() => void pair()}>
            Pair with Lockdown
          </Button>
          <OpenLockdown state={state} onChange={onChange} say={say} />
        </Ways>
      </Column>
    )
  }
  return (
    <Column>
      <Title>Lockdown is locked.</Title>
      <Body>Unlock it in the app to fill and search.</Body>
      <Ways>
        <OpenLockdown state={state} onChange={onChange} say={say} primary />
      </Ways>
    </Column>
  )
}

/** How long the popup waits for an app the host started to answer. */
const LAUNCH_WAIT_MS = 20_000

/**
 * Open Lockdown (review R16): a running app brings its window forward; with
 * none, the native host starts it and the popup waits for it to answer,
 * then shows where things stand. Where neither works, it says how.
 */
function OpenLockdown({ state, onChange, say, primary = false }: { state: LinkState; onChange: (s: LinkState) => void; say: Say; primary?: boolean }) {
  const [busy, setBusy] = useState(false)
  const open = async () => {
    setBusy(true)
    const r = await send<OpenedApp>({ t: "open_app" })
    if (r?.opened === "shown") {
      say(state.paired && state.locked ? "Lockdown is open on this computer. Unlock it there to fill and search." : "Lockdown is open on this computer.")
    } else if (r?.opened === "launched") {
      say("Opening Lockdown…")
      const until = Date.now() + LAUNCH_WAIT_MS
      for (;;) {
        await new Promise((ok) => setTimeout(ok, 1000))
        const s = await send<LinkState>({ t: "status" })
        if (s?.link === "ready") {
          say("Lockdown is open on this computer.")
          onChange(s)
          break
        }
        if (Date.now() > until) {
          say(OPEN_HOW)
          break
        }
      }
    } else {
      say(r?.error ?? OPEN_HOW)
    }
    setBusy(false)
  }
  return (
    <Button variant={primary ? "default" : "ghost"} loading={busy} onClick={() => void open()}>
      Open Lockdown
    </Button>
  )
}

/**
 * Up and Down move between the rows' actions in a list; from the search
 * field, Down moves into the results (B1: keys).
 */
function moveAmongActions(e: KeyboardEvent<HTMLElement>) {
  if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return
  const list = e.currentTarget
  const actions = Array.from(list.querySelectorAll<HTMLElement>('[data-slot="list-row-action"] button'))
  const at = actions.indexOf(document.activeElement as HTMLElement)
  if (at < 0) return
  const next = actions[at + (e.key === "ArrowDown" ? 1 : -1)]
  if (!next) return
  e.preventDefault()
  next.focus()
}

/** A list of compact rows: a plain list, so the rows are its items (review R7). */
function Rows({ label, children }: { label: string; children: ReactNode }) {
  return (
    <ul role="list" aria-label={label} className="flex flex-col" onKeyDown={moveAmongActions}>
      {children}
    </ul>
  )
}

function Main({ site, say }: { site: Site | null; say: Say }) {
  const [view, setView] = useState<"main" | "fix">("main")
  const fixButton = useRef<HTMLButtonElement>(null)
  const back = () => {
    setView("main")
    // Closing the second view returns focus to what opened it.
    requestAnimationFrame(() => fixButton.current?.focus())
  }
  const lookalike = site?.matches?.lookalike_of ?? []
  if (view === "fix") return <FixView tabId={site?.tab_id} onBack={back} say={say} />
  return (
    <>
      <h1 className="sr-only">Lockdown</h1>
      {site && <SiteSection site={site} say={say} onFix={() => setView("fix")} fixButton={fixButton} />}
      {site && <Aliases site={site} say={say} />}
      <Search lookalike={lookalike} say={say} />
    </>
  )
}

function SiteSection({ site, say, onFix, fixButton }: { site: Site; say: Say; onFix: () => void; fixButton: RefObject<HTMLButtonElement | null> }) {
  const m = site.matches
  if (!m) return null
  const stop = m.lookalike_of.length > 0
  const fill = async (itemId: string, title: string) => {
    if (site.tab_id === undefined) return
    const r = await send<PickAnswer & { error?: string }>({ t: "fill_tab", tab_id: site.tab_id, item_id: itemId })
    say(r?.done ? `Filled ${title}.` : (r?.confirm ?? r?.refuse ?? r?.error ?? "Lockdown didn't fill."))
  }
  return (
    <>
      {stop && (
        // D166: the stop, and no Fill anywhere on a page that imitates a saved site.
        <Banner size="compact" title="Stop:" body={`${m.host} isn't ${m.lookalike_of.join(", ")}. It only looks like a site you have a login for. Lockdown won't fill here.`} />
      )}
      {m.logins.length > 0 && (
        <section className="flex flex-col">
          {/* The host is in the top line already; a long one would push the
              section's action out of the 336 px column (doc 87 §2). */}
          <SectionHeader
            inset
            // The count rides in the overline: SectionHeader's own count is
            // the 12 px meta role, under the popup's 13 px floor (R16).
            title={`For this page ${count(m.logins.length)}`}
            action={
              <Button ref={fixButton} variant="ghost" size="sm" onClick={onFix}>
                Fix this page
              </Button>
            }
          />
          <Rows label={`Logins for ${m.host}`}>
            {m.logins.map((c, i) => {
              const title = c.title || m.host
              return (
                <ListRow
                  key={c.item_id}
                  render={<li />}
                  size="compact"
                  metaStyle="plain"
                  title={title}
                  meta={c.crown_jewel ? "Protected" : c.username}
                  marks={c.asked ? <Badge>From quick access</Badge> : undefined}
                  action={
                    stop ? undefined : (
                      <Button variant={i === 0 ? "default" : "secondary"} size="sm" aria-label={`Fill ${title}`} onClick={() => void fill(c.item_id, title)}>
                        Fill
                      </Button>
                    )
                  }
                  menu={<ChangePassword itemId={c.item_id} title={title} say={say} />}
                />
              )
            })}
          </Rows>
        </section>
      )}
    </>
  )
}

type Opened = { kind: ChangeLink["kind"]; checked: boolean; note: string | null; host: string; error?: string }

/** The security center's checked change-password link (docs/plan/23-bridges.md §7), in the row's menu. */
function ChangePassword({ itemId, title, say }: { itemId: string; title: string; say: Say }) {
  const open = async () => {
    const r = await send<Opened>({ t: "change_link", item_id: itemId })
    if (!r || r.error) {
      say(r?.error ?? "Lockdown couldn't find where to change it.")
      return
    }
    if (r.kind === "well_known") say(`Opened the change-password page for ${title}.`)
    else if (r.kind === "recipe") say(`Opened the page where ${r.host} changes passwords.${r.note ? ` ${r.note}` : ""}`)
    else say(r.checked ? `${r.host} has no change-password page Lockdown could find, so its front page is open.` : `Opened ${r.host}; look for its password settings.`)
  }
  return <DropdownMenuItem onClick={() => void open()}>Change password</DropdownMenuItem>
}

// --- "Fix this page": per-site fill fixes (docs/plan/23-bridges.md §5) -----------

const ROLES: [FieldRole | "", string][] = [
  ["", "Lockdown decides"],
  ["username", "Username or email"],
  ["password", "Password"],
  ["new_password", "New password"],
  ["code", "One-time code"],
  ["ignore", "Not a sign-in field"],
]

const fieldName = (f: PageField) => f.label || f.name || f.id || f.autocomplete

/** How a fix names a field: its id and name, or its autocomplete when it has neither. */
const matchFor = (f: PageField): FieldMatch => ({ id: f.id || null, name: f.name || null, autocomplete: f.id || f.name ? null : f.autocomplete || null })

const sameMatch = (a: FieldMatch, b: FieldMatch) =>
  (a.id ?? null) === (b.id ?? null) && (a.name ?? null) === (b.name ?? null) && (a.autocomplete ?? null) === (b.autocomplete ?? null)

type Fields = { host: string; fields: PageField[]; view: SiteFixView | null; error?: string }

/** Fix this page: a second view inside the popup (B1), in place of a disclosure. */
function FixView({ tabId, onBack, say }: { tabId?: number; onBack: () => void; say: Say }) {
  const [data, setData] = useState<Fields | null>(null)
  const [never, setNever] = useState(false)
  const [roles, setRoles] = useState<Record<number, FieldRole | "">>({})
  const [busy, setBusy] = useState(false)
  const title = useRef<HTMLHeadingElement>(null)
  useEffect(() => {
    // A pushed view moves focus to its title.
    title.current?.focus()
    void send<Fields>({ t: "fix_fields", tab_id: tabId }).then((r) => {
      setData(r ?? { host: "", fields: [], view: null, error: "Lockdown couldn't read this page." })
      setNever(r?.view?.never === true && r.view.never_source === "yours")
      const mine: Record<number, FieldRole | ""> = {}
      r?.fields.forEach((f, i) => {
        mine[i] = r.view?.fields.find((x) => x.source === "yours" && sameMatch(x.match, matchFor(f)))?.role ?? ""
      })
      setRoles(mine)
    })
  }, [tabId])
  const save = async () => {
    if (!data) return
    const fix: FillFix = {
      never,
      fields: data.fields.flatMap((f, i) => (roles[i] ? [{ match: matchFor(f), role: roles[i] as FieldRole }] : [])),
    }
    setBusy(true)
    const done = await send<{ logins?: number; error?: string }>({ t: "fix_set", tab_id: tabId, fix })
    setBusy(false)
    if (!done || done.error) say(done?.error ?? "Lockdown didn't keep the fix.")
    else say(`Kept the fix with ${done.logins} ${done.logins === 1 ? "login" : "logins"} for ${data.host}. It works on the page now.`)
  }
  return (
    <section className="flex flex-col gap-3" onKeyDown={(e) => e.key === "Escape" && !e.defaultPrevented && onBack()}>
      <div className="flex items-center gap-2">
        <Button variant="ghost" size="icon-sm" aria-label="Back" onClick={onBack}>
          <Icon name="arrow--left" />
        </Button>
        <h1 ref={title} tabIndex={-1} className="type-subtitle text-foreground outline-none">
          Fix this page
        </h1>
      </div>
      {!data ? (
        <p className="flex items-center gap-2 type-body text-muted-foreground">
          <Spinner /> Reading this page…
        </p>
      ) : data.error ? (
        <Body>{data.error}</Body>
      ) : (
        <>
          <Body>If Lockdown fills the wrong field here, say which is which. It's kept with your logins for this site and syncs with them.</Body>
          <ToggleRow label={`Never fill on ${data.host}`} checked={never} onCheckedChange={(on) => setNever(on === true)} rule="none" />
          {data.view?.never_source === "community" && (
            <Body>
              Lockdown's list says not to fill on {data.host}.{data.view.note ? ` ${data.view.note}` : ""}
            </Body>
          )}
          {data.fields.length ? (
            <Rows label="Fields on this page">
              {data.fields.map((f, i) => (
                <ListRow
                  key={i}
                  render={<li />}
                  size="compact"
                  metaStyle="plain"
                  title={<label htmlFor={`fix-role-${i}`}>{fieldName(f)}</label>}
                  meta={`${f.type} // now ${ROLES.find(([v]) => v === (f.role ?? ""))?.[1].toLowerCase() ?? "an ordinary field"}`}
                  // The Select sits under the words, not at the row's end: at
                  // 336 px a trailing one leaves the field's name three lines
                  // and its mono line cut (doc 87 §2).
                  detail={
                    <NativeSelect id={`fix-role-${i}`} size="sm" value={roles[i] ?? ""} onChange={(e) => setRoles({ ...roles, [i]: e.target.value as FieldRole | "" })}>
                      {ROLES.map(([value, words]) => (
                        <NativeSelectOption key={value} value={value}>
                          {words}
                        </NativeSelectOption>
                      ))}
                    </NativeSelect>
                  }
                />
              ))}
            </Rows>
          ) : (
            <Body>This page has no fields Lockdown can name.</Body>
          )}
          <div className="flex justify-end">
            <Button loading={busy} onClick={() => void save()}>
              Save fix
            </Button>
          </div>
        </>
      )}
    </section>
  )
}

// --- "Hide my email": an alias for a sign-up form (docs/plan/23-bridges.md §5) -----

function Aliases({ site, say }: { site: Site; say: Say }) {
  const [services, setServices] = useState<AliasOption[]>([])
  const [busy, setBusy] = useState<string | null>(null)
  const web = Boolean(site.url && /^https?:/.test(site.url))
  useEffect(() => {
    if (!web) return
    void send<{ services?: AliasOption[] }>({ t: "alias_options" }).then((r) => setServices(r?.services ?? []))
  }, [web])
  if (!web || services.length === 0) return null
  const make = async (svc: AliasOption) => {
    setBusy(svc.item_id)
    say(`Asking ${svc.name} for a new address…`)
    const made = await send<{ address?: string; filled?: boolean; error?: string }>({ t: "alias_fill", tab_id: site.tab_id, item_id: svc.item_id })
    setBusy(null)
    if (!made || made.error || !made.address) say(made?.error ?? "No alias was made.")
    else say(made.filled ? `Made ${made.address} and put it in the email field.` : `Made ${made.address}. This page has no email field to put it in; copy it from here.`)
  }
  return (
    <section className="flex flex-col">
      <SectionHeader inset title="Hide my email" />
      <Rows label="Alias services">
        {services.map((svc) => (
          <ListRow
            key={svc.item_id}
            render={<li />}
            size="compact"
            metaStyle="plain"
            title={svc.name}
            meta={svc.domain ? `On ${svc.domain}` : "A new address that forwards to you"}
            action={
              <Button variant="secondary" size="sm" aria-label={`New alias from ${svc.name}`} loading={busy === svc.item_id} onClick={() => void make(svc)}>
                New alias
              </Button>
            }
          />
        ))}
      </Rows>
    </section>
  )
}

// --- quick access: the core's search, copy and a code shown in place ------------

/** A result's mono line: `kestrel04 // github.com`, or `card // protected`. */
function resultLine(item: SearchItem): string {
  if (item.crown_jewel) return `${item.kind} // protected`
  return [item.username, item.host || item.subtitle].filter(Boolean).join(" // ") || item.kind
}

function Search({ lookalike, say }: { lookalike: string[]; say: Say }) {
  const [query, setQuery] = useState("")
  const [items, setItems] = useState<SearchItem[]>([])
  const [confirming, setConfirming] = useState<string | null>(null)
  const [shown, setShown] = useState<(ShownCode & { item_id: string; expires?: number }) | null>(null)
  const latest = useRef(0)
  const input = useRef<HTMLInputElement>(null)
  const results = useRef<HTMLUListElement>(null)
  // The search has focus as soon as the popup opens (code-display aids).
  useEffect(() => input.current?.focus(), [])
  useEffect(() => {
    const mine = ++latest.current
    const t = setTimeout(
      async () => {
        const r = await send<{ items?: SearchItem[]; error?: string }>({ t: "search", query })
        // An older search that answers late never replaces a newer one.
        if (mine !== latest.current) return
        setItems(r?.items ?? [])
        if (r?.error) say(r.error)
        else if (query && (r?.items ?? []).length === 0) say("Nothing matches.")
      },
      query ? 120 : 0,
    )
    return () => clearTimeout(t)
  }, [query, say])
  useEffect(() => {
    if (!shown) return
    const t = setTimeout(() => setShown(null), Math.min(SHOWN_CODE_MS, (shown.seconds_left ?? 30) * 1000))
    return () => clearTimeout(t)
  }, [shown])

  const copy = async (item: SearchItem): Promise<boolean> => {
    if (lookalike.length > 0 && confirming !== item.item_id) {
      // The paste path's explicit confirm (D166): this page imitates a saved site.
      setConfirming(item.item_id)
      say(`This page imitates ${lookalike.join(", ")}. Copy only if you mean to paste somewhere else.`)
      return false
    }
    setConfirming(null)
    const r = await send<{ label?: string; clears_in_secs?: number; error?: string }>({ t: "copy", item_id: item.item_id })
    if (r?.error) {
      say(r.error)
      return false
    }
    say(`Copied the ${r.label} for ${item.title}. Lockdown clears it in ${r.clears_in_secs} seconds.`)
    return true
  }
  const showCode = async (item: SearchItem) => {
    const r = await send<ShownCode & { error?: string }>({ t: "code_show", item_id: item.item_id })
    if (!r || r.error) say(r?.error ?? "Lockdown didn't show the code.")
    else setShown({ ...r, item_id: item.item_id, expires: r.seconds_left === null ? undefined : Math.floor(Date.now() / 1000) + r.seconds_left })
  }
  // From the field, Down moves into the results.
  const intoResults = (e: KeyboardEvent<HTMLElement>) => {
    if (e.key !== "ArrowDown") return
    const first = results.current?.querySelector<HTMLElement>('[data-slot="list-row-action"] button')
    if (!first) return
    e.preventDefault()
    first.focus()
  }
  return (
    <section className="flex flex-col gap-2">
      <div onKeyDown={intoResults}>
        <SearchField ref={input} placeholder="Search Lockdown" aria-label="Search Lockdown" value={query} onValueChange={setQuery} />
      </div>
      <ul ref={results} role="list" aria-label="Results" className="flex flex-col" onKeyDown={moveAmongActions}>
        {items.map((item) => {
          const title = item.title || "Untitled"
          const showing = shown?.item_id === item.item_id
          // One action: Copy, or for a code, Show code and then Copy code
          // while it is shown.
          const action = !item.copy ? undefined : item.copy.how === "code" && !showing ? (
            <Button variant="ghost" size="sm" aria-label={`Show code for ${title}`} onClick={() => void showCode(item)}>
              Show code
            </Button>
          ) : (
            <Button variant="ghost" size="sm" onClick={() => void copy(item)}>
              {confirming === item.item_id ? "Copy anyway" : showing ? "Copy code" : `Copy ${item.copy.label}`}
            </Button>
          )
          return (
            <ListRow
              key={item.item_id}
              render={<li />}
              size="compact"
              metaStyle="plain"
              title={title}
              meta={resultLine(item)}
              marks={item.exact === false ? <Badge>Close match</Badge> : undefined}
              detail={
                showing ? (
                  // The code shown in place for its 30 seconds. The row's own
                  // action copies it through the app, which reads the
                  // clipboard's seconds; the code's own copy goes the same way
                  // and leaves the countdown to the status line (doc 87 §3).
                  <CodeDisplay
                    layout="compact"
                    code={shown.code}
                    period={30}
                    expiresAt={shown.expires}
                    onCopy={async () => {
                      await copy(item)
                      throw new Error("the status line reports this copy")
                    }}
                  />
                ) : undefined
              }
              action={action}
            />
          )
        })}
      </ul>
    </section>
  )
}
