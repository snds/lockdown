import { useMemo, useState, type FormEvent } from "react"

import {
  Button,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
  Field,
  FieldDescription,
  FieldError,
  FieldGroup,
  FieldLabel,
  Icon,
  Input,
  NativeSelect,
  NativeSelectOption,
  Spinner,
} from "@unicron/ui"

import { LabelledAction, LabelledActions } from "@/components/labelled-actions"
import { PaneHeader } from "@/components/master-detail"
import { ClockBanner } from "@/features/bridges/clock"
import { describeError, useClient } from "@/core/client"
import type { ItemSummary } from "@/core/types"
import { CodeDisplayList } from "@unicron/ui/security/code-display"

import { ItemCodeDisplay } from "@/features/vault/code-display"
import { ScanDialog } from "@/features/scan/scan-dialog"
import { toast } from "sonner"

import { codeKey, useCodes } from "./use-codes"

interface CodesViewProps {
  /** The vault's rows, so the add form can offer the Logins to put a code inside. */
  rows: ItemSummary[]
  onChanged: () => void
}

/**
 * The Codes view (brief §3.2): every code item and every code inside a
 * Login, scanning fast, each with its countdown and Copy. Nothing here
 * holds a seed; the core hands over values and timing only (D102).
 */
export function CodesView({ rows, onChanged }: CodesViewProps) {
  const { codes, error, now, expiresAt, reload } = useCodes(true)
  const [query, setQuery] = useState("")
  const [adding, setAdding] = useState(false)
  const [scanning, setScanning] = useState(false)

  const shown = useMemo(() => {
    const q = query.trim().toLowerCase()
    if (!codes) return []
    if (!q) return codes
    return codes.filter((c) => [c.title, c.issuer ?? "", c.account ?? ""].some((s) => s.toLowerCase().includes(q)))
  }, [codes, query])

  return (
    <div className="flex min-h-0 flex-1 flex-col bg-surface-chrome">
      <PaneHeader title="Codes">
        <Button size="sm" variant="outline" onClick={() => setScanning(true)}>
          <Icon name="qr_code_scanner" data-icon="inline-start" />
          Sign in a TV
        </Button>
        <Button size="sm" onClick={() => setAdding(true)}>
          <Icon name="add" data-icon="inline-start" />
          Add a code
        </Button>
      </PaneHeader>
      <div className="flex flex-col gap-2 px-4 pb-4">
        <ClockBanner />
        <Input type="search" placeholder="Search issuers and accounts" aria-label="Search codes" value={query} onChange={(e) => setQuery(e.target.value)} />
        {error && (
          <p role="alert" className="text-sm text-destructive">
            {error}
          </p>
        )}
        {!codes && !error && <Spinner />}
        {codes && codes.length === 0 && (
          <Empty>
            <EmptyHeader>
              <EmptyMedia variant="icon">
                <Icon name="timer" />
              </EmptyMedia>
              <EmptyTitle>No codes yet</EmptyTitle>
              <EmptyDescription>
                Add one from the set-up link the site gives you (otpauth:// or steam://). The camera here is for signing in a TV, not for adding
                codes.
              </EmptyDescription>
            </EmptyHeader>
          </Empty>
        )}
        <CodeDisplayList role="list" aria-label="One-time codes">
          {shown.map((c) => (
            <ItemCodeDisplay key={codeKey(c)} code={c} now={now} expiresAt={expiresAt[codeKey(c)] ?? null} density="list" />
          ))}
        </CodeDisplayList>
      </div>
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
      <ScanDialog open={scanning} onOpenChange={setScanning} onNotice={(t) => toast(t)} />
    </div>
  )
}

function AddCodeDialog({ open, logins, onClose, onAdded }: { open: boolean; logins: ItemSummary[]; onClose: () => void; onAdded: () => void }) {
  const client = useClient()
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const submit = async (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    const form = e.currentTarget
    const data = new FormData(form)
    const text = (n: string) => String(data.get(n) ?? "").trim()
    setBusy(true)
    setError(null)
    try {
      await client.addCode({ uri: text("uri"), title: text("title") || undefined, into: text("into") || undefined })
      form.reset()
      onAdded()
    } catch (err) {
      setError(describeError(err))
    } finally {
      setBusy(false)
    }
  }
  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent size="md">
        <form onSubmit={submit} className="grid gap-4">
          <DialogHeader>
            <DialogTitle>Add a code</DialogTitle>
            <DialogDescription>Paste the otpauth:// or steam:// link the site gave you. The set-up is checked here before it is kept.</DialogDescription>
          </DialogHeader>
          <FieldGroup>
            <Field>
              <FieldLabel htmlFor="code-uri">Set-up link</FieldLabel>
              <Input id="code-uri" name="uri" type="password" placeholder="otpauth://totp/…" required autoComplete="off" />
              <FieldDescription>Kept masked while you type; it holds the code's secret.</FieldDescription>
            </Field>
            <Field>
              <FieldLabel htmlFor="code-into">Inside a login</FieldLabel>
              <NativeSelect id="code-into" name="into" defaultValue="">
                <NativeSelectOption value="">Its own item</NativeSelectOption>
                {logins.map((l) => (
                  <NativeSelectOption key={l.id} value={l.id}>
                    {l.title || l.site}
                  </NativeSelectOption>
                ))}
              </NativeSelect>
              <FieldDescription>A code usually belongs to the login it protects.</FieldDescription>
            </Field>
            <Field>
              <FieldLabel htmlFor="code-title">Title</FieldLabel>
              <Input id="code-title" name="title" autoComplete="off" />
              <FieldDescription>For a standalone code; left empty, the issuer and account are used.</FieldDescription>
            </Field>
          </FieldGroup>
          {error && <FieldError role="alert">{error}</FieldError>}
          <LabelledActions>
            <LabelledAction kind="cancel" type="button" onClick={onClose} disabled={busy}>
              Cancel
            </LabelledAction>
            <LabelledAction kind="confirm" type="submit" disabled={busy}>
              Add
            </LabelledAction>
          </LabelledActions>
        </form>
      </DialogContent>
    </Dialog>
  )
}
