import { encodeSecret } from "@/core/secret-bytes"
import type { Control, FieldChangeInput, FieldTemplateView, ItemDetail, Recipe, TemplateView } from "@/core/types"

/** What happens to a secret on save. `keep` sends no bytes at all. */
export type SecretMode = "keep" | "type" | "generate" | "remove"

/** One row of the editor: a template field, a field the item already has, or one the person is adding. */
export interface EditorRow {
  /** Stable React key: the field id, or `new:<n>`. */
  key: string
  /** `null` for a field the person is adding by hand. */
  fieldId: string | null
  label: string
  control: Control
  template: FieldTemplateView | null
  /** On the item now (an edit). */
  exists: boolean
  /** A field added by hand: its `CustomKindView.kind`. */
  customKind: string | null
  /** False for material that can only be kept or removed (passkeys, links, files). */
  editable: boolean
  /** The plain value as it was, to tell an edit from no change. */
  original: string
  /** The plain value, or a secret as typed (a webview string until save, D91). */
  text: string
  flag: boolean
  /** For a secret control. */
  secret: SecretMode
  /** A plain field the person took off. */
  removed: boolean
  /** A web address's match rule, and the one it had (an edit). */
  matchRule?: string
  originalRule?: string
  /** A passkey, kept as it is: deleted from the item's Passkeys, which warns first (docs/plan/19-passkeys.md). */
  managed?: boolean
  /** Kept and sent as `keep`, never drawn (a passkey's other parts). */
  quiet?: boolean
  /** With `generate`: the person's options (D230); the template's rule when absent. */
  recipe?: Recipe
}

const SECRET_CONTROLS: readonly Control[] = ["secret", "secret-multiline", "totp", "passkey"]

export const isSecretControl = (c: Control) => SECRET_CONTROLS.includes(c)

/** The control for a field outside the template, from its stored kind (`FieldView.kind`). */
export function controlForKind(kind: string): { control: Control; editable: boolean } {
  switch (kind) {
    case "hidden":
      return { control: "secret", editable: true }
    case "url":
    case "email":
    case "phone":
    case "date":
    case "boolean":
    case "totp":
    case "linked":
      return { control: kind as Control, editable: true }
    case "text":
    case "address":
      return { control: "text", editable: true }
    default:
      return { control: "passkey", editable: false }
  }
}

function row(partial: Partial<EditorRow> & Pick<EditorRow, "key" | "fieldId" | "label" | "control">): EditorRow {
  return {
    template: null,
    exists: false,
    customKind: null,
    editable: true,
    original: "",
    text: "",
    flag: false,
    secret: "type",
    removed: false,
    ...partial,
  }
}

/**
 * The editor's rows: the template's fields in its order (a creatable
 * item never offers passkey material), then every other field the item
 * holds (an import's second website, passkey material, fields added by
 * hand) so the edit can name each one.
 */
export function initialRows(template: TemplateView, item: ItemDetail | null): EditorRow[] {
  const has = (id: string) => item?.fields.find((f) => f.field_id === id) ?? null
  const rows: EditorRow[] = []
  for (const t of template.fields) {
    const f = has(t.field_id)
    if (!f && t.control === "passkey") continue
    const secret = isSecretControl(t.control)
    rows.push(
      row({
        key: t.field_id,
        fieldId: t.field_id,
        label: t.label,
        control: t.control,
        template: t,
        exists: f !== null,
        editable: t.control !== "passkey",
        original: secret ? "" : (f?.value ?? ""),
        text: secret ? "" : (f?.value ?? ""),
        flag: f?.value === "true",
        secret: f ? "keep" : "type",
        // A field of the person's own template (D232) names its kind when it is added to an item.
        customKind: t.field_id.startsWith("tpl.") ? kindOfControl(t.control) : null,
        ...(t.control === "url" ? { matchRule: f?.match_rule ?? "exact_host", originalRule: f?.match_rule ?? "exact_host" } : {}),
      }),
    )
  }
  for (const f of item?.fields ?? []) {
    if (f.field_id === "title" || template.fields.some((t) => t.field_id === f.field_id)) continue
    // A passkey is one group of fields: one line for it, the rest kept quietly.
    const pk = /^passkey\.(\d+)\.(.+)$/.exec(f.field_id)
    if (pk) {
      const rp = item?.fields.find((x) => x.field_id === `passkey.${pk[1]}.rp_id`)?.value ?? ""
      rows.push(
        row({
          key: f.field_id,
          fieldId: f.field_id,
          label: pk[2] === "key" ? `Passkey${rp ? ` for ${rp}` : ""}` : f.label,
          control: "passkey",
          exists: true,
          editable: false,
          secret: "keep",
          managed: pk[2] === "key",
          quiet: pk[2] !== "key",
        }),
      )
      continue
    }
    // A later copy of a repeating field (a Login's second website) takes the first's template.
    const repeat = template.fields.find((t) => t.repeats && f.field_id.startsWith(t.field_id.replace(/\.0$/, ".")))
    if (repeat) {
      rows.push(
        row({
          key: f.field_id,
          fieldId: f.field_id,
          label: repeat.label,
          control: repeat.control,
          template: repeat,
          exists: true,
          original: f.value ?? "",
          text: f.value ?? "",
          secret: "keep",
          matchRule: f.match_rule ?? "exact_host",
          originalRule: f.match_rule ?? "exact_host",
        }),
      )
      continue
    }
    const { control, editable } = controlForKind(f.kind)
    const secret = isSecretControl(control)
    rows.push(
      row({
        key: f.field_id,
        fieldId: f.field_id,
        label: f.label,
        control,
        exists: true,
        editable,
        original: secret ? "" : (f.value ?? ""),
        text: secret ? "" : (f.value ?? ""),
        flag: f.value === "true",
        secret: "keep",
      }),
    )
  }
  return rows
}

