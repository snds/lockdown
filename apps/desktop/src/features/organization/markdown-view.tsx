import { useMemo, type ReactNode } from "react"

import { Button, Prose } from "@unicron/ui"

import { useClient } from "@/core/client"

import { parseMarkdown, type Block, type Inline } from "./markdown"

/**
 * A secure note's body drawn from its Markdown (`markdown.ts`): every node
 * becomes a React element, so text is escaped by React and no HTML in the
 * note is ever run. unicron's `Prose` (body) draws the elements from the
 * type roles (brief I2), so this file sets no look of its own. A link is a
 * button that opens the default browser through the shell's `open_site`
 * (http and https only), never this window.
 */
export function MarkdownView({ text }: { text: string }) {
  const client = useClient()
  const blocks = useMemo(() => parseMarkdown(text), [text])
  const open = (href: string) => void client.openSite(href).catch(() => {})
  return <Prose variant="body">{blocks.map((b, i) => block(b, i, open))}</Prose>
}

function inline(nodes: Inline[], open: (href: string) => void): ReactNode[] {
  return nodes.map((n, i) => {
    switch (n.t) {
      case "text":
        return n.text
      case "br":
        return <br key={i} />
      case "strong":
        return <strong key={i}>{inline(n.children, open)}</strong>
      case "em":
        return <em key={i}>{inline(n.children, open)}</em>
      case "del":
        return <del key={i}>{inline(n.children, open)}</del>
      case "code":
        return <code key={i}>{n.text}</code>
      case "link":
        // A button, not an anchor: nothing in a note can navigate this window.
        return n.href.startsWith("mailto:") ? (
          <span key={i}>{inline(n.children, open)}</span>
        ) : (
          <Button key={i} variant="link" size="inline" title={n.href} onClick={() => open(n.href)}>
            {inline(n.children, open)}
          </Button>
        )
    }
  })
}

function block(b: Block, key: number, open: (href: string) => void): ReactNode {
  switch (b.t) {
    case "heading": {
      const Tag = `h${Math.min(6, b.level + 2)}` as "h3"
      return (
        <Tag key={key}>
          {inline(b.children, open)}
        </Tag>
      )
    }
    case "paragraph":
      return <p key={key}>{inline(b.children, open)}</p>
    case "rule":
      return <hr key={key} />
    case "code":
      return (
        // A long line scrolls sideways, so the block takes focus for the keyboard (review R18).
        <pre key={key} tabIndex={0}>
          <code>{b.text}</code>
        </pre>
      )
    case "quote":
      return (
        <blockquote key={key}>
          {b.children.map((c, i) => block(c, i, open))}
        </blockquote>
      )
    case "list": {
      const items = b.items.map((it, i) => (
        <li key={i}>
          {it.checked !== null && (
            <span role="img" aria-label={it.checked ? "done" : "not done"}>
              {it.checked ? "☑ " : "☐ "}
            </span>
          )}
          {inline(it.children, open)}
        </li>
      ))
      return b.ordered ? (
        <ol key={key} start={b.start}>
          {items}
        </ol>
      ) : (
        <ul key={key}>
          {items}
        </ul>
      )
    }
  }
}
