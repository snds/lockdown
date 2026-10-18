import { StrictMode } from "react"
import { createRoot } from "react-dom/client"

import { ClientProvider } from "@/core/client"

import "sonner/dist/styles.css"
import "./index.css"
import { WebApp } from "./app/web-app"
import { WebClipboard } from "./core/clipboard"
import { WorkerLink } from "./core/link"
import { webClient, webSession } from "./core/web-client"
import { followColorScheme } from "./theme"

followColorScheme()

const worker = new Worker(new URL("./worker/worker.ts", import.meta.url), { type: "module", name: "lockdown-core" })
const link = new WorkerLink(worker)
const client = webClient(link, new WebClipboard(navigator.clipboard))
const session = webSession(link)

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <ClientProvider value={client}>
      <WebApp session={session} server={window.location.origin} />
    </ClientProvider>
  </StrictMode>,
)
