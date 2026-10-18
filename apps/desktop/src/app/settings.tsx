import { createContext, useCallback, useContext, useEffect, useLayoutEffect, useMemo, useState, type ReactNode } from "react"

import { describeError, useClient } from "@/core/client"
import type { Settings } from "@/core/types"

/** The defaults, the same as `Settings::default()` on the Rust side. */
export const DEFAULT_SETTINGS: Settings = {
  v: 1,
  density: "comfortable",
  reveal: "hold",
  clipboard_clear_secs: 30,
  idle_lock_secs: 600,
  hidden_lock_secs: 300,
  server: null,
  breach_check: true,
  breach_endpoint: null,
  change_password_check: true,
  backup: { enabled: false, folder: null, interval_hours: 24, keep_last: 10 },
  browser_passkeys: true,
}

interface SettingsContextValue {
  settings: Settings
  /** Whether the file has been read yet (before that, the defaults show). */
  loaded: boolean
  error: string | null
  /** Writes the change through the core (`settings.cbor`) and applies it. */
  update: (change: Partial<Settings>) => Promise<Settings>
}

const SettingsContext = createContext<SettingsContextValue>({
  settings: DEFAULT_SETTINGS,
  loaded: false,
  error: null,
  update: () => Promise.resolve(DEFAULT_SETTINGS),
})

/** Where the density lands (D92): `<html data-density="comfortable|compact">`. */
export function applyDensity(density: Settings["density"]) {
  document.documentElement.dataset.density = density
}

/**
 * The settings file as the UI sees it (D100). Read once at start, before
 * the vault opens (nothing in it needs the vault), and written through the
 * core so the CLI reads the same file. Density becomes a `data-density`
 * attribute on the root, which `app-extension.css` turns into row sizes.
 */
export function SettingsProvider({ children }: { children: ReactNode }) {
  const client = useClient()
  const [settings, setSettings] = useState<Settings>(DEFAULT_SETTINGS)
  const [loaded, setLoaded] = useState(false)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    let live = true
    client
      .getSettings()
      .then((s) => {
        if (!live) return
        setSettings(s)
        setLoaded(true)
      })
      .catch((e) => live && setError(describeError(e)))
    return () => {
      live = false
    }
  }, [client])

  // Before paint, in the same commit that shows the new settings: a passive
  // effect could let one frame draw the old row height after the file is read.
  useLayoutEffect(() => {
    applyDensity(settings.density)
  }, [settings.density])

  const update = useCallback(
    async (change: Partial<Settings>) => {
      const next = { ...settings, ...change }
      const saved = await client.setSettings(next)
      setSettings(saved)
      return saved
    },
    [client, settings],
  )

  const value = useMemo(() => ({ settings, loaded, error, update }), [settings, loaded, error, update])
  return <SettingsContext.Provider value={value}>{children}</SettingsContext.Provider>
}

export function useSettings(): SettingsContextValue {
  return useContext(SettingsContext)
}
