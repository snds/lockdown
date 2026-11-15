import { useMemo, useState, type FormEvent } from "react"

import {
  AdaptiveDialog,
  AdaptiveMenuItem,
  Button,
  EmptyState,
  Field,
  FieldDescription,
  FieldLabel,
  IconButton,
  NativeSelect,
  NativeSelectOption,
  SearchField,
  TextField,
  useAppFrame,
} from "@unicron/ui"
import { CodeDisplay, CodeDisplayList, CodeDisplayMenuItem } from "@unicron/ui/security/code-display"
import { LargeTypeView } from "@unicron/ui/security/large-type-view"
import { SecretField } from "@unicron/ui/security/secret-field"

import { useGate } from "@/app/gate"
import { notify } from "@/app/notify"
import { ScreenHead } from "@/app/page-head"
import { useStatusItem } from "@/app/status"
import { describeError, useClient } from "@/core/client"
import type { CodeView, ItemSummary } from "@/core/types"
import { count } from "@/lib/format"
import { ClockSkew } from "@/features/bridges/clock"
import { useCaptureProtect } from "@/features/hygiene/hygiene-client"
import { ScanDialog } from "@/features/scan/scan-dialog"
import { LARGE_TYPE_LIFETIME_S } from "@/features/vault/large-type"

import { codeKey, useCodes } from "./use-codes"
import { useCodeHost } from "./use-code-host"

interface CodesViewProps {
  /** The vault's rows, so the add form can offer the Logins to put a code inside. */
  rows: ItemSummary[]
  onChanged: () => void
  /** Opens the item a code belongs to (a code's ⋯ → Open item). Left out, that row is not offered. */
  onOpenItem?: (itemId: string) => void
}

/** The search field shows from this many codes up (brief C1). */
export const SEARCH_FROM = 10

/** What the status line says on Codes (brief X2, row 3), and the phone's overline. */
const REFRESH_WORDS = "Refresh every 30 s"

/** How many placeholders show while the codes load. */
const LOADING = 3

/**
 * Codes as a page (brief C1; Mobile / 08, Desktop / 06): every code item
 * and every code inside a Login, on unicron's CodeDisplay, rows on a phone
 * and tiles from the medium width up. The page head says how many, with
 * ADD CODE and SIGN IN TV; the clock notice sits under it. Nothing here
 * holds a seed: the core hands over values and timing only (D102).
 */
