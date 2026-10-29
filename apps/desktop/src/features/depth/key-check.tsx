import { useCallback, useEffect, useRef, useState, type FormEvent } from "react"

import {
  Button,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  Field,
  FieldDescription,
  FieldError,
  FieldLabel,
  Input,
  Spinner,
} from "@unicron/ui"

import { LabelledAction, LabelledActions } from "@/components/labelled-actions"
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

/** The outcome in a sentence, or the waiting spinner. */
function CheckState({ check }: { check: PhraseCheckView }) {
  if (check.state === "waiting") return <Spinner />
  return (
    <p role={check.state === "matched" ? "status" : "alert"} className={check.state === "matched" ? "text-sm" : "text-sm text-destructive"}>
      {check.words}
    </p>
  )
}

/**
 * "Check with a phrase" (docs/plan/22-security-depth.md §9): this app shows
 * nine digits to say to the other person; they type them into theirs; the
 * two apps prove to each other, through the server but out of its reach,
 * that each holds the other's real keys. A match verifies them here.
 */
export function PhraseInvite({ login, onVerified }: { login: string; onVerified: () => void }) {
  const depth = useDepthClient()
  const [error, setError] = useState<string | null>(null)
  const { check, setCheck, stop } = useCheck(
    useCallback((v: PhraseCheckView) => v.state === "matched" && onVerified(), [onVerified]),
  )
  if (!depth) return null

  const start = async () => {
    setError(null)
    try {
      setCheck(await depth.phraseInvite(login))
    } catch (e) {
      setError(describeError(e))
    }
  }
  const close = () => {
    stop()
    if (check) void depth.phraseCancel(check.verify_id).catch(() => undefined)
    setCheck(null)
  }

  return (
    <>
      <Button variant="outline" size="sm" onClick={() => void start()}>
        Check with a phrase
      </Button>
      {error && (
        <span role="alert" className="text-sm text-destructive">
          {error}
        </span>
      )}
      <Dialog open={check !== null} onOpenChange={(o) => !o && close()}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Tell {login} this phrase</DialogTitle>
            <DialogDescription>
              Say it in person or on a call you trust, never in a message. They open Sharing, find your check, and type it. It works once.
            </DialogDescription>
          </DialogHeader>
          {check?.phrase && (
            <p className="text-center font-mono text-4xl tracking-[0.25em]" aria-label="Phrase">
              {check.phrase}
            </p>
          )}
          {check && <CheckState check={check} />}
          <LabelledActions>
            <LabelledAction kind={check?.state === "waiting" ? "cancel" : "confirm"} onClick={close}>
              {check?.state === "waiting" ? "Cancel" : "Done"}
            </LabelledAction>
          </LabelledActions>
        </DialogContent>
      </Dialog>
    </>
  )
}

/** The checks others opened with this person, each answered with the phrase they said. */
export function PhraseIncomingList({ online, onVerified }: { online: boolean; onVerified: () => void }) {
  const depth = useDepthClient()
  const [incoming, setIncoming] = useState<PhraseIncoming[]>([])
  const [answering, setAnswering] = useState<PhraseIncoming | null>(null)
  const [error, setError] = useState<string | null>(null)
  const { check, setCheck, stop } = useCheck(
    useCallback((v: PhraseCheckView) => v.state === "matched" && onVerified(), [onVerified]),
  )

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
    setAnswering(null)
    setCheck(null)
    setError(null)
    setIncoming((all) => all.filter((i) => i.verify_id !== answering?.verify_id))
  }

  return (
    <div className="flex flex-col gap-2" aria-label="Key checks for you">
      {incoming.map((i) => (
        <p key={i.verify_id} className="flex items-center justify-between gap-2 text-sm">
          <span>{i.login} wants to check keys with you.</span>
          <Button size="sm" variant="outline" onClick={() => setAnswering(i)}>
            Type their phrase
          </Button>
        </p>
      ))}
      <Dialog open={answering !== null} onOpenChange={(o) => !o && close()}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Check keys with {answering?.login}</DialogTitle>
            <DialogDescription>Type the nine digits they told you. You get one try; if it's wrong, they start a new check.</DialogDescription>
          </DialogHeader>
          {check ? (
            <CheckState check={check} />
          ) : (
            <form id="phrase-answer" onSubmit={(e) => void submit(e)}>
              <Field>
                <FieldLabel htmlFor="phrase-input">Their phrase</FieldLabel>
                <Input id="phrase-input" name="phrase" inputMode="numeric" autoComplete="off" />
                <FieldDescription>Spaces and dashes don't matter.</FieldDescription>
              </Field>
            </form>
          )}
          {error && <FieldError role="alert">{error}</FieldError>}
          <LabelledActions>
            <LabelledAction kind="cancel" onClick={close}>
              {check && check.state !== "waiting" ? "Done" : "Cancel"}
            </LabelledAction>
            {!check && (
              <LabelledAction kind="confirm" type="submit" form="phrase-answer">
                Check
              </LabelledAction>
            )}
          </LabelledActions>
        </DialogContent>
      </Dialog>
    </div>
  )
}
