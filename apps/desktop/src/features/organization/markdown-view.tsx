import { useMemo, type ReactNode } from "react"

import { Button } from "@unicron/ui"

import { useClient } from "@/core/client"

import { parseMarkdown, type Block, type Inline } from "./markdown"

/**
 * A secure note's body drawn from its Markdown (`markdown.ts`): every node
 * becomes a React element, so text is escaped by React and no HTML in the
 * note is ever run. A link is a button that opens the default browser
 * through the shell's `open_site` (http and https only), never this window.
 */
export function MarkdownView({ text }: { text: string }) {
  const client = useClient()
  const blocks = useMemo(() => parseMarkdown(text), [text])
  const open = (href: string) => void client.openSite(href).catch(() => {})
  return <div className="flex flex-col gap-3 text-sm leading-relaxed">{blocks.map((b, i) => block(b, i, open))}</div>
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
        return (
          <code key={i} className="rounded bg-muted px-1 font-mono text-xs">
            {n.text}
          </code>
        )
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

const HEADING_CLASS: Record<number, string> = {
  1: "text-xl font-semibold tracking-tight",
  2: "text-lg font-semibold tracking-tight",
  3: "text-base font-semibold",
  4: "text-sm font-semibold",
  5: "text-sm font-medium",
  6: "text-sm font-medium text-muted-foreground",
}

function block(b: Block, key: number, open: (href: string) => void): ReactNode {
  switch (b.t) {
    case "heading": {
      const Tag = `h${Math.min(6, b.level + 2)}` as "h3"
      return (
        <Tag key={key} className={HEADING_CLASS[b.level]}>
          {inline(b.children, open)}
        </Tag>
      )
    }
    case "paragraph":
      return <p key={key}>{inline(b.children, open)}</p>
    case "rule":
      return <hr key={key} className="border-border" />
    case "code":
      return (
        <pre key={key} className="overflow-x-auto rounded-md bg-muted p-3 font-mono text-xs">
          <code>{b.text}</code>
        </pre>
      )
    case "quote":
      return (
        <blockquote key={key} className="flex flex-col gap-2 border-l-2 pl-3 text-muted-foreground">
          {b.children.map((c, i) => block(c, i, open))}
        </blockquote>
      )
    case "list": {
      const items = b.items.map((it, i) => (
        <li key={i}>
          {it.checked !== null && (
            <span role="img" aria-label={it.checked ? "done" : "not done"} className="mr-1">
              {it.checked ? "☑" : "☐"}
            </span>
          )}
          {inline(it.children, open)}
        </li>
      ))
      return b.ordered ? (
        <ol key={key} start={b.start} className="list-decimal pl-5">
          {items}
        </ol>
      ) : (
        <ul key={key} className="list-disc pl-5">
          {items}
        </ul>
      )
    }
  }
}
