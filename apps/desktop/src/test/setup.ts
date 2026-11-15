import "@testing-library/jest-dom/vitest"

// jsdom has no ResizeObserver; Base UI's tabs measure their indicator with it.
if (typeof globalThis.ResizeObserver === "undefined") {
  class ResizeObserver {
    observe() {}
    unobserve() {}
    disconnect() {}
  }
  globalThis.ResizeObserver = ResizeObserver as unknown as typeof globalThis.ResizeObserver
}

// jsdom lays nothing out, so it has no scrollIntoView; cmdk scrolls the
// highlighted result into view when the highlight moves on its own.
if (typeof Element.prototype.scrollIntoView !== "function") {
  Element.prototype.scrollIntoView = function scrollIntoView() {}
}

// jsdom has no matchMedia. Every reader in unicron treats a missing one as "no match", and the
// shell's Toaster (sonner) reads the colour scheme through it, so a stub that never matches keeps
// both the same: this is what unicron's own test setup does. Put back before each test, because a
// test that answers media queries itself removes its own afterwards.
function stubMatchMedia() {
  if (typeof window.matchMedia !== "undefined") return
  Object.defineProperty(window, "matchMedia", {
    configurable: true,
    writable: true,
    value: (query: string) => ({
      matches: false,
      media: query,
      onchange: null,
      addEventListener: () => {},
      removeEventListener: () => {},
      addListener: () => {},
      removeListener: () => {},
      dispatchEvent: () => false,
    }),
  })
}
stubMatchMedia()
beforeEach(stubMatchMedia)
