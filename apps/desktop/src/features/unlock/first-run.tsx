import { useCallback, useEffect, useMemo, useState, type FormEvent } from "react"
import { createPortal } from "react-dom"

import { Button, Checkbox, Icon, Label, Spinner } from "@unicron/ui"
import { LargeTypeView } from "@unicron/ui/security/large-type-view"

import { describeError, useClient } from "@/core/client"
import type { ImportFormatView, OncePage, Status } from "@/core/types"
import { useCaptureProtect } from "@/features/hygiene/hygiene-client"
import { ImportWizard } from "@/features/import/import-wizard"
import { usePaper, usePrinting } from "@/features/paper/paper-client"
import { kitValues, type ShownPage } from "@/features/status/once-page"

import { useDemoEntry } from "./demo-entry"
import { hostOf, useFirstRunClient, type Candidate, type FirstRunClient, type QuickUnlock } from "./first-run-client"
import { JoinScreen } from "./join"
import { EMPTY_VALUES, checkValues, decodePage, useMasterStrength, type MasterPasswordValues } from "./master-password"
import { MasterPasswordFields } from "./master-password-fields"
import { WelcomeActions, WelcomeColumn, WelcomeScreen } from "./welcome"

/**
 * The first run (docs/plan/39-first-run.md §1.9; D94: one sentence per
 * screen, detail behind Learn more; D95: the proofs once, at account
 * creation). No server words anywhere on the create path:
 *
 * 0. Welcome: what Lockdown is; **Create a new vault**, or **I already
 *    use Lockdown** (screen J), and **Explore the demo** in a build that
 *    has one.
 * 1. Your name and master password, with the expectations set plainly.
 * 2. Your recovery kit: Save as PDF, Print, and where to keep it.
 * 3. Unlock with Touch ID? (only where this build can offer it).
 * 4. Bring your passwords (optional).
 * 5. The proof moment, then the vault.
 */
export type FirstRunScreen = "welcome" | "password" | "kit" | "quick" | "import" | "proof" | "join"

interface FirstRunProps {
  onOpened: (status: Status) => void
  /** Where to start: a first run that stopped after the account resumes at the kit. */
  start?: FirstRunScreen
  /** The kit page to show when starting at the kit (a new one, made on resume). */
  kit?: ShownPage | null
}

export function FirstRun({ onOpened, start = "welcome", kit: resumed = null }: FirstRunProps) {
  const client = useClient()
  const frc = useFirstRunClient()
  const [screen, setScreen] = useState<FirstRunScreen>(start)
  const [kit, setKit] = useState<ShownPage | null>(resumed)
  const [name, setName] = useState<string | undefined>(undefined)
  const [quick, setQuick] = useState<QuickUnlock | null>(null)

  useEffect(() => {
    frc?.quickUnlock().then(setQuick, () => setQuick(null))
  }, [frc])
  const steps = quick?.available ? 4 : 3
  const stage = (s: Parameters<FirstRunClient["stage"]>[0]) => frc?.stage(s).catch(() => {})
  const finish = async () => {
    await stage("done")
    // The page's string lives only while this flow does.
    setKit(null)
    onOpened(await client.status())
  }

  if (!frc) return null
  switch (screen) {
    case "welcome":
      return <Welcome frc={frc} onCreate={() => setScreen("password")} onJoin={() => setScreen("join")} />
    case "join":
      return <JoinScreen onBack={() => setScreen("welcome")} onOpened={onOpened} />
    case "password":
      return (
        <PasswordScreen
          frc={frc}
          steps={steps}
          onBack={() => setScreen("welcome")}
          onMade={(page, made) => {
            setName(made)
            setKit(decodePage(page))
            setScreen("kit")
          }}
        />
      )
    case "kit":
      return (
        <KitScreen
          page={kit}
          name={name}
          step={[2, steps]}
          onContinue={() => {
            void stage("kit_saved")
            setKit(null)
            setScreen(quick?.available ? "quick" : "import")
          }}
        />
      )
    case "quick":
      return (
        <QuickScreen
          quick={quick}
          step={[3, steps]}
          onDone={() => {
            void stage("quick_offered")
            setScreen("import")
          }}
        />
      )
    case "import":
      return (
        <ImportScreen
          step={[steps, steps]}
          onDone={() => {
            void stage("import_done")
            setScreen("proof")
          }}
        />
      )
    case "proof":
      return <ProofScreen onOpen={() => void finish()} />
  }
}