export function CodesView({ rows, onChanged, onOpenItem }: CodesViewProps) {
  const { codes, error, now, expiresAt, reload } = useCodes(true)
  const frame = useAppFrame()
  const [query, setQuery] = useState("")
  const [adding, setAdding] = useState(false)
  const [scanning, setScanning] = useState(false)
  useStatusItem({ text: REFRESH_WORDS })

  const total = codes?.length ?? 0
  const searchable = !frame.phone && total >= SEARCH_FROM
  const q = searchable ? query.trim() : ""
  const shown = useMemo(() => {
    if (!codes) return []
    const words = q.toLowerCase()
    if (!words) return codes
    return codes.filter((c) => [c.title, c.issuer ?? "", c.account ?? ""].some((s) => s.toLowerCase().includes(words)))
  }, [codes, q])

  const layout = frame.phone ? "row" : "tile"
  const rail = frame.layout === "rail"
  const add = () => setAdding(true)
  const actions = frame.phone ? (
    <>
      <IconButton label="Sign in TV" icon="screen" variant="ghost" onClick={() => setScanning(true)} />
      <IconButton label="Add code" icon="add" variant="ghost" onClick={add} />
    </>
  ) : (
    <>
      {searchable && (
        // 348 wide, left of the two actions (brief C1).
        <div className="w-87 max-w-full">
          <SearchField aria-label="Search codes" placeholder="Search issuers and accounts" value={query} onValueChange={setQuery} />
        </div>
      )}
      <Button variant="ghost" onClick={add}>
        Add code
      </Button>
      <Button variant="ghost" onClick={() => setScanning(true)}>
        Sign in TV
      </Button>
    </>
  )

  return (
    <div className="flex flex-col gap-6">
      <ScreenHead
        overline={frame.phone ? `Codes // ${count(total)} // ${REFRESH_WORDS}` : `Codes // ${count(total)}`}
        title="Codes"
        count={frame.phone ? count(total) : undefined}
        actions={rail ? undefined : actions}
      />
      {/* A phone on its side draws the head collapsed, without its actions (doc 88 §5): they stand here instead. */}
      {rail && <div className="flex gap-2">{actions}</div>}
      <ClockSkew />
      {error && (
        <EmptyState tone="critical" overline="Codes // 00" title="Codes didn't load." body={error} actionLabel="Try again" onAction={reload} />
      )}
      {!codes && !error && (
        <div className={frame.phone ? "-mx-4" : undefined}>
          <CodeDisplayList layout={frame.phone ? "list" : "grid"} aria-busy="true">
            {Array.from({ length: LOADING }, (_, i) => (
              <CodeDisplay key={i} layout={layout} loading />
            ))}
          </CodeDisplayList>
        </div>
      )}
      {codes && total === 0 && (
        <EmptyState overline="Codes // 00" title="No codes yet." body="Add one from the set-up link a site gives you." actionLabel="Add code" onAction={add} />
      )}
      {codes && total > 0 && shown.length === 0 && <EmptyState overline="Search // 00" title={`No code for ${q}.`} />}
      {/* unicron's CodeDisplayList puts its live region beside the codes, so the list is
          Lockdown's own, inside it, with one item per code (review R7). */}
      {shown.length > 0 && (
        // A phone's rows are full-bleed (Mobile / 08): each row keeps its own 16 inside, so the list
        // steps back out of the frame's 16 (AppFrameInset's px-4) that the page column puts round it.
        <div className={frame.phone ? "-mx-4" : undefined}>
          <CodeDisplayList layout={frame.phone ? "list" : "grid"}>
            <ul role="list" aria-label="One-time codes" className="contents">
              {shown.map((c) => (
                <li key={codeKey(c)} className="flex flex-col">
                  <CodeOnPage code={c} now={now} expiresAt={expiresAt[codeKey(c)] ?? null} layout={layout} onOpenItem={onOpenItem} />
                </li>
              ))}
            </ul>
          </CodeDisplayList>
        </div>
      )}
      <AddCodeDialog
        open={adding}
        logins={rows.filter((r) => r.kind === "login" && r.state === "live")}
        onClose={() => setAdding(false)}
        onAdded={() => {
          setAdding(false)
          reload()
          onChanged()
        }}
      />
      <ScanDialog open={scanning} onOpenChange={setScanning} onNotice={(t) => void notify(t)} />
    </div>
  )
}

/**
 * One code on the page: unicron's CodeDisplay in the row or tile layout, with
 * its trailing ⋯ (LA5, C18): Open item · Show large type · Copy. A tap or a
 * click anywhere on it copies; the component draws the countdown, "Copied"
 * and "Cleared".
 */
