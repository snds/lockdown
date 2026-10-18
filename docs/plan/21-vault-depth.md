# 21 — Vault depth

_Status: built, 2026-10-18, on branch `slice/vault-depth` (D122: desktop and web first; the extension where a row names it). Templates now come as data: thirty kinds of item to make by hand, variants on a kind, and templates a person makes that sync and keep a history. The generator makes passphrases and follows a site's password rules, in the editor and in the browser. Wi-Fi items have a guest card, cards can ask before their security code, any secret shows in large type, a sign-in on a hotel TV is logged and reminded, files stream a chunk at a time, and the server keeps an attachment quota per account. Proposed decisions D230–D244 are at the end. Companion to [`10-desktop-shell.md`](10-desktop-shell.md) (the layers, the command surface, the gates), [`02-model.md`](02-model.md) (items, fields, attachments), [`14-import-export.md`](14-import-export.md) (CXF), [`16-browser-extension.md`](16-browser-extension.md) and [`20-self-host-server.md`](20-self-host-server.md)._

## For future agent

- **TL;DR:** A template is data in `lockdown_vault::template`. A variant is a template on an existing kind, with the id `kind/variant` (`identity/passport`, `wi-fi/router`). An item made from one carries the reserved field `lockdown.template`. A person's own template is an item of the hidden kind `template`, and the items made from it are Custom items with `tpl.<key>` fields. Generation lives in `lockdown_crypto::generator`: characters or words (EFF's long list), a site's rules in Apple's `passwordrules` language, rejection sampling, and statistical tests in CI. Every new path that hands out a secret goes through the existing gates: a link reads through its target's gates, the Wi-Fi QR is a `Reveal`, and a card's re-prompt is an `Approve`.
- **Reads before this:** `10-desktop-shell.md` §2–§3 (the command surface, secret handling) and §12 (D112 generate on save, D116 strength). `02-model.md` §2–§4 (kinds, fields, crown jewels) and §8 (attachments). Decisions D62, D101, D110, D112, D113, D151.
- **Not here:** an identity fill that knows the new identity variants (it still fills the fixed `IDENTITY_FIELDS`), attachments in the web vault, attachment bytes through CXF (see §4), and a streamed sync upload (see §6).

## 1. What was built, by checklist row

