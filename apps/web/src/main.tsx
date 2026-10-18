import { StrictMode } from "react"
import { createRoot } from "react-dom/client"

import { ClientProvider } from "@/core/client"

import "sonner/dist/styles.css"
import "./index.css"
import { WebApp } from "./app/web-app"
import { WebClipboard } from "./core/clipboard"
import { WorkerLink } from "./core/link"
import { linkFromLocation } from "./core/page-link"
import { webClient, webSession } from "./core/web-client"
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

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    {page ? (
      <LinkPage page={page} link={link} clipboard={clipboard} />
    ) : (
      <ClientProvider value={webClient(link, clipboard)}>
        <WebApp session={webSession(link)} server={window.location.origin} link={link} clipboard={clipboard} />
      </ClientProvider>
    )}
  </StrictMode>,
)
