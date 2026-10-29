import type { ReactNode } from "react"

import { GateProvider } from "@/app/gate"
import { SettingsProvider } from "@/app/settings"
import { Toaster, TooltipProvider } from "@unicron/ui"
import { ThemeProvider } from "next-themes"

/** A component on its own, in the desktop's providers (a seam no screen mounts yet). */
export function Standalone({ children }: { children: ReactNode }) {
  return (
    <ThemeProvider attribute="class" defaultTheme="system" enableSystem disableTransitionOnChange>
      <TooltipProvider>
        <SettingsProvider>
          <GateProvider>
            <div className="h-full bg-surface-canvas" />
            {children}
          </GateProvider>
        </SettingsProvider>
        <Toaster position="bottom-center" />
      </TooltipProvider>
    </ThemeProvider>
  )
}