| Row | State | What it is |
|---|---|---|
| `typed-templates` | done | 30 templates a person makes by hand, grouped in New item. Variants on a kind, with their own icons. The person's own templates sync and keep versions, with a designer in the desktop. |
| `identities` | done | Identity variants Person, Address, Business, Passport, Driving licence, National ID, plus Vehicle. A separate Contacts list (`identity/contact`) that is never filled. |
| `custom-fields` | done | Two new kinds for fields of your own: One-time code (TOTP, shown as a live code) and Link to another item (reads that item's field through its gates). |
| `password-generator` | done | Word passphrases from EFF's long list (licence shipped), with the entropy and a strength word shown. Length and class options in the desktop and web editors. Per-site rules: saved on a login or taken from the published list, and used by the extension when a site has one. Statistical randomness tests run in CI. |
| `wifi-items` | done | A guest card with a QR code that hides itself after 60 seconds and on lock. A printable card. Export through CXF's own Wi-Fi type. |
| `lossless-transfer` | partial | Wi-Fi, passports, driving licences and ID documents travel in CXF's own types. A file travels as CXF's File credential: its name, size and SHA-256 integrity hash, not its bytes (§4 says why). |
| `large-type` | done | Large type for every hidden field. Hotel and rental mode: "Signing in on a TV?" logs where and when, and reminds the person to sign out. |
| `payment-cards` | done | "Ask before using the security code" on a card. The core runs the `Approve` gate before the CVC is revealed or filled. A browser fill leaves the CVC out until the person approves. |
| `attachments` | done | Seal and open a chunk at a time; the local store keeps large blobs as files. A server quota per account (5 GB by default, an operator setting), shown in the desktop's Files panel. |

## 2. Architecture

### 2.1 Templates as data (`crates/vault/src/template.rs`, `user_template.rs`)

- `TEMPLATES: &[Template]` lists every template. Each one has a `kind`, an optional `variant`, a name, an icon, a summary and its ordered fields (`id`, label, `Control`, fill role, secret, required). `Template::id()` is `kind` or `kind/variant`. `Template::fills()` says whether the browser may offer it: Contacts never.
- **Variants** reuse a kind's storage and gates, so a passport is still an identity, a router is still a Wi-Fi item, and an outdoor licence is still a document. `compose_new_from` writes `lockdown.template = <variant>`. `template_of(kind, fields)` reads it back, so the editor, the detail pane and the list subtitle draw the right form. An unknown variant reads as its kind. A newer build's variant therefore opens on an older one as the plain kind, with every field kept.
- New variants: `wi-fi/router`; `identity/address`, `business`, `passport`, `driver-licence`, `national-id`, `vehicle`, `contact`; `document/medical-record`, `insurance`, `recovery-codes`, `outdoor-licence`; `ssh-key/pgp`; `server/email-account`. Together with the existing kinds that makes 30 creatable templates. New fields: `card.reprompt` (Boolean), `pw.rule` (a login's own password rules) and `id.personal_number`.
- **A person's template** is an item of the hidden kind `ItemKind::Template`. Its definition (`UserTemplate { name, summary, icon, fields: [UserField { key, label, kind, required }] }`) is sealed as CBOR in the field `template.definition`, at most 40 fields. As an item it syncs, keeps versions, goes to the trash and shares with its vault. Items made from it are `Custom` items. They carry `lockdown.template = user:<template id>` and one `tpl.<key>` field per definition field, with that field's label and kind. Changing the template later leaves items already made as they are: each item keeps its own labels. The template only shapes new items and the editor's empty rows.
- `CUSTOM_KINDS` gains `Totp` ("One-time code") and `Linked` ("Link to another item"). `FieldKind::Linked` is stored as text `<item uuid>#<field id>` and edited with `Control::Linked`.

### 2.2 The generator (`crates/crypto/src/generator.rs`)

- `Recipe` (serde `#[serde(tag = "style")]`): `characters { length, digits, symbols, unambiguous, avoid }` or `words { words, separator, capitalize, number }`. `Recipe::check()` refuses anything outside the bounds (8–128 characters, 3–20 words, a site's rule from 4 characters).
- `CharacterRule` is required sets, an allowed set and `max_consecutive`. Each character is drawn uniformly from the allowed set by rejection sampling (`uniform_below` for a byte, `uniform_below_wide` for a `u32`). A draw that misses a required set is thrown away whole and drawn again, so every value that meets the rule is equally likely.
- `PassphraseRule` draws from EFF's long list (`crates/crypto/data/eff_large_wordlist.txt`, 7,776 words, 12.925 bits each, CC BY 3.0 US, licence beside it and in `NOTICE`). "A digit on one word" adds log2(10) + log2(words) bits.
- `SiteRule` parses Apple's `passwordrules` language (`minlength`, `maxlength`, `required`, `allowed`, `max-consecutive`, including the `[...]]` form). `Recipe::generate_for(&SiteRule)` keeps the person's length where the rule allows it. The published rules are a snapshot of Apple's password-manager-resources `quirks/password-rules.json` (436 sites, MIT, `crates/app/data/`, licence beside it and in `NOTICE`). A test parses every one of them.
- **Randomness tests** (`generator::tests::stats`, a named CI step): chi-squared over characters and over each position's classes, serial pairs, passphrase words, NIST SP 800-22 monobit and runs over the raw source, and the wide draw's modulo bias. Each threshold is far in the tail for a fair source (well under one in a million of a false alarm), so a failure means a real fault, not bad luck.

### 2.3 Where a generated value is made

- **Editor (desktop and web):** `Change::GenerateWith(Recipe, Option<SiteRule>)`. The webview sends the recipe with a `generate` (`recipe` is added to the existing field input, per D62). The value is made in the core on save, as with D112. `App::generator_preview` gives the editor words, bits and a strength word without making anything. `apply_site_rules` turns a login's `pw` generate into one under its rule: `pw.rule` if set, else the published rule for the first website's host or the nearest parent domain.
- **Extension:** on a new-password field (marked `new-password`, or the unmarked pair or trio of password fields), the inline menu lists "Use a strong password" first. Its subtitle names the rules it follows (`password_rule`: the rule only, never a value). The value is made only on the armed pick (`suggest`). The background measures the field and the menu as for a fill. The content script writes the value into the field and its repeat, and the sign-up's own save offer keeps it. The rules are those saved on the logins exactly for this page when they agree on one, else the site's published rules, else the strong default.

### 2.4 Links (`compose::parse_link`, `session/depth.rs`)

`Session::resolve_link` sends a reveal or a code on a linked field to its target, inside the same vault only. A link to a link is refused, and so are a missing target and the item itself. `Core::reveal_for_fill` and `code_for_fill` resolve first. They refuse a fill-only target, run the target's gates (a crown jewel asks as itself), and note a use on both items (D151). A link carries no value of its own. Detail views show `LinkView { item_id, field_id, title, label, broken }` by label only.

### 2.5 Wi-Fi guest card

`wifi_payload` builds the ZXing `WIFI:T:<WPA|WEP|nopass>;S:<ssid>;P:<password>;H:<true>;;` string, escaping `\ ; , : "`. `qr_bytes` encodes it with the `qrcode` 0.14 crate (no default features, MIT or Apache-2.0) and returns `[width, modules…]` as `Revealed`. `App::wifi_qr` refuses a fill-only item, runs the `Reveal` gate on a crown jewel, counts a use and wipes its buffers. The desktop draws the grid as one SVG path. It hides the card and wipes the grid after 60 seconds, on lock, when the window loses focus and when the item's pane closes. Print uses a `data-print-card` block that the print stylesheet alone shows, so the printout is the network name, the QR and the password in large type.

### 2.6 Hotel and rental mode

The reserved field `lockdown.away` holds JSON `[{place, at, remind_at}]` (20 at most, a signed field of the item). "Signing in on a TV?" logs it. A due entry adds the warning `signed-in-elsewhere`, so the item shows in Needs attention. After unlock the desktop shows one toast for the due reminders. "Signed out" clears an entry. The field is hidden from the item's field list.

### 2.7 Streaming attachments and the quota

- `seal_attachment_stream(reader, …)` reads 1 MiB at a time. It hashes in a first pass, then seals in a second, and writes each sealed chunk to a `BlobSink`. `open_attachment_stream` checks every chunk and writes plaintext a chunk at a time to a writer. `Vault::seal_stream` then `add_prepared` attaches a `PreparedAttachment`. `open_attachment_to` saves one to a file.
- `Store` gains `blob_sink`, `keep_blob` and `open_blob`, with in-memory defaults. `SqliteStore` schema 3 adds the `blob_files` table and keeps large blobs in `<db>.blobs/<hex>`. Each is written as `.incoming-*`, synced, then renamed; leftovers are swept on open.
- Desktop: `attach_choose` / `attach_chosen` and `attachment_save_choose` / `attachment_save` go through the operating system's file dialog, so the path stays in the shell and the bytes never reach the webview. The Files panel falls back to bytes where the shell has no dialog.
- Server: `Config.attachment_quota` (`--attachment-quota`, `LOCKDOWN_ATTACHMENT_QUOTA`; default 5,000,000,000 bytes; 0 is no limit) counts blob bytes over every vault an account owns. A shared vault's files count against its owner. An upload past the room gets `ErrorCode::QuotaExceeded`. `GET /v1/account/storage` answers `StorageUsage { v, used, quota }`, and `Features.storage` advertises it. The desktop's Files panel shows "1.2 GB of 5 GB used" (`lockdown_sync::bytes_words`).

### 2.8 Command surface (added; nothing renamed)

The same commands on desktop (`apps/desktop/src-tauri/src/depth.rs`) and web (`crates/web/src/commands.rs`):

- `user_templates`, `save_user_template`, `delete_user_template`
- `link_targets`
- `generator_preview`, `site_rule`
- `wifi_qr`
- `log_away`, `clear_away`, `away_reminders`
- `storage_usage`
- desktop only: `attach_choose`, `attach_chosen`, `attachment_save_choose`, `attachment_save`

Added fields:

- `ItemSummary.template`
- `TemplateView` gains `id`, `variant`, `icon` and `fills`
- `FieldView.link`
- `ItemDetail` gains `template`, `away` and `reprompt`
- `recipe` on a field's `generate`

Native messaging gains the ops `password_rule` and `suggest`, and the extension's `MenuData` gains `suggest`. Every addition is a new field or op. Readers that don't know one ignore it (D62).

## 3. Security notes

- **No new way out for a stored secret.** A link reads through its target's gates. The Wi-Fi QR is a `Reveal`. The CVC re-prompt is an `Approve` before the value is read. A fill-only item never yields a QR or a linked value. Each one counts as a use where D151 says so.
- **The suggested password is new, not read from the vault, so no gate applies.** It is made only on a deliberate, armed pick in the extension's own frame. The field and the menu must be visible, as for a fill. The answer frame is a `Zeroizing` buffer. It enters the vault only through the existing save offer.
- **Treat as hostile:**
  - Every template definition from sync is checked (`UserTemplate::checked`: names, keys, kinds, 40 fields).
  - Every link is parsed and confined to one vault.
  - Every site rule is parsed with bounds; an unparsable saved rule is refused on save.
  - Every `lockdown.away` record is parsed with a fallback to none.
  - A page's field attributes only choose whether the menu offers a suggestion; the core decides the rest.
- **Logs:** nothing new is logged. The quota error names sizes, never names or contents.

## 4. CXF and lossless transfer

- Wi-Fi exports as CXF `wifi` (`ssid`, `networkSecurityType`, `passphrase`, `hidden`). Passport, driving licence and ID document export as `passport`, `drivers-license` and `identity-document`, with the holder's name split into `fullName` fields as the format has them. Import maps them back to the matching variant.
- A file exports as a `file` credential: `name`, `decryptedSize` and `integrityHash` (SHA-256, base64url). Its bytes are not carried. CXF 1.0 describes a file and leaves its bytes to the exchange protocol (CXP), which is still a Working Draft with no settled way to carry them. Lockdown's own backup (`../format/backup-v1.md`) carries files, so it is the lossless path between Lockdown installs. An import of a `file` credential names the file and its size in the import report, so nothing is silently lost.
- The spec site could not be reached while this was built. The field names above come from the published 1.0 text as remembered, and a review against the spec is a flagged item (D240).

## 5. How to run and test

All cargo runs use the shared target with `CARGO_INCREMENTAL=0` and `CARGO_PROFILE_DEV_DEBUG=line-tables-only`.

- **Rust:** `cargo test --workspace`. The new tests:
  - `lockdown-crypto`: `generator::tests` and `generator::tests::stats`.
  - `lockdown-vault`: `template`, `user_template`, `compose`, `attachment`.
  - `lockdown-store`: `tests/backends.rs`, `a_streamed_attachment_goes_to_a_file_and_back_in_chunks`.
  - `lockdown-cxf`: `convert::tests`, the identity documents and files.
  - `lockdown-sync`: `bytes_words`.
  - `lockdown-server`: `tests/api.rs`, `attachments_count_against_the_vaults_owner_up_to_the_quota` and `a_server_without_a_quota_says_so`.
  - `lockdown-app`: `depth::tests` and `tests/vault_depth.rs`, the room and templates over a real server.
  - `lockdown-web`: `tests/web_vault.rs`, `the_web_vault_reaches_vault_depth_through_the_same_commands`.
- **Desktop shell:** `cargo test -p lockdown-desktop --features shell`. This runs `depth::tests` and `browser::tests::a_new_password_field_is_offered_a_strong_password_under_the_sites_rules`.
- **Desktop UI:** in `apps/desktop`, `pnpm test`. It runs `vault-depth-editor.test.tsx`, `vault-depth-detail.test.tsx`, `template-designer.test.tsx` and `new-item-menu.test.tsx`. `pnpm ds:ratchet` stays at 4.
- **Web:** in `apps/web`, `pnpm build:wasm && pnpm test && pnpm e2e`. The e2e test is "vault depth in the browser…": a variant, a template of your own, a link and the generator's preview.
- **Extension:** in `extension`, `pnpm build:wasm && pnpm build && pnpm build:native && pnpm test`. The new tests are "a sign-up field is offered a strong password under the site's published rules, then saved" and "a sign-in field is not offered a new password". On a machine where checkouts share a target directory, copy `browser-endpoint` and `lockdown-native-host` into a folder and set `LOCKDOWN_TEST_BINS` to it.
- **Server quota by hand:** `lockdown-server --attachment-quota 1000000 …`, then attach a file over 1 MB from the desktop. The upload is refused with words saying how much of the room is used and how much more the file needs, and the Files panel shows the room.

## 6. Gaps

- **Sync moves a blob's ciphertext whole.** Plaintext is never whole in memory on the local paths. The upload and download through `HttpTransport` still hold one sealed blob at a time. A streamed blob route on the server is the next step.
- **The web vault has no attachments.** Its client refuses attach, save and detach in words.
- **Identity fill still uses the fixed `IDENTITY_FIELDS`** (name, email, phone, address). A business name, passport number or vehicle plate is not filled into pages. Contacts are never offered, by design.
- **WPA3 is coded as `T:WPA` in the QR.** Phones join WPA3 networks with it today; the `WPA3`/`SAE` value of newer ZXing readers is not used until it is widely read.
- **Hotel mode reminds on unlock and in Needs attention only.** There is no operating-system notification at the due time while Lockdown is locked or closed.
- **A person's template has no sections or per-field generator options.** Its fields take the custom kinds only.
- **The extension offers no generator options.** The suggestion uses Lockdown's strong recipe under the site's rules; options live in the editor.
- **CXF field names are unverified against the spec text** (§4).

## 7. unicron host-contract gaps

`@unicron/ui` has no component for these. Each is built from the existing primitives inside the feature, with no class overrides beyond the ratchet's baseline of 4:

- **QR code.** The guest card draws an SVG path from the core's module grid. A `QrCode` primitive taking `[width, modules…]` would serve the passkey hand-off and pairing too.
- **Progress or meter.** The attachment room is a sentence ("1.2 GB of 5 GB used"), and the generator's strength is a word. A `Meter` with value, low, high and optimum would show both.
- **Slider.** Length and word count are number inputs. A `Slider` with a paired number field is the usual generator control.
- **Popover.** The generator's options sit inline under the field. A `Popover` anchored to the Generate button would keep the editor shorter.
- **A print-only region.** The Wi-Fi card prints through a `data-print-card` block and a print rule in `app-extension.css`. A `PrintRegion` primitive would own that contract.

## 8. Proposed decisions

| ID | Date | Decision | Status |
|---|---|---|---|
| D230 | 2026-10-18 | **The generator makes characters or words, from options the person sets, and shows the strength without a value.** `Recipe` (`characters`: 8–128, digits, symbols, no look-alikes, characters to leave out; `words`: 3–20 from EFF's long list, separator, capitals, a digit) travels on a field's `generate` (an added field, D62), and the core makes the value on save (D112). Rejection sampling only. The preview says the options in words, the bits and weak / fair / strong (under 50, under 80, else). Default stays 20 characters with digits and symbols; a passphrase defaults to 6 words joined by hyphens (77.5 bits, "fair"). Statistical tests (chi-squared, NIST monobit and runs, modulo bias) run as a named CI step. Plain version: you can ask for a password of words you can say, and see how strong it is before you save. | Proposed; **flagged for Sean** (the 6-word default rated "fair"; the strength thresholds) |
| D231 | 2026-10-18 | **Templates are data, and a variant is a template on an existing kind.** Thirty templates to make by hand, in four groups in New item (Sign-ins and money; Identities and documents; Networks and machines; Other). A variant has the id `kind/variant`, keeps its kind's storage and gates, and is recorded in the reserved field `lockdown.template`; an unknown variant reads as its kind, so older builds keep every field. Identity variants: Person, Address, Business, Passport, Driving licence, National ID, Vehicle. Plain version: New item lists passports, routers and insurance policies by name, each with the right fields. | Proposed; **flagged for Sean** (the thirty, their names, icons and groups) |
| D232 | 2026-10-18 | **A person's own template is an item of a hidden kind.** `ItemKind::Template` holds a checked, CBOR-sealed definition (name, summary, up to 40 fields with label, kind and needed), so it syncs, keeps versions, shares with its vault and goes to the trash like an item. Items made from it are Custom items with `tpl.<key>` fields and keep their own labels; changing the template later shapes only new items. Plain version: you can design a form of your own, and it follows you to every device. | Proposed; **flagged for Sean** (user-visible: "Your templates" in New item, the designer) |
| D233 | 2026-10-18 | **Attachments are sealed and opened a chunk at a time, and the local store keeps large ones as files.** 1 MiB chunks, a hash pass then a seal pass, each chunk checked on open; SQLite schema 3 with `blob_files` and a `<db>.blobs` folder written through `.incoming-*`, sync, rename, swept on open. The desktop picks files in the operating system's dialog so paths and bytes stay out of the webview. Sync still moves one sealed blob whole (gap). | Proposed; audit item (the file-backed store's crash behaviour) |
| D234 | 2026-10-18 | **The server keeps an attachment quota per account: 5 GB by default, an operator setting.** `--attachment-quota` / `LOCKDOWN_ATTACHMENT_QUOTA` (bytes; 0 is no limit) counts blob bytes over the vaults an account owns, a shared vault's files against its owner; past it, `QuotaExceeded`. `GET /v1/account/storage` answers `{v, used, quota}`, and the desktop's Files panel shows the room. Plain version: each account has room for 5 GB of files unless whoever runs the server says otherwise. | Proposed; **flagged for Sean** (the 5 GB default; counting a shared vault against its owner) |
| D235 | 2026-10-18 | **A field can link to another item's field, inside one vault, and reads through that item's gates.** Stored as `<item uuid>#<field id>`; a link to a link, to itself, to a missing item or across vaults is refused; a reveal, copy, code or fill resolves to the target, refuses a fill-only target, runs the target's gates and counts a use on both items (D151). Views show the target by title and label only. Plain version: a field can point at a password you already keep, and using it asks what that item asks. | Proposed; audit item; **flagged for Sean** (user-visible: "Link to another item" among fields of your own) |
| D236 | 2026-10-18 | **New passwords follow a site's rules: the ones saved on its login, else the ones it publishes.** Apple's `passwordrules` language, parsed with bounds; a login's own `pw.rule`; a bundled snapshot of Apple's password-manager-resources rules (436 sites, MIT, notice shipped). The editor's generate and the extension's suggestion both use them. In the extension, a new-password field's menu offers "Use a strong password", naming the rules it follows; the value is made only on the armed pick, the field and menu are measured as for a fill, and it enters the vault through the sign-up's own save offer. A fresh value is not a stored secret, so no gate applies. Plain version: Lockdown makes passwords a site will accept, and offers one when you sign up. | Proposed; audit item (the suggestion path); **flagged for Sean** (user-visible: the new menu row; how often the rules snapshot is refreshed) |
| D237 | 2026-10-18 | **Hotel and rental mode logs a sign-in on someone else's screen and reminds you to sign out.** "Signing in on a TV?" on an item logs a place and a reminder time (4 hours to two weeks) in the reserved, synced field `lockdown.away`; a due reminder puts the item in Needs attention and shows a toast after unlock; "Signed out" clears it. Nothing about the sign-in leaves the vault. Plain version: Lockdown remembers you signed in on the hotel TV and nags you to sign out. | Proposed; **flagged for Sean** (the words; no OS notification at the due time) |
| D238 | 2026-10-18 | **A card can ask before its security code is used.** The switch "Ask before using the security code" (`card.reprompt`) makes the core run the `Approve` gate before `card.cvc` is revealed, copied or filled; a browser fill fills the rest and leaves the code out until the person approves. Off by default. Plain version: your card's three digits need a second yes. | Proposed; **flagged for Sean** (user-visible; off by default) |
| D239 | 2026-10-18 | **A Wi-Fi item has a guest card that hides itself.** The ZXing `WIFI:` payload, encoded by `qrcode` 0.14 (MIT or Apache-2.0, no default features), returned as a `Revealed` grid behind the `Reveal` gate on a crown jewel, never for a fill-only item, counted as a use; the card shows the network name, the QR and the password in large type, hides after 60 seconds, on lock and when the window loses focus, and prints alone. WPA3 is coded as `T:WPA`. Plain version: show guests a code to join your Wi-Fi, or print it for the fridge. | Proposed; **flagged for Sean** (the 60-second card; the printed card shows the password) |
| D240 | 2026-10-18 | **CXF carries Wi-Fi and identity documents in its own types, and files by name and checksum only.** `wifi`, `passport`, `drivers-license` and `identity-document` both ways, mapped to the matching variants; a file as a `file` credential with `name`, `decryptedSize` and a SHA-256 `integrityHash`, no bytes, because CXF leaves bytes to CXP, still a draft. Lockdown's own backup is the lossless path for files. Templates travel in Lockdown backups only. | Proposed; **flagged for Sean** (field names to check against the CXF 1.0 text; files not carried by CXF) |
| D241 | 2026-10-18 | **Contacts are a separate list that is never filled.** The `identity/contact` variant has every field `Fill::Never`, is left out of the browser wallet, and has its own Contacts view; a person's own identities are what pages are offered. Plain version: keep your plumber's number in Lockdown without it ever landing in a form. | Proposed; **flagged for Sean** (user-visible: a Contacts view in the sidebar) |
| D242 | 2026-10-18 | **Every secret field shows in large type**, not only a login's password: one action on any hidden field but a code's set-up and passkey material, through the same `reveal` path and gates as Show. Plain version: any secret can be shown big enough to type on a TV. | Proposed; **flagged for Sean** (user-visible) |
| D243 | 2026-10-18 | **Vault depth reaches the web vault through the same commands.** Templates, variants, links, the generator's options and preview, the Wi-Fi card, sign-ins elsewhere and the room are the same command names and shapes in `crates/web` as on the desktop; attaching and saving files refuse in words there until the web vault has attachments. | Proposed |
| D244 | 2026-10-18 | **A field of your own can be a one-time code.** The custom kind `Totp` takes an `otpauth://` link or a bare secret, normalised on save, and shows a live code on any kind of item through the existing code path (copy is a use; a crown jewel asks). Plain version: any item can hold a two-step code, not only a login. | Proposed; **flagged for Sean** (user-visible) |
