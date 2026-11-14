// What every state file starts from (foundation): the desktop app and the
// web vault mounted over the fake core, and the two ways to declare a state
// of each. The lanes' state files import these and never mount their own.

import App from "@/App"
import { ClientProvider } from "@/core/client"

import { WebApp } from "../../../../apps/web/src/app/web-app"
import { WebClipboard } from "../../../../apps/web/src/core/clipboard"
import { webClient, webSession } from "../../../../apps/web/src/core/web-client"
import { LinkPage } from "../../../../apps/web/src/features/link-page"
import { followColorScheme } from "../../../../apps/web/src/theme"

import { PERSON } from "../fixture/seed"
import { fakeClipboard, fakeLink } from "../host/web-link"
import type { World } from "../fixture/world"
import type { StateDef } from "../harness/types"
import { WIDTHS } from "../harness/types"

/** The vaults, folders and tags trees are hidden below the `md` breakpoint, so these states have no phone width. */
export const WIDE = [1280, 900]

const app = () => <App />

function webVault() {
  followColorScheme()
  const clipboard = new WebClipboard(fakeClipboard())
  return (
    <ClientProvider value={webClient(fakeLink, clipboard)}>
      <WebApp session={webSession(fakeLink)} server={PERSON.server} link={fakeLink} clipboard={clipboard} />
    </ClientProvider>
  )
}

/** A share or request link's own page, which needs no account. */
export function linkPage(kind: "share" | "request") {
  followColorScheme()
  const link = `${PERSON.server}/${kind === "share" ? "s" : "r"}/7Hq2Lm9V#k=4f9K2mXq7HtDpL8b3ZnWqR`
  return <LinkPage page={{ kind, link }} link={fakeLink} clipboard={new WebClipboard(fakeClipboard())} />
}

type Def = Omit<StateDef, "surface" | "widths" | "mount"> & Partial<Pick<StateDef, "widths" | "mount">>

/** A state of the desktop app: the real `App` at every width unless it says otherwise. */
export const d = (s: Def): StateDef => ({ surface: "desktop", widths: WIDTHS, mount: app, ...s })

/** A state of the web vault: the real `WebApp` on the fake worker link. */
export const w = (s: Def): StateDef => ({ surface: "web", widths: WIDTHS, mount: webVault, ...s })

/** Keeps only some of the fixture's items: what makes a view empty, or a list short. */
export const only = (keep: (w: World["items"][number]) => boolean) => (w: World) => {
  w.items = w.items.filter(keep)
}
