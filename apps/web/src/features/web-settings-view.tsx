import { useSyncExternalStore } from "react"

import { Button, FieldRow, SectionHeader, toast } from "@unicron/ui"

import { usePlaces } from "@/app/places"
import { Embedded } from "@/components/master-detail"
import type { LockState, Status } from "@/core/types"
import { DevicesView } from "@/features/devices/devices-view"
import { ServerChecks } from "@/features/server/server-checks"
import { DensitySetting } from "@/features/settings/density-setting"
import { SaveError, useRowSize, useSettingSave } from "@/features/settings/setting-rows"
import { SettingsPlaces } from "@/features/settings/settings-places"
import { ThemeSetting } from "@/features/settings/theme-setting"
import { SecurityRows } from "@/features/settings/this-device"

import { setThemeChoice, subscribeTheme, themeChoice } from "../theme"

interface WebSettingsViewProps {
  /** For the Devices section: every session signed in to the account. */
  status: Status
  onChanged: () => void
  onNotice: (text: string) => void
  server: string
  login: string
  onLock: (state: LockState) => void
  onSignOut: () => void
}

/** A server address as a row shows it: its host. */
function hostOf(address: string): string {
  try {
    return new URL(address).host
  } catch {
    return address
  }
}

/** THIS WEB VAULT (brief T1, W1): who this tab is signed in as, the trust note (D180), and the way out. */
function ThisWebVault({ server, login, onSignOut }: { server: string; login: string; onSignOut: () => void }) {
  const size = useRowSize()
  return (
    <div className="flex flex-col">
      <SectionHeader level={3} title="This web vault" />
      <FieldRow size={size} label="Signed in as" description={hostOf(server)}>
        {login}
      </FieldRow>
      <div className="flex flex-col gap-4 pt-3">
        <p className="type-caption text-text-secondary">
          The page's code comes from that server too, so it can only be as trustworthy as the server that sends it; the desktop app's code comes signed
          from Lockdown. For crown jewels and recovery, prefer the desktop app. This tab is one of your devices: signing out forgets its key, and another
          device can then remove it from the list.
        </p>
        <div>
          <Button variant="secondary" onClick={onSignOut}>
            Sign out of this tab
          </Button>
        </div>
      </div>
    </div>
  )
}

/**
 * The web vault's settings, in four sections (brief T0, T1 and W1: This
 * browser, Appearance, Devices, Server): the lock timers (the core's, as on
 * the desktop), the clipboard timer and the reveal gesture, then this tab's
 * place: where it signed in and how to sign out. They hold for this tab; a
 * new sign-in starts from the defaults. Auto-lock has no Never here: a
 * browser tab is too easy to walk away from.
 */
export function WebSettingsView({ status, onChanged, onNotice, server, login, onLock, onSignOut }: WebSettingsViewProps) {
  const { nav } = usePlaces()
  const { settings, save, busy, errorOf } = useSettingSave()
  // The ground is a choice kept in this browser (src/theme.ts), not a setting of the vault (doc 62 §3).
  const theme = useSyncExternalStore(subscribeTheme, themeChoice)

  const thisBrowser = (
    <div className="flex flex-col gap-8">
      <SecurityRows surface="web" onLock={onLock} />
      <ThisWebVault server={server} login={login} onSignOut={onSignOut} />
    </div>
  )
  return (
    <SettingsPlaces overview={thisBrowser}>
      {(shown) => (
        <>
          {shown === "this-device" && thisBrowser}
          {shown === "appearance" && (
            <div className="flex flex-col">
              <ThemeSetting value={theme} onChange={setThemeChoice} />
              <DensitySetting value={settings.density} onChange={(density) => void save("density", { density })} busy={busy === "density"} />
              <SaveError text={errorOf("density")} />
            </div>
          )}
          {shown === "devices" && (
            <Embedded>
              <DevicesView status={status} onChanged={onChanged} onNotice={onNotice} focus={nav.device} />
            </Embedded>
          )}
          {shown === "server" && <ServerChecks onNotice={(text) => toast(text)} framed={false} />}
        </>
      )}
    </SettingsPlaces>
  )
}
