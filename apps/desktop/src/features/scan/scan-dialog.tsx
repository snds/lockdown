import { useCallback, useState, type ChangeEvent, type FormEvent } from "react"

import {
  Badge,
  Button,
  Dialog,
  DialogAction,
  DialogActions,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  Field,
  FieldDescription,
  FieldError,
  FieldLabel,
  Icon,
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
  Textarea,
} from "@unicron/ui"

import { useGate } from "@/app/gate"
import { describeError, isAppError } from "@/core/client"
import { useCaptureProtect } from "@/features/hygiene/hygiene-client"

import { CameraView } from "./camera-view"
import { frameOfFile } from "./frames"
import { useScanClient, type ActivationStarted, type Frame, type ScanOutcome } from "./scan-client"

interface ScanDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  onNotice: (text: string) => void
}

/** The kinds of outcome, as a heading. */
const HEADINGS: Record<ScanOutcome["kind"], string> = {
  device_code: "A TV or console sign-in",
  passkey_qr: "A passkey sign-in from another device",
  steam_qr: "A Steam sign-in",
  unlisted: "Not an activation page Lockdown knows",
  other: "Not a sign-in code",
  none: "No code found",
}

/**
 * "Sign in a TV or console" (docs/plan/26-desktop-finish.md §3; brief §2.7):
 * point the camera at the code the TV shows, or open a picture of it, or
 * paste its address. Every way in goes to the core, which reads the code,
 * runs the proximity gate (only the camera, pointed at a screen in front
 * of you, can lead to a yes; a picture or a paste is refused and says why,
 * with no way round it), checks the page against Lockdown's signed list,
 * and shows the service's full domain before anything happens. The yes
 * goes through the `Approve` gate, then the real page opens in the browser
 * with the code, and Lockdown's extension offers the login picked first
 * there. A code the page doesn't take in its address is shown here, large,
 * to type on the page.
 */
export function ScanDialog({ open, onOpenChange, onNotice }: ScanDialogProps) {
  const scan = useScanClient()
  const gate = useGate()
  const [tab, setTab] = useState("camera")
  const [outcome, setOutcome] = useState<ScanOutcome | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [account, setAccount] = useState<string | null>(null)
  const [started, setStarted] = useState<ActivationStarted | null>(null)
  const [busy, setBusy] = useState(false)
  useCaptureProtect(open && (outcome?.decision === "approve" || started !== null))

  const reset = () => {
    setOutcome(null)
    setError(null)
    setAccount(null)
    setStarted(null)
  }
  const close = (o: boolean) => {
    if (!o && outcome?.activation_id && !started) void scan.cancel(outcome.activation_id)
    if (!o) reset()
    onOpenChange(o)
  }
  const show = (o: ScanOutcome) => {
    setOutcome(o)
    setAccount(o.accounts[0]?.item_id ?? null)
  }
  const onFrame = useCallback(
    async (frame: Frame) => {
      const o = await scan.frame(frame, "camera")
      if (!o.found) return false
      show(o)
      return true
    },
    [scan],
  )
  const onFile = async (e: ChangeEvent<HTMLInputElement>) => {
    const file = e.currentTarget.files?.[0]
    e.currentTarget.value = ""
    if (!file) return
    setError(null)
    try {
      const frame = await frameOfFile(file)
      if (!frame) throw new Error("That file isn't a picture Lockdown can read.")
      show(await scan.frame(frame, "image"))
    } catch (err) {
      setError(describeError(err))
    }
  }
  const onPaste = async (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    const text = String(new FormData(e.currentTarget).get("code") ?? "").trim()
    if (!text) return
    setError(null)
    try {
      show(await scan.text(text))
    } catch (err) {
      setError(describeError(err))
    }
  }
  const approve = async () => {
    if (!outcome?.activation_id) return
    setBusy(true)
    setError(null)
    try {
      const s = await gate.run((g) => scan.approve(outcome.activation_id!, account, g))
      setStarted(s)
      const who = outcome.accounts.find((a) => a.item_id === account)
      onNotice(
        who
          ? `Opened ${s.host}. Lockdown offers ${who.title || who.username} first on that page; sign in there to finish.`
          : `Opened ${s.host}. Sign in there to finish.`,
      )
    } catch (err) {
      setError(isAppError(err) && err.message.startsWith("Cancelled") ? "Nothing was approved." : describeError(err))
    } finally {
      setBusy(false)
    }
  }
  const deny = () => close(false)

  return (
    <Dialog open={open} onOpenChange={close}>
      <DialogContent size="lg">
        <DialogHeader>
          <DialogTitle>Sign in a TV or console</DialogTitle>
          <DialogDescription>
            Point your camera at the code the TV shows. Lockdown approves only a code you scan from a screen in front of you: codes sent to you,
            pasted, or in a picture are how attackers get sign-ins approved.
          </DialogDescription>
        </DialogHeader>
        {!outcome && (
          <Tabs value={tab} onValueChange={(v) => setTab(String(v))}>
            <TabsList>
              <TabsTrigger value="camera">Camera</TabsTrigger>
              <TabsTrigger value="image">Picture</TabsTrigger>
              <TabsTrigger value="paste">Paste</TabsTrigger>
            </TabsList>
            <TabsContent value="camera">{open && tab === "camera" && <CameraView label="The camera's picture" onFrame={onFrame} onError={setError} />}</TabsContent>
            <TabsContent value="image">
              <Field>
                <FieldLabel htmlFor="scan-file">A picture of the code</FieldLabel>
                <input id="scan-file" type="file" accept="image/*" onChange={(e) => void onFile(e)} className="text-sm" />
                <FieldDescription>Lockdown reads it, and tells you why it won't approve it.</FieldDescription>
              </Field>
            </TabsContent>
            <TabsContent value="paste">
              <form id="scan-paste" onSubmit={(e) => void onPaste(e)} className="flex flex-col gap-2">
                <Field>
                  <FieldLabel htmlFor="scan-text">The code's address</FieldLabel>
                  <Textarea id="scan-text" name="code" rows={2} autoComplete="off" spellCheck={false} />
                </Field>
                <Button type="submit" variant="outline" size="sm">
                  Check it
                </Button>
              </form>
            </TabsContent>
          </Tabs>
        )}
        {outcome && <Outcome outcome={outcome} account={account} onAccount={setAccount} started={started} />}
        {error && <FieldError role="alert">{error}</FieldError>}
        <DialogActions>
          {outcome && !started && outcome.decision !== "approve" && (
            <DialogAction kind="cancel" onClick={reset}>
              Scan again
            </DialogAction>
          )}
          {outcome?.decision === "approve" && !started ? (
            <>
              <DialogAction kind="cancel" onClick={deny}>
                Don't sign in
              </DialogAction>
              <DialogAction kind="confirm" disabled={busy} onClick={() => void approve()}>
                Sign in on {outcome.host}
              </DialogAction>
            </>
          ) : (
            <DialogAction kind="confirm" onClick={() => close(false)}>
              Done
            </DialogAction>
          )}
        </DialogActions>
      </DialogContent>
    </Dialog>
  )
}

