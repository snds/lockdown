import { useState, type FormEvent } from "react"

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
  Input,
  Item,
  ItemActions,
  ItemContent,
  ItemDescription,
  ItemGroup,
  ItemTitle,
} from "@unicron/ui"

import { describeError } from "@/core/client"
import { PhraseIncomingList, PhraseInvite } from "@/features/depth/key-check"

import { useSharingClient, type ContactView } from "./sharing-client"

interface PeopleSectionProps {
  contacts: ContactView[]
  online: boolean
  onChanged: () => void
  onNotice: (text: string) => void
}

/**
 * The people this device has pinned (docs/plan/18-sharing.md §3). Looking
 * someone up checks their keys against the server's key-transparency log
 * before anything is pinned; comparing fingerprints with them (read out,
 * or typed from their screen) marks them verified. A pinned person whose
 * keys later change is refused until they are forgotten and looked up
 * again, so a swapped key never slips in.
 */
export function PeopleSection({ contacts, online, onChanged, onNotice }: PeopleSectionProps) {
  const sharing = useSharingClient()
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const [comparing, setComparing] = useState<ContactView | null>(null)
  const [forgetting, setForgetting] = useState<ContactView | null>(null)

  const lookup = async (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    const form = e.currentTarget
    const login = String(new FormData(form).get("login") ?? "").trim()
    if (!login) return
    setError(null)
    setBusy(true)
    try {
      const c = await sharing.lookup(login)
      form.reset()
      onNotice(`${c.login}'s keys checked out against the log. Compare fingerprints with them to be sure it's them.`)
      onChanged()
      setComparing(c)
    } catch (err) {
      setError(describeError(err))
    } finally {
      setBusy(false)
    }
  }
  const forget = async (c: ContactView) => {
    setForgetting(null)
    try {
      await sharing.forget(c.account_id)
      onNotice(`Forgot ${c.login} on this device.`)
      onChanged()
    } catch (e) {
      setError(describeError(e))
    }
  }

  return (
    <section aria-labelledby="sharing-people" className="flex flex-col gap-2">
      <h2 id="sharing-people" className="text-base font-semibold">
        People
      </h2>
      <form className="flex flex-wrap items-end gap-2" onSubmit={(e) => void lookup(e)}>
        <Field>
          <FieldLabel htmlFor="lookup-login">Look someone up by their sign-in</FieldLabel>
          <Input id="lookup-login" name="login" type="email" autoComplete="off" disabled={!online} placeholder="name@example.com" />
        </Field>
        <Button type="submit" variant="outline" size="sm" disabled={!online || busy}>
          <Icon name="person_search" data-icon="inline-start" />
          Look up
        </Button>
      </form>
      {error && <FieldError role="alert">{error}</FieldError>}
      {contacts.length === 0 ? (
        <p className="text-sm text-muted-foreground">Nobody yet. Look someone up to share with them or name them an emergency contact.</p>
      ) : (
        <ItemGroup role="list" aria-label="People">
          {contacts.map((c) => (
            <Item key={c.account_id} variant="outline" size="sm" role="listitem">
              <ItemContent>
                <ItemTitle>
                  {c.login}
                  {c.verified ? (
                    <Badge variant="secondary">
                      <Icon name="verified" data-icon="inline-start" />
                      Fingerprint compared
                    </Badge>
                  ) : (
                    <Badge variant="outline">Not compared yet</Badge>
                  )}
                </ItemTitle>
                <ItemDescription>
                  <code>{c.fingerprint}</code> · {c.devices === 1 ? "1 device" : `${c.devices} devices`}
                </ItemDescription>
              </ItemContent>
              <ItemActions>
                {!c.verified && (
                  <Button variant="outline" size="sm" onClick={() => setComparing(c)}>
                    Compare
                  </Button>
                )}
                {!c.verified && online && <PhraseInvite login={c.login} onVerified={onChanged} />}
                <Button variant="ghost-destructive" size="sm" onClick={() => setForgetting(c)}>
                  Forget
                </Button>
              </ItemActions>
            </Item>
          ))}
        </ItemGroup>
      )}
      <PhraseIncomingList online={online} onVerified={onChanged} />
      <CompareDialog
        contact={comparing}
        onClose={() => setComparing(null)}
        onVerified={(c) => {
          setComparing(null)
          onNotice(`The fingerprints match: ${c.login} is verified.`)
          onChanged()
        }}
      />
      <Dialog open={forgetting !== null} onOpenChange={(o) => !o && setForgetting(null)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Forget {forgetting?.login}?</DialogTitle>
            <DialogDescription>
              Their pinned keys are dropped from this device. What you already share with them stays shared; remove them from a vault to stop
              that. Looking them up again pins whatever keys the log has then.
            </DialogDescription>
          </DialogHeader>
          <DialogActions>
            <DialogAction kind="cancel" onClick={() => setForgetting(null)}>
              Cancel
            </DialogAction>
            <DialogAction kind="destructive" onClick={() => forgetting && void forget(forgetting)}>
              Forget
            </DialogAction>
          </DialogActions>
        </DialogContent>
      </Dialog>
    </section>
  )
}

/** Compares the fingerprint the other person reads out (or shows) with the one pinned here. */
function CompareDialog({ contact, onClose, onVerified }: { contact: ContactView | null; onClose: () => void; onVerified: (c: ContactView) => void }) {
  const sharing = useSharingClient()
  const [error, setError] = useState<string | null>(null)
  const submit = async (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    if (!contact) return
    const typed = String(new FormData(e.currentTarget).get("fingerprint") ?? "")
    setError(null)
    try {
      onVerified(await sharing.verify(contact.account_id, typed))
    } catch (err) {
      setError(describeError(err))
    }
  }
  return (
    <Dialog
      open={contact !== null}
      onOpenChange={(o) => {
        if (!o) {
          setError(null)
          onClose()
        }
      }}
    >
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Compare fingerprints with {contact?.login}</DialogTitle>
          <DialogDescription>
            Ask them to open Sharing and read out their fingerprint, in person or on a call you trust. Type what they read; it has to match
            the one their keys give here:
          </DialogDescription>
        </DialogHeader>
        <p className="text-center font-mono text-lg tracking-wide">{contact?.fingerprint}</p>
        <form id="compare-fingerprint" onSubmit={(e) => void submit(e)}>
          <Field>
            <FieldLabel htmlFor="compare-input">What they read out</FieldLabel>
            <Input id="compare-input" name="fingerprint" inputMode="numeric" autoComplete="off" />
            <FieldDescription>Spaces don't matter. If it doesn't match, don't share with this account.</FieldDescription>
          </Field>
        </form>
        {error && <FieldError role="alert">{error}</FieldError>}
        <DialogActions>
          <DialogAction kind="cancel" onClick={onClose}>
            Later
          </DialogAction>
          <DialogAction kind="confirm" type="submit" form="compare-fingerprint">
            They match
          </DialogAction>
        </DialogActions>
      </DialogContent>
    </Dialog>
  )
}
