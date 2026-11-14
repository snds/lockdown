import { count, countName, countdown, fullDate, rowDate } from "./format"

// Local time, so the test reads the same in every zone.
const at = (y: number, m: number, d: number, h = 0, min = 0) => new Date(y, m - 1, d, h, min).getTime() / 1000

describe("counts (cross-app decisions §13)", () => {
  it("writes two digits, three from 100, a comma from 1,000", () => {
    expect([0, 4, 61, 99, 100, 248, 999, 1000, 1204, 12045].map((n) => count(n))).toEqual(["00", "04", "61", "99", "100", "248", "999", "1,000", "1,204", "12,045"])
  })

  it("stops at 99+ on a mark", () => {
    expect([4, 99, 100, 1204].map((n) => count(n, { cap: true }))).toEqual(["04", "99", "99+", "99+"])
  })

  it("is named by the plain number and its noun", () => {
    expect(countName(1, "item")).toBe("1 item")
    expect(countName(61, "item")).toBe("61 items")
    expect(countName(1204, "item")).toBe("1,204 items")
    expect(countName(3, "to fix", "to fix")).toBe("3 to fix")
    expect(countName(2, "type")).toBe("2 types")
  })
})

describe("countdowns", () => {
  it("is NN S under a minute and MM:SS from one", () => {
    expect([0, 8, 59, 60, 100, 605, 3599].map(countdown)).toEqual(["00 S", "08 S", "59 S", "01:00", "01:40", "10:05", "59:59"])
  })

  it("rounds a part of a second up and never goes below zero", () => {
    expect(countdown(7.2)).toBe("08 S")
    expect(countdown(-3)).toBe("00 S")
  })
})

describe("dates in rows (brief 4.0 Data)", () => {
  const now = at(2026, 11, 14, 12, 0)

  it("says today and yesterday with the clock", () => {
    expect(rowDate(at(2026, 11, 14, 9, 2), now)).toBe("Today 09:02")
    expect(rowDate(at(2026, 11, 13, 18, 40), now)).toBe("Yesterday 18:40")
  })

  it("names the day within a week", () => {
    expect(rowDate(at(2026, 11, 9, 9, 2), now)).toBe("MON 09:02")
    expect(rowDate(at(2026, 11, 8, 23, 59), now)).toBe("SUN 23:59")
  })

  it("is the month and day earlier this year, and the full date before that", () => {
    expect(rowDate(at(2026, 11, 7, 9, 2), now)).toBe("NOV 07")
    expect(rowDate(at(2026, 1, 2), now)).toBe("JAN 02")
    expect(rowDate(at(2025, 12, 31), now)).toBe("2025.12.31")
  })

  it("writes a date ahead of now in full", () => {
    expect(rowDate(at(2026, 11, 20), now)).toBe("2026.11.20")
    expect(fullDate(at(2026, 10, 2))).toBe("2026.10.02")
  })
})