function CodeOnPage({ code, now, expiresAt, layout, onOpenItem }: { code: CodeView; now: number; expiresAt: number | null; layout: "row" | "tile"; onOpenItem?: (id: string) => void }) {
  const host = useCodeHost(code, now, expiresAt)
  const { asking } = useGate()
  const [big, setBig] = useState(false)
  useCaptureProtect(big)
  const named = [code.issuer, code.account].filter(Boolean).join(" · ") || code.title
  const shown = host.shown
  // The code is on the screen already: large type draws the same characters, bigger.
  const getSecret = useMemo(() => () => Promise.resolve(new TextEncoder().encode(shown)), [shown])
  return (
    <>
      <CodeDisplay
        layout={layout}
        variant={host.variant}
        digits={host.digits}
        code={host.code}
        next={host.next}
        period={host.period}
        now={host.now}
        expiresAt={host.expiresAt}
        issuer={host.issuer}
        account={host.account}
        warnings={host.warnings}
        clipboardTimer={host.clipboardTimer}
        counter={host.counter}
        onCopy={host.onCopy}
        onClipboardClear={host.onClipboardClear}
        onAdvance={host.onAdvance}
        onFixClock={host.onFixClock}
        menu={
          <>
            {onOpenItem && (
              <AdaptiveMenuItem icon="launch" onSelect={() => onOpenItem(code.item_id)}>
                Open item
              </AdaptiveMenuItem>
            )}
            <AdaptiveMenuItem icon="text--scale" disabled={asking} onSelect={() => setBig(true)}>
              Show large type
            </AdaptiveMenuItem>
            <CodeDisplayMenuItem action="copy">Copy</CodeDisplayMenuItem>
          </>
        }
      />
      <LargeTypeView open={big} onOpenChange={setBig} getSecret={getSecret} title={`Code · ${named}`} groupSize={code.digits === 8 ? 4 : 3} lifetime={LARGE_TYPE_LIFETIME_S} />
    </>
  )
}

/**
 * Add a code (brief C1): a Form. The set-up link in a SecretField (it holds
 * the code's secret, so it stays masked), where to keep it, and a title for
 * its own item. A link the core refuses shows the core's sentence. Adding by
 * camera is not built (review R5, W11): the words say "paste".
 */
function AddCodeDialog({ open, logins, onClose, onAdded }: { open: boolean; logins: ItemSummary[]; onClose: () => void; onAdded: () => void }) {
  const client = useClient()
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [uri, setUri] = useState("")
  const [into, setInto] = useState("")
  const [title, setTitle] = useState("")
  const clear = () => {
    setUri("")
    setInto("")
    setTitle("")
    setError(null)
  }
  const close = () => {
    clear()
    onClose()
  }
  const submit = async (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    if (!uri.trim()) {
      setError("Paste the set-up link first.")
      return
    }
    setBusy(true)
    setError(null)
    try {
      await client.addCode({ uri: uri.trim(), title: (!into && title.trim()) || undefined, into: into || undefined })
      clear()
      onAdded()
    } catch (err) {
      setError(describeError(err))
    } finally {
      setBusy(false)
    }
  }
  return (
    <AdaptiveDialog
      open={open}
      onOpenChange={(o) => !o && close()}
      size="form"
      overline="Codes // new"
      title="Add a code"
      closeOnOutsideClick={false}
      footer={
        <>
          <Button variant="secondary" onClick={close} disabled={busy}>
            Cancel
          </Button>
          <Button type="submit" form="add-code" loading={busy}>
            Add code
          </Button>
        </>
      }
    >
      <form id="add-code" onSubmit={(e) => void submit(e)} className="flex flex-col gap-4">
        <SecretField
          mode="edit"
          label="Set-up link"
          name="uri"
          mono
          value={uri}
          onChange={setUri}
          autoComplete="off"
          placeholder="otpauth://totp/…"
          description="Paste the otpauth:// or steam:// link the site gave you."
          error={error ?? undefined}
        />
        <Field>
          <FieldLabel htmlFor="code-into">Keep it</FieldLabel>
          <NativeSelect id="code-into" name="into" value={into} onChange={(e) => setInto(e.currentTarget.value)}>
            <NativeSelectOption value="">As its own item</NativeSelectOption>
            {logins.map((l) => (
              <NativeSelectOption key={l.id} value={l.id}>
                Inside {l.title || l.site}
              </NativeSelectOption>
            ))}
          </NativeSelect>
          <FieldDescription>A code usually belongs to the login it protects.</FieldDescription>
        </Field>
        {!into && <TextField label="Title" name="title" autoComplete="off" value={title} onChange={(e) => setTitle(e.currentTarget.value)} helper="Left empty, the issuer and account are used." />}
      </form>
    </AdaptiveDialog>
  )
}
