/**
 * Light or dark with the system, as a `dark` class on the root. The
 * desktop uses next-themes, whose provider writes an inline script; the
 * web vault's Content Security Policy runs none, so this module does the
 * same job from the page's own bundle.
 */
export function followColorScheme() {
  const query = window.matchMedia("(prefers-color-scheme: dark)")
  const apply = () => {
    document.documentElement.classList.toggle("dark", query.matches)
    document.documentElement.style.colorScheme = query.matches ? "dark" : "light"
  }
  apply()
  query.addEventListener("change", apply)
}
