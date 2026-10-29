// What the fake background answers in each extension state. The popup
// draws in the page; an in-page frame draws in an iframe that reads its
// scenario by name, so both look the scenario up here.

import type { MenuData } from "../../../../extension/src/shared/protocol"

import { LINK, byTitle, candidate, type ExtScenario } from "../host/chrome"

const menu = (over: Partial<MenuData>): MenuData => ({
  kind: "login",
  link: LINK.ready(),
  host: "mail.northwind.example",
  logins: [],
  wallet: [],
  lookalike_of: [],
  note: null,
  passkeys: [],
  suggest: null,
  ...over,
})

const bank = () => ({ url: "https://secure.bluefin-bank.example/login", matches: { host: "secure.bluefin-bank.example", logins: [candidate(byTitle("Bluefin Bank"), "exact", true)], lookalike_of: [], secure: true } })

export const EXT_SCENARIOS: Record<string, () => ExtScenario> = {
  // --- the popup ---------------------------------------------------------------
  "popup-connecting": () => ({ link: null }),
  "popup-no-app": () => ({ link: LINK.noApp() }),
  "popup-unpaired": () => ({ link: LINK.unpaired() }),
  "popup-pairing": () => ({ link: LINK.pairing() }),
  "popup-locked": () => ({ link: LINK.locked() }),
  "popup-results": () => ({ link: LINK.ready(), site: bank() }),
  "popup-close-match": () => ({ link: LINK.ready(), site: bank(), closeMatch: true }),
  "popup-lookalike": () => ({
    link: LINK.ready(),
    site: { url: "https://secure.b1uefin-bank.example/alerts", matches: { host: "secure.b1uefin-bank.example", logins: [], lookalike_of: ["bluefin-bank.example"], secure: true } },
  }),
  "popup-hide-email": () => ({
    link: LINK.ready(),
    site: { url: "https://boards.pinecone.example/join", matches: { host: "boards.pinecone.example", logins: [], lookalike_of: [], secure: true } },
    aliases: [{ item_id: "relaymail", name: "Relaymail", domain: "relay.rivera.example" }],
  }),

  // --- in-page frames ------------------------------------------------------------
  "menu-logins": () => ({ link: LINK.ready(), menu: menu({ logins: [candidate(byTitle("Northwind Mail")), { ...candidate(byTitle("Northwind Mail recovery codes"), "base_domain"), title: "Northwind (old account)", username: "sam.r@northwind.example" }] }) }),
  "menu-protected": () => ({ link: LINK.ready(), menu: menu({ host: "app.quillnotes.example", logins: [candidate(byTitle("Quill Notes")), { ...candidate(byTitle("Cold wallet")), title: "Quill (work)", crown_jewel: true }] }) }),
  "menu-passkeys": () => {
    const px = byTitle("Pixelforge")
    return {
      link: LINK.ready(),
      menu: menu({
        host: "pixelforge.example",
        logins: [candidate(px)],
        passkeys: [{ item_id: px.id, credential_id: "AbF3kQ9xLm2vR8sT1wYz4Q", title: "Pixelforge", user_name: "sam_r", user_display_name: "Sam R." }],
      }),
    }
  },
  "menu-code": () => ({ link: LINK.ready(), menu: menu({ kind: "code", logins: [candidate(byTitle("Northwind Mail"))] }) }),
  "menu-card": () => ({
    link: LINK.ready(),
    menu: menu({
      kind: "card",
      host: "checkout.harbourmarket.example",
      wallet: [
        { item_id: byTitle("Bluefin Visa").id, kind: "card", title: "Bluefin Visa", subtitle: "Visa ending 1111", crown_jewel: false },
        { item_id: byTitle("Family Mastercard").id, kind: "card", title: "Family Mastercard", subtitle: "Mastercard ending 4444", crown_jewel: false },
      ],
    }),
  }),
  "menu-identity": () => ({ link: LINK.ready(), menu: menu({ kind: "identity", host: "checkout.harbourmarket.example", wallet: [{ item_id: byTitle("Sam Rivera").id, kind: "identity", title: "Sam Rivera", subtitle: "48 Alder Lane, Port Elm", crown_jewel: false }] }) }),
  "menu-suggest": () => ({
    link: LINK.ready(),
    menu: menu({
      host: "boards.pinecone.example",
      suggest: { words: "20 characters: letters, digits and symbols", bits: 129, strength: "strong", site: { host: "boards.pinecone.example", rule: "minlength: 12; maxlength: 64; required: digit;", words: "12 to 64 characters, with a digit", source: "published" } },
    }),
  }),
  "menu-lookalike": () => ({ link: LINK.ready(), menu: menu({ host: "secure.b1uefin-bank.example", lookalike_of: ["bluefin-bank.example"] }) }),
  "menu-locked": () => ({ link: LINK.locked(), menu: menu({ link: LINK.locked() }) }),
  "menu-unpaired": () => ({ link: LINK.unpaired(), menu: menu({ link: LINK.unpaired() }) }),
  "menu-no-app": () => ({ link: LINK.noApp(), menu: menu({ link: LINK.noApp() }) }),
  "menu-covered": () => ({ link: LINK.ready(), menu: menu({ logins: [candidate(byTitle("Northwind Mail"))] }) }),
  "prompt-save": () => ({ link: LINK.ready(), prompt: { offer_id: "o1", kind: "new", title: "boards.pinecone.example", username: "sam.rivera@northwind.example" } }),
  "prompt-update": () => ({ link: LINK.ready(), prompt: { offer_id: "o2", kind: "update", title: "Parcelpost", username: "sam.rivera@northwind.example" } }),
  "sheet-get": () => {
    const px = byTitle("Pixelforge")
    return {
      link: LINK.ready(),
      sheet: {
        kind: "get",
        host: "pixelforge.example",
        rp_id: "pixelforge.example",
        rp_name: "Pixelforge",
        options: [
          { item_id: px.id, credential_id: "AbF3kQ9xLm2vR8sT1wYz4Q", title: "Pixelforge", user_name: "sam_r", user_display_name: "Sam R." },
          { item_id: px.id, credential_id: "Zz9yXw8vUt7sRq6pOn5mLk", title: "Pixelforge (team)", user_name: "rivera-studio", user_display_name: "Rivera Studio" },
        ],
        plan: null,
      },
    }
  },
  "sheet-create": () => ({
    link: LINK.ready(),
    sheet: { kind: "create", host: "boards.pinecone.example", rp_id: "pinecone.example", rp_name: "Pinecone Boards", options: [], plan: { rp_id: "pinecone.example", excluded: false, item_id: null, title: "", user_name: "sam.rivera@northwind.example" } },
  }),
  "sheet-create-held": () => ({
    link: LINK.ready(),
    sheet: { kind: "create", host: "pixelforge.example", rp_id: "pixelforge.example", rp_name: "Pixelforge", options: [], plan: { rp_id: "pixelforge.example", excluded: true, item_id: byTitle("Pixelforge").id, title: "Pixelforge", user_name: "sam_r" } },
  }),
  "sheet-locked": () => ({ link: LINK.locked(), sheet: { kind: "locked", host: "pixelforge.example", rp_id: "pixelforge.example", rp_name: "Pixelforge", options: [], plan: null } }),
}

export function scenario(name: string): ExtScenario {
  const make = EXT_SCENARIOS[name]
  if (!make) throw new Error(`no extension scenario called ${name}`)
  return make()
}
