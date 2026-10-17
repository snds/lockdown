import type { ReactNode } from "react"

import { Badge, Icon } from "@unicron/ui"

/**
 * The crown-jewel treatment (D98): a section that says, in a word and a
 * glyph, that what is inside is the strictest class of secret, and that
 * using it takes a fresh check and revealing it the password (D101). The
 * gate itself runs in the core and asks through `app/gate.tsx`; this
 * wrapper is the cue, and the placement (the app window, never a page) is
 * the guarantee.
 *
 * Placeholder for unicron's CrownJewel treatment (docs/specs/security/
 * crown-jewel-treatment.md). The inverted `protected` surface is that
 * spec's to define: the `--c-crown-jewel-*` knobs in app-extension.css
 * stay empty until it lands, so this draws a plain bordered section with
 * the word, nothing more.
 */
export function CrownJewel({ children }: { children: ReactNode }) {
  return (
    <section aria-label="Protected item" data-crown-jewel="" className="flex flex-col gap-3 rounded-xl border-2 p-3">
      <p className="flex items-center gap-2 text-sm">
        <Badge variant="outline">
          <Icon name="lock" data-icon="inline-start" />
          Protected
        </Badge>
        <span className="text-muted-foreground">Using it asks for a fresh check; seeing it asks for your account password.</span>
      </p>
      {children}
    </section>
  )
}