/** Screen 0: what Lockdown is, the two doors, what discovery found, the demo. */
function Welcome({ frc, onCreate, onJoin }: { frc: FirstRunClient; onCreate: () => void; onJoin: () => void }) {
  const demo = useDemoEntry()
  const [found, setFound] = useState<Candidate | null>(null)
  useEffect(() => {
    let live = true
    // This computer and its config first (fast), then the network.
    const pick = (list: Candidate[]) => list.find((c) => c.state === "ready") ?? null
    frc
      .candidates(false)
      .then((l) => live && setFound((f) => f ?? pick(l)))
      .catch(() => {})
      .then(() => frc.candidates(true))
      .then((l) => live && l && setFound((f) => f ?? pick(l)))
      .catch(() => {})
    return () => {
      live = false
    }
  }, [frc])
  const foundLine = found
    ? found.sources[0]?.kind === "policy"
      ? `Your organization set a server: ${hostOf(found.address)}`
      : found.sources[0]?.kind === "network"
        ? `We found a Lockdown server on your network: ${hostOf(found.address)}`
        : `We found a Lockdown server on this computer: ${hostOf(found.address)}`
    : null
  return (
    <WelcomeColumn>
      <WelcomeScreen
        mark="welcome"
        centered
        title="Every way you sign in. Locked down, even from us."
        sentence="Lockdown keeps your passwords, passkeys and one-time codes in one vault on this computer."
        learnMore={{
          title: "Every way you sign in. Locked down, even from us.",
          paragraphs: [
            "Your vault is encrypted on this computer, and only your master password opens it. Lockdown never sees that password.",
            "Each sign-in keeps its password, passkeys and one-time codes together, so one account is one record.",
            "Later you can add your other devices, or move to your own server. Nothing here needs an account with us.",
            "You can take everything out at any time, in a file other apps can read.",
          ],
        }}
      />
      <WelcomeActions>
        <Button size="lg" onClick={onCreate}>
          Create a new vault
        </Button>
        <Button size="lg" variant="outline" onClick={onJoin}>
          I already use Lockdown
        </Button>
      </WelcomeActions>
      <div className="flex flex-col items-center gap-(--c-welcome-gap-tight) text-center">
        {foundLine && (
          <Button variant="link" size="inline" onClick={onJoin}>
            {foundLine}
          </Button>
        )}
        {/* Only in a build with the demo (docs/plan/40-devtools.md §3). */}
        {demo && (
          <Button variant="link" size="inline" onClick={demo.explore} disabled={demo.busy}>
            {demo.busy ? "Opening the demo…" : "Explore the demo"}
          </Button>
        )}
      </div>
    </WelcomeColumn>
  )
}

const PASSWORD_LEARN_MORE = [
  "Lockdown turns your master password into the key that opens your vault, here on this computer. The password itself is never stored and never sent anywhere.",
  "That's why nobody, Lockdown included, can tell you what it was or reset it. If you forget it, your recovery kit is the only way back in.",
  "Length matters most. A phrase of four or more ordinary words is easier to remember, and much harder to guess, than a short word with symbols.",
  "Your name is only a label for the lock screen. It isn't an email address and isn't sent anywhere.",
]

