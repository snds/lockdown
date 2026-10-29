import { setIconCodepoints } from "@unicron/ui/icon"
import codepoints from "virtual:icon-codepoints"

/**
 * Imported once by each entry (the desktop's, the web vault's, the review
 * harness's): unicron's Icon then draws every icon by its codepoint in the
 * shipped font rather than by its ligature, which Apple's WebKit (so the
 * Mac app) does not shape inside SVG text (docs/plan/38-mac-fixes.md §2).
 * The codepoints are worked out at build time from the font itself.
 */
setIconCodepoints(codepoints)
