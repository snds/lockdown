/**
 * A small Markdown reader for secure notes (02-model §4 "Note: Markdown
 * body"). It turns text into a tree of plain nodes that `markdown-view`
 * draws as React elements, so no HTML in a note is ever interpreted: a
 * `<script>` is shown as the characters it is, and there is no
 * `dangerouslySetInnerHTML` anywhere on the way. Links keep only `http`,
 * `https` and `mailto` addresses; anything else is shown as text.
 *
 * What it reads: `#` to `######` headings, paragraphs (a line break inside
 * one stays a line break), `-` `*` `+` and `1.` lists (with `[ ]` and
 * `[x]` task boxes), `>` quotes, fenced code blocks, `---` rules, and
 * inline `**strong**`, `*em*`, `~~struck~~`, `` `code` `` and
 * `[text](https://…)` links.
 */

export type Inline =
  | { t: "text"; text: string }
  | { t: "strong"; children: Inline[] }
  | { t: "em"; children: Inline[] }
  | { t: "del"; children: Inline[] }
  | { t: "code"; text: string }
  | { t: "link"; href: string; children: Inline[] }
  | { t: "br" }

export interface ListItem {
  /** `true` or `false` for a task box, `null` for a plain item. */
  checked: boolean | null
  children: Inline[]
}

export type Block =
  | { t: "heading"; level: 1 | 2 | 3 | 4 | 5 | 6; children: Inline[] }
  | { t: "paragraph"; children: Inline[] }
  | { t: "list"; ordered: boolean; start: number; items: ListItem[] }
  | { t: "quote"; children: Block[] }
  | { t: "code"; text: string }
  | { t: "rule" }

/** The most text a note body is read as Markdown; past it, the rest shows as plain text. */
export const MAX_MARKDOWN = 200_000

/** Whether a link target is one a note may open: the web, or an email. */
export function safeHref(href: string): string | null {
  const h = href.trim()
  if (/^https?:\/\/[^\s]+$/i.test(h) || /^mailto:[^\s]+$/i.test(h)) return h
  return null
}

const INLINE_MARKS: Array<[string, "strong" | "em" | "del"]> = [
  ["**", "strong"],
  ["__", "strong"],
  ["~~", "del"],
  ["*", "em"],
  ["_", "em"],
]

