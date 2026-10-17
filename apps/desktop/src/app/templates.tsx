import { createContext, useContext, useEffect, useMemo, useState, type ReactNode } from "react"

import { describeError, useClient } from "@/core/client"
import type { FieldTemplateView, TemplateView, Templates } from "@/core/types"

interface TemplatesValue {
  /** `null` while loading. */
  templates: Templates | null
  error: string | null
  /** One kind's template, once loaded. */
  forKind: (kind: string) => TemplateView | null
  /** One field's template, or `null` for a field outside it (an import's extra, or one added by hand). */
  fieldOf: (kind: string, fieldId: string) => FieldTemplateView | null
}

const TemplatesContext = createContext<TemplatesValue>({
  templates: null,
  error: null,
  forKind: () => null,
  fieldOf: () => null,
})

/**
 * The core's per-kind templates (D110), loaded once: the "New item" list,
 * the editor and the detail are all drawn from them, so a kind looks the
 * same everywhere and the core checks what the editor sends against the
 * same table.
 */
export function TemplatesProvider({ children }: { children: ReactNode }) {
  const client = useClient()
  const [templates, setTemplates] = useState<Templates | null>(null)
  const [error, setError] = useState<string | null>(null)

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

  const value = useMemo<TemplatesValue>(() => {
    const byKind = new Map((templates?.kinds ?? []).map((t) => [t.kind, t]))
    return {
      templates,
      error,
      forKind: (kind) => byKind.get(kind) ?? null,
      fieldOf: (kind, fieldId) => byKind.get(kind)?.fields.find((f) => f.field_id === fieldId) ?? null,
    }
  }, [templates, error])

  return <TemplatesContext.Provider value={value}>{children}</TemplatesContext.Provider>
}

export function useTemplates(): TemplatesValue {
  return useContext(TemplatesContext)
}
