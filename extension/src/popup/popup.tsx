// The toolbar popup on unicron (docs/plan/26-desktop-finish.md §9): pairing
// with the app, the logins for the page in front, quick access over the
// core's one search index (ranked, a typo forgiven, never a secret), copy
// through the desktop's clipboard path, a one-time code shown in place, per-
// site fill fixes and "Hide my email". It shows summaries only: title,
// username, site, kind (D113 for crown jewels). The in-page menu and prompts
// stay plain HTML (frame.ts): they load nothing a page could influence.

import { useCallback, useEffect, useRef, useState } from "react"

import {
  Badge,
  Button,
  Checkbox,
  Field,
  FieldLabel,
  Icon,
  Input,
  Item,
  ItemActions,
  ItemContent,
  ItemDescription,
  ItemGroup,
  ItemTitle,
  NativeSelect,
  NativeSelectOption,
  Spinner,
} from "@unicron/ui"

import type { AliasOption, ChangeLink, FieldMatch, FieldRole, FillFix, LinkState, PageField, PageMatches, PickAnswer, SearchItem, ShownCode, SiteFixView } from "../shared/protocol"

const params = new URLSearchParams(location.search)
const tabParam = params.has("tab") ? Number(params.get("tab")) : undefined

function send<T>(msg: Record<string, unknown>): Promise<T> {
  return chrome.runtime.sendMessage(msg) as Promise<T>
}

type Say = (text: string) => void

/** How long a code shown here stays on screen. */
const SHOWN_CODE_MS = 30_000

export function Popup() {
  const [state, setState] = useState<LinkState | null>(null)
  const [status, setStatus] = useState("")
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

  return (
    <main className="flex w-[360px] flex-col gap-3 bg-background p-3 text-foreground">
      {/* The pairing and locked states title themselves; the others get the same
          page heading for a screen reader only (design review R18). */}
      {(!state || (state.link === "ready" && state.paired && !state.locked)) && <h1 className="sr-only">Lockdown</h1>}
      {!state ? (
        <p className="flex items-center gap-2 text-sm text-muted-foreground">
          <Spinner /> Connecting to Lockdown…
        </p>
      ) : state.link === "ready" && state.paired && !state.locked ? (
        <Main say={setStatus} />
      ) : (
        <Link state={state} onChange={setState} onRetry={start} say={setStatus} />
      )}
      <p role="status" aria-live="polite" className="text-sm text-muted-foreground">
        {status}
      </p>
    </main>
  )
}

function Link({ state, onChange, onRetry, say }: { state: LinkState; onChange: (s: LinkState) => void; onRetry: () => void; say: Say }) {
  const [busy, setBusy] = useState(false)
  if (state.link !== "ready") {
    return (
      <section className="flex flex-col gap-2">
        <h1 className="text-base font-semibold">Lockdown</h1>
        <p className="text-sm">{state.message || "Lockdown isn't reachable."}</p>
        <Button onClick={onRetry}>Try again</Button>
      </section>
    )
  }
  if (state.pairing) {
    const code = state.pairing.code
    return (
      <section className="flex flex-col gap-2">
        <h1 className="text-base font-semibold">Lockdown</h1>
        <p className="text-sm">Check that Lockdown on this computer shows this code, then approve it there.</p>
        <p className="text-center font-mono text-3xl tracking-widest" aria-label={`Pairing code ${code.split("").join(" ")}`}>
          {code.slice(0, 3)} {code.slice(3)}
        </p>
      </section>
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
      <section className="flex flex-col gap-2">
        <h1 className="text-base font-semibold">Lockdown</h1>
        <p className="text-sm">{state.message || "Pair this browser with the Lockdown app on this computer to fill and save sign-ins."}</p>
        <Button disabled={busy} onClick={() => void pair()}>
          Pair with Lockdown
        </Button>
      </section>
    )
  }
  return (
    <section className="flex flex-col gap-2">
      <h1 className="text-base font-semibold">Lockdown</h1>
      <p className="text-sm">Lockdown is locked. Unlock it in the app to fill and search.</p>
    </section>
  )
}

type Site = { url?: string; tab_id?: number; matches: PageMatches | null }

function Main({ say }: { say: Say }) {
  const [site, setSite] = useState<Site>({ matches: null })
  useEffect(() => {
    void send<Site>({ t: "site", tab_id: tabParam }).then((s) => setSite(s ?? { matches: null }))
  }, [])
  const lookalike = site.matches?.lookalike_of ?? []
  return (
    <>
      <SiteSection site={site} say={say} />
      <Aliases site={site} say={say} />
      <Search lookalike={lookalike} say={say} />
    </>
  )
}

