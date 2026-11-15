import { useCallback, useEffect, useState, type FormEvent } from "react"

import { Badge, Banner, Button, FieldRow, FieldRowValue, ListRow, SectionHeader, TextField } from "@unicron/ui"

import { describeError } from "@/core/client"
import { SaveError, useGroupLevel, useRowSize } from "@/features/settings/setting-rows"
import { count } from "@/lib/format"

import { useServerClient, type MirrorCheck, type MirrorsView, type RestoreDrillView, type ServerClient } from "./server-client"
import { mirrorLine, splits } from "./server-words"

/**
 * The critical Banner a split view earns (docs/plan/27-web-server-finish.md
 * §4; brief T4, T5): a mirror holds a head this server signed that its log
 * can't be connected to, so the server shows different people different
 * histories. D328's whole sentence, as changed by D375: nothing in the
 * vault was changed by it, and what the core pauses until it clears is
 * named. Then each mirror that caught it, in its own words.
 */
export function SplitWarning({ checks }: { checks: readonly MirrorCheck[] | undefined | null }) {
  const found = splits(checks)
  if (found.length === 0) return null
  return (
    <Banner
      tone="critical"
      title={`This server's public log doesn't match ${found.length === 1 ? "a mirror" : `${found.length} mirrors`}`}
      body={
        <>
          <span className="block">
            The server signed one history for you and another for the world, and that is when it could swap someone's keys. Your vault is unchanged
            and keeps syncing, but until this clears Lockdown pauses new sharing through this server: looking people up, sharing vaults and items,
            share and request links, naming an emergency contact and opening emergency access. Tell whoever runs the server, and check your devices
            and the people you share with.
          </span>
          {found.map((c) => (
            <span key={c.url} className="block break-words pt-2">
              {`${c.url}: ${mirrorLine(c)}`}
            </span>
          ))}
        </>
      }
    />
  )
}

interface ServerChecksProps {
  onNotice: (text: string) => void
  /** @deprecated Both hosts draw it plain now, as a group of the Server section (brief T5). Kept so callers compile. */
  framed?: boolean
}

/**
 * THIS SERVER (brief T5, doc 82), in Settings → Server on the desktop and
 * the web vault's only Server part: the server's last restore drill, read
 * from the server and never made up here, as a Field Row with a Badge;
 * LOG MIRRORS NN, each mirror the syncs compare the server's log against,
 * with what it said; and "Your own mirrors" with SAVE MIRRORS.
 */
export function ServerChecks({ onNotice }: ServerChecksProps) {
  const server = useServerClient()
  return server ? <Checks server={server} onNotice={onNotice} /> : null
}

/** RESTORE DRILL's badge: the server's own word, said once (brief T5). */
export function drillBadge(drill: RestoreDrillView): { tone: "success" | "critical" | "neutral"; word: string } {
  if (!drill.reported) return { tone: "neutral", word: "Not reported" }
  if (drill.last_passed === false) return { tone: "critical", word: "Failed" }
  if (drill.last_success_at) return { tone: "success", word: "Passed" }
  return { tone: "neutral", word: "Not reported" }
}

function Checks({ server, onNotice }: ServerChecksProps & { server: ServerClient }) {
  const level = useGroupLevel()
  const size = useRowSize()
  const [drill, setDrill] = useState<RestoreDrillView | null>(null)
  const [drillError, setDrillError] = useState<string | null>(null)
  const [mirrors, setMirrors] = useState<MirrorsView | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const [typed, setTyped] = useState<string | null>(null)

  const load = useCallback(() => {
    server
      .restoreDrill()
      .then((d) => {
        setDrill(d)
        setDrillError(null)
      })
      .catch((e) => setDrillError(describeError(e)))
    server
      .ktMirrors()
      .then(setMirrors)
      .catch((e) => setError(describeError(e)))
  }, [server])
  useEffect(load, [load])

  const configured = (mirrors?.configured ?? []).join("\n")
  const save = async (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    const text = typed ?? configured
    const urls = text
      .split(/\s+/)
      .map((u) => u.trim())
      .filter(Boolean)
    setBusy(true)
    setError(null)
    try {
      setMirrors(await server.setKtMirrors(urls))
      setTyped(null)
      onNotice(urls.length ? "Mirrors saved. The next sync compares against them" : "Your mirrors were removed")
    } catch (err) {
      setError(describeError(err))
    } finally {
      setBusy(false)
    }
  }

  const badge = drill ? drillBadge(drill) : null
  const checks = mirrors?.checks ?? []
  return (
    <section aria-labelledby="server-checks-title" className="flex flex-col gap-8">
      <div className="flex flex-col">
        <SectionHeader id="server-checks-title" level={level} title="This server" />
        <FieldRow
          size={size}
          label="Restore drill"
          badge={badge ? <Badge tone={badge.tone}>{badge.word}</Badge> : undefined}
          description="As the server reports it. A drill proves its backups would bring it back. Your vault never depends on it."
        >
          <FieldRowValue truncate="wrap">{drill ? drill.words : (drillError ?? "Asking the server…")}</FieldRowValue>
        </FieldRow>
      </div>

      <div className="flex flex-col">
        <SectionHeader level={level} title="Log mirrors" count={mirrors ? count(checks.length) : undefined} />
        <p className="type-caption text-text-secondary pt-3 pb-3">
          Each sync compares this server's public log with copies kept elsewhere. A server that showed you a different history from everyone else is
          caught this way.
        </p>
        <SplitWarning checks={checks} />
        {checks.length > 0 ? (
          <ul role="list" aria-label="Log mirrors" className="flex flex-col">
            {checks.map((c) => (
              <ListRow
                key={c.url}
                render={<li />}
                size="compact"
                title={c.url}
                meta={c.named_by === "server" ? "Named by the server" : "Added by you"}
                detail={mirrorLine(c)}
                badge={<Badge tone={c.state === "agrees" ? "success" : c.state === "split" ? "critical" : "warning"}>{c.state === "agrees" ? "Agrees" : c.state === "split" ? "Split" : "Not checked"}</Badge>}
              />
            ))}
          </ul>
        ) : mirrors ? (
          <p className="type-body text-text-secondary">No mirror has been compared yet. The server names none, and you've added none.</p>
        ) : null}
        <form onSubmit={(e) => void save(e)} className="flex flex-col gap-3 pt-6" aria-label="Mirrors you add">
          <TextField
            id="kt-mirrors"
            name="mirrors"
            label="Your own mirrors"
            multiline
            minRows={2}
            placeholder="https://mirror.example"
            value={typed ?? configured}
            onChange={(e) => setTyped(e.target.value)}
            autoComplete="off"
            spellCheck={false}
            helper="One address per line, up to eight. Mirrors are public: nothing of yours goes to them."
          />
          <SaveError text={error} />
          <div>
            <Button type="submit" size="sm" variant="secondary" loading={busy} disabled={!mirrors}>
              Save mirrors
            </Button>
          </div>
        </form>
      </div>
    </section>
  )
}
