import { createContext, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from "react"

import { describeError, useClient } from "@/core/client"
import type { FieldTemplateView, TemplateView, Templates, UserTemplateView } from "@/core/types"

import { userTemplateView } from "./template-model"

interface TemplatesValue {
  /** `null` while loading. */
  templates: Templates | null
  error: string | null
  /** One kind's own template, once loaded. */
  forKind: (kind: string) => TemplateView | null
  /**
   * One template by id (D231, D232): a kind (`identity`), a variant
   * (`identity/passport`) or the person's own (`user:<id>`); a variant this
   * build doesn't know, or a template since deleted, falls back to its kind's.
   */
  forTemplate: (id: string, kind?: string) => TemplateView | null
  /** One field's template, or `null` for a field outside it (an import's extra, or one added by hand). */
  fieldOf: (kind: string, fieldId: string) => FieldTemplateView | null
  /** The person's own templates, by name. */
  userTemplates: UserTemplateView[]
  /** Loads them again (after a save or a delete). */
  refreshUserTemplates: () => void
}

const TemplatesContext = createContext<TemplatesValue>({
  templates: null,
  error: null,
  forKind: () => null,
  forTemplate: () => null,
  fieldOf: () => null,
  userTemplates: [],
  refreshUserTemplates: () => {},
})

/**
 * The core's per-kind templates (D110) and their variants (D231), loaded
 * once, and the person's own (D232), loaded when the vault is open: the
 * "New item" list, the editor and the detail are all drawn from them, so
 * a kind looks the same everywhere and the core checks what the editor
 * sends against the same table.
 */
export function TemplatesProvider({ children }: { children: ReactNode }) {
  const client = useClient()
  const [templates, setTemplates] = useState<Templates | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [userTemplates, setUserTemplates] = useState<UserTemplateView[]>([])
  const [round, setRound] = useState(0)

  useEffect(() => {
    let live = true
    client
      .templates()
      .then((t) => live && setTemplates(t))
      .catch((e) => live && setError(describeError(e)))
    return () => {
      live = false
    }
  }, [client])

  useEffect(() => {
    let live = true
    // A shell without them (an older core, a test) has none; nothing to show.
    Promise.resolve()
      .then(() => client.userTemplates())
      .then((u) => live && setUserTemplates(Array.isArray(u) ? u : []))
      .catch(() => live && setUserTemplates([]))
    return () => {
      live = false
    }
  }, [client, round])

  const refreshUserTemplates = useCallback(() => setRound((n) => n + 1), [])

  const value = useMemo<TemplatesValue>(() => {
    const kinds = templates?.kinds ?? []
    const byKind = new Map(kinds.filter((t) => !t.variant).map((t) => [t.kind, t]))
    const byId = new Map(kinds.map((t) => [t.id ?? t.kind, t]))
    for (const u of userTemplates) byId.set(`user:${u.id}`, userTemplateView(u))
    return {
      templates,
      error,
      forKind: (kind) => byKind.get(kind) ?? null,
      forTemplate: (id, kind) => byId.get(id) ?? byKind.get(kind ?? id.split("/")[0] ?? id) ?? null,
      fieldOf: (kind, fieldId) => byKind.get(kind)?.fields.find((f) => f.field_id === fieldId) ?? null,
      userTemplates,
      refreshUserTemplates,
    }
  }, [templates, error, userTemplates, refreshUserTemplates])

  return <TemplatesContext.Provider value={value}>{children}</TemplatesContext.Provider>
}

export function useTemplates(): TemplatesValue {
  return useContext(TemplatesContext)
}