function SiteSection({ site, say }: { site: Site; say: Say }) {
  const m = site.matches
  if (!m) return null
  const fill = async (itemId: string, title: string) => {
    if (site.tab_id === undefined) return
    const r = await send<PickAnswer & { error?: string }>({ t: "fill_tab", tab_id: site.tab_id, item_id: itemId })
    say(r?.done ? `Filled ${title}.` : (r?.confirm ?? r?.refuse ?? r?.error ?? "Lockdown didn't fill."))
  }
  return (
    <section className="flex flex-col gap-2">
      {m.lookalike_of.length > 0 && (
        <p role="alert" className="rounded-md border border-destructive bg-destructive/10 p-2 text-sm text-destructive">
          Stop: {m.host} isn't {m.lookalike_of.join(", ")}. It only looks like a site you have a login for. Lockdown won't fill here.
        </p>
      )}
      {m.logins.length > 0 && (
        <>
          <h2 className="text-sm font-semibold">For {m.host}</h2>
          <ItemGroup role="list" aria-label={`Logins for ${m.host}`}>
            {m.logins.map((c) => (
              <Item key={c.item_id} role="listitem" variant="outline" size="sm">
                <ItemContent>
                  <ItemTitle>
                    {c.title || m.host}
                    {c.asked && <Badge variant="secondary">From quick access</Badge>}
                  </ItemTitle>
                  <ItemDescription>{c.crown_jewel ? "protected" : c.username}</ItemDescription>
                </ItemContent>
                <ItemActions>
                  <Button size="sm" onClick={() => void fill(c.item_id, c.title)}>
                    Fill
                  </Button>
                  <ChangePassword itemId={c.item_id} title={c.title || m.host} say={say} />
                </ItemActions>
              </Item>
            ))}
          </ItemGroup>
          <FixSection tabId={site.tab_id} say={say} />
        </>
      )}
    </section>
  )
}

type Opened = { kind: ChangeLink["kind"]; checked: boolean; note: string | null; host: string; error?: string }

/** The security center's checked change-password link (docs/plan/23-bridges.md §7). */
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
  return (
    <Button variant="link" size="sm" onClick={() => void open()}>
      Change password
    </Button>
  )
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

function FixSection({ tabId, say }: { tabId?: number; say: Say }) {
  const [data, setData] = useState<Fields | null>(null)
  const [never, setNever] = useState(false)
  const [roles, setRoles] = useState<Record<number, FieldRole | "">>({})
  const [busy, setBusy] = useState(false)
  const load = async () => {
    if (data) return
    const r = await send<Fields>({ t: "fix_fields", tab_id: tabId })
    setData(r ?? { host: "", fields: [], view: null, error: "Lockdown couldn't read this page." })
    setNever(r?.view?.never === true && r.view.never_source === "yours")
    const mine: Record<number, FieldRole | ""> = {}
    r?.fields.forEach((f, i) => {
      mine[i] = r.view?.fields.find((x) => x.source === "yours" && sameMatch(x.match, matchFor(f)))?.role ?? ""
    })
    setRoles(mine)
  }
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
    <details className="rounded-md border p-2 text-sm" onToggle={(e) => e.currentTarget.open && void load()}>
      <summary className="cursor-default">Fix this page</summary>
      {data?.error && <p className="mt-2">{data.error}</p>}
      {data && !data.error && (
        <div className="mt-2 flex flex-col gap-2">
          <p>If Lockdown fills the wrong field here, say which is which. It's kept with your logins for this site and syncs with them.</p>
          <Field orientation="horizontal">
            <Checkbox id="fix-never" checked={never} onCheckedChange={(on) => setNever(on === true)} />
            <FieldLabel htmlFor="fix-never">Never fill on {data.host}</FieldLabel>
          </Field>
          {data.view?.never_source === "community" && (
            <p>
              Lockdown's list says not to fill on {data.host}.{data.view.note ? ` ${data.view.note}` : ""}
            </p>
          )}
          {data.fields.length ? (
            <ItemGroup role="list" aria-label="Fields on this page">
              {data.fields.map((f, i) => (
                <Item key={i} role="listitem" size="sm">
                  <ItemContent>
                    <ItemTitle>
                      <label htmlFor={`fix-role-${i}`}>{fieldName(f)}</label>
                    </ItemTitle>
                    <ItemDescription>
                      {f.type} · now {ROLES.find(([v]) => v === (f.role ?? ""))?.[1].toLowerCase() ?? "an ordinary field"}
                    </ItemDescription>
                  </ItemContent>
                  <ItemActions>
                    <NativeSelect id={`fix-role-${i}`} value={roles[i] ?? ""} onChange={(e) => setRoles({ ...roles, [i]: e.target.value as FieldRole | "" })}>
                      {ROLES.map(([value, words]) => (
                        <NativeSelectOption key={value} value={value}>
                          {words}
                        </NativeSelectOption>
                      ))}
                    </NativeSelect>
                  </ItemActions>
                </Item>
              ))}
            </ItemGroup>
          ) : (
            <p>This page has no fields Lockdown can name.</p>
          )}
          <Button size="sm" disabled={busy} onClick={() => void save()}>
            Save fix
          </Button>
        </div>
      )}
    </details>
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
    <section className="flex flex-col gap-2">
      <h2 className="text-sm font-semibold">Hide my email</h2>
      <ItemGroup role="list" aria-label="Alias services">
        {services.map((svc) => (
          <Item key={svc.item_id} role="listitem" variant="outline" size="sm">
            <ItemContent>
              <ItemTitle>{svc.name}</ItemTitle>
              <ItemDescription>{svc.domain ? `on ${svc.domain}` : "a new address that forwards to you"}</ItemDescription>
            </ItemContent>
            <ItemActions>
              <Button variant="outline" size="sm" aria-label={`New alias from ${svc.name}`} disabled={busy === svc.item_id} onClick={() => void make(svc)}>
                New alias
              </Button>
            </ItemActions>
          </Item>
        ))}
      </ItemGroup>
    </section>
  )
}

