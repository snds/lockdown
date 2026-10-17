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
