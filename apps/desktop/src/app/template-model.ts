import { iconFromMaterial, isIconName, type IconName } from "@unicron/ui"

import type { Control, FieldTemplateView, TemplateView, UserTemplateView } from "@/core/types"

/**
 * The icon a template names, as one unicron can draw. A template's icon
 * arrives as data: the core's templates (crates/vault/src/template.rs) and
 * a template a person saved name Material Symbols, which unicron drew until
 * 1.0. unicron's icons are IBM Carbon now, so the name is translated here,
 * once, where it is read (unicron docs/07-migrating-to-1.0.md §2.5):
 * Material first, because a few words are names in both sets with different
 * drawings; then a Carbon name as it is; then `fallback`, for a name neither
 * set knows.
 */
export function templateIcon(name: string | undefined, fallback: IconName): IconName {
  if (!name) return fallback
  return iconFromMaterial(name) ?? (isIconName(name) ? name : fallback)
}

/** The control a field of a person's template draws, from its kind (`CustomKindView.kind`). */
export function controlOfKind(kind: string): Control {
  switch (kind) {
    case "hidden":
      return "secret"
    case "url":
    case "email":
    case "phone":
    case "date":
    case "boolean":
    case "totp":
    case "linked":
      return kind
    default:
      return "text"
  }
}

const STRONG_WORDS = "20 characters: letters, digits and symbols"

/** A person's template as the editor and the detail draw it: a Custom item whose fields are `tpl.<key>`. */
export function userTemplateView(t: UserTemplateView): TemplateView {
  return {
    kind: "custom",
    id: `user:${t.id}`,
    variant: "",
    icon: t.icon || "category",
    fills: false,
    name: t.name,
    summary: t.summary || "One of your own templates",
    creatable: true,
    crown_jewel: false,
    autofill: false,
    primary: null,
    subtitle: null,
    fields: [
      ...t.fields.map((f): FieldTemplateView => {
        const control = controlOfKind(f.kind)
        return {
          field_id: f.field_id || `tpl.${f.key}`,
          label: f.label,
          control,
          secret: control === "secret" || control === "totp",
          required: f.required ?? false,
          crown_jewel: false,
          fill: "never",
          generator: control === "secret" ? { length: 20, symbols: true, unambiguous: false, words: STRONG_WORDS } : null,
          options: [],
          section: f.section || t.name,
          grouped: false,
          hint: f.hint ?? "",
        }
      }),
      { field_id: "notes", label: "Notes", control: "multiline", secret: false, required: false, crown_jewel: false, fill: "never", generator: null, options: [], section: "Notes", grouped: false, hint: "" },
    ],
  }
}

