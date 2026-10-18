import { render, screen } from "@testing-library/react"
import userEvent from "@testing-library/user-event"

import { ClientProvider, type Client } from "@/core/client"
import { fakeClient } from "@/test/fake-client"

import { parseMarkdown, parseInline, safeHref } from "./markdown"
import { MarkdownView } from "./markdown-view"

describe("secure notes render Markdown safely", () => {
  it("reads headings, lists, task boxes, quotes, code and rules", () => {
    const blocks = parseMarkdown("# Plans\n\nFirst line\nsecond line\n\n* one\n* [x] done\n\n1. a\n2. b\n\n> quoted\n\n```\nraw <b>\n```\n\n---")
    expect(blocks.map((b) => b.t)).toEqual(["heading", "paragraph", "list", "list", "quote", "code", "rule"])
    const list = blocks[2]
    expect(list?.t === "list" && list.items[1]?.checked).toBe(true)
    const ordered = blocks[3]
    expect(ordered?.t === "list" && ordered.ordered).toBe(true)
    const code = blocks[5]
    expect(code?.t === "code" && code.text).toBe("raw <b>")
  })

  it("keeps only web and email links; marks inside text nest; unclosed marks stay text", () => {
    expect(safeHref("https://example.com/a")).toBe("https://example.com/a")
    expect(safeHref("javascript:alert(1)")).toBeNull()
    expect(safeHref("data:text/html,x")).toBeNull()
    expect(parseInline("[x](javascript:alert(1))")).toEqual([{ t: "text", text: "[x](javascript:alert(1)" }, { t: "text", text: ")" }])
    expect(parseInline("**bold *and em***")[0]?.t).toBe("strong")
    expect(parseInline("2 * 3 = 6")).toEqual([{ t: "text", text: "2 * 3 = 6" }])
  })

  it("shows HTML as the characters it is and opens links in the browser, never here", async () => {
    const openSite = vi.fn<Client["openSite"]>(() => Promise.resolve())
    const { container } = render(
      <ClientProvider value={fakeClient({ openSite })}>
        <MarkdownView text={'# Title\n\n<script>alert("x")</script> <img src=x onerror=alert(1)>\n\nSee [the site](https://example.com) and [bad](javascript:alert(1)).'} />
      </ClientProvider>,
    )
    expect(screen.getByRole("heading", { name: "Title" })).toBeInTheDocument()
    expect(container.querySelector("script")).toBeNull()
    expect(container.querySelector("img")).toBeNull()
    expect(container.querySelector("a")).toBeNull()
    expect(screen.getByText(/<script>alert\("x"\)<\/script>/)).toBeInTheDocument()
    await userEvent.setup().click(screen.getByRole("button", { name: "the site" }))
    expect(openSite).toHaveBeenCalledWith("https://example.com")
    expect(screen.queryByRole("button", { name: "bad" })).not.toBeInTheDocument()
  })
})