/** Inline marks inside one block's text. Unclosed marks are text. */
export function parseInline(text: string, depth = 0): Inline[] {
  const out: Inline[] = []
  let buf = ""
  const flush = () => {
    if (buf) out.push({ t: "text", text: buf })
    buf = ""
  }
  let i = 0
  while (i < text.length) {
    const c = text.charAt(i)
    if (c === "\\" && i + 1 < text.length && /[\\`*_~[\]()#>+\-.!]/.test(text.charAt(i + 1))) {
      buf += text.charAt(i + 1)
      i += 2
      continue
    }
    if (c === "\n") {
      flush()
      out.push({ t: "br" })
      i += 1
      continue
    }
    if (c === "`") {
      const end = text.indexOf("`", i + 1)
      if (end > i) {
        flush()
        out.push({ t: "code", text: text.slice(i + 1, end) })
        i = end + 1
        continue
      }
    }
    if (c === "[" && depth < 4) {
      const close = text.indexOf("]", i + 1)
      if (close > i && text.charAt(close + 1) === "(") {
        const end = text.indexOf(")", close + 2)
        if (end > close) {
          const label = text.slice(i + 1, close)
          const href = safeHref(text.slice(close + 2, end))
          flush()
          if (href) out.push({ t: "link", href, children: parseInline(label, depth + 1) })
          else out.push({ t: "text", text: text.slice(i, end + 1) })
          i = end + 1
          continue
        }
      }
    }
    if ((c === "*" || c === "_" || c === "~") && depth < 4) {
      const mark = INLINE_MARKS.find(([m]) => text.startsWith(m, i))
      if (mark) {
        const [m, kind] = mark
        const end = text.indexOf(m, i + m.length)
        // A mark needs something inside it and no space just inside the opener.
        if (end > i + m.length && !/\s/.test(text.charAt(i + m.length))) {
          flush()
          out.push({ t: kind, children: parseInline(text.slice(i + m.length, end), depth + 1) })
          i = end + m.length
          continue
        }
      }
    }
    buf += c
    i += 1
  }
  flush()
  return out
}

const HEADING = /^(#{1,6})\s+(.*?)\s*#*\s*$/
const BULLET = /^\s*([-*+])\s+(.*)$/
const NUMBERED = /^\s*(\d{1,9})[.)]\s+(.*)$/
const RULE = /^\s*([-*_])(\s*\1){2,}\s*$/
const FENCE = /^\s*(```|~~~)/
const QUOTE = /^\s*>\s?(.*)$/
const TASK = /^\[( |x|X)\]\s+(.*)$/

function listItem(text: string): ListItem {
  const task = TASK.exec(text)
  if (task) return { checked: task[1] !== " ", children: parseInline(task[2] ?? "") }
  return { checked: null, children: parseInline(text) }
}

/** The blocks of a note's text. */
export function parseMarkdown(source: string, depth = 0): Block[] {
  const text = source.length > MAX_MARKDOWN ? source.slice(0, MAX_MARKDOWN) : source
  const lines = text.replace(/\r\n?/g, "\n").split("\n")
  const blocks: Block[] = []
  const L = (k: number) => lines[k] ?? ""
  let i = 0
  while (i < lines.length) {
    const line = L(i)
    if (!line.trim()) {
      i += 1
      continue
    }
    const fence = FENCE.exec(line)
    if (fence) {
      const body: string[] = []
      i += 1
      while (i < lines.length && !L(i).trim().startsWith(fence[1] ?? "```")) {
        body.push(L(i))
        i += 1
      }
      i += 1
      blocks.push({ t: "code", text: body.join("\n") })
      continue
    }
    const heading = HEADING.exec(line)
    if (heading) {
      blocks.push({ t: "heading", level: (heading[1] ?? "#").length as 1 | 2 | 3 | 4 | 5 | 6, children: parseInline(heading[2] ?? "") })
      i += 1
      continue
    }
    if (RULE.test(line)) {
      blocks.push({ t: "rule" })
      i += 1
      continue
    }
    if (QUOTE.test(line) && depth < 4) {
      const body: string[] = []
      while (i < lines.length && QUOTE.test(L(i))) {
        body.push(QUOTE.exec(L(i))?.[1] ?? "")
        i += 1
      }
      blocks.push({ t: "quote", children: parseMarkdown(body.join("\n"), depth + 1) })
      continue
    }
    const bullet = BULLET.exec(line)
    const numbered = NUMBERED.exec(line)
    if (bullet || numbered) {
      const ordered = !bullet
      const start = numbered ? Number(numbered[1] ?? 1) : 1
      const items: ListItem[] = []
      while (i < lines.length) {
        const b = BULLET.exec(L(i))
        const n = NUMBERED.exec(L(i))
        const m = ordered ? n : b
        if (m) {
          items.push(listItem(m[2] ?? ""))
          i += 1
        } else if (L(i).trim() && /^\s{2,}/.test(L(i)) && items.length) {
          // A continuation line of the item above.
          items[items.length - 1]?.children.push({ t: "br" }, ...parseInline(L(i).trim()))
          i += 1
        } else break
      }
      blocks.push({ t: "list", ordered, start, items })
      continue
    }
    const para: string[] = []
    while (
      i < lines.length &&
      L(i).trim() &&
      !HEADING.test(L(i)) &&
      !FENCE.test(L(i)) &&
      !QUOTE.test(L(i)) &&
      !BULLET.test(L(i)) &&
      !NUMBERED.test(L(i)) &&
      !RULE.test(L(i))
    ) {
      para.push(L(i).trim())
      i += 1
    }
    blocks.push({ t: "paragraph", children: parseInline(para.join("\n")) })
  }
  return blocks
}