/** Screen 1: the name and the master password, with the expectations set. */
function PasswordScreen({ frc, steps, onBack, onMade }: { frc: FirstRunClient; steps: number; onBack: () => void; onMade: (page: OncePage, name: string) => void }) {
  const [values, setValues] = useState<MasterPasswordValues>(EMPTY_VALUES)
  const [errors, setErrors] = useState<ReturnType<typeof checkValues>>({})
  const [busy, setBusy] = useState(false)
  const [failure, setFailure] = useState<string | null>(null)
  const strength = useMasterStrength(values, true)
  const submit = async (e: FormEvent) => {
    e.preventDefault()
    const found = checkValues(values, true, strength)
    setErrors(found)
    setFailure(null)
    if (Object.keys(found).length) return
    setBusy(true)
    try {
      const name = values.name.trim()
      const page = await frc.create(name, values.password)
      setValues(EMPTY_VALUES)
      onMade(page, name)
    } catch (err) {
      const words = describeError(err)
      // The core's own floor (Q3) speaks to the password field.
      if (words.includes("at least fair")) setErrors({ password: words })
      else setFailure(words)
    } finally {
      setBusy(false)
    }
  }
  return (
    <WelcomeColumn>
      <WelcomeScreen
        step={[1, steps]}
        title="Choose your master password"
        sentence="This is the one password to remember: it opens your vault, and Lockdown can't see it or reset it."
        learnMore={{ title: "Your master password", paragraphs: PASSWORD_LEARN_MORE }}
      />
      <form onSubmit={submit} className="flex flex-col gap-(--c-welcome-gap)" noValidate>
        <MasterPasswordFields idPrefix="fr" values={values} onChange={setValues} errors={errors} withName readOnly={busy} strength={strength} />
        {failure && (
          <p role="alert" className="text-sm text-destructive">
            {failure}
          </p>
        )}
        <WelcomeActions>
          <Button size="lg" type="submit" disabled={busy}>
            {busy ? <Spinner data-icon="inline-start" /> : null}
            {busy ? "Setting up your vault…" : "Continue"}
          </Button>
          <Button size="lg" type="button" variant="outline" onClick={onBack} disabled={busy}>
            Back
          </Button>
          {busy ? (
            <p className="text-center text-sm text-muted-foreground">This takes a few seconds.</p>
          ) : (
            <p className="text-center text-sm text-muted-foreground">If you forget it, only your recovery kit gets you back in. You'll make the kit next.</p>
          )}
        </WelcomeActions>
      </form>
    </WelcomeColumn>
  )
}

/** How long the kit's values stay on screen before "Show again". */
const KIT_SHOWN_MS = 60_000

/** The characters a person types, without the printed dashes. */
const typed = (value: string) => value.replace(/-/g, "")

