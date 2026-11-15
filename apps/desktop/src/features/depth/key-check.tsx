import { useCallback, useEffect, useRef, useState, type FormEvent } from "react"

import { AdaptiveDialog, Button, DigitGroups, ListRow, SectionHeader, TextField } from "@unicron/ui"

import { describeError } from "@/core/client"

import { POLL_MS, phraseDigits, useDepthClient, type PhraseCheckView, type PhraseIncoming } from "./depth-client"

/** Keeps asking how a check stands while it waits; stops when it ends. */
function useCheck(onEnd: (v: PhraseCheckView) => void) {
  const depth = useDepthClient()
  const [check, setCheck] = useState<PhraseCheckView | null>(null)
  const timer = useRef<ReturnType<typeof setInterval> | null>(null)
  const stop = useCallback(() => {
    if (timer.current) clearInterval(timer.current)
    timer.current = null
  }, [])
  useEffect(() => {
    if (!depth || !check || check.state !== "waiting") return stop
    const id = check.verify_id
    timer.current = setInterval(() => {
      depth
        .phrasePoll(id)
        .then((v) => {
          setCheck(v)
          if (v.state !== "waiting") onEnd(v)
        })
        .catch(stop)
    }, POLL_MS)
    return stop
  }, [depth, check, stop, onEnd])
  return { check, setCheck, stop }
}

/** The one line that follows a check: waiting, matched, or the core's words when it fails (brief S2). */
function CheckLine({ check }: { check: PhraseCheckView }) {
  if (check.state === "waiting" || check.state === "matched")
    return (
      <p role="status" className="type-body text-text-body">
        {check.words}
      </p>
    )
  return (
    <p role="alert" className="type-body text-critical-text">
      {check.words}
    </p>
  )
}

/**
 * "Check with a phrase" (docs/plan/22-security-depth.md §9, brief S2): this
 * app shows nine digits to say to the other person; they type them into
 * theirs; the two apps prove to each other, through the server but out of
 * its reach, that each holds the other's real keys. A match verifies them
 * here. A question-size dialog on a desktop, a sheet on a phone; the check
 * starts when it opens and is called off when it closes before the end.
 */
export function SayPhrase({ login, open, onClose, onVerified }: { login: string; open: boolean; onClose: () => void; onVerified: () => void }) {
  const depth = useDepthClient()
  const [error, setError] = useState<string | null>(null)
  const { check, setCheck, stop } = useCheck(useCallback((v: PhraseCheckView) => v.state === "matched" && onVerified(), [onVerified]))
  const started = useRef(false)
  useEffect(() => {
    if (!open || !depth || started.current) return
    started.current = true
    setError(null)
    depth
      .phraseInvite(login)
      .then(setCheck)
      .catch((e) => setError(describeError(e)))
  }, [open, depth, login, setCheck])
  if (!depth) return null
  const close = () => {
    stop()
    if (check?.state === "waiting") void depth.phraseCancel(check.verify_id).catch(() => undefined)
    started.current = false
    setCheck(null)
    setError(null)
    onClose()
  }
  const waiting = check === null || check.state === "waiting"
  return (
    <AdaptiveDialog
      open={open}
      onOpenChange={(o) => !o && close()}
      size="question"
      overline={`Key check // ${login}`}
      title={`Tell ${login} this phrase`}
      description="Say it in person or on a call you trust, never in a message. It works once."
      footer={
        <Button type="button" variant={waiting ? "secondary" : "default"} onClick={close}>
          {waiting ? "Cancel" : "Done"}
        </Button>
      }
    >
      <div className="flex flex-col gap-4">
        {check?.phrase && <DigitGroups value={check.phrase} size="lg" label="Phrase" />}
        {check && <CheckLine check={check} />}
        {error && (
          <p role="alert" className="type-body text-critical-text">
            {error}
          </p>
        )}
      </div>
    </AdaptiveDialog>
  )
}

