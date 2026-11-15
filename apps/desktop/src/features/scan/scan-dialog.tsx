import { useCallback, useRef, useState, type ChangeEvent, type FormEvent } from "react"

import { AdaptiveDialog, Banner, Button, FieldRow, RadioGroup, RadioRow, Tabs, TabsContent, TabsList, TabsTrigger, TextField } from "@unicron/ui"
import { ApprovalSheet } from "@unicron/ui/security/approval-sheet"
import { DigitGroups } from "@unicron/ui/security/digit-groups"

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

/** The kinds of outcome, as the review's title. */
const HEADINGS: Record<ScanOutcome["kind"], string> = {
  device_code: "A TV or console sign-in",
  passkey_qr: "A passkey sign-in from another device",
  steam_qr: "A Steam sign-in",
  unlisted: "Not an activation page Lockdown knows",
  other: "Not a sign-in code",
  none: "No code found",
}

/** Both layers' overline, so the two read as one flow. */
const SCAN_OVERLINE = "Codes // TV sign-in"

/** The Form's one sentence (brief C2). */
const SCAN_SENTENCE = "Point your camera at the code the TV shows. Codes sent to you, pasted or in a picture are how attackers get sign-ins approved."

/** An activation waits this long in the core, and is used once (D311). */
const ACTIVATION_SECS = 300

/** Where the code came from: only the camera can lead to a yes (the proximity rule, D99, D310). */
type Way = "camera" | "picture" | "paste"

/**
 * "Sign in a TV or console" (docs/plan/26-desktop-finish.md §3; brief C2, doc 88).
 * Two layers in sequence. The Form reads the code three ways (CAMERA, PICTURE,
 * PASTE); reading one moves on by itself, so it has no primary. The core reads
 * the code, runs the proximity gate and checks the page against Lockdown's
 * signed list. A device code then opens the Review on unicron's ApprovalSheet:
 * its `origin` is `scan` only for the camera, so a picture or a paste renders
 * no approve control at all. The yes goes through the `Approve` gate, then the
 * real page opens; a code the page won't take in its address stays here to
 * type. Anything else (refused, not something this computer can do, not a
 * sign-in) is a Review with the core's words and SCAN AGAIN.
 */
