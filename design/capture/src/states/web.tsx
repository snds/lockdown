// The web vault's states: its own screens (sign-in, unlock, key pages, the
// link page, Links, People, Settings) and the desktop's shared views as the
// web build draws them, all over the web vault's real clients
// (apps/web/src/core) on the fake worker link.

import { ClientProvider } from "@/core/client"

import { WebApp } from "../../../../apps/web/src/app/web-app"
import { WebClipboard } from "../../../../apps/web/src/core/clipboard"
import { webClient, webSession } from "../../../../apps/web/src/core/web-client"
import { SecondFactorPage, SecurityKeyPage } from "../../../../apps/web/src/features/key-pages"
import { LinkPage } from "../../../../apps/web/src/features/link-page"
import { followColorScheme } from "../../../../apps/web/src/theme"

import { PERSON } from "../fixture/seed"
import { uid } from "../fixture/ids"
import { fail, forever } from "../host/core"
import { fakeClipboard, fakeLink, waitingCredentials } from "../host/web-link"
import type { StateDef, Step } from "../harness/types"
import { WIDTHS } from "../harness/types"

function webVault() {
  followColorScheme()
  const clipboard = new WebClipboard(fakeClipboard())
  return (
    <ClientProvider value={webClient(fakeLink, clipboard)}>
      <WebApp session={webSession(fakeLink)} server={PERSON.server} link={fakeLink} clipboard={clipboard} />
    </ClientProvider>
  )
}

function linkPage(kind: "share" | "request") {
  followColorScheme()
  const link = `${PERSON.server}/${kind === "share" ? "s" : "r"}/7Hq2Lm9V#k=4f9K2mXq7HtDpL8b3ZnWqR`
  return <LinkPage page={{ kind, link }} link={fakeLink} clipboard={new WebClipboard(fakeClipboard())} />
}

const esc = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")
const starts = (s: string) => new RegExp(`^${esc(s)}(\\s|$)`)
const side = (label: string): Step => ({ click: starts(label), within: 'nav[aria-label="Sidebar"]' })
const unlock: Step[] = [{ fill: /password/i, value: "correct horse battery staple" }, { click: "Unlock", role: "button" }, { wait: /^All items/ }]
const item = (title: string): Step[] => [{ click: starts(title), within: "main" }, { wait: title, role: "heading" }]

type WebDef = Omit<StateDef, "surface" | "widths" | "mount"> & Partial<Pick<StateDef, "widths" | "mount">>
const w = (s: WebDef): StateDef => ({ surface: "web", widths: WIDTHS, mount: webVault, ...s })
const signedOut = (world: Parameters<NonNullable<StateDef["setup"]>>[0]) => {
  world.web = { signed_in: false, locked: true, login: null }
}

