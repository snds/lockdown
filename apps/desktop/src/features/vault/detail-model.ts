import type { FieldTemplateView, FieldView, TemplateView } from "@/core/types"

const isCustom = (f: FieldView) => f.field_id.startsWith("custom.")

/**
 * The fields under their template's section headings, in the template's
 * order; then the fields outside it ("Other fields"), then those added by
 * hand ("Your fields"); notes last, as in the editor.
 */
export function sectionsOf(fields: FieldView[], template: TemplateView | null): { name: string; fields: FieldView[] }[] {
  const out: { name: string; fields: FieldView[] }[] = []
  const push = (name: string, f: FieldView) => {
    const s = out.find((x) => x.name === name)
    if (s) s.fields.push(f)
    else out.push({ name, fields: [f] })
  }
  const known = (f: FieldView): FieldTemplateView | undefined => template?.fields.find((t) => t.field_id === f.field_id)
  const ordered = [...fields].sort((a, b) => {
    const i = (f: FieldView) => {
      const at = template?.fields.findIndex((t) => t.field_id === f.field_id) ?? -1
      return at < 0 ? Number.MAX_SAFE_INTEGER : at
    }
    return i(a) - i(b)
  })
  for (const f of ordered.filter((f) => known(f)?.section !== "Notes")) {
    const t = known(f)
    push(t ? t.section : isCustom(f) ? "Your fields" : "Other fields", f)
  }
  for (const f of ordered.filter((f) => known(f)?.section === "Notes")) push("Notes", f)
  return out
}
