import { StrictMode } from "react"
import { createRoot } from "react-dom/client"

import { TooltipProvider } from "@unicron/ui"

import "./popup.css"
import { Popup } from "./popup"

// The colour scheme follows the system, as a class on the root (no inline
// script: the extension's pages allow only their own files).
const dark = window.matchMedia("(prefers-color-scheme: dark)")
const theme = () => document.documentElement.classList.toggle("dark", dark.matches)
theme()
dark.addEventListener("change", theme)

createRoot(document.getElementById("app")!).render(
  <StrictMode>
    <TooltipProvider>
      <Popup />
    </TooltipProvider>
  </StrictMode>,
)
