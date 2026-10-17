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
