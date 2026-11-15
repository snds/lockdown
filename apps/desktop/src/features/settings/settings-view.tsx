import { type FormEvent } from "react"

import { useTheme } from "next-themes"

import { Button, TextField } from "@unicron/ui"

import { usePlaces } from "@/app/places"
import { useStatusItem } from "@/app/status"
import { Embedded } from "@/components/master-detail"
import type { LockState, Status } from "@/core/types"
import { BackupsSection } from "@/features/backups/backups-section"
import { AliasSettings } from "@/features/bridges/alias-settings"
import { ClockSetting } from "@/features/bridges/clock"
import { ImportExportSection } from "@/features/import/import-export-section"
import { DevicesView } from "@/features/devices/devices-view"
import { RecoveryView } from "@/features/recovery/recovery-view"
import { SecuritySettings } from "@/features/security/security-settings"
import { StatusView } from "@/features/status/status-view"
import { BrowsersSection } from "@/features/browsers/browsers-section"
import { QuickSettings } from "@/features/quick/quick-settings"
import { ServerChecks } from "@/features/server/server-checks"
import { ServerSettings } from "@/features/server/server-settings"
import { useFirstRunClient } from "@/features/unlock/first-run-client"
import { AboutSection } from "@/features/updates/about-section"
import { updateMark } from "@/features/updates/restart-row"
import { useUpdates } from "@/features/updates/use-updates"

import { DensitySetting } from "./density-setting"
import { SaveError, useSettingSave } from "./setting-rows"
import { SettingsPlaces } from "./settings-places"
import { ThemeSetting, type ThemeChoice } from "./theme-setting"
import { ThisDevice } from "./this-device"

/** The Theme row's words for next-themes' own: Stone is its light, Graphite its dark. */
const THEME_OF: Record<ThemeChoice, string> = { system: "system", stone: "light", graphite: "dark" }

interface SettingsViewProps {
  status: Status
  /** The core's status changed (Status' own check, a log-in): the app keeps the new one. */
  onStatus: (status: Status) => void
  lock: LockState | null
  onLock: (state: LockState) => void
  onChanged: () => void
  onNotice: (text: string) => void
}

/**
 * Settings (brief T0, doc 79): the `SettingsFrame` of every section. This
 * Mac is built here (T1); the other sections keep their own components and
 * props, and their slices restyle them. Everything here is written to
 * `settings.cbor` through the core (D100), which also applies the lock
 * timers; this screen only edits them. Saves are silent; a failed one puts
 * the old value back with one critical line.
 */
export function SettingsView({ status, onStatus, lock: _lock, onLock, onChanged, onNotice }: SettingsViewProps) {
  const { nav, go } = usePlaces()
  const frc = useFirstRunClient()
  const updates = useUpdates()
  // The ground is a choice kept on this device by next-themes, not a setting of the vault (doc 62 §3).
  const { theme, setTheme } = useTheme()
  const { settings, save, busy, errorOf } = useSettingSave()

  // The status line's end on Settings: the version that runs (Desktop / 12's "Lockdown 1.8").
  useStatusItem(updates ? { text: `Lockdown // ${updates.version}` } : null)

  const thisDevice = <ThisDevice status={status} onLock={onLock} onChanged={onChanged} onNotice={onNotice} />

  const saveServer = (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    const value = String(new FormData(e.currentTarget).get("server") ?? "").trim()
    void save("server", { server: value || null })
  }

  return (
    <>
      <SettingsPlaces
        overview={thisDevice}
        marks={{
          recovery: status.recovery === "drilled" ? undefined : { square: true, label: status.recovery === "none" ? "no recovery kit yet" : "kit not tested" },
          about: updateMark(updates),
        }}
      >
        {(shown) => (
          <>
            {shown === "this-device" && thisDevice}
            {shown === "browsers" && <BrowsersSection onNotice={onNotice} />}
            {shown === "quick-access" && <QuickSettings onNotice={onNotice} />}
            {shown === "appearance" && (
              <div className="flex flex-col">
                <ThemeSetting value={theme === "light" ? "stone" : theme === "dark" ? "graphite" : "system"} onChange={(choice) => setTheme(THEME_OF[choice])} />
                <DensitySetting value={settings.density} onChange={(density) => void save("density", { density })} busy={busy === "density"} />
                <SaveError text={errorOf("density")} />
              </div>
            )}
            {shown === "devices" && (
              <Embedded>
                <DevicesView status={status} onChanged={onChanged} onNotice={onNotice} focus={nav.device} />
              </Embedded>
            )}
            {shown === "recovery" && (
              <Embedded>
                <RecoveryView status={status} onChanged={onChanged} onNotice={onNotice} onEmergency={() => go("sharing-emergency")} />
              </Embedded>
            )}
            {shown === "status" && <StatusView status={status} onStatus={onStatus} onNotice={onNotice} />}
            {shown === "server" && (
              <div className="flex flex-col gap-8">
                {frc ? (
                  <ServerSettings frc={frc} status={status} onNotice={onNotice} onChanged={onChanged} />
                ) : (
                  <form onSubmit={saveServer} className="flex flex-col gap-3">
                    <TextField
                      label="Server to suggest"
                      name="server"
                      type="url"
                      placeholder="https://"
                      defaultValue={settings.server ?? ""}
                      autoComplete="off"
                      helper="Filled in on the join, register and log-in forms. Your own server is a first-class choice."
                      error={errorOf("server") ?? undefined}
                    />
                    <div>
                      <Button type="submit" variant="secondary" size="sm" loading={busy === "server"}>
                        Save server
                      </Button>
                    </div>
                  </form>
                )}
                {!(status.account?.local ?? false) && <ServerChecks onNotice={onNotice} />}
              </div>
            )}
            {shown === "import-export" && <ImportExportSection onChanged={onChanged} onNotice={onNotice} onFindDuplicates={() => go("health")} />}
            {shown === "backups" && <BackupsSection onChanged={onChanged} onNotice={onNotice} />}
            {shown === "aliases" && <AliasSettings onNotice={onNotice} onChanged={onChanged} />}
            {shown === "codes" && <ClockSetting />}
            {shown === "security-checks" && <SecuritySettings onSaved={onChanged} />}
            {shown === "about" && <AboutSection status={status} />}
          </>
        )}
      </SettingsPlaces>
    </>
  )
}