/** Screen 2: the recovery kit, saved, printed or written down. */
export function KitScreen({ page, name, step, onContinue }: { page: ShownPage | null; name?: string; step?: [number, number]; onContinue: () => void }) {
  const paper = usePaper()
  const { print, printing } = usePrinting()
  const [shown, setShown] = useState(true)
  const [checked, setChecked] = useState(false)
  const [saving, setSaving] = useState(false)
  const [saved, setSaved] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [printCopy, setPrintCopy] = useState(false)
  const text = page?.text ?? null
  const values = useMemo(() => (text ? kitValues(text) : { secretKey: null, recoveryCode: null }), [text])
  useCaptureProtect(shown && text !== null)

  // Hidden after a minute, and when the window loses focus.
  useEffect(() => {
    if (!shown) return
    const timer = window.setTimeout(() => setShown(false), KIT_SHOWN_MS)
    const hide = () => setShown(false)
    window.addEventListener("blur", hide)
    return () => {
      window.clearTimeout(timer)
      window.removeEventListener("blur", hide)
    }
  }, [shown])

  const encoder = useMemo(() => new TextEncoder(), [])
  const secretKey = useCallback(() => Promise.resolve(encoder.encode(typed(values.secretKey ?? ""))), [encoder, values.secretKey])
  const recoveryCode = useCallback(() => Promise.resolve(encoder.encode(typed(values.recoveryCode ?? ""))), [encoder, values.recoveryCode])

  const save = async () => {
    if (!paper.saveKitPdf || !page?.text) return
    setSaving(true)
    setError(null)
    try {
      if ((await paper.saveKitPdf(page.lines, page.text)) === "saved") setSaved(true)
    } catch (e) {
      setError(`Lockdown couldn't save the PDF: ${describeError(e)}. Try another folder, or print it instead.`)
    } finally {
      setSaving(false)
    }
  }
  const printKit = async () => {
    // The page prints alone from a copy outside the app (the print rule
    // keeps the app off the paper while a `data-print-page` is up).
    setPrintCopy(true)
    await new Promise<void>((r) => requestAnimationFrame(() => r()))
    try {
      await print()
    } finally {
      setPrintCopy(false)
    }
  }
  return (
    <WelcomeColumn>
      <WelcomeScreen
        step={step}
        title="Save your recovery kit"
        sentence="If you forget your master password, this page is the only way back into your vault."
        learnMore={{
          title: "Your recovery kit",
          paragraphs: [
            "The kit holds your Secret Key and a recovery code. Lockdown made them here, keeps no copy of the page, and has never seen your Secret Key.",
            "With the kit you can choose a new master password on this computer if you forget yours. Anyone with both the kit and this computer could open your vault, so keep them apart.",
            "If this computer is lost, the kit brings your vault back only once you've turned on backups or added another device. You can do both later.",
          ],
        }}
      />
      <section aria-label={name ? `Recovery kit for ${name}` : "Your recovery kit"} className="flex flex-col gap-(--c-welcome-gap-tight) rounded-xl border bg-card p-(--c-page-gutter-narrow)">
        <h2 className="font-medium">{name ? `Recovery kit for ${name}` : "Your recovery kit"}</h2>
        {shown && values.secretKey && <LargeTypeView print getSecret={secretKey} title="Secret Key" groupSize={4} />}
        {shown && values.recoveryCode && <LargeTypeView print getSecret={recoveryCode} title="Recovery code" groupSize={4} />}
        {!shown && (
          <div className="flex flex-col items-start gap-2">
            <p className="text-sm text-muted-foreground">The kit is hidden while you're away from it.</p>
            <Button variant="link" size="inline" onClick={() => setShown(true)}>
              Show again
            </Button>
          </div>
        )}
      </section>
      <p className="text-base">Keep it somewhere safe away from this computer, such as a drawer at home or with your important papers.</p>
      {saved && (
        <p role="status" className="text-sm">
          Saved. A PDF on this computer alone won't help if this computer is lost: print it, or copy it somewhere else safe.
        </p>
      )}
      {error && (
        <p role="alert" className="text-sm text-destructive">
          {error}
        </p>
      )}
      <WelcomeActions>
        {paper.saveKitPdf && (
          <Button size="lg" variant={checked ? "outline" : "default"} onClick={() => void save()} disabled={saving || !text}>
            {saving ? <Spinner data-icon="inline-start" /> : null}
            {saving ? "Saving…" : "Save as PDF"}
          </Button>
        )}
        <Button size="lg" variant="outline" onClick={() => void printKit()} disabled={printing || !text}>
          Print
        </Button>
        <div className="flex items-center gap-2 py-2">
          <Checkbox id="fr-kit-saved" checked={checked} onCheckedChange={(c) => setChecked(c === true)} />
          <Label htmlFor="fr-kit-saved">I saved, printed or wrote down the kit</Label>
        </div>
        <Button size="lg" variant={checked ? "default" : "outline"} onClick={onContinue} disabled={!checked}>
          Continue
        </Button>
      </WelcomeActions>
      {printCopy &&
        text &&
        createPortal(
          <div data-print-page="" className="hidden print:block">
            <pre className="font-mono text-sm leading-relaxed whitespace-pre-wrap">{text}</pre>
          </div>,
          document.body,
        )}
    </WelcomeColumn>
  )
}

/** Screen 3: Touch ID or Windows Hello, where this build can offer it. */
function QuickScreen({ quick, step, onDone }: { quick: QuickUnlock | null; step: [number, number]; onDone: () => void }) {
  const hello = quick?.kind === "windows_hello"
  return (
    <WelcomeColumn>
      <WelcomeScreen
        step={step}
        title={hello ? "Unlock with Windows Hello?" : "Unlock with Touch ID?"}
        sentence={
          hello
            ? "Use your face, fingerprint or PIN instead of typing your master password most of the time."
            : "Use your fingerprint instead of typing your master password most of the time."
        }
        learnMore={{
          title: hello ? "Unlocking with Windows Hello" : "Unlocking with Touch ID",
          paragraphs: [
            "Lockdown still asks for your master password each time it starts, and at least every 14 days, so you don't forget it.",
            hello
              ? "Your face or fingerprint never reaches Lockdown: Windows checks it and then releases a key kept for this vault."
              : "Your fingerprint never reaches Lockdown: macOS checks it and then releases a key kept for this vault.",
          ],
        }}
      />
      <Icon name="fingerprint" aria-hidden="true" />
      <WelcomeActions>
        <Button size="lg" onClick={onDone} disabled={!quick?.available}>
          {hello ? "Use Windows Hello" : "Use Touch ID"}
        </Button>
        <Button size="lg" variant="outline" onClick={onDone}>
          Not now
        </Button>
        <p className="text-center text-sm text-muted-foreground">You can change this later in Settings → Lock.</p>
      </WelcomeActions>
    </WelcomeColumn>
  )
}

