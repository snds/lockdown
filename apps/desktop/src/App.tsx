import { ThemeProvider } from "next-themes"

import { Toaster, TooltipProvider } from "@unicron/ui"

/** The screens land in the next commit; this is the shell's empty frame. */
export default function App() {
  return (
    <ThemeProvider attribute="class" defaultTheme="system" enableSystem disableTransitionOnChange>
      <TooltipProvider>
        <main className="flex min-h-full items-center justify-center bg-surface-locked p-6">
          <h1 className="text-lg font-semibold tracking-tight">Lockdown</h1>
        </main>
        <Toaster position="bottom-center" />
      </TooltipProvider>
    </ThemeProvider>
  )
}
