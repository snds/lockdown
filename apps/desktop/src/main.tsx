import { StrictMode } from "react"
import { createRoot } from "react-dom/client"
import "./index.css"
import App from "./App.tsx"
import { QuickWindow } from "./app/quick-window"

// The system-wide shortcut's window loads the same page with `?window=quick`
// (docs/plan/26-desktop-finish.md §2) and draws only quick access.
const quick = new URLSearchParams(window.location.search).get("window") === "quick"

createRoot(document.getElementById("root")!).render(<StrictMode>{quick ? <QuickWindow /> : <App />}</StrictMode>)