export const WEB_STATES: StateDef[] = [
  // --- sign in --------------------------------------------------------------------
  w({ id: "web-sign-in", screen: "Sign in", state: "Password and Secret Key", brief_row: "Sign in", notes: "This tab becomes one of your devices until you sign out.", setup: signedOut, steps: [{ wait: "Login name" }] }),
  w({ id: "web-sign-in-device", screen: "Sign in", state: "With a device", brief_row: "Sign in", notes: "Join with a code or QR from a signed-in device.", setup: signedOut, steps: [{ click: "With a device", role: "tab" }, { pause: 300 }] }),
  w({ id: "web-sign-in-new", screen: "Sign in", state: "No account yet?", brief_row: "Sign in", notes: "The web vault never creates accounts (D176).", setup: signedOut, steps: [{ click: "No account yet?", role: "tab" }, { pause: 300 }] }),
  w({
    id: "web-sign-in-error",
    screen: "Sign in",
    state: "Wrong password or Secret Key",
    brief_row: "Sign in",
    notes: "The server refused the sign-in; the words come from the core.",
    setup: signedOut,
    answers: () => ({ web_sign_in: () => fail("refused", "That login name, password and Secret Key don't open an account on this server.") }),
    steps: [{ fill: "Login name", value: PERSON.login }, { fill: "Account password", value: "not it" }, { fill: "Secret Key", value: "A3-K7QF2M" }, { click: "Sign in", role: "button", within: '[role="tabpanel"]' }, { wait: /don't open an account/ }],
  }),
  w({
    id: "web-sign-in-rate-limited",
    screen: "Sign in",
    state: "Rate limited",
    brief_row: "Sign in",
    notes: "Too many tries: the server's wait, in words.",
    setup: signedOut,
    answers: () => ({ web_sign_in: () => fail("refused", "Too many sign-in attempts for this login. Try again in 15 minutes.") }),
    steps: [{ fill: "Login name", value: PERSON.login }, { fill: "Account password", value: "x" }, { fill: "Secret Key", value: "A3" }, { click: "Sign in", role: "button", within: '[role="tabpanel"]' }, { wait: /Too many sign-in attempts/ }],
  }),
  w({
    id: "web-sign-in-security-key",
    screen: "Sign in",
    state: "Waiting for a security key",
    brief_row: "Security-key or email wait",
    notes: "The account asks for a security key on a device it has never seen (D250).",
    setup: signedOut,
    answers: () => ({ web_sign_in: () => fail("approval_needed", "This account asks for a security key on a new device.", "security_key") }),
    steps: [{ fill: "Login name", value: PERSON.login }, { fill: "Account password", value: "x" }, { fill: "Secret Key", value: "A3" }, { click: "Sign in", role: "button", within: '[role="tabpanel"]' }, { pause: 800 }],
  }),
  w({
    id: "web-sign-in-email-wait",
    screen: "Sign in",
    state: "Waiting for an email approval",
    brief_row: "Security-key or email wait",
    notes: "The server emailed a link to approve this tab (D300); the address is masked.",
    setup: signedOut,
    answers: () => ({ web_sign_in: () => fail("approval_needed", "we emailed s••@rivera.example a link to approve this tab", "email"), web_sign_in_continue: () => forever() }),
    steps: [{ fill: "Login name", value: PERSON.login }, { fill: "Account password", value: "x" }, { fill: "Secret Key", value: "A3" }, { click: "Sign in", role: "button", within: '[role="tabpanel"]' }, { pause: 800 }],
  }),
  w({ id: "web-starting", screen: "Sign in", state: "Starting", brief_row: "Sign in", notes: "The worker has not answered yet.", answers: () => ({ web_state: () => forever() }), steps: [{ pause: 300 }] }),
  w({ id: "web-cant-start", screen: "Sign in", state: "Can't start", brief_row: "Sign in", notes: "The core module failed its integrity check.", answers: () => ({ web_state: () => fail("stopped", "The vault's core module didn't match its published hash, so it wasn't run. Reload the page; if this stays, tell the server's operator.") }), steps: [{ wait: "The web vault can't start" }] }),

  // --- unlock --------------------------------------------------------------------------
  w({ id: "web-unlock", screen: "Unlock", state: "Locked", brief_row: "Unlock", notes: "Signed in, locked: the password only (D174).", steps: [{ wait: "Unlock", role: "button" }] }),
  w({ id: "web-unlock-wrong", screen: "Unlock", state: "Wrong password", brief_row: "Unlock", notes: "The password didn't match.", steps: [{ fill: /password/i, value: "wrong" }, { click: "Unlock", role: "button" }, { wait: /didn't match/ }] }),

  // --- the shared views, as the web vault draws them --------------------------------
  w({ id: "web-vault", screen: "Vault", state: "Filled", brief_row: "Vault", notes: "The desktop's vault view in the web build: Links, People, Devices and Settings in the sidebar.", steps: unlock }),
  w({ id: "web-item-login", screen: "Item detail", state: "Login", brief_row: "Item detail", notes: "A login in the web build: copies go through the page's clipboard.", steps: [...unlock, ...item("Northwind Mail")] }),
  w({ id: "web-item-wallet", screen: "Item detail", state: "Crown jewel", brief_row: "Item detail", notes: "A crown jewel in the web build.", steps: [...unlock, ...item("Cold wallet")] }),
  w({ id: "web-codes", screen: "Codes view", state: "Filled", brief_row: "Codes view", notes: "Codes in the web build.", steps: [...unlock, side("Codes")] }),
  w({ id: "web-devices", screen: "Devices", state: "Devices and sessions", brief_row: "Signed in", notes: "Devices and where the account is signed in.", steps: [...unlock, side("Devices")] }),
  w({ id: "web-links", screen: "Links", state: "Links and requests", brief_row: "Links", notes: "Make text and request links; list; open answers.", steps: [...unlock, side("Links")] }),
  w({ id: "web-people", screen: "People", state: "Contacts", brief_row: "People", notes: "Contacts and your code, as on the desktop.", steps: [...unlock, side("People")] }),
  w({ id: "web-security", screen: "Security", state: "Quantum-safe line", brief_row: "Security", notes: "The web vault's Security screen: the quantum-safe line; password checks stay in the desktop app.", steps: [...unlock, side("Security")] }),
  w({ id: "web-settings", screen: "Settings", state: "Tab settings", brief_row: "Settings", notes: "Lock timers, clipboard, density, reveal, This server, the trust note; Sign out of this tab.", steps: [...unlock, side("Settings")] }),

  // --- pages with no account --------------------------------------------------------
  w({ id: "web-link-share", screen: "Link page", state: "Share link: before opening", brief_row: "Link page (`/s/…`, `/r/…`)", notes: "Opening counts a view; one view left.", mount: () => linkPage("share"), steps: [{ pause: 500 }] }),
  w({ id: "web-link-share-opened", screen: "Link page", state: "Share link: opened", brief_row: "Link page (`/s/…`, `/r/…`)", notes: "The link's values, one at a time.", mount: () => linkPage("share"), steps: [{ click: /^Open/, role: "button" }, { pause: 500 }] }),
  w({ id: "web-link-request", screen: "Link page", state: "Request link", brief_row: "Link page (`/s/…`, `/r/…`)", notes: "Answer a request for a secret.", mount: () => linkPage("request"), steps: [{ pause: 500 }] }),
  w({ id: "web-key-second-factor", screen: "Key pages", state: "Confirm a sign-in with a key", brief_row: "Key pages", notes: "The page the desktop opens when a new device waits for a security key.", mount: () => {
    followColorScheme()
    return <SecondFactorPage link={fakeLink} id={uid("pending", 1)} credentials={waitingCredentials} />
  }, steps: [{ pause: 500 }] }),
  w({ id: "web-key-add", screen: "Key pages", state: "Add a security key", brief_row: "Key pages", notes: "Adding a security key to the account.", mount: () => {
    followColorScheme()
    return <SecurityKeyPage link={fakeLink} id={uid("pending", 2)} credentials={waitingCredentials} />
  }, steps: [{ pause: 500 }] }),
]