export function ScanDialog({ open, onOpenChange, onNotice }: ScanDialogProps) {
  const scan = useScanClient()
  const gate = useGate()
  const [tab, setTab] = useState<Way>("camera")
  const [read, setRead] = useState<{ outcome: ScanOutcome; way: Way; at: number } | null>(null)
  const [error, setError] = useState<string | null>(null)
  /** The camera's own trouble, shown on the Camera tab only (design review R6). */
  const [cameraError, setCameraError] = useState<string | null>(null)
  const [account, setAccount] = useState<string | null>(null)
  const [started, setStarted] = useState<ActivationStarted | null>(null)
  const [busy, setBusy] = useState(false)
  const [tries, setTries] = useState(0)
  const file = useRef<HTMLInputElement>(null)
  const outcome = read?.outcome ?? null
  useCaptureProtect(open && (outcome?.decision === "approve" || started !== null))

  const again = () => {
    setRead(null)
    setError(null)
    setCameraError(null)
    setAccount(null)
    setStarted(null)
    setBusy(false)
  }
  const close = () => {
    if (outcome?.activation_id && !started) void scan.cancel(outcome.activation_id)
    again()
    setTab("camera")
    onOpenChange(false)
  }
  const show = (o: ScanOutcome, way: Way) => {
    setRead({ outcome: o, way, at: Date.now() })
    setAccount(o.accounts[0]?.item_id ?? null)
    setError(null)
  }
  const onFrame = useCallback(
    async (frame: Frame) => {
      const o = await scan.frame(frame, "camera")
      if (!o.found) return false
      setRead({ outcome: o, way: "camera", at: Date.now() })
      setAccount(o.accounts[0]?.item_id ?? null)
      setError(null)
      return true
    },
    [scan],
  )
  const onFile = async (e: ChangeEvent<HTMLInputElement>) => {
    const picked = e.currentTarget.files?.[0]
    e.currentTarget.value = ""
    if (!picked) return
    setError(null)
    try {
      const frame = await frameOfFile(picked)
      if (!frame) throw new Error("That file isn't a picture Lockdown can read.")
      show(await scan.frame(frame, "image"), "picture")
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
      show(await scan.text(text), "paste")
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
      const who = outcome.accounts.find((a) => a.item_id === account)
      onNotice(who ? `Opened ${s.host}. Lockdown offers ${who.title || who.username} first on that page` : `Opened ${s.host}. Sign in there to finish`)
      if (s.code_in_url) close()
      else setStarted(s)
    } catch (err) {
      setError(isAppError(err) && err.message.startsWith("Cancelled") ? "Nothing was approved." : describeError(err))
    } finally {
      setBusy(false)
    }
  }

  // The Review on the security component: a device code, before the yes.
  const asking = open && !!outcome && outcome.kind === "device_code" && outcome.decision === "approve" && !started
  // The Review with the core's words: everything else the core read, and a sign-in that needs its code typed.
  const telling = open && !!outcome && !asking

  return (
    <>
      <AdaptiveDialog
        open={open && !outcome}
        onOpenChange={(o) => !o && close()}
        size="form"
        overline={SCAN_OVERLINE}
        title="Sign in a TV or console"
        description={SCAN_SENTENCE}
      >
        <Tabs
          value={tab}
          onValueChange={(v) => {
            // A tab's trouble stays on that tab: the camera tries again when its tab
            // comes back, and a picture's or a paste's error is about what was tried there.
            setTab(v as Way)
            setError(null)
            setCameraError(null)
          }}
        >
          <TabsList variant="line">
            <TabsTrigger value="camera">Camera</TabsTrigger>
            <TabsTrigger value="picture">Picture</TabsTrigger>
            <TabsTrigger value="paste">Paste</TabsTrigger>
          </TabsList>
          <TabsContent value="camera">
            <div className="flex flex-col items-center gap-4 pt-4">
              {open && !outcome && tab === "camera" && !cameraError && (
                <CameraView key={tries} label="The camera's picture" onFrame={onFrame} onError={setCameraError} />
              )}
              {cameraError && (
                <Banner
                  size="compact"
                  title={cameraError}
                  action={
                    <Button variant="secondary" size="sm" onClick={() => (setCameraError(null), setTries((n) => n + 1))}>
                      Try again
                    </Button>
                  }
                />
              )}
            </div>
          </TabsContent>
          <TabsContent value="picture">
            <div className="flex flex-col items-start gap-3 pt-4">
              <input ref={file} type="file" accept="image/*" hidden aria-label="A picture of the code" onChange={(e) => void onFile(e)} />
              <Button variant="secondary" onClick={() => file.current?.click()}>
                Choose picture
              </Button>
              <p className="type-caption text-text-secondary">Lockdown reads it, and tells you why it won't approve it.</p>
              {error && <Banner size="compact" title={error} />}
            </div>
          </TabsContent>
          <TabsContent value="paste">
            <form id="scan-paste" onSubmit={(e) => void onPaste(e)} className="flex flex-col items-start gap-3 pt-4">
              <TextField label="The code's address" name="code" autoComplete="off" spellCheck={false} />
              <Button type="submit" variant="secondary">
                Check it
              </Button>
              {error && <Banner size="compact" title={error} />}
            </form>
          </TabsContent>
        </Tabs>
      </AdaptiveDialog>

      {asking && read && (
        <ApprovalSheet
          key={read.at}
          kind="device-code"
          origin={read.way === "camera" ? "scan" : "push"}
          overline={SCAN_OVERLINE}
          title={HEADINGS[outcome.kind]}
          context={[
            ...(outcome.service ? [{ label: "Service", value: outcome.service }] : []),
            ...(outcome.host ? [{ label: "Page", value: outcome.host, emphasis: true }] : []),
          ]}
          matching={{ mode: "none" }}
          expiresAt={read.at + ACTIVATION_SECS * 1000}
          note={read.way === "camera" ? <SignInWith outcome={outcome} account={account} onAccount={setAccount} /> : undefined}
          approveLabel={outcome.host ? `Sign in on ${outcome.host}` : "Sign in"}
          denyLabel={read.way === "camera" ? "Don't sign in" : "Scan again"}
          busy={busy}
          error={error ?? undefined}
          onApprove={() => void approve()}
          onDeny={(reason) => {
            if (reason === "origin") {
              if (outcome.activation_id) void scan.cancel(outcome.activation_id)
              again()
            } else if (reason !== "timeout") close()
          }}
          onReport={close}
          onOpenChange={(o) => !o && close()}
        />
      )}

      <AdaptiveDialog
        open={telling}
        onOpenChange={(o) => !o && close()}
        size="review"
        overline={SCAN_OVERLINE}
        title={outcome ? HEADINGS[outcome.kind] : ""}
        footer={
          started ? (
            <Button onClick={close}>Done</Button>
          ) : (
            <Button variant="secondary" onClick={again}>
              Scan again
            </Button>
          )
        }
      >
        {outcome && <Told outcome={outcome} started={started} />}
      </AdaptiveDialog>
    </>
  )
}

