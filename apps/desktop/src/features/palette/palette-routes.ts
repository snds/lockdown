import { useEffect } from "react"

import { usePlaces, type Surface } from "@/app/places"

/**
 * The way from Quick access to a place (brief X4, GO TO). Quick access is
 * drawn outside the shell (App.tsx mounts it beside AppShell, and App.tsx is
 * the foundation's), so it cannot read the shell's `usePlaces()` itself.
 * The screens inside the shell that the vault lane owns hand the shell's
 * `go` and the build's surface over here while they show. The shell's `go`
 * only calls the app's own setter, so it keeps working after the screen
 * that handed it over has gone. The app opens on All items, so it is here
 * from the first unlock.
 *
 * Stand-in: once App.tsx passes `onGo` (and `surface`) to `QuickPalette`,
 * the palette uses those and this goes (doc 73 §7, the hand-off's ask).
 */
interface Routes {
  go: (placeId: string) => void
  surface: Surface
}

let routes: Routes | null = null

/** Called by a screen inside the shell: hands the shell's `go` and the surface to Quick access. */
export function useOfferRoutes(): void {
  const { go, surface } = usePlaces()
  useEffect(() => {
    routes = { go, surface }
  }, [go, surface])
}

/** What Quick access goes with: the shell's `go`, once a screen has handed it over. */
export function offeredRoutes(): Routes | null {
  return routes
}
