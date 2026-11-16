import { useSyncExternalStore } from "react"

const COARSE = "(pointer: coarse)"

function subscribe(change: () => void): () => void {
  if (typeof window === "undefined" || !window.matchMedia) return () => {}
  const query = window.matchMedia(COARSE)
  query.addEventListener("change", change)
  return () => query.removeEventListener("change", change)
}

const coarse = () => typeof window !== "undefined" && typeof window.matchMedia === "function" && window.matchMedia(COARSE).matches

/** Whether the main pointer is a finger (a touch tablet, a phone): `(pointer: coarse)`. */
export function useCoarsePointer(): boolean {
  return useSyncExternalStore(subscribe, coarse, () => false)
}

/**
 * Whether a list's rows swipe (brief V1, Lockdown 18): on a phone, and at
 * the medium width when the pointer is a finger (a touch tablet), where
 * people expect to swipe. Never with a mouse at the medium width or on a
 * desktop: every swipe action is also in the row's menu (WCAG 2.5.7).
 */
export function swipesAt(layout: string, phone: boolean, coarsePointer: boolean): boolean {
  return phone || (layout === "medium" && coarsePointer)
}
