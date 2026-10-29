import { useEffect, useRef, useState } from "react"

export type Mode = "menu" | "prompt" | "passkey"
const WIDTH: Record<Mode, number> = { menu: 320, prompt: 320, passkey: 360 }

/**
 * A plain sign-in page and the frame where the content script would put
 * it: under the focused field (the menu), top right (the save prompt), or
 * top centre (the passkey sheet). The frame is an iframe of the harness's
 * frame.html named for its scenario; it posts its height as the real one
 * does. The plain page is a page: its title is its `h1` and its form sits
 * in its `main`, so axe's page rules (one `h1`, content in landmarks) judge
 * Lockdown's frame and not the stand-in around it.
 */
export function TestPage({ mode, name, covered }: { mode: Mode; name: string; covered?: boolean }) {
  const [height, setHeight] = useState(mode === "menu" ? 56 : 150)
  const frame = useRef<HTMLIFrameElement>(null)
  useEffect(() => {
    const on = (e: MessageEvent) => {
      const d = e.data as { lockdown?: string; px?: number }
      if (e.source === frame.current?.contentWindow && d?.lockdown === "height" && typeof d.px === "number") setHeight(Math.max(40, Math.min(420, d.px)))
    }
    window.addEventListener("message", on)
    return () => window.removeEventListener("message", on)
  }, [])
  const src = new URL("./frame.html", document.baseURI).href
  const dark = window.matchMedia("(prefers-color-scheme: dark)").matches
  const place: React.CSSProperties =
    mode === "menu" ? { top: 182, left: 24 } : mode === "prompt" ? { top: 12, right: 12 } : { top: 12, left: Math.max(4, (480 - WIDTH.passkey) / 2) }
  return (
    <div className="min-h-full bg-white p-4">
      <main data-capture-frame="" style={{ position: "relative", width: 480, height: mode === "menu" ? 182 + height + 40 : 360, fontFamily: "Georgia, serif", color: "#222", background: "#f6f3ee", border: "1px solid #ddd" }}>
        <h1 style={{ margin: 0, padding: "16px 24px", borderBottom: "1px solid #ddd", fontSize: 16, fontWeight: 700 }}>A plain test page</h1>
        <form style={{ padding: "16px 24px", display: "flex", flexDirection: "column", gap: 8 }} onSubmit={(e) => e.preventDefault()}>
          <label htmlFor="tp-user" style={{ fontSize: 14 }}>
            Email or username
          </label>
          <input id="tp-user" autoComplete="username" style={{ width: 320, padding: "6px 8px", border: "2px solid #4a6fa5", background: "white" }} />
          <label htmlFor="tp-pass" style={{ fontSize: 14 }}>
            Password
          </label>
          <input id="tp-pass" type="password" autoComplete="current-password" style={{ width: 320, padding: "6px 8px", border: "1px solid #999", background: "white" }} />
        </form>
        <iframe
          ref={frame}
          title="Lockdown"
          name={`ld-extframe:${mode}:${name}:${dark ? "dark" : "light"}`}
          src={src}
          scrolling="no"
          style={{ position: "absolute", ...place, width: WIDTH[mode], height, border: 0, borderRadius: 10, boxShadow: "0 6px 24px rgba(0,0,0,.28)", colorScheme: dark ? "dark" : "light" }}
        />
        {covered && (
          <div
            aria-hidden="true"
            style={{ position: "absolute", top: 182 + 20, left: 24, width: 320, height: 40, background: "rgba(255, 214, 0, 0.35)", border: "1px dashed #b38f00" }}
            title="A page element drawn over the menu"
          />
        )}
      </main>
    </div>
  )
}