/**
 * Under the facts on the Review: the TV's code in groups, then which saved
 * login signs in (or by hand), and the one caption that says when to say yes.
 * It sits in the sheet's note line, so every element in it is a phrasing one.
 */
function SignInWith({ outcome, account, onAccount }: { outcome: ScanOutcome; account: string | null; onAccount: (id: string | null) => void }) {
  return (
    <span className="flex flex-col gap-4">
      {outcome.code && (
        <span className="flex items-baseline gap-4">
          <span className="type-overline text-text-label">TV's code</span>
          <DigitGroups size="sm" value={outcome.code} label="TV's code" />
        </span>
      )}
      <span className="flex flex-col gap-2">
        <span id="sign-in-with" className="type-overline text-text-label">
          Sign in with
        </span>
        <RadioGroup render={<span />} aria-labelledby="sign-in-with" value={account ?? ""} onValueChange={(v) => onAccount(v ? String(v) : null)}>
          {outcome.accounts.map((a) => (
            <RadioRow key={a.item_id} value={a.item_id} label={a.title || a.username} description={a.title ? a.username : undefined} />
          ))}
          <RadioRow value="" label="I'll sign in by hand" />
        </RadioGroup>
      </span>
      <span className="type-caption text-text-secondary">Only say yes if this is the TV or console in front of you, and it asked for this service.</span>
    </span>
  )
}

/** The Review with the core's words: what it read, why it won't go on, or the code to type on the page. */
function Told({ outcome, started }: { outcome: ScanOutcome; started: ActivationStarted | null }) {
  if (started)
    return (
      <div className="flex flex-col gap-4">
        <p className="type-body text-text-body">{started.service}'s page is open in your browser. Type this code on the page:</p>
        <DigitGroups size="lg" value={started.code} label="Code" />
      </div>
    )
  const refused = outcome.decision === "refused"
  return (
    <div className="flex flex-col gap-4">
      {refused ? (
        <Banner size="compact" tone="critical" title="Refused" body={outcome.reason} />
      ) : (
        outcome.reason && <Banner size="compact" tone="info" title={outcome.reason} />
      )}
      {outcome.host && (
        <div role="list" className="flex flex-col">
          {outcome.service && (
            <div role="listitem">
              <FieldRow label="Service">{outcome.service}</FieldRow>
            </div>
          )}
          <div role="listitem">
            <FieldRow label="Page">{outcome.host}</FieldRow>
          </div>
          {outcome.code && (
            <div role="listitem">
              <FieldRow label="TV's code">
                <DigitGroups size="sm" value={outcome.code} label="TV's code" />
              </FieldRow>
            </div>
          )}
        </div>
      )}
    </div>
  )
}