/** Another copy of a repeating field (a Login's next website), numbered after the ones there. */
export function repeatRow(rows: EditorRow[], template: FieldTemplateView): EditorRow {
  const stem = template.field_id.replace(/\.0$/, ".")
  const used = rows.map((r) => (r.fieldId?.startsWith(stem) ? Number(r.fieldId.slice(stem.length)) : -1)).filter((n) => Number.isInteger(n) && n >= 0)
  const n = used.length ? Math.max(...used) + 1 : 0
  return row({
    key: `${stem}${n}`,
    fieldId: `${stem}${n}`,
    label: template.label,
    control: template.control,
    template,
    ...(template.control === "url" ? { matchRule: "exact_host", originalRule: "exact_host" } : {}),
  })
}

/** The kind a field of your own stores, from its control (the inverse of `controlForKind`). */
export function kindOfControl(control: Control): string {
  switch (control) {
    case "secret":
    case "secret-multiline":
      return "hidden"
    case "url":
    case "email":
    case "phone":
    case "date":
    case "boolean":
    case "totp":
    case "linked":
      return control
    default:
      return "text"
  }
}

/** A field the person adds by hand ("Add field"). */
export function customRow(n: number, label: string, kind: string): EditorRow {
  const { control } = controlForKind(kind)
  return row({ key: `new:${n}`, fieldId: null, label, control, customKind: kind })
}

/**
 * What the editor sends. An edit names every field the item has (the core
 * refuses an edit that leaves one out): `keep` for a secret not retyped,
 * so its bytes never cross back; `remove` for one taken off.
 */
export function buildChanges(rows: EditorRow[]): FieldChangeInput[] {
  const out: FieldChangeInput[] = []
  for (const r of rows) {
    const id = r.fieldId ?? undefined
    // A field added by hand, or a template's own (`tpl.*`) the item lacks yet, says its label and kind.
    const custom = r.fieldId === null || (!r.exists && r.customKind) ? { label: r.label, kind: r.customKind ?? "text" } : {}
    const secret = isSecretControl(r.control)
    if (r.exists && (!r.editable || (!secret && r.removed))) {
      out.push({ field_id: id, action: r.removed ? "remove" : "keep" })
      continue
    }
    if (secret) {
      if (r.secret === "generate") out.push({ field_id: id, ...custom, action: "generate", ...(r.recipe ? { recipe: r.recipe } : {}) })
      else if (r.secret === "remove" && r.exists) out.push({ field_id: id, action: "remove" })
      else if (r.secret === "type" && r.text !== "") out.push({ field_id: id, ...custom, action: "set", secret: encodeSecret(r.text) })
      else if (r.exists) out.push({ field_id: id, action: "keep" })
      continue
    }
    if (r.control === "boolean") {
      if (r.exists || r.flag) out.push({ field_id: id, ...custom, action: "set", flag: r.flag })
      continue
    }
    const text = r.text.trim()
    const rule = r.control === "url" && r.matchRule ? { match_rule: r.matchRule } : {}
    const ruleChanged = r.control === "url" && r.matchRule !== r.originalRule
    if (r.exists) {
      if (text === r.original.trim() && !ruleChanged) out.push({ field_id: id, action: "keep" })
      else if (text === "") out.push({ field_id: id, action: "remove" })
      else out.push({ field_id: id, action: "set", text: r.text, ...rule })
    } else if (text !== "") {
      out.push({ field_id: id, ...custom, action: "set", text: r.text, ...rule })
    }
  }
  return out
}

/** Drops every typed secret from the rows (after a save, or on close). */
export function wipeRows(rows: EditorRow[]): EditorRow[] {
  return rows.map((r) => (isSecretControl(r.control) ? { ...r, text: "" } : r))
}

/** The kinds whose title the core works out when it is left empty. */
export const DERIVED_TITLE: Record<string, string> = {
  login: "Left empty, the website's name is used.",
  server: "Left empty, the address is used.",
  code: "Left empty, the code's issuer is used.",
  "wi-fi": "Left empty, the network name is used.",
}
