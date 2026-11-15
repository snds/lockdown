import { StrictMode } from "react"
import { createRoot } from "react-dom/client"

import { ClientProvider } from "@/core/client"

import "./index.css"
import { WebApp } from "./app/web-app"
import { startBuildWatch } from "./core/build-watch-dom"
import { WebClipboard } from "./core/clipboard"
import { WorkerLink } from "./core/link"
import { linkFromLocation } from "./core/page-link"
import { webClient, webSession } from "./core/web-client"
import { keyPageFromHash } from "./core/webauthn"
import { SecondFactorPage, SecurityKeyPage } from "./features/key-pages"
import { LinkPage } from "./features/link-page"
import { followColorScheme } from "./theme"

followColorScheme()

const worker = new Worker(new URL("./worker/worker.ts", import.meta.url), { type: "module", name: "lockdown-core" })
const link = new WorkerLink(worker)
const clipboard = new WebClipboard(navigator.clipboard)

// A share or request link: its key is in the fragment, held here in memory
// and taken out of the address bar and the history at once.
const page = linkFromLocation(window.location.origin, window.location.pathname, window.location.hash)
if (page) window.history.replaceState(null, "", window.location.pathname)
// A page the desktop app opened for a security key: the id is all it holds.
const keyPage = page ? null : keyPageFromHash(window.location.hash)

// The vault's own session, made once: the app uses it, and the build watch
// asks it one thing (whether anyone is signed in in this tab).
const session = webSession(link)
// The vault only. A link page and a security-key page hold a key from the
// address in memory, so they never reload by themselves (docs/plan/66-shared-feed.md §4).
if (!page && !keyPage) startBuildWatch(session)

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    {page ? (
      <LinkPage page={page} link={link} clipboard={clipboard} />
    ) : keyPage?.kind === "second-factor" ? (
      <SecondFactorPage link={link} id={keyPage.id} credentials={navigator.credentials} />
    ) : keyPage?.kind === "security-key" ? (
      <SecurityKeyPage link={link} id={keyPage.id} credentials={navigator.credentials} />
    ) : (
      <ClientProvider value={webClient(link, clipboard)}>
        <WebApp session={session} server={window.location.origin} link={link} clipboard={clipboard} />
      </ClientProvider>
    )}
  </StrictMode>,
)
