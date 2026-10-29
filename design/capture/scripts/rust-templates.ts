// Every item template exactly as the core defines it
// (crates/vault/src/template.rs), read from the Rust source at build time
// and shaped as the core's `templates` command answers
// (crates/app/src/session/items.rs `templates`). The harness never builds
// Rust; this reads the `const` tables, which are plain data.

import { readFileSync } from "node:fs"

type Token = { t: "str"; v: string } | { t: "id"; v: string } | { t: "p"; v: string }

function tokenize(src: string): Token[] {
  const out: Token[] = []
  let i = 0
  while (i < src.length) {
    const c = src[i]!
    if (/\s/.test(c)) {
      i++
    } else if (src.startsWith("//", i)) {
      while (i < src.length && src[i] !== "\n") i++
    } else if (src.startsWith("/*", i)) {
      i = src.indexOf("*/", i + 2) + 2
    } else if (c === '"') {
      let v = ""
      i++
      while (src[i] !== '"') {
        if (src[i] === "\\") {
          const n = src[i + 1]
          if (n === "\n") {
            // A line continuation: the newline and the next line's indent go.
            i += 2
            while (/\s/.test(src[i]!)) i++
            continue
          }
          v += n === "n" ? "\n" : n === "t" ? "\t" : n
          i += 2
        } else {
          v += src[i]
          i++
        }
      }
      i++
      out.push({ t: "str", v })
    } else if (c === "'") {
      // A char or a lifetime: neither carries template data.
      const m = /^'(\\.|[^'\\])'/.exec(src.slice(i))
      i += m ? m[0].length : 1
    } else if (/[A-Za-z_]/.test(c)) {
      const m = /^[A-Za-z_][A-Za-z0-9_]*(::[A-Za-z_][A-Za-z0-9_]*)*/.exec(src.slice(i))!
      out.push({ t: "id", v: m[0] })
      i += m[0].length
    } else if (src.startsWith("..", i)) {
      out.push({ t: "p", v: ".." })
      i += 2
    } else {
      out.push({ t: "p", v: c })
      i++
    }
  }
  return out
}

export interface RustField {
  field_id: string
  label: string
  control: string
  secret: boolean
  required: boolean
  crown_jewel: boolean
  fill: string
  generator: { length: number; symbols: boolean; unambiguous: boolean; words: string } | null
  options: string[]
  section: string
  grouped: boolean
  hint: string
  repeats: boolean
}

export interface RustTemplate {
  kind: string
  id: string
  variant: string
  icon: string
  fills: boolean
  name: string
  summary: string
  creatable: boolean
  crown_jewel: boolean
  autofill: boolean
  primary: string | null
  subtitle: string | null
  fields: RustField[]
}

const kebab = (name: string) => (name === "WiFi" ? "wi-fi" : name.replace(/([a-z0-9])([A-Z])/g, "$1-$2").toLowerCase())

const RULES: Record<string, RustField["generator"]> = {
  STRONG: { length: 20, symbols: true, unambiguous: false, words: "20 characters: letters, digits and symbols" },
  READABLE: { length: 20, symbols: false, unambiguous: true, words: "20 characters: letters and digits, none that look alike" },
}

/** Kinds never filled into a page (crates/vault/src/item.rs `autofill_allowed`). */
const NO_AUTOFILL = new Set(["ssh-key", "bank-account", "consent", "vault-info", "template"])

class Parser {
  private at = 0
  readonly strLists = new Map<string, string[]>()
  readonly fieldConsts = new Map<string, RustField>()
  readonly fieldLists = new Map<string, RustField[]>()
  private readonly toks: Token[]

  constructor(toks: Token[]) {
    this.toks = toks
  }

  private peek(n = 0): Token | undefined {
    return this.toks[this.at + n]
  }
  private next(): Token {
    const t = this.toks[this.at++]
    if (!t) throw new Error("template.rs ended early")
    return t
  }
  private expect(v: string) {
    const t = this.next()
    if (t.v !== v) throw new Error(`template.rs: expected ${v}, found ${t.v} near token ${this.at}`)
  }
  private str(): string {
    const t = this.next()
    if (t.t !== "str") throw new Error(`template.rs: expected a string, found ${t.v}`)
    return t.v
  }
  private is(v: string, n = 0) {
    return this.peek(n)?.v === v
  }

  private strList(): string[] {
    if (this.peek()?.t === "id") return this.strLists.get(this.next().v) ?? []
    this.expect("&")
    this.expect("[")
    const out: string[] = []
    while (!this.is("]")) {
      out.push(this.str())
      if (this.is(",")) this.next()
    }
    this.expect("]")
    return out
  }

  private field(): RustField {
    const head = this.next()
    if (head.v !== "field") {
      const known = this.fieldConsts.get(head.v)
      if (!known) throw new Error(`template.rs: unknown field constant ${head.v}`)
      return { ...known }
    }
    this.expect("(")
    const field_id = this.fieldId()
    this.expect(",")
    const label = this.str()
    this.expect(",")
    const control = kebab(this.next().v.replace(/^Control::/, ""))
    if (this.is(",")) this.next()
    this.expect(")")
    const f: RustField = {
      field_id,
      label,
      control,
      secret: ["secret", "secret-multiline", "totp", "passkey"].includes(control),
      required: false,
      crown_jewel: false,
      fill: "never",
      generator: null,
      options: [],
      section: "Details",
      grouped: false,
      hint: "",
      repeats: false,
    }
    while (this.is(".")) {
      this.next()
      const m = this.next().v
      this.expect("(")
      if (m === "required") f.required = true
      else if (m === "crown") f.crown_jewel = true
      else if (m === "grouped") f.grouped = true
      else if (m === "repeats") f.repeats = true
      else if (m === "fill") f.fill = this.next().v.replace(/^Fill::/, "").toLowerCase()
      else if (m === "generate") f.generator = RULES[this.next().v.replace(/^PasswordRule::/, "")] ?? RULES.STRONG!
      else if (m === "options") f.options = this.strList()
      else if (m === "section") f.section = this.str()
      else if (m === "hint") f.hint = this.str()
      else throw new Error(`template.rs: unknown field method .${m}()`)
      this.expect(")")
    }
    return f
  }