/** A button that starts a phrase check with someone, and its dialog. */
export function PhraseInvite({ login, onVerified, variant = "secondary", disabled, disabledReason }: {
  login: string
  onVerified: () => void
  variant?: "secondary" | "ghost"
  disabled?: boolean
  disabledReason?: string
}) {
  const depth = useDepthClient()
  const [open, setOpen] = useState(false)
  if (!depth) return null
  return (
    <>
      <Button type="button" variant={variant} disabled={disabled} disabledReason={disabled ? disabledReason : undefined} onClick={() => setOpen(true)}>
        Check with a phrase
      </Button>
      <SayPhrase login={login} open={open} onClose={() => setOpen(false)} onVerified={onVerified} />
    </>
  )
}

/**
 * KEY CHECKS FOR YOU (brief S2): the checks others opened with this
 * person, a row each with TYPE THEIR PHRASE. The answer is a question-size
 * form: one field, one try.
 */
export function PhraseIncomingList({ online, onVerified }: { online: boolean; onVerified: () => void }) {
  const depth = useDepthClient()
  const [incoming, setIncoming] = useState<PhraseIncoming[]>([])
  const [answering, setAnswering] = useState<PhraseIncoming | null>(null)
  const [error, setError] = useState<string | null>(null)
  const { check, setCheck, stop } = useCheck(useCallback((v: PhraseCheckView) => v.state === "matched" && onVerified(), [onVerified]))

  useEffect(() => {
    if (!depth || !online) return
    depth
      .phraseIncoming()
      .then(setIncoming)
      .catch(() => setIncoming([]))
  }, [depth, online])

  if (!depth || incoming.length === 0) return null

  const submit = async (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    if (!answering) return
    const form = e.currentTarget
    const digits = phraseDigits(String(new FormData(form).get("phrase") ?? ""))
    if (!digits) {
      setError("The phrase is nine digits.")
      return
    }
    setError(null)
    try {
      setCheck(await depth.phraseAnswer(answering.verify_id, digits))
      form.reset()
    } catch (err) {
      setError(describeError(err))
    }
  }
  const close = () => {
    stop()
    const done = answering?.verify_id
    setAnswering(null)
    setCheck(null)
    setError(null)
    if (check) setIncoming((all) => all.filter((i) => i.verify_id !== done))
  }
  const ended = check !== null && check.state !== "waiting"

  return (
    <section aria-label="Key checks for you" className="flex flex-col">
      <SectionHeader title="Key checks for you" count={incoming.length} />
      <div role="list" aria-label="Key checks for you">
        {incoming.map((i) => (
          <ListRow
            key={i.verify_id}
            role="listitem"
            size="compact"
            title={i.login}
            meta="Wants to check keys with you"
            action={
              <Button type="button" size="sm" variant="secondary" onClick={() => setAnswering(i)}>
                Type their phrase
              </Button>
            }
          />
        ))}
      </div>
      <AdaptiveDialog
        open={answering !== null}
        onOpenChange={(o) => !o && close()}
        size="question"
        overline={answering ? `Key check // ${answering.login}` : undefined}
        title={`Check keys with ${answering?.login ?? ""}`}
        description="Type the nine digits they told you."
        footer={
          <>
            <Button type="button" variant={ended ? "default" : "ghost"} onClick={close}>
              {ended ? "Done" : "Cancel"}
            </Button>
            {!check && (
              <Button type="submit" form="phrase-answer">
                Check
              </Button>
            )}
          </>
        }
      >
        {check ? (
          <CheckLine check={check} />
        ) : (
          <form id="phrase-answer" aria-label="Answer their check" onSubmit={(e) => void submit(e)}>
            <TextField
              label="Their phrase"
              name="phrase"
              inputMode="numeric"
              autoComplete="off"
              helper="You get one try. If it's wrong, they start a new check."
              error={error ?? undefined}
            />
          </form>
        )}
      </AdaptiveDialog>
    </section>
  )
}