/** The importers first run lists, in this order, when this build has them. */
const FIRST_RUN_FORMATS = ["1pux", "bitwarden", "csv", "kdbx", "proton", "dashlane", "lastpass", "keeper", "enpass", "cxf"]

/** Screen 4: bring passwords in from the app used now, or skip. */
function ImportScreen({ step, onDone }: { step: [number, number]; onDone: () => void }) {
  const client = useClient()
  const [formats, setFormats] = useState<ImportFormatView[]>([])
  const [open, setOpen] = useState<string | null>(null)
  const [brought, setBrought] = useState<number | null>(null)
  useEffect(() => {
    client
      .importFormats()
      .then((all) => setFormats(FIRST_RUN_FORMATS.flatMap((id) => all.filter((f) => f.id === id))))
      .catch(() => setFormats([]))
  }, [client])
  return (
    <WelcomeColumn>
      <WelcomeScreen
        step={step}
        title="Bring your passwords"
        sentence="Move your passwords in from the app you use now, or skip this and add them as you go."
        learnMore={{
          title: "Bringing your passwords",
          paragraphs: [
            "Lockdown reads the file on this computer and shows what it holds, and anything it can't bring along, before anything is saved.",
            "Most apps export a file that isn't protected. Once your passwords are in, delete that file.",
          ],
        }}
      />
      {brought !== null && (
        <p role="status" className="text-base">
          {brought} {brought === 1 ? "item" : "items"} brought in. Delete the exported file now: it isn't protected.
        </p>
      )}
      <ul className="flex flex-col divide-y rounded-xl border" aria-label="Apps Lockdown can bring passwords in from">
        {formats.map((f) => (
          <li key={f.id}>
            <button type="button" data-row="" className="flex w-full items-center gap-3 px-4 text-left hover:bg-muted/50" onClick={() => setOpen(f.id)}>
              <span className="flex flex-1 flex-col">
                <span className="font-medium">{f.name}</span>
                <span className="text-sm text-muted-foreground">{f.how}</span>
              </span>
              <Icon name="chevron_right" aria-hidden="true" />
            </button>
          </li>
        ))}
      </ul>
      <div className="text-center">
        <Button variant="link" size="inline" onClick={() => setOpen("auto")}>
          {brought === null ? "Other apps and authenticator files" : "Bring in another"}
        </Button>
      </div>
      <WelcomeActions>
        <Button size="lg" variant={brought === null ? "outline" : "default"} onClick={onDone}>
          {brought === null ? "Skip for now" : "Continue"}
        </Button>
      </WelcomeActions>
      <ImportWizard
        key={open ?? "closed"}
        open={open !== null}
        initialFormat={open ?? undefined}
        onClose={() => setOpen(null)}
        onImported={(report) => setBrought((n) => (n ?? 0) + report.imported)}
      />
    </WelcomeColumn>
  )
}

/** Screen 5: the proof moment (D95), once. */
export function ProofScreen({ onOpen }: { onOpen: () => void }) {
  return (
    <WelcomeColumn>
      <WelcomeScreen title="Locked, even from us" sentence="Three things are true from now on; Status can check them again any time." />
      <ul className="flex flex-col gap-(--c-welcome-gap-tight) text-base">
        {[
          "Your master password never leaves this computer.",
          "Lockdown has never seen the Secret Key on your kit.",
          "Your vault's history is signed on this computer, so a swapped or older copy is caught.",
        ].map((line) => (
          <li key={line} className="flex items-start gap-3">
            <Icon name="check_circle" aria-hidden="true" />
            <span>{line}</span>
          </li>
        ))}
      </ul>
      <WelcomeActions>
        <Button size="lg" onClick={onOpen}>
          Open the vault
        </Button>
      </WelcomeActions>
    </WelcomeColumn>
  )
}