function Outcome({ outcome, account, onAccount, started }: { outcome: ScanOutcome; account: string | null; onAccount: (id: string | null) => void; started: ActivationStarted | null }) {
  return (
    <section aria-label="What Lockdown read" className="flex flex-col gap-3">
      <h3 className="flex items-center gap-2 text-sm font-semibold">
        {HEADINGS[outcome.kind]}
        {outcome.decision === "refused" && <Badge variant="destructive">Refused</Badge>}
        {outcome.decision === "unsupported" && <Badge variant="outline">Not on this computer</Badge>}
      </h3>
      {outcome.host && (
        <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1 text-sm">
          <dt className="text-muted-foreground">Service</dt>
          <dd>{outcome.service}</dd>
          <dt className="text-muted-foreground">Page</dt>
          <dd className="font-semibold">{outcome.host}</dd>
          {outcome.code && (
            <>
              <dt className="text-muted-foreground">TV's code</dt>
              <dd className="font-mono">{outcome.code}</dd>
            </>
          )}
        </dl>
      )}
      {outcome.reason && (
        <p className="text-sm" role={outcome.decision === "refused" ? "alert" : undefined}>
          {outcome.reason}
        </p>
      )}
      {outcome.decision === "approve" && !started && (
        <fieldset className="flex flex-col gap-1">
          <legend className="text-sm text-muted-foreground">Sign in with</legend>
          {outcome.accounts.map((a) => (
            <label key={a.item_id} className="flex items-center gap-2 text-sm">
              <input type="radio" name="activation-account" checked={account === a.item_id} onChange={() => onAccount(a.item_id)} />
              {a.title || a.username} <span className="text-muted-foreground">{a.username}</span>
            </label>
          ))}
          <label className="flex items-center gap-2 text-sm">
            <input type="radio" name="activation-account" checked={account === null} onChange={() => onAccount(null)} />
            I'll sign in by hand
          </label>
          <p className="text-xs text-muted-foreground">Only say yes if this is the TV or console in front of you, and it asked for this service.</p>
        </fieldset>
      )}
      {started && (
        <div className="flex flex-col gap-1">
          <p className="flex items-center gap-2 text-sm">
            <Icon name="open_in_browser" /> {started.service}'s page is open in your browser.
          </p>
          {!started.code_in_url && (
            <>
              <p className="text-sm">Type this code on the page:</p>
              <p className="text-center font-mono text-4xl tracking-[0.3em]" aria-label={`Code ${started.code.split("").join(" ")}`}>
                {started.code}
              </p>
            </>
          )}
        </div>
      )}
    </section>
  )
}
