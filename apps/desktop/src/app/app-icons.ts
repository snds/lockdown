import { registerIcons } from "@unicron/ui"

/*
 * Two of IBM Carbon's drawings Lockdown uses that left unicron's curated set
 * in 1.1 (decision Unicron 98: 47 extras were cut so every app stops paying
 * for icons it never draws). Registered here, as unicron's
 * docs/08-migrating-to-1.1.md says, with the exact markup 1.0 drew, so
 * nothing looks different. Carbon's icons are Apache-2.0 (NOTICE).
 *
 *   star--filled   a favorite, as a mark and as the pressed star
 *   data-backup    Settings → Backups
 *
 * Imported once by each entry (apps/desktop/src/main.tsx, apps/web/src/main.tsx,
 * design/capture/src/app-main.tsx) before the first render.
 */
registerIcons({
  "star--filled": '<path d="M16,2l-4.55,9.22L1.28,12.69l7.36,7.18L6.9,30,16,25.22,25.1,30,23.36,19.87l7.36-7.17L20.55,11.22Z"/>',
  "data-backup":
    '<circle cx="7" cy="7" r="1"/><circle cx="7" cy="15" r="1"/><circle cx="7" cy="23" r="1"/><path d="M12,26H4V20h8V18H4V12H22V4a2,2,0,0,0-2-2H4A2,2,0,0,0,2,4V26a2,2,0,0,0,2,2h8ZM4,4H20v6H4Z"/><path d="M28,17v2.4131A6.996,6.996,0,1,0,22,30V28a5,5,0,1,1,4.5762-7H24v2h6V17Z"/>',
})

declare module "@unicron/ui" {
  interface UnicronIconRegistry {
    "star--filled": true
    "data-backup": true
  }
}