// --- quick access: the core's search, copy and a code shown in place ------------

function Search({ lookalike, say }: { lookalike: string[]; say: Say }) {
  const [query, setQuery] = useState("")
  const [items, setItems] = useState<SearchItem[]>([])
  const [confirming, setConfirming] = useState<string | null>(null)
  const [shown, setShown] = useState<(ShownCode & { item_id: string }) | null>(null)
  const latest = useRef(0)
  const input = useRef<HTMLInputElement>(null)
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

  const copy = async (item: SearchItem) => {
    if (lookalike.length > 0 && confirming !== item.item_id) {
      // The paste path's explicit confirm: this page imitates a saved site.
      setConfirming(item.item_id)
      say(`This page imitates ${lookalike.join(", ")}. Copy only if you mean to paste somewhere else.`)
      return
    }
    setConfirming(null)
    const r = await send<{ label?: string; clears_in_secs?: number; error?: string }>({ t: "copy", item_id: item.item_id })
    if (r?.error) say(r.error)
    else say(`Copied the ${r.label} for ${item.title}. Lockdown clears it in ${r.clears_in_secs} seconds.`)
  }
  const showCode = async (item: SearchItem) => {
    const r = await send<ShownCode & { error?: string }>({ t: "code_show", item_id: item.item_id })
    if (!r || r.error) say(r?.error ?? "Lockdown didn't show the code.")
    else setShown({ ...r, item_id: item.item_id })
  }
  return (
    <section className="flex flex-col gap-2">
      <Input ref={input} type="search" placeholder="Search Lockdown" aria-label="Search Lockdown" value={query} onChange={(e) => setQuery(e.target.value)} />
      <ItemGroup role="list" aria-label="Results">
        {items.map((item) => (
          <Item key={item.item_id} role="listitem" size="sm">
            <ItemContent>
              <ItemTitle>
                {item.title || "Untitled"}
                {item.exact === false && <Badge variant="outline">Close match</Badge>}
              </ItemTitle>
              <ItemDescription>
                {item.crown_jewel ? `${item.kind} · protected` : [item.username, item.host || item.subtitle].filter(Boolean).join(" · ") || item.kind}
              </ItemDescription>
              {shown?.item_id === item.item_id && (
                <p className="font-mono text-2xl tracking-widest" aria-label={`Code ${shown.code.split("").join(" ")}`}>
                  {shown.code}
                </p>
              )}
            </ItemContent>
            <ItemActions>
              {item.copy?.how === "code" && (
                <Button variant="ghost" size="sm" aria-label={`Show code for ${item.title}`} onClick={() => void showCode(item)}>
                  <Icon name="pin" />
                </Button>
              )}
              {item.copy && (
                <Button variant="outline" size="sm" onClick={() => void copy(item)}>
                  {confirming === item.item_id ? "Copy anyway" : `Copy ${item.copy.label}`}
                </Button>
              )}
            </ItemActions>
          </Item>
        ))}
      </ItemGroup>
    </section>
  )
}
