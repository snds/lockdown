import { swipesAt } from "./pointer"

describe("where a list's rows swipe (Lockdown 18)", () => {
  it("on a phone, and at the medium width only with a finger", () => {
    expect(swipesAt("phone", true, false)).toBe(true)
    expect(swipesAt("medium", false, true)).toBe(true)
    expect(swipesAt("medium", false, false)).toBe(false)
    expect(swipesAt("expanded", false, true)).toBe(false)
  })
})