  /** A field id is a string, or a path to one of the organization constants. */
  private fieldId(): string {
    const t = this.next()
    if (t.t === "str") return t.v
    return t.v.split("::").pop()!.toLowerCase()
  }

  private fieldList(): RustField[] {
    if (this.peek()?.t === "id") {
      const name = this.next().v
      const list = this.fieldLists.get(name)
      if (!list) throw new Error(`template.rs: unknown field list ${name}`)
      return list
    }
    this.expect("&")
    this.expect("[")
    const out: RustField[] = []
    while (!this.is("]")) {
      out.push(this.field())
      if (this.is(",")) this.next()
    }
    this.expect("]")
    return out
  }

  private optStr(): string | null {
    if (this.is("None")) {
      this.next()
      return null
    }
    this.expect("Some")
    this.expect("(")
    const s = this.str()
    this.expect(")")
    return s
  }

  private template(): RustTemplate {
    if (this.is("Template")) {
      this.next()
      this.expect("{")
      let creatable = true
      let base: RustTemplate | null = null
      while (!this.is("}")) {
        if (this.is("..")) {
          this.next()
          base = this.template()
        } else {
          const key = this.next().v
          this.expect(":")
          const v = this.next().v
          if (key === "creatable") creatable = v === "true"
        }
        if (this.is(",")) this.next()
      }
      this.expect("}")
      if (!base) throw new Error("template.rs: a Template literal without ..template()")
      return { ...base, creatable }
    }
    this.expect("template")
    this.expect("(")
    const kind = kebab(this.next().v.replace(/^ItemKind::/, ""))
    this.expect(",")
    const name = this.str()
    this.expect(",")
    const summary = this.str()
    this.expect(",")
    const fields = this.fieldList()
    this.expect(",")
    const primary = this.optStr()
    this.expect(",")
    const subtitle = this.optStr()
    if (this.is(",")) this.next()
    this.expect(")")
    const t: RustTemplate = {
      kind,
      id: kind,
      variant: "",
      icon: "category",
      fills: false,
      name,
      summary,
      creatable: true,
      crown_jewel: kind === "crypto-wallet",
      autofill: !NO_AUTOFILL.has(kind),
      primary,
      subtitle,
      fields,
    }
    while (this.is(".")) {
      this.next()
      const m = this.next().v
      this.expect("(")
      const v = this.str()
      this.expect(")")
      if (m === "icon") t.icon = v
      else if (m === "variant") t.variant = v
    }
    t.id = t.variant ? `${kind}/${t.variant}` : kind
    t.fills = t.autofill && t.fields.some((f) => f.fill !== "never")
    return t
  }

  rerun(): RustTemplate[] {
    this.at = 0
    return this.run()
  }

  /** Walks the file's top level, reading the constants the templates use. */
  run(): RustTemplate[] {
    let templates: RustTemplate[] = []
    while (this.at < this.toks.length) {
      const t = this.next()
      if (t.v !== "const") continue
      const name = this.next().v
      if (!this.is(":")) continue
      this.next()
      // The type, up to `=`.
      const type: string[] = []
      while (!this.is("=")) type.push(this.next().v)
      this.next()
      const ty = type.join("")
      if (ty === "&[&str]") this.strLists.set(name, this.strList())
      else if (ty === "FieldTemplate") this.fieldConsts.set(name, this.field())
      else if (ty === "&[FieldTemplate]") this.fieldLists.set(name, this.fieldList())
      else if (ty === "&[Template]") {
        this.expect("&")
        this.expect("[")
        const out: RustTemplate[] = []
        while (!this.is("]")) {
          out.push(this.template())
          if (this.is(",")) this.next()
        }
        this.expect("]")
        templates = out
      }
    }
    return templates
  }
}

const CUSTOM_KINDS = [
  { kind: "text", label: "Text" },
  { kind: "hidden", label: "Secret" },
  { kind: "url", label: "Web address" },
  { kind: "email", label: "Email" },
  { kind: "phone", label: "Phone" },
  { kind: "date", label: "Date" },
  { kind: "boolean", label: "Yes or no" },
  { kind: "totp", label: "One-time code" },
  { kind: "linked", label: "Link to another item" },
]

/** The core's `templates` answer, from the Rust source at `file`. */
export function readTemplates(file: string): { kinds: RustTemplate[]; custom_kinds: typeof CUSTOM_KINDS } {
  const src = readFileSync(file, "utf8")
  // Only the data part: the tests at the bottom hold no tables.
  const body = src.split("#[cfg(test)]")[0]!
  const parser = new Parser(tokenize(body))
  // Twice: a list of choices may be defined below the field that names it.
  parser.run()
  const kinds = parser.rerun()
  if (kinds.length < 20) throw new Error(`template.rs: read only ${kinds.length} templates; the parser needs a look`)
  return { kinds, custom_kinds: CUSTOM_KINDS }
}
